//! Moving, sizing and organising windows on X11 through EWMH client messages to the window manager.
//! Wayland has no equivalent; those calls report the KWin script / GNOME extension requirement.

use std::collections::HashSet;
use std::sync::mpsc::channel;
use std::time::Duration;

use x11rb::protocol::xproto::{
    Atom, ChangeWindowAttributesAux, ClientMessageEvent, EventMask, PropMode, Window,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

use super::*;
use crate::common::EventStream;
use crate::window_control::{DesktopState, PointerState, WindowState, WorkArea};

x11rb::atom_manager! {
    Ext: ExtCookie {
        _NET_MOVERESIZE_WINDOW,
        _NET_CLOSE_WINDOW,
        _NET_CURRENT_DESKTOP,
        _NET_NUMBER_OF_DESKTOPS,
        _NET_WM_DESKTOP,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_WM_STRUT_PARTIAL,
        _NET_WM_STRUT,
        _MOTIF_WM_HINTS,
        WM_CHANGE_STATE,
        WM_NORMAL_HINTS,
    }
}

fn root_mask() -> EventMask {
    EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT
}
const SOURCE_PAGER: u32 = 2;

fn ext(x: &X) -> Result<Ext> {
    Ext::new(&x.conn).x()?.reply().x()
}

fn message(x: &X, win: Window, atom: Atom, data: [u32; 5]) -> Result<()> {
    let ev = ClientMessageEvent::new(32, win, atom, data);
    x.conn.send_event(false, x.root, root_mask(), ev).x()?;
    x.conn.flush().x()
}

fn win(id: WindowId) -> Window {
    id.0 as Window
}

fn state_change(x: &X, w: Window, add: bool, a: Atom, b: Atom) -> Result<()> {
    message(x, w, x.atoms._NET_WM_STATE, [u32::from(add), a, b, 1, 0])
}

pub(crate) fn focus(backend: Backend, id: WindowId) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| message(x, win(id), x.atoms._NET_ACTIVE_WINDOW, [SOURCE_PAGER, 0, 0, 0, 0]))
}

pub(crate) fn close(backend: Backend, id: WindowId) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| message(x, win(id), ext(x)?._NET_CLOSE_WINDOW, [0, SOURCE_PAGER, 0, 0, 0]))
}

pub(crate) fn minimize(backend: Backend, id: WindowId) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        let ev = ClientMessageEvent::new(32, win(id), e.WM_CHANGE_STATE, [3, 0, 0, 0, 0]);
        x.conn.send_event(false, x.root, root_mask(), ev).x()?;
        x.conn.flush().x()
    })
}

pub(crate) fn maximize(backend: Backend, id: WindowId) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        state_change(x, win(id), true, e._NET_WM_STATE_MAXIMIZED_VERT, e._NET_WM_STATE_MAXIMIZED_HORZ)
    })
}

pub(crate) fn restore(backend: Backend, id: WindowId) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        state_change(x, win(id), false, e._NET_WM_STATE_MAXIMIZED_VERT, e._NET_WM_STATE_MAXIMIZED_HORZ)?;
        state_change(x, win(id), false, x.atoms._NET_WM_STATE_HIDDEN, 0)?;
        message(x, win(id), x.atoms._NET_ACTIVE_WINDOW, [SOURCE_PAGER, 0, 0, 0, 0])
    })
}

/// Sets the window's outer bounds (frame included); the manager takes the client rectangle.
pub(crate) fn set_bounds(backend: Backend, id: WindowId, target: Rect) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        let w = win(id);
        state_change(x, w, false, e._NET_WM_STATE_MAXIMIZED_VERT, e._NET_WM_STATE_MAXIMIZED_HORZ)?;
        let (l, r, t, b) = match x.prop32(w, x.atoms._NET_FRAME_EXTENTS)[..] {
            [l, r, t, b] => (l as i32, r as i32, t as i32, b as i32),
            _ => (0, 0, 0, 0),
        };
        let flags = 1 << 8 | 1 << 9 | 1 << 10 | 1 << 11 | SOURCE_PAGER << 12;
        let data = [
            flags,
            (target.x + l) as u32,
            (target.y + t) as u32,
            (target.width as i32 - l - r).max(1) as u32,
            (target.height as i32 - t - b).max(1) as u32,
        ];
        message(x, w, e._NET_MOVERESIZE_WINDOW, data)
    })
}

pub(crate) fn set_decorations(backend: Backend, id: WindowId, on: bool) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        // flags = decorations field valid, decorations = all or none.
        let hints = [2u32, 0, u32::from(on), 0, 0];
        x.conn.change_property32(PropMode::REPLACE, win(id), e._MOTIF_WM_HINTS, e._MOTIF_WM_HINTS, &hints).x()?;
        x.conn.flush().x()
    })
}

pub(crate) fn window_state(backend: Backend, id: WindowId) -> Result<WindowState> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        let w = win(id);
        let states = x.prop32(w, x.atoms._NET_WM_STATE);
        let fixed = x
            .prop32(w, e.WM_NORMAL_HINTS)
            .get(..9)
            .is_some_and(|h| h[0] & (1 << 4 | 1 << 5) == (1 << 4 | 1 << 5) && h[5] == h[7] && h[6] == h[8] && h[5] != 0);
        Ok(WindowState {
            maximized: states.contains(&e._NET_WM_STATE_MAXIMIZED_VERT) && states.contains(&e._NET_WM_STATE_MAXIMIZED_HORZ),
            minimized: states.contains(&x.atoms._NET_WM_STATE_HIDDEN),
            fullscreen: states.contains(&x.atoms._NET_WM_STATE_FULLSCREEN),
            resizable: !fixed,
        })
    })
}

/// Reserved edge space from a dock's strut: (left, right, top, bottom).
fn strut(x: &X, e: &Ext, w: Window) -> Option<[u32; 4]> {
    let v = x.prop32(w, e._NET_WM_STRUT_PARTIAL);
    let v = if v.len() >= 4 { v } else { x.prop32(w, e._NET_WM_STRUT) };
    (v.len() >= 4).then(|| [v[0], v[1], v[2], v[3]])
}

pub(crate) fn work_areas(backend: Backend) -> Result<Vec<WorkArea>> {
    x11_only(backend)?;
    let outputs = randr_outputs()?;
    with_x(|x| {
        let e = ext(x)?;
        let screen = x.conn.get_geometry(x.root).x()?.reply().x()?;
        let (sw, sh) = (i64::from(screen.width), i64::from(screen.height));
        let docks: Vec<[u32; 4]> = x
            .prop32(x.root, x.atoms._NET_CLIENT_LIST)
            .into_iter()
            .filter(|w| x.prop32(*w, x.atoms._NET_WM_WINDOW_TYPE).contains(&x.atoms._NET_WM_WINDOW_TYPE_DOCK))
            .filter_map(|w| strut(x, &e, w))
            .collect();
        Ok(outputs
            .into_iter()
            .filter(|o| o.bounds.width > 0)
            .map(|o| {
                let b = o.bounds;
                let (mut l, mut r, mut t, mut bt) = (0i64, 0i64, 0i64, 0i64);
                // A strut is measured from the screen edge; it only belongs to an output sitting on that edge.
                for [sl, sr, st, sb] in &docks {
                    if b.x == 0 {
                        l = l.max(i64::from(*sl));
                    }
                    if i64::from(b.x) + i64::from(b.width) == sw {
                        r = r.max(i64::from(*sr));
                    }
                    if b.y == 0 {
                        t = t.max(i64::from(*st));
                    }
                    if i64::from(b.y) + i64::from(b.height) == sh {
                        bt = bt.max(i64::from(*sb));
                    }
                }
                let work = Rect {
                    x: b.x + l as i32,
                    y: b.y + t as i32,
                    width: (i64::from(b.width) - l - r).max(1) as u32,
                    height: (i64::from(b.height) - t - bt).max(1) as u32,
                };
                WorkArea { display_id: o.name, bounds: b, work, scale: 1.0 }
            })
            .collect())
    })
}

pub(crate) fn pointer(backend: Backend) -> Result<PointerState> {
    x11_only(backend)?;
    with_x(|x| {
        let q = x.conn.query_pointer(x.root).x()?.reply().x()?;
        let m = u16::from(q.mask);
        Ok(PointerState {
            x: q.root_x.into(),
            y: q.root_y.into(),
            left: m & (1 << 8) != 0,
            right: m & (1 << 10) != 0,
            shift: m & 1 != 0,
            ctrl: m & 4 != 0,
            alt: m & 8 != 0,
            meta: m & 64 != 0,
        })
    })
}

pub(crate) fn quit_process(backend: Backend, pid: u32) -> Result<()> {
    x11_only(backend)?;
    if pid == 0 || pid == std::process::id() {
        return Err(PlatformError::backend("x11", "refusing to end Notifi itself"));
    }
    let kill = |sig: &str| std::process::Command::new("kill").args([sig, &pid.to_string()]).status().is_ok_and(|s| s.success());
    kill("-TERM");
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return Ok(());
        }
    }
    kill("-KILL");
    Ok(())
}

pub(crate) fn desktop_state(backend: Backend) -> Result<DesktopState> {
    x11_only(backend)?;
    with_x(|x| {
        let e = ext(x)?;
        let count = x.prop32(x.root, e._NET_NUMBER_OF_DESKTOPS).first().copied().unwrap_or(1).max(1);
        let current = x.prop32(x.root, e._NET_CURRENT_DESKTOP).first().copied().unwrap_or(0);
        Ok(DesktopState { count, current: current + 1 })
    })
}

pub(crate) fn switch_desktop(backend: Backend, n: u32) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| message(x, x.root, ext(x)?._NET_CURRENT_DESKTOP, [n.saturating_sub(1), 0, 0, 0, 0]))
}

pub(crate) fn create_desktop(backend: Backend) -> Result<()> {
    let count = desktop_state(backend)?.count;
    with_x(|x| message(x, x.root, ext(x)?._NET_NUMBER_OF_DESKTOPS, [count + 1, 0, 0, 0, 0]))
}

pub(crate) fn move_to_desktop(backend: Backend, id: WindowId, n: u32) -> Result<()> {
    x11_only(backend)?;
    with_x(|x| message(x, win(id), ext(x)?._NET_WM_DESKTOP, [n.saturating_sub(1), SOURCE_PAGER, 0, 0, 0]))
}

/// Created, destroyed and focus changes from root property changes. X11 has no move/size start events.
pub(crate) fn events(backend: Backend) -> Result<EventStream<WindowEvent>> {
    x11_only(backend)?;
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("notifi-x11-events".into())
        .spawn(move || {
            let Ok(x) = connect() else { return };
            let _ = x.conn.change_window_attributes(x.root, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE));
            let _ = x.conn.flush();
            let mut seen: HashSet<u32> = x.prop32(x.root, x.atoms._NET_CLIENT_LIST).into_iter().collect();
            while let Ok(ev) = x.conn.wait_for_event() {
                let Event::PropertyNotify(p) = ev else { continue };
                let sent = if p.atom == x.atoms._NET_CLIENT_LIST {
                    let now: HashSet<u32> = x.prop32(x.root, x.atoms._NET_CLIENT_LIST).into_iter().collect();
                    let mut out: Vec<WindowEvent> = now.difference(&seen).map(|w| WindowEvent::Created(WindowId((*w).into()))).collect();
                    out.extend(seen.difference(&now).map(|w| WindowEvent::Destroyed(WindowId((*w).into()))));
                    seen = now;
                    out
                } else if p.atom == x.atoms._NET_ACTIVE_WINDOW {
                    x.active().map(|w| WindowEvent::Focused(WindowId(w.into()))).into_iter().collect()
                } else {
                    continue;
                };
                if sent.into_iter().any(|e| tx.send(e).is_err()) {
                    return;
                }
            }
        })
        .map_err(|e| PlatformError::backend("x11", e))?;
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_is_unsupported() {
        assert!(matches!(work_areas(Backend::OtherWayland), Err(PlatformError::Unsupported { .. })));
        assert!(matches!(set_bounds(Backend::OtherWayland, WindowId(1), Rect::default()), Err(PlatformError::Unsupported { .. })));
    }
}
