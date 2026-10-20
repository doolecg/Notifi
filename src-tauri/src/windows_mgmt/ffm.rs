//! Focus follows mouse. Windows has it built in (active window tracking with a delay and an option
//! not to raise), so Notifi switches that on and restores the user's setting afterwards. Elsewhere
//! Notifi watches the pointer itself and focuses the window it rests on.

use super::config::Config;

#[cfg(windows)]
pub fn apply(_app: &tauri::AppHandle, cfg: &Config) {
    super::win_sys::set_focus_follows_mouse(cfg.focus_follows, cfg.focus_delay_ms, cfg.focus_raise);
}

#[cfg(not(windows))]
pub fn apply(app: &tauri::AppHandle, cfg: &Config) {
    use std::sync::atomic::{AtomicU64, Ordering};

    static RUN: AtomicU64 = AtomicU64::new(0);
    let generation = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    if !cfg.focus_follows {
        return;
    }
    let wm = app.state::<notifi_platform::Platform>().inner().clone();
    let (delay, own) = (u64::from(cfg.focus_delay_ms), std::process::id());
    std::thread::spawn(move || {
        use std::time::{Duration, Instant};
        let mut resting: Option<((i32, i32), Instant)> = None;
        let mut handled = false;
        while RUN.load(Ordering::SeqCst) == generation {
            std::thread::sleep(Duration::from_millis(40));
            let Ok(p) = wm.windows().pointer() else { continue };
            if p.left || p.right {
                resting = None;
                continue;
            }
            let at = (p.x, p.y);
            match resting {
                Some((was, _)) if was == at => {}
                _ => {
                    resting = Some((at, Instant::now()));
                    handled = false;
                    continue;
                }
            }
            if handled || resting.is_some_and(|(_, since)| since.elapsed() < Duration::from_millis(delay)) {
                continue;
            }
            handled = true;
            if let Ok(Some(win)) = wm.windows().window_at(p.x, p.y) {
                if !win.focused && win.pid != Some(own) {
                    let _ = wm.windows().focus(win.id);
                }
            }
        }
    });
}

#[cfg(not(windows))]
use tauri::Manager;
