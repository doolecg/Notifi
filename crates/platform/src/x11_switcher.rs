//! What the app switcher and taskbar preview need from X11: XComposite window snapshots,
//! `_NET_WM_ICON` icons, EWMH focus/close/minimise requests and a key grab that can hold a
//! modifier (grab the trigger combo on the root window, then the whole keyboard while open).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use x11rb::connection::Connection as _;
use x11rb::protocol::composite::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, GrabMode, ImageFormat, ModMask, Window,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::common::{Guard, Image};
use crate::error::{PlatformError, Result};
use crate::traits::WindowId;

fn xerr(e: impl std::fmt::Display) -> PlatformError {
    PlatformError::backend("x11", e)
}

struct Conn {
    conn: RustConnection,
    root: Window,
}

fn with_conn<T>(f: impl FnOnce(&Conn) -> Result<T>) -> Result<T> {
    static CACHE: Mutex<Option<Arc<Conn>>> = Mutex::new(None);
    let c = {
        let mut cached = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match &*cached {
            Some(c) => c.clone(),
            None => {
                let (conn, screen) = x11rb::connect(None).map_err(xerr)?;
                let root = conn.setup().roots.get(screen).ok_or_else(|| xerr("no such screen"))?.root;
                cached.insert(Arc::new(Conn { conn, root })).clone()
            }
        }
    };
    let r = f(&c);
    if r.is_err() {
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    r
}

fn atom(c: &Conn, name: &str) -> Result<u32> {
    Ok(c.conn.intern_atom(false, name.as_bytes()).map_err(xerr)?.reply().map_err(xerr)?.atom)
}

/// Nearest-pixel downscale so the image fits in `max_w` x `max_h`.
pub(crate) fn fit(width: u32, height: u32, rgba: &[u8], max_w: u32, max_h: u32) -> Image {
    let scale = (max_w as f64 / width.max(1) as f64).min(max_h as f64 / height.max(1) as f64).min(1.0);
    let (w, h) = (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1));
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = ((y as f64 + 0.5) / scale) as u32;
        for x in 0..w {
            let sx = ((x as f64 + 0.5) / scale) as u32;
            let i = ((sy.min(height - 1) * width + sx.min(width - 1)) * 4) as usize;
            out.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    Image { width: w, height: h, rgba: out }
}

/// A snapshot of the window's contents through XComposite, scaled to fit `max_w` x `max_h`.
pub fn snapshot(source: WindowId, max_w: u32, max_h: u32) -> Result<Image> {
    with_conn(|c| {
        let win = source.0 as Window;
        c.conn.composite_query_version(0, 4).map_err(xerr)?.reply().map_err(xerr)?;
        c.conn.composite_redirect_window(win, composite::Redirect::AUTOMATIC).map_err(xerr)?;
        let pixmap = c.conn.generate_id().map_err(xerr)?;
        c.conn.composite_name_window_pixmap(win, pixmap).map_err(xerr)?;
        let geo = c.conn.get_geometry(win).map_err(xerr)?.reply().map_err(xerr)?;
        let img = c.conn.get_image(ImageFormat::Z_PIXMAP, pixmap, 0, 0, geo.width, geo.height, !0).map_err(xerr)?.reply();
        let _ = c.conn.free_pixmap(pixmap);
        let _ = c.conn.composite_unredirect_window(win, composite::Redirect::AUTOMATIC);
        let _ = c.conn.flush();
        let img = img.map_err(xerr)?;
        let (w, h) = (u32::from(geo.width), u32::from(geo.height));
        if img.data.len() < (w * h * 4) as usize {
            return Err(xerr("unexpected image depth"));
        }
        let rgba: Vec<u8> = img.data.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
        Ok(fit(w, h, &rgba, max_w, max_h))
    })
}

/// Picks the `_NET_WM_ICON` entry closest to `size` (smallest at least that big, else the largest).
pub(crate) fn pick_icon(data: &[u32], size: u32) -> Option<Image> {
    let mut best: Option<(u32, u32, &[u32])> = None;
    let mut i = 0;
    while i + 2 <= data.len() {
        let (w, h) = (data[i], data[i + 1]);
        let n = (w as usize) * (h as usize);
        let Some(px) = data.get(i + 2..i + 2 + n) else { break };
        let better = match best {
            None => true,
            Some((bw, _, _)) => (w >= size && (bw < size || w < bw)) || (bw < size && w > bw),
        };
        if better && w > 0 && h > 0 {
            best = Some((w, h, px));
        }
        i += 2 + n;
    }
    let (w, h, px) = best?;
    let rgba: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
    Some(fit(w, h, &rgba, size, size))
}

pub fn icon(source: WindowId, size: u32) -> Option<Image> {
    with_conn(|c| {
        let a = atom(c, "_NET_WM_ICON")?;
        let r = c.conn.get_property(false, source.0 as Window, a, AtomEnum::CARDINAL, 0, 1 << 20).map_err(xerr)?.reply().map_err(xerr)?;
        let data: Vec<u32> = r.value32().map(Iterator::collect).unwrap_or_default();
        pick_icon(&data, size).ok_or_else(|| xerr("no icon"))
    })
    .ok()
}

fn send(c: &Conn, win: Window, name: &str, data: [u32; 5]) -> Result<()> {
    let ev = ClientMessageEvent::new(32, win, atom(c, name)?, data);
    c.conn
        .send_event(false, c.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev)
        .map_err(xerr)?;
    c.conn.flush().map_err(xerr)
}

/// Asks the window manager to activate (raise, restore and focus) the window.
pub fn activate(id: WindowId) -> Result<()> {
    with_conn(|c| send(c, id.0 as Window, "_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0]))
}

pub fn close(id: WindowId) -> Result<()> {
    with_conn(|c| send(c, id.0 as Window, "_NET_CLOSE_WINDOW", [0, 2, 0, 0, 0]))
}

pub fn minimize(id: WindowId) -> Result<()> {
    with_conn(|c| send(c, id.0 as Window, "WM_CHANGE_STATE", [3, 0, 0, 0, 0]))
}

/// One key event seen by [`grab_keys`]: the first keysym of the key, and the modifier mask after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrabbedKey {
    pub keysym: u32,
    pub pressed: bool,
    /// X modifier mask (Shift 1, Control 4, Mod1/Alt 8, Mod4/Super 64) after this event.
    pub mods: u16,
}

pub const MOD_SHIFT: u16 = 1;
pub const MOD_CONTROL: u16 = 4;
pub const MOD_ALT: u16 = 8;
pub const MOD_SUPER: u16 = 64;

fn modifier_bit(keysym: u32) -> u16 {
    match keysym {
        0xffe1 | 0xffe2 => MOD_SHIFT,
        0xffe3 | 0xffe4 => MOD_CONTROL,
        0xffe9 | 0xffea | 0xffe7 | 0xffe8 => MOD_ALT,
        0xffeb | 0xffec => MOD_SUPER,
        _ => 0,
    }
}

/// Grabs each `(mods, keysym)` combo on the root window. While `keyboard` is true the whole
/// keyboard is grabbed too, so modifier releases and typing reach `on_key`. Dropping the guard
/// stops the thread and releases the grabs.
pub fn grab_keys(combos: Vec<(u16, u32)>, keyboard: Arc<AtomicBool>, mut on_key: impl FnMut(GrabbedKey) + Send + 'static) -> Result<Guard> {
    let (conn, screen) = x11rb::connect(None).map_err(xerr)?;
    let root = conn.setup().roots.get(screen).ok_or_else(|| xerr("no such screen"))?.root;
    let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
    let map = conn.get_keyboard_mapping(min, max - min + 1).map_err(xerr)?.reply().map_err(xerr)?;
    let per = map.keysyms_per_keycode.max(1) as usize;
    let keysym_of = move |code: u8| map.keysyms.get((code - min) as usize * per).copied().unwrap_or(0);
    let codes: Vec<(u16, u8)> = combos
        .iter()
        .flat_map(|&(mods, sym)| {
            let ks = &keysym_of;
            (min..=max).filter(move |&c| ks(c) == sym).map(move |c| (mods, c))
        })
        .collect();
    for &(mods, code) in &codes {
        for extra in [0u16, 2, 16, 18] {
            conn.grab_key(true, root, ModMask::from(mods | extra), code, GrabMode::ASYNC, GrabMode::ASYNC).map_err(xerr)?;
        }
    }
    conn.flush().map_err(xerr)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = stop.clone();
    std::thread::Builder::new()
        .name("x11-switcher-keys".into())
        .spawn(move || {
            let mut grabbed = false;
            while !stop.load(Ordering::SeqCst) {
                let want = keyboard.load(Ordering::SeqCst);
                if want != grabbed {
                    if want {
                        let _ = conn.grab_keyboard(true, root, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC);
                    } else {
                        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
                    }
                    let _ = conn.flush();
                    grabbed = want;
                }
                match conn.poll_for_event() {
                    Ok(Some(Event::KeyPress(e))) => {
                        let sym = keysym_of(e.detail);
                        on_key(GrabbedKey { keysym: sym, pressed: true, mods: u16::from(e.state) | modifier_bit(sym) });
                    }
                    Ok(Some(Event::KeyRelease(e))) => {
                        let sym = keysym_of(e.detail);
                        on_key(GrabbedKey { keysym: sym, pressed: false, mods: u16::from(e.state) & !modifier_bit(sym) });
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => std::thread::sleep(Duration::from_millis(8)),
                    Err(_) => break,
                }
            }
            let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
            for &(mods, code) in &codes {
                for extra in [0u16, 2, 16, 18] {
                    let _ = conn.ungrab_key(code, root, ModMask::from(mods | extra));
                }
            }
            let _ = conn.flush();
        })
        .map_err(xerr)?;
    Ok(Guard::new(move || stopper.store(true, Ordering::SeqCst)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_inside() {
        let img = fit(4, 2, &[255u8; 32], 2, 2);
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.rgba.len(), 8);
        let same = fit(2, 2, &[1u8; 16], 10, 10);
        assert_eq!((same.width, same.height), (2, 2));
    }

    #[test]
    fn picks_icon_size() {
        let data = [2u32, 2, 0xff00_00ff, 0xff00_00ff, 0xff00_00ff, 0xff00_00ff, 1, 1, 0x80ff_0000];
        let i = pick_icon(&data, 2).unwrap();
        assert_eq!((i.width, i.height), (2, 2));
        assert_eq!(&i.rgba[..4], &[0, 0, 255, 255]);
        let small = pick_icon(&data[..6], 1).unwrap();
        assert_eq!(small.width, 1);
    }
}
