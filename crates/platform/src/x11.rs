use std::sync::{Arc, Mutex};

use x11rb::connection::Connection as _;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, GetPropertyReply, Window};
use x11rb::rust_connection::RustConnection;

use crate::backend::Backend;
use crate::caps::Feature;
use crate::common::{covers, overlap, Rect};
use crate::error::{PlatformError, Result};
use crate::linux_common::portal_dark_mode;
use crate::traits::*;
use crate::{gnome, kde, linux_display};

/// X11 sessions can run any desktop, so ask the portal first, then the desktop's own setting.
pub(crate) async fn dark_mode() -> Result<bool> {
    match portal_dark_mode().await {
        Ok(dark) => Ok(dark),
        Err(e) => {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_uppercase();
            if desktop.split(':').any(|d| d == "KDE") {
                kde::dark_mode().await
            } else if desktop.contains("GNOME") {
                gnome::dark_mode().await
            } else {
                Err(e)
            }
        }
    }
}

#[path = "x11_window_control.rs"]
pub(crate) mod control;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        UTF8_STRING,
        _NET_WM_PID,
        _NET_WM_STATE,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_FRAME_EXTENTS,
    }
}

struct X {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

trait XResult<T> {
    fn x(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> XResult<T> for std::result::Result<T, E> {
    fn x(self) -> Result<T> {
        self.map_err(|e| PlatformError::backend("x11", e))
    }
}

fn connect() -> Result<X> {
    let (conn, screen) = x11rb::connect(None).x()?;
    let root = conn.setup().roots.get(screen).ok_or_else(|| PlatformError::backend("x11", "no such screen"))?.root;
    let atoms = Atoms::new(&conn).x()?.reply().x()?;
    Ok(X { conn, root, atoms })
}

/// One shared connection; dropped after a failure so the next call reconnects.
fn with_x<T>(f: impl FnOnce(&X) -> Result<T>) -> Result<T> {
    static CACHE: Mutex<Option<Arc<X>>> = Mutex::new(None);
    let x = {
        let mut cached = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match &*cached {
            Some(x) => x.clone(),
            None => cached.insert(Arc::new(connect()?)).clone(),
        }
    };
    let r = f(&x);
    if r.is_err() {
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    r
}

pub(crate) struct RandrOutput {
    pub name: String,
    pub bounds: Rect,
    pub primary: bool,
}

/// Connected RandR outputs with the position of their CRTC (zero when switched off).
pub(crate) fn randr_outputs() -> Result<Vec<RandrOutput>> {
    with_x(|x| {
        let c = &x.conn;
        c.randr_query_version(1, 3).x()?.reply().x()?;
        let res = c.randr_get_screen_resources_current(x.root).x()?.reply().x()?;
        let primary = c.randr_get_output_primary(x.root).x()?.reply().x()?.output;
        let mut out = Vec::new();
        for &o in &res.outputs {
            let info = c.randr_get_output_info(o, res.config_timestamp).x()?.reply().x()?;
            if info.connection != randr::Connection::CONNECTED {
                continue;
            }
            let bounds = if info.crtc == 0 {
                Rect::default()
            } else {
                let crtc = c.randr_get_crtc_info(info.crtc, res.config_timestamp).x()?.reply().x()?;
                Rect { x: crtc.x.into(), y: crtc.y.into(), width: crtc.width.into(), height: crtc.height.into() }
            };
            out.push(RandrOutput { name: String::from_utf8_lossy(&info.name).into_owned(), bounds, primary: o == primary });
        }
        Ok(out)
    })
}

/// The class part of WM_CLASS (`instance\0class\0`), else the instance.
pub(crate) fn wm_class(bytes: &[u8]) -> Option<String> {
    let mut parts = bytes.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned());
    let instance = parts.next();
    parts.next().or(instance)
}

/// The display holding most of `bounds`.
pub(crate) fn monitor_for(bounds: Rect, monitors: &[(String, Rect)]) -> Option<String> {
    monitors
        .iter()
        .map(|(id, r)| (overlap(bounds, *r), id))
        .filter(|(area, _)| *area > 0)
        .max_by_key(|(area, _)| *area)
        .map(|(_, id)| id.clone())
}

struct Client {
    info: WindowInfo,
    fullscreen: bool,
    desktop: bool,
    dock: bool,
}

impl X {
    fn prop(&self, win: Window, atom: impl Into<Atom>) -> Option<GetPropertyReply> {
        let r = self.conn.get_property(false, win, atom, AtomEnum::ANY, 0, 1 << 16).ok()?.reply().ok()?;
        (r.format != 0).then_some(r)
    }

    fn prop32(&self, win: Window, atom: impl Into<Atom>) -> Vec<u32> {
        self.prop(win, atom).and_then(|r| r.value32().map(Iterator::collect)).unwrap_or_default()
    }

    fn active(&self) -> Option<Window> {
        self.prop32(self.root, self.atoms._NET_ACTIVE_WINDOW).first().copied().filter(|w| *w != 0)
    }

    fn bounds(&self, win: Window) -> Option<Rect> {
        let geo = self.conn.get_geometry(win).ok()?.reply().ok()?;
        let at = self.conn.translate_coordinates(win, self.root, 0, 0).ok()?.reply().ok()?;
        let (l, r, t, b) = match self.prop32(win, self.atoms._NET_FRAME_EXTENTS)[..] {
            [l, r, t, b] => (l, r, t, b),
            _ => (0, 0, 0, 0),
        };
        Some(Rect {
            x: i32::from(at.dst_x) - l as i32,
            y: i32::from(at.dst_y) - t as i32,
            width: u32::from(geo.width) + l + r,
            height: u32::from(geo.height) + t + b,
        })
    }

    fn client(&self, win: Window, active: Option<Window>, monitors: &[(String, Rect)]) -> Option<Client> {
        let a = &self.atoms;
        let bounds = self.bounds(win)?;
        let title = self
            .prop(win, a._NET_WM_NAME)
            .filter(|r| r.type_ == a.UTF8_STRING)
            .or_else(|| self.prop(win, AtomEnum::WM_NAME))
            .map(|r| String::from_utf8_lossy(&r.value).into_owned())
            .unwrap_or_default();
        let states = self.prop32(win, a._NET_WM_STATE);
        let types = self.prop32(win, a._NET_WM_WINDOW_TYPE);
        Some(Client {
            info: WindowInfo {
                id: WindowId(win.into()),
                title,
                app_id: self.prop(win, AtomEnum::WM_CLASS).and_then(|r| wm_class(&r.value)),
                pid: self.prop32(win, a._NET_WM_PID).first().copied(),
                monitor: monitor_for(bounds, monitors),
                bounds,
                minimized: states.contains(&a._NET_WM_STATE_HIDDEN),
                focused: Some(win) == active,
            },
            fullscreen: states.contains(&a._NET_WM_STATE_FULLSCREEN),
            desktop: types.contains(&a._NET_WM_WINDOW_TYPE_DESKTOP),
            dock: types.contains(&a._NET_WM_WINDOW_TYPE_DOCK),
        })
    }
}

fn wayland_unsupported() -> PlatformError {
    PlatformError::unsupported(Feature::WindowList, "Wayland window list needs the KWin script / GNOME extension (later phase)")
}

/// For window features X11 doesn't have yet either: the Wayland reason there, else "not implemented yet".
pub(crate) fn wm_unsupported(backend: Backend, feature: Feature) -> PlatformError {
    if backend.is_wayland() { wayland_unsupported() } else { PlatformError::not_implemented(feature) }
}

fn x11_only(backend: Backend) -> Result<()> {
    if backend.is_wayland() { Err(wayland_unsupported()) } else { Ok(()) }
}

fn monitors() -> Vec<(String, Rect)> {
    linux_display::connected_displays(true).into_iter().map(|d| (d.id, d.bounds)).collect()
}

/// App windows front to back (`_NET_CLIENT_LIST_STACKING` is bottom to top).
pub(crate) fn windows(backend: Backend) -> Result<Vec<WindowInfo>> {
    x11_only(backend)?;
    let monitors = monitors();
    with_x(|x| {
        let mut list = x.prop32(x.root, x.atoms._NET_CLIENT_LIST_STACKING);
        if list.is_empty() {
            list = x.prop32(x.root, x.atoms._NET_CLIENT_LIST);
        }
        let active = x.active();
        Ok(list
            .into_iter()
            .rev()
            .filter_map(|w| x.client(w, active, &monitors))
            .filter(|c| !c.desktop && !c.dock)
            .map(|c| c.info)
            .collect())
    })
}

pub(crate) fn foreground(backend: Backend) -> Result<Option<WindowInfo>> {
    x11_only(backend)?;
    let monitors = monitors();
    with_x(|x| {
        let Some(win) = x.active() else { return Ok(None) };
        Ok(x.client(win, Some(win), &monitors).filter(|c| !c.desktop && !c.dock).map(|c| c.info))
    })
}

pub(crate) fn foreground_fullscreen(backend: Backend, monitor: Rect) -> Result<bool> {
    x11_only(backend)?;
    with_x(|x| {
        let Some(win) = x.active() else { return Ok(false) };
        Ok(x.client(win, Some(win), &[]).is_some_and(|c| {
            !c.desktop && !c.info.minimized && (c.fullscreen || covers(c.info.bounds, monitor))
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wm_class_parts() {
        assert_eq!(wm_class(b"navigator\0firefox\0").as_deref(), Some("firefox"));
        assert_eq!(wm_class(b"xterm\0").as_deref(), Some("xterm"));
        assert_eq!(wm_class(b""), None);
    }

    #[test]
    fn monitor_by_overlap() {
        let r = |x, w| Rect { x, y: 0, width: w, height: 1080 };
        let mons = vec![("card1-eDP-1".to_string(), r(0, 1920)), ("card1-DP-1".to_string(), r(1920, 2560))];
        assert_eq!(monitor_for(r(1800, 800), &mons).as_deref(), Some("card1-DP-1"));
        assert_eq!(monitor_for(r(100, 800), &mons).as_deref(), Some("card1-eDP-1"));
        assert_eq!(monitor_for(r(-5000, 10), &mons), None);
    }
}
