//! X11 side of [`crate::observe`]: the RECORD extension copies every client's device events to a
//! second connection, and key codes become text through the core keyboard mapping.

use std::sync::mpsc::channel;

use x11rb::connection::{Connection as _, RequestConnection as _};
use x11rb::protocol::record::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _, KeyPressEvent};
use x11rb::x11_utils::TryParse;

use crate::caps::Feature;
use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::observe::{Observed, Watch};
use crate::traits::MouseButton;

/// Reply category for data the server sent.
const FROM_SERVER: u8 = 0;

fn xerr(e: impl std::fmt::Display) -> PlatformError {
    PlatformError::backend("x11 record", e)
}

pub fn observe(watch: Watch) -> Result<EventStream<Observed>> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Err(PlatformError::unsupported(Feature::InputHook, "Wayland sessions need evdev access, which Notifi does not set up yet"));
    }
    let (ctrl, _) = x11rb::connect(None).map_err(xerr)?;
    let (data, _) = x11rb::connect(None).map_err(xerr)?;
    if ctrl.extension_information(record::X11_EXTENSION_NAME).map_err(xerr)?.is_none() {
        return Err(PlatformError::unsupported(Feature::InputHook, "the X server has no RECORD extension"));
    }
    let (min, max) = (ctrl.setup().min_keycode, ctrl.setup().max_keycode);
    let map = ctrl.get_keyboard_mapping(min, max - min + 1).map_err(xerr)?.reply().map_err(xerr)?;
    let keymap = Keymap { min, per: map.keysyms_per_keycode as usize, syms: map.keysyms };

    let rc = ctrl.generate_id().map_err(xerr)?;
    let first = if watch.keys { xproto::KEY_PRESS_EVENT } else { xproto::BUTTON_PRESS_EVENT };
    let last = if watch.motion { xproto::MOTION_NOTIFY_EVENT } else { xproto::BUTTON_RELEASE_EVENT };
    let range = record::Range { device_events: record::Range8 { first, last }, ..Default::default() };
    ctrl.record_create_context(rc, 0, &[record::CS::ALL_CLIENTS.into()], &[range])
        .map_err(xerr)?
        .check()
        .map_err(xerr)?;

    let (tx, rx) = channel();
    std::thread::Builder::new().name("notifi-observe".into()).spawn(move || {
        if let Ok(replies) = data.record_enable_context(rc) {
            'replies: for reply in replies {
                let Ok(reply) = reply else { break };
                if reply.client_swapped || reply.category != FROM_SERVER {
                    continue;
                }
                // Device events are 32 bytes each; key, button and motion events share one layout.
                for chunk in reply.data.chunks_exact(32) {
                    let Ok((ev, _)) = KeyPressEvent::try_parse(chunk) else { continue };
                    if let Some(event) = event(chunk[0] & 0x7f, &ev, &keymap, watch) {
                        if tx.send(event).is_err() {
                            break 'replies;
                        }
                    }
                }
            }
        }
        let _ = ctrl.record_disable_context(rc);
        let _ = ctrl.record_free_context(rc);
        let _ = ctrl.flush();
    })?;
    Ok(rx)
}

fn event(kind: u8, ev: &KeyPressEvent, keymap: &Keymap, watch: Watch) -> Option<Observed> {
    let (x, y) = (ev.root_x as i32, ev.root_y as i32);
    match kind {
        xproto::KEY_PRESS_EVENT => keymap.translate(ev.detail, u16::from(ev.state)),
        xproto::BUTTON_PRESS_EVENT | xproto::BUTTON_RELEASE_EVENT if watch.buttons => {
            let button = match ev.detail {
                1 => MouseButton::Left,
                2 => MouseButton::Middle,
                3 => MouseButton::Right,
                // 4 to 7 are wheel notches.
                4..=7 => return None,
                8 => MouseButton::Back,
                9 => MouseButton::Forward,
                n => MouseButton::Other(n as u16),
            };
            Some(Observed::Button { button, down: kind == xproto::BUTTON_PRESS_EVENT, x, y })
        }
        xproto::MOTION_NOTIFY_EVENT if watch.motion => Some(Observed::Motion { x, y }),
        _ => None,
    }
}

struct Keymap {
    min: u8,
    per: usize,
    syms: Vec<u32>,
}

impl Keymap {
    fn sym(&self, code: u8, level: usize) -> u32 {
        if level >= self.per {
            return 0;
        }
        self.syms.get((code - self.min) as usize * self.per + level).copied().unwrap_or(0)
    }

    fn translate(&self, code: u8, state: u16) -> Option<Observed> {
        if code < self.min {
            return None;
        }
        let (shift, lock, ctrl, alt, num, level3) =
            (state & 0x1 != 0, state & 0x2 != 0, state & 0x4 != 0, state & 0x8 != 0, state & 0x10 != 0, state & 0x80 != 0);
        let base = self.sym(code, 0);
        if modifier(base) {
            return None;
        }
        if ctrl || alt {
            return Some(Observed::Break);
        }
        let sym = if (0xff80..=0xffbd).contains(&base) {
            // Keypad: Num Lock (or Shift) picks the digit level.
            let digit = self.sym(code, 1);
            if (num ^ shift) && to_char(digit).is_some() { digit } else { base }
        } else {
            // Mod5 is usually the AltGr level, kept in the third group of the core mapping.
            let group = if level3 && self.per > 4 { 4 } else { 0 };
            let lower = self.sym(code, group);
            let upper = Some(self.sym(code, group + 1)).filter(|s| *s != 0).unwrap_or(lower);
            let letter = to_char(upper).is_some_and(char::is_uppercase) && to_char(lower).is_some_and(char::is_lowercase);
            if shift ^ (lock && letter) { upper } else { lower }
        };
        match sym {
            0xff08 => Some(Observed::Backspace),
            0xff09 | 0xff0d | 0xff1b | 0xff50..=0xff58 | 0xff8d | 0xff95..=0xff9f | 0xffbe..=0xffe0 | 0xffff => Some(Observed::Break),
            _ => to_char(sym).map(Observed::Text),
        }
    }
}

/// Shift, Ctrl, Alt, Super, the locks, the level shifts and dead keys.
fn modifier(sym: u32) -> bool {
    matches!(sym, 0xffe1..=0xffee | 0xfe01..=0xfe13 | 0xfe50..=0xfe8f | 0xff7e | 0xff7f | 0xff14)
}

fn to_char(sym: u32) -> Option<char> {
    match sym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
        0xff80 => Some(' '),
        // KP_Multiply to KP_9 sit 0xff80 above their ASCII characters.
        0xffaa..=0xffb9 => char::from_u32(sym - 0xff80),
        0xffbd => Some('='),
        0x0100_0100..=0x0110_ffff => char::from_u32(sym - 0x0100_0000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Keymap {
        // Keycode 10: a/A, 11: BackSpace, 12: Return, 13: KP_Home/KP_7, 14: Shift_L.
        let syms = vec![0x61, 0x41, 0xff08, 0, 0xff0d, 0, 0xff95, 0xffb7, 0xffe1, 0];
        Keymap { min: 10, per: 2, syms }
    }

    #[test]
    fn keys_become_text() {
        let m = map();
        assert_eq!(m.translate(10, 0), Some(Observed::Text('a')));
        assert_eq!(m.translate(10, 0x1), Some(Observed::Text('A')));
        assert_eq!(m.translate(10, 0x2), Some(Observed::Text('A')));
        assert_eq!(m.translate(10, 0x3), Some(Observed::Text('a')));
        assert_eq!(m.translate(10, 0x4), Some(Observed::Break));
        assert_eq!(m.translate(11, 0), Some(Observed::Backspace));
        assert_eq!(m.translate(12, 0), Some(Observed::Break));
        assert_eq!(m.translate(13, 0x10), Some(Observed::Text('7')));
        assert_eq!(m.translate(13, 0), Some(Observed::Break));
        assert_eq!(m.translate(14, 0), None);
    }
}
