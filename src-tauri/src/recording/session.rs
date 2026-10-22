//! One recording on Windows: countdown, capture loop, sound, encoder, and the saved file.

use std::{
    ffi::c_void,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::TrySendError,
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};
use windows::Win32::{
    System::{
        Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
        SystemInformation::GetLocalTime,
    },
    UI::Shell::{SHGetKnownFolderPath, FOLDERID_Videos, KF_FLAG_DEFAULT},
};

use super::{
    audio,
    capture::{Capturer, Compositor},
    emit,
    encode::{Config, Encoder, Msg},
    phase_name,
    pure::*,
    save_dir, set_phase, Opts, Outcome, Phase, Session,
};

pub fn videos_dir() -> PathBuf {
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Videos, KF_FLAG_DEFAULT, None) {
            let text = p.to_string();
            CoTaskMemFree(Some(p.0 as *const c_void));
            if let Ok(text) = text {
                return PathBuf::from(text);
            }
        }
    }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Videos")).unwrap_or_default()
}

fn local_now() -> ((u16, u16, u16), (u16, u16, u16)) {
    let t = unsafe { GetLocalTime() };
    ((t.wYear, t.wMonth, t.wDay), (t.wHour, t.wMinute, t.wSecond))
}

/// The id of the capture device whose name matches the setting, or `None` for the default.
fn mic_id(app: &AppHandle, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let platform = app.state::<notifi_platform::Platform>();
    let devices = platform.audio().devices(notifi_platform::DeviceDirection::Input).ok()?;
    let lower = name.to_lowercase();
    devices
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
        .or_else(|| devices.iter().find(|d| d.name.to_lowercase().contains(&lower)))
        .map(|d| d.id.clone())
}

struct Com;

impl Com {
    fn init() -> Self {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        Com
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Waits `seconds` one second at a time, telling the bar the count. `false` when stopped meanwhile.
fn count_down(app: &AppHandle, session: &Session, seconds: u32) -> bool {
    for left in (1..=seconds).rev() {
        emit(app, "countdown", 0, left, None, "");
        for _ in 0..10 {
            if session.stop.load(Ordering::SeqCst) {
                return false;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    !session.stop.load(Ordering::SeqCst)
}

pub fn record(app: &AppHandle, region: Region, o: &Opts, session: &Arc<Session>) -> Outcome {
    let _com = Com::init();
    if !count_down(app, session, o.countdown) {
        return Outcome::Cancelled;
    }

    let mut cap = match Capturer::new(region) {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(e),
    };
    let region = cap.region;
    let (w, h) = (region.w as u32, region.h as u32);
    let mut comp = match Compositor::new(region, o.effects, o.typing_zoom) {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(e),
    };

    let gif = o.format == "gif";
    if o.format == "webm" {
        crate::island::notice_to(app, "WebM is not available yet, so this recording is saved as MP4".into(), Some("module:captures"));
    }
    let streams = (!gif && (o.system_audio || o.mic)).then(|| {
        let id = o.mic.then(|| mic_id(app, &o.mic_device)).flatten();
        audio::open(o.system_audio, o.mic, id.as_deref())
    });
    if let Some(warning) = streams.as_ref().and_then(|s| s.warning.as_deref()) {
        crate::island::notice_to(app, warning.to_string(), Some("module:captures"));
    }
    let audio_tracks = streams.as_ref().map_or(0, audio::Streams::track_count);

    let dir = save_dir(&o.folder);
    if let Err(e) = fs::create_dir_all(&dir) {
        return Outcome::Failed(format!("Cannot use the folder {}: {e}", dir.display()));
    }
    let (date, time) = local_now();
    let path = unique_path(&dir, &file_stem(&o.name, date, time), if gif { "gif" } else { "mp4" }, |p| p.exists());

    let enc = match Encoder::start(Config { path: path.clone(), w, h, fps: o.fps, bitrate: bitrate(&o.quality, w, h, o.fps), audio_tracks, gif }) {
        Ok(e) => e,
        Err(e) => {
            let _ = fs::remove_file(&path);
            return Outcome::Failed(e);
        }
    };

    *session.clock.lock().unwrap() = ActiveClock::new(Instant::now());
    set_phase(app, Phase::Recording);
    let audio_stop = Arc::new(AtomicBool::new(false));
    let audio_thread = streams.filter(audio::Streams::any).and_then(|s| audio::spawn(s, session.clone(), audio_stop.clone(), enc.tx.clone()));

    let tx = enc.tx.clone();
    let mut next_index = 0u64;
    let mut frames = 0u64;
    let mut failure: Option<String> = None;
    let mut last_tick = Instant::now();
    emit(app, "recording", 0, 0, None, "");
    while !session.stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        let (elapsed, paused) = {
            let clock = session.clock.lock().unwrap();
            (clock.elapsed(now), clock.is_paused())
        };
        if now.duration_since(last_tick) >= Duration::from_millis(250) {
            last_tick = now;
            emit(app, phase_name(app), elapsed.as_millis() as u64, 0, None, "");
        }
        if paused {
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        let wait = frame_time(next_index, o.fps).saturating_sub(elapsed);
        if let Err(e) = cap.acquire(wait.as_millis().clamp(1, 50) as u32) {
            failure = Some(e);
            break;
        }
        let elapsed = session.clock.lock().unwrap().elapsed(Instant::now());
        let index = frame_index(elapsed, o.fps);
        if index < next_index {
            continue;
        }
        if let Err(e) = cap.read() {
            failure = Some(e);
            break;
        }
        let bgra = comp.compose(&cap.base, elapsed.as_millis() as u64).to_vec();
        match tx.try_send(Msg::Video { bgra, ts: frame_ts(index, o.fps) }) {
            Ok(()) => frames += 1,
            // The encoder is behind: drop this frame instead of stalling the capture.
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                failure = Some("The encoder stopped unexpectedly".into());
                break;
            }
        }
        next_index = index + 1;
    }

    set_phase(app, Phase::Saving);
    emit(app, "saving", 0, 0, None, "");
    audio_stop.store(true, Ordering::SeqCst);
    if let Some(t) = audio_thread {
        let _ = t.join();
    }
    drop(tx);
    let written = enc.finish();
    drop(comp);
    drop(cap);

    match (failure, written) {
        (Some(e), _) | (None, Err(e)) => {
            let _ = fs::remove_file(&path);
            Outcome::Failed(e)
        }
        (None, Ok(())) if frames == 0 => {
            let _ = fs::remove_file(&path);
            Outcome::Failed("Nothing was recorded".into())
        }
        (None, Ok(())) => Outcome::Saved(path),
    }
}
