//! Music app blocker: after a media key press with no player running, a launch of a listed app
//! inside the detection window is ended. If the key listener cannot start, nothing is blocked.

use std::{
    collections::HashSet,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use notifi_platform::{InputEvent, KeyCode, Platform};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};

use crate::settings::Store;

/// evdev codes: next, play/pause, previous, stop, play, pause.
const MEDIA_KEYS: [u16; 6] = [163, 164, 165, 166, 200, 201];
const SCAN_EVERY: Duration = Duration::from_millis(150);

pub fn start(app: &AppHandle) {
    let app = app.clone();
    let platform = app.state::<Platform>().inner().clone();
    let _ = std::thread::Builder::new().name("notifi-media-key-blocker".into()).spawn(move || run(app, platform));
}

struct Config {
    names: Vec<String>,
    window: Duration,
}

fn config(app: &AppHandle) -> Option<Config> {
    let settings = app.state::<Store>().get();
    if !settings.enabled("mediaKeyBlocker") {
        return None;
    }
    let names: Vec<String> = settings
        .option("mediaKeyBlocker", "blockedApps")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| normalise(s)).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let ms = settings.option("mediaKeyBlocker", "detectionWindow").and_then(|v| v.as_f64()).unwrap_or(1500.0);
    (!names.is_empty()).then(|| Config { names, window: Duration::from_millis(ms.clamp(200.0, 5000.0) as u64) })
}

fn normalise(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    lower.strip_suffix(".exe").map(str::to_string).unwrap_or(lower)
}

pub fn matches_blocked(process: &str, blocked: &[String]) -> bool {
    let process = normalise(process);
    blocked.iter().any(|b| process == *b || process.contains(b.as_str()))
}

fn run(app: AppHandle, platform: Platform) {
    let mut stream: Option<Receiver<InputEvent>> = None;
    let mut system = System::new();
    let mut armed: Option<(Instant, HashSet<Pid>)> = None;
    loop {
        let Some(config) = config(&app) else {
            stream = None;
            armed = None;
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        let Some(rx) = stream.as_ref() else {
            match platform.input().hook() {
                Ok(rx) => stream = Some(rx),
                Err(_) => std::thread::sleep(Duration::from_secs(5)),
            }
            continue;
        };
        match rx.recv_timeout(SCAN_EVERY) {
            Ok(InputEvent::Key { key: KeyCode(code), pressed: true }) if MEDIA_KEYS.contains(&code) => {
                let playing = platform.media().sessions().is_ok_and(|s| !s.is_empty());
                if !playing {
                    system.refresh_processes(ProcessesToUpdate::All, true);
                    armed = Some((Instant::now() + config.window, system.processes().keys().copied().collect()));
                }
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => stream = None,
        }
        if let Some((until, known)) = armed.as_mut() {
            if Instant::now() > *until {
                armed = None;
                continue;
            }
            system.refresh_processes(ProcessesToUpdate::All, true);
            for (pid, process) in system.processes() {
                if known.insert(*pid) && matches_blocked(&process.name().to_string_lossy(), &config.names) {
                    process.kill();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_names_loosely() {
        let blocked = vec![normalise("Spotify.exe"), normalise("Music")];
        assert!(matches_blocked("Spotify.exe", &blocked));
        assert!(matches_blocked("spotify", &blocked));
        assert!(!matches_blocked("firefox.exe", &blocked));
    }
}
