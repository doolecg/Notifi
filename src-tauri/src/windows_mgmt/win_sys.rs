//! Windows system settings Notifi borrows while a feature is on and hands back when it is switched
//! off or Notifi quits: active window tracking (focus follows mouse) and Aero Snap.

use std::ffi::c_void;
use std::sync::Mutex;

use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPI_GETACTIVEWINDOWTRACKING, SPI_GETACTIVEWNDTRKTIMEOUT, SPI_GETACTIVEWNDTRKZORDER,
    SPI_GETWINARRANGING, SPI_SETACTIVEWINDOWTRACKING, SPI_SETACTIVEWNDTRKTIMEOUT, SPI_SETACTIVEWNDTRKZORDER, SPI_SETWINARRANGING,
    SYSTEM_PARAMETERS_INFO_ACTION,
};

#[derive(Clone, Copy)]
struct Tracking {
    on: u32,
    timeout: u32,
    raise: u32,
}

static TRACKING_BEFORE: Mutex<Option<Tracking>> = Mutex::new(None);
static SNAP_BEFORE: Mutex<Option<u32>> = Mutex::new(None);

fn get(action: SYSTEM_PARAMETERS_INFO_ACTION) -> Option<u32> {
    let mut v = 0u32;
    unsafe { SystemParametersInfoW(action, 0, Some(&mut v as *mut u32 as *mut c_void), Default::default()) }.ok().map(|_| v)
}

/// The value goes in the pointer argument for these settings.
fn put(action: SYSTEM_PARAMETERS_INFO_ACTION, value: u32) {
    unsafe {
        let _ = SystemParametersInfoW(action, 0, Some(value as usize as *mut c_void), SPIF_SENDCHANGE);
    }
}

pub fn set_focus_follows_mouse(on: bool, delay_ms: u32, raise: bool) {
    let mut before = TRACKING_BEFORE.lock().unwrap_or_else(|e| e.into_inner());
    if on {
        if before.is_none() {
            *before = Some(Tracking {
                on: get(SPI_GETACTIVEWINDOWTRACKING).unwrap_or(0),
                timeout: get(SPI_GETACTIVEWNDTRKTIMEOUT).unwrap_or(500),
                raise: get(SPI_GETACTIVEWNDTRKZORDER).unwrap_or(0),
            });
        }
        put(SPI_SETACTIVEWNDTRKTIMEOUT, delay_ms);
        put(SPI_SETACTIVEWNDTRKZORDER, u32::from(raise));
        put(SPI_SETACTIVEWINDOWTRACKING, 1);
    } else if let Some(saved) = before.take() {
        put(SPI_SETACTIVEWINDOWTRACKING, saved.on);
        put(SPI_SETACTIVEWNDTRKTIMEOUT, saved.timeout);
        put(SPI_SETACTIVEWNDTRKZORDER, saved.raise);
    }
}

/// Aero Snap would also place a window dragged to an edge, fighting Notifi's own snapping.
pub fn set_native_snap_suspended(suspended: bool) {
    let mut before = SNAP_BEFORE.lock().unwrap_or_else(|e| e.into_inner());
    if suspended {
        if before.is_none() {
            *before = get(SPI_GETWINARRANGING);
        }
        if before.is_some_and(|v| v != 0) {
            put(SPI_SETWINARRANGING, 0);
        }
    } else if let Some(saved) = before.take() {
        put(SPI_SETWINARRANGING, saved);
    }
}

pub fn restore_all() {
    set_focus_follows_mouse(false, 0, false);
    set_native_snap_suspended(false);
}
