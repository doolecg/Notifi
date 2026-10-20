//! Bluetooth off during sleep. The radio is switched off when the computer goes to sleep and back
//! on at wake, but only when Notifi was the one that switched it off. The debt is saved to disk so
//! a restore still happens after a crash or restart during sleep.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

use super::{flag, saved, update_saved};

static REPORTED: AtomicBool = AtomicBool::new(false);

/// `Some(on)` for the Bluetooth radio, `None` when there is no adapter.
fn radio_on() -> Result<Option<bool>, String> {
    imp::state()
}

fn report(error: String) {
    if !REPORTED.swap(true, Ordering::SeqCst) {
        eprintln!("notifi: bluetooth sleep: {error}");
    }
}

/// Called as sleep begins. Must finish quickly: sleep waits for it.
pub fn before_sleep(app: &AppHandle) {
    let s = super::settings(app);
    if !s.enabled("bluetoothSleep") || !flag(&s, "bluetoothSleep", "disconnectHeadphones", true) || saved(app).bt_owed {
        return;
    }
    match radio_on() {
        Ok(Some(true)) => match imp::set(false) {
            Ok(()) => update_saved(app, |f| f.bt_owed = true),
            Err(e) => report(e),
        },
        Ok(_) => {}
        Err(e) => report(e),
    }
}

/// Called at wake and at startup: turns Bluetooth back on if Notifi turned it off.
pub fn after_wake(app: &AppHandle) {
    if !saved(app).bt_owed {
        return;
    }
    match radio_on() {
        // The user turned it on themselves, so there is nothing to restore.
        Ok(Some(false)) => {
            if let Err(e) = imp::set(true) {
                report(e);
                return;
            }
        }
        Err(e) => {
            report(e);
            return;
        }
        Ok(_) => {}
    }
    update_saved(app, |f| f.bt_owed = false);
}

#[cfg(windows)]
mod imp {
    use windows::{
        Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState},
        Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    };

    /// WinRT calls run on a thread of their own that is set up for them.
    fn on_mta<T: Send>(f: impl FnOnce() -> Result<T, String> + Send) -> Result<T, String> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let initialised = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                    let out = f();
                    if initialised {
                        unsafe { CoUninitialize() };
                    }
                    out
                })
                .join()
                .unwrap_or_else(|_| Err("Bluetooth call panicked".into()))
        })
    }

    fn adapter() -> Result<Option<Radio>, String> {
        let radios = Radio::GetRadiosAsync().and_then(|op| op.join()).map_err(|e| e.message())?;
        for radio in radios {
            if radio.Kind().is_ok_and(|k| k == RadioKind::Bluetooth) {
                return Ok(Some(radio));
            }
        }
        Ok(None)
    }

    pub fn state() -> Result<Option<bool>, String> {
        on_mta(|| Ok(adapter()?.map(|r| r.State().is_ok_and(|s| s == RadioState::On))))
    }

    pub fn set(on: bool) -> Result<(), String> {
        on_mta(|| {
            let Some(radio) = adapter()? else { return Ok(()) };
            let target = if on { RadioState::On } else { RadioState::Off };
            let status = radio.SetStateAsync(target).and_then(|op| op.join()).map_err(|e| e.message())?;
            if status == RadioAccessStatus::Allowed { Ok(()) } else { Err("Windows did not allow changing Bluetooth".into()) }
        })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn state() -> Result<Option<bool>, String> {
        Ok(None)
    }

    pub fn set(_on: bool) -> Result<(), String> {
        Err("Bluetooth control is only built for Windows".into())
    }
}
