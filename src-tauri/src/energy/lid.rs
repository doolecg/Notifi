//! Keep awake with the lid closed: while a session runs, closing the lid does nothing, and the
//! built-in screen can be dimmed. The normal lid action comes back when the session ends.

use notifi_platform::{Guard, Platform};
use tauri::{AppHandle, Manager};

use super::{block, flag, saved, settings, update_saved, EnergyState};

#[derive(Default)]
pub struct Lid {
    closed: bool,
    /// Held where the lid is blocked through the platform layer instead of the power plan.
    #[cfg_attr(windows, allow(dead_code))]
    guard: Option<Guard>,
    /// The built-in display and the brightness to give back.
    dimmed: Option<(String, u8)>,
}

/// Matches the lid behaviour to the settings and to whether a Keep awake session is running.
pub fn sync(app: &AppHandle) {
    let s = settings(app);
    let wanted = s.enabled("lidClosedAwake") && crate::keepawake::is_active(app);
    let state = app.state::<EnergyState>();
    let mut lid = state.lid.lock().unwrap();

    let holding = imp::holding(app, &lid);
    if wanted && !holding {
        if let Err(e) = imp::engage(app, &mut lid) {
            eprintln!("notifi: lid closed awake: {e}");
        }
    } else if !wanted && holding {
        imp::release(app, &mut lid);
    }

    let dim = wanted && lid.closed && flag(&s, "lidClosedAwake", "dimBuiltIn", false);
    match (dim, lid.dimmed.is_some()) {
        (true, false) => lid.dimmed = dim_internal(app),
        (false, true) => {
            if let Some((id, level)) = lid.dimmed.take() {
                let platform = app.state::<Platform>().inner().clone();
                let _ = block(platform.display().set_brightness(&id, level));
            }
        }
        _ => {}
    }
}

pub fn lid_changed(app: &AppHandle, closed: bool) {
    app.state::<EnergyState>().lid.lock().unwrap().closed = closed;
    sync(app);
}

/// Dims the laptop panel and returns what is needed to undo it.
fn dim_internal(app: &AppHandle) -> Option<(String, u8)> {
    let platform = app.state::<Platform>().inner().clone();
    let list = block(platform.display().list()).ok()?;
    let panel = list.into_iter().find(|d| d.internal)?;
    let before = block(platform.display().brightness(&panel.id)).ok()?;
    block(platform.display().set_brightness(&panel.id, 0)).ok()?;
    Some((panel.id, before))
}

/// Puts the lid action back if the app died while holding it.
pub fn recover(app: &AppHandle) {
    if saved(app).lid_prev.is_some() && !crate::keepawake::is_active(app) {
        let state = app.state::<EnergyState>();
        let mut lid = state.lid.lock().unwrap();
        imp::release(app, &mut lid);
    }
}

/// Puts everything back when the app quits.
pub fn on_exit(app: &AppHandle) {
    let state = app.state::<EnergyState>();
    let mut lid = state.lid.lock().unwrap();
    if imp::holding(app, &lid) {
        imp::release(app, &mut lid);
    }
    if let Some((id, level)) = lid.dimmed.take() {
        let platform = app.state::<Platform>().inner().clone();
        let _ = block(platform.display().set_brightness(&id, level));
    }
}

#[cfg(windows)]
mod imp {
    use windows::{
        core::GUID,
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            System::{
                Power::{
                    PowerGetActiveScheme, PowerReadACValueIndex, PowerReadDCValueIndex, PowerSetActiveScheme,
                    PowerWriteACValueIndex, PowerWriteDCValueIndex,
                },
                SystemServices::{GUID_LIDCLOSE_ACTION, GUID_SYSTEM_BUTTON_SUBGROUP},
            },
        },
    };

    use super::*;

    /// Lid action value for "do nothing".
    const DO_NOTHING: u32 = 0;

    pub fn holding(app: &AppHandle, _lid: &Lid) -> bool {
        saved(app).lid_prev.is_some()
    }

    unsafe fn active_scheme() -> Result<GUID, String> {
        let mut scheme: *mut GUID = std::ptr::null_mut();
        let status = PowerGetActiveScheme(None, &mut scheme);
        if status.0 != 0 || scheme.is_null() {
            return Err(format!("PowerGetActiveScheme failed ({})", status.0));
        }
        let guid = *scheme;
        let _ = LocalFree(Some(HLOCAL(scheme as *mut _)));
        Ok(guid)
    }

    fn write(ac: u32, dc: u32) -> Result<(), String> {
        unsafe {
            let scheme = active_scheme()?;
            let a = PowerWriteACValueIndex(None, &scheme, Some(&GUID_SYSTEM_BUTTON_SUBGROUP as *const GUID), Some(&GUID_LIDCLOSE_ACTION as *const GUID), ac);
            let d = PowerWriteDCValueIndex(None, &scheme, Some(&GUID_SYSTEM_BUTTON_SUBGROUP as *const GUID), Some(&GUID_LIDCLOSE_ACTION as *const GUID), dc);
            if a.0 != 0 || d != 0 {
                return Err("Windows would not change the lid action".into());
            }
            let applied = PowerSetActiveScheme(None, Some(&scheme as *const GUID));
            if applied.0 != 0 {
                return Err("Windows would not apply the power plan".into());
            }
            Ok(())
        }
    }

    pub fn engage(app: &AppHandle, _lid: &mut Lid) -> Result<(), String> {
        let (mut ac, mut dc) = (0u32, 0u32);
        unsafe {
            let scheme = active_scheme()?;
            let a = PowerReadACValueIndex(None, Some(&scheme as *const GUID), Some(&GUID_SYSTEM_BUTTON_SUBGROUP as *const GUID), Some(&GUID_LIDCLOSE_ACTION as *const GUID), &mut ac);
            let d = PowerReadDCValueIndex(None, Some(&scheme as *const GUID), Some(&GUID_SYSTEM_BUTTON_SUBGROUP as *const GUID), Some(&GUID_LIDCLOSE_ACTION as *const GUID), &mut dc);
            if a.0 != 0 || d != 0 {
                return Err("Windows would not tell the current lid action".into());
            }
        }
        // Saved first, so a crash after the change still restores it.
        update_saved(app, |f| f.lid_prev = Some((ac, dc)));
        if let Err(e) = write(DO_NOTHING, DO_NOTHING) {
            update_saved(app, |f| f.lid_prev = None);
            return Err(e);
        }
        Ok(())
    }

    pub fn release(app: &AppHandle, _lid: &mut Lid) {
        if let Some((ac, dc)) = saved(app).lid_prev {
            if write(ac, dc).is_ok() {
                update_saved(app, |f| f.lid_prev = None);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn holding(_app: &AppHandle, lid: &Lid) -> bool {
        lid.guard.is_some()
    }

    pub fn engage(app: &AppHandle, lid: &mut Lid) -> Result<(), String> {
        let platform = app.state::<Platform>().inner().clone();
        lid.guard = Some(block(platform.power().inhibit_lid("Keep awake with the lid closed")).map_err(|e| e.to_string())?);
        Ok(())
    }

    pub fn release(_app: &AppHandle, lid: &mut Lid) {
        lid.guard = None;
    }
}
