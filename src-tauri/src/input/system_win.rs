//! Windows settings the input features borrow: pointer acceleration (SPI_SETMOUSE) and the
//! touchpad settings page.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::sync::Mutex;

use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPIF_SENDCHANGE, SPI_GETMOUSE, SPI_SETMOUSE};

/// Threshold 1, threshold 2 and the acceleration level (0 is flat).
type MouseParams = [i32; 3];

static SAVED: Mutex<Option<MouseParams>> = Mutex::new(None);

fn get() -> Option<MouseParams> {
    let mut v = [0i32; 3];
    unsafe { SystemParametersInfoW(SPI_GETMOUSE, 0, Some(v.as_mut_ptr() as *mut c_void), Default::default()) }.ok().map(|_| v)
}

/// Not written to the user profile, so a crash costs at most the session.
fn set(mut v: MouseParams) {
    unsafe {
        let _ = SystemParametersInfoW(SPI_SETMOUSE, 0, Some(v.as_mut_ptr() as *mut c_void), SPIF_SENDCHANGE);
    }
}

fn parse(text: &str) -> Option<MouseParams> {
    let mut it = text.split_whitespace().map(|n| n.parse::<i32>());
    Some([it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?])
}

/// Puts back the setting a previous run saved and never restored (a crash).
pub fn recover(file: &Path) {
    if let Some(saved) = std::fs::read_to_string(file).ok().and_then(|t| parse(&t)) {
        set(saved);
    }
    let _ = std::fs::remove_file(file);
}

/// Turns acceleration off (`flat`) or puts the saved setting back. `file` keeps the saved setting
/// so the next start can recover it.
pub fn set_flat_pointer(flat: bool, file: &Path) {
    let mut saved = SAVED.lock().unwrap_or_else(|e| e.into_inner());
    if flat {
        let Some(current) = get() else { return };
        if saved.is_none() {
            if current[2] == 0 {
                return;
            }
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(file, format!("{} {} {}", current[0], current[1], current[2]));
            *saved = Some(current);
        }
        set([current[0], current[1], 0]);
    } else if let Some(before) = saved.take() {
        set(before);
        let _ = std::fs::remove_file(file);
    }
}

/// The page where a precision touchpad's three finger tap is set.
pub fn open_touchpad_settings() -> Result<(), String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:devices-touchpad")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_saved_setting() {
        assert_eq!(parse("6 10 1"), Some([6, 10, 1]));
        assert_eq!(parse("6 10"), None);
        assert_eq!(parse("a b c"), None);
    }
}
