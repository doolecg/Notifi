//! After-recording editor window and its commands. An edit (cuts, speed, zoom, blur, arrows, text,
//! sound levels) is saved as a new file next to the original; the original is never changed. GIFs
//! are re-written frame by frame and can only be trimmed; MP4 files are decoded, edited and
//! re-encoded by `export.rs`, so the copy goes through the same H.264 and AAC encoders as a recording.

use std::{
    borrow::Cow,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::{
    http::{header, Request, Response, StatusCode},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use super::{
    edit::{sound_tracks, EditSpec},
    pure::unique_path,
    RecordingState,
};

pub const TRIM: &str = "recordtrim";
/// A trimmed copy must keep at least this much.
const MIN_KEEP_MS: u64 = 100;

// ---- Pure rules ----

/// The start and end (milliseconds) to keep, put in order and inside `0..=duration`. None when less
/// than `MIN_KEEP_MS` would be left.
pub fn clamp_range(start_ms: f64, end_ms: f64, duration_ms: u64) -> Option<(u64, u64)> {
    if !start_ms.is_finite() || !end_ms.is_finite() {
        return None;
    }
    let max = duration_ms as f64;
    let (a, b) = (start_ms.min(end_ms).clamp(0.0, max).round() as u64, start_ms.max(end_ms).clamp(0.0, max).round() as u64);
    (b - a >= MIN_KEEP_MS).then_some((a, b))
}

/// How long (in hundredths of a second) a GIF frame shown from `at_ms` for `delay_cs` is inside
/// `start_ms..end_ms`. None when the frame is outside.
pub fn frame_overlap_cs(at_ms: u64, delay_cs: u16, start_ms: u64, end_ms: u64) -> Option<u16> {
    let frame_end = at_ms + u64::from(delay_cs) * 10;
    let (from, to) = (at_ms.max(start_ms), frame_end.min(end_ms));
    (to > from).then(|| (((to - from) as f64 / 10.0).round() as u16).max(1))
}

/// `Recording (label).mp4`, or `Recording (label 2).mp4` and so on. Never an existing file.
fn variant_path(src: &Path, label: &str) -> PathBuf {
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Recording".into());
    let ext = src.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mp4".into());
    let first = dir.join(format!("{stem} ({label}).{ext}"));
    if !first.exists() {
        return first;
    }
    unique_path(dir, &format!("{stem} ({label})"), &ext, |p| p.exists())
}

pub fn trimmed_path(src: &Path) -> PathBuf {
    variant_path(src, "trimmed")
}

pub fn edited_path(src: &Path) -> PathBuf {
    variant_path(src, "edited")
}

fn is_gif(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))
}

/// The byte range a media request asks for ("bytes=a-b", "bytes=a-" or "bytes=-n") inside a file of
/// `len` bytes. None for a missing, broken or empty range.
pub fn byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (from, to) = spec.split(',').next()?.split_once('-')?;
    let (a, b) = match (from.trim(), to.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (len.saturating_sub(n), len.checked_sub(1)?)
        }
        (a, "") => (a.parse().ok()?, len.checked_sub(1)?),
        (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (a <= b && b < len).then_some((a, b))
}

// ---- Commands ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimTarget {
    path: String,
    name: String,
    /// `gif` or `mp4`.
    kind: &'static str,
    /// Known up front for a GIF; for MP4 the page reads it from the video.
    duration_ms: Option<u64>,
    /// Sound tracks in an MP4 (system sound and microphone are kept apart); 0 for a GIF.
    audio_tracks: usize,
}

fn target(app: &AppHandle) -> Result<PathBuf, String> {
    app.state::<RecordingState>().inner.lock().unwrap().trim.clone().ok_or_else(|| "There is no recording to trim.".to_string())
}

fn gif_duration_ms(path: &Path) -> Result<u64, String> {
    let file = File::open(path).map_err(|e| format!("Could not open the GIF: {e}"))?;
    let mut dec = gif::DecodeOptions::new().read_info(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    while let Some(frame) = dec.next_frame_info().map_err(|e| e.to_string())? {
        total += u64::from(frame.delay) * 10;
    }
    Ok(total)
}

#[tauri::command]
pub fn recording_trim_target(app: AppHandle) -> Result<TrimTarget, String> {
    let path = target(&app)?;
    if !path.is_file() {
        return Err("That recording is gone.".into());
    }
    let gif = is_gif(&path);
    Ok(TrimTarget {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        kind: if gif { "gif" } else { "mp4" },
        duration_ms: if gif { Some(gif_duration_ms(&path)?) } else { None },
        audio_tracks: if gif { 0 } else { sound_tracks(&path) },
        path: path.display().to_string(),
    })
}

/// Saves the part between `start_ms` and `end_ms` as a new file and returns its path.
#[tauri::command]
pub async fn recording_trim_save(app: AppHandle, start_ms: f64, end_ms: f64, duration_ms: f64) -> Result<String, String> {
    let src = target(&app)?;
    let (start, end) = clamp_range(start_ms, end_ms, duration_ms.max(0.0) as u64).ok_or("Pick a part that is at least a tenth of a second long.")?;
    if !is_gif(&src) {
        return save_edit(&app, src, EditSpec::trim(duration_ms, start as f64, end as f64), "Trimmed").await;
    }
    let dst = trimmed_path(&src);
    let (from, to) = (src.clone(), dst.clone());
    let done = tauri::async_runtime::spawn_blocking(move || trim_gif(&from, &to, start, end)).await.map_err(|e| e.to_string())?;
    finish_copy(&app, &dst, done, "Trimmed")
}

/// Saves the edited video (cuts, speed, effects, sound levels) as a new file and returns its path.
#[tauri::command]
pub async fn recording_edit_save(app: AppHandle, spec: EditSpec) -> Result<String, String> {
    let src = target(&app)?;
    if is_gif(&src) {
        return Err("A GIF can only be trimmed.".into());
    }
    save_edit(&app, src, spec, "Edited").await
}

async fn save_edit(app: &AppHandle, src: PathBuf, spec: EditSpec, what: &'static str) -> Result<String, String> {
    let spec = spec.check()?;
    let dst = edited_path(&src);
    let (from, to) = (src, dst.clone());
    let done = tauri::async_runtime::spawn_blocking(move || render_mp4(&from, &to, &spec)).await.map_err(|e| e.to_string())?;
    finish_copy(app, &dst, done, what)
}

/// Keeps a finished copy and tells the user, or removes what a failed save left behind.
fn finish_copy(app: &AppHandle, dst: &Path, done: Result<(), String>, what: &str) -> Result<String, String> {
    if let Err(e) = done {
        let _ = fs::remove_file(dst);
        return Err(e);
    }
    crate::capture::recent::remember(app, dst, "video");
    crate::island::notice_to(app, format!("{what} copy saved \u{b7} {}", dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), Some("module:captures"));
    Ok(dst.display().to_string())
}

#[tauri::command]
pub fn recording_trim_reveal(path: String) {
    crate::apps_clean::apps_clean_reveal(path);
}

#[tauri::command]
pub fn recording_trim_close(window: WebviewWindow) {
    if window.label() == TRIM {
        let _ = window.close();
    }
}

/// Opens the trimmer for a recording, or points the open one at it.
pub fn open(app: &AppHandle, path: &Path) {
    app.state::<RecordingState>().inner.lock().unwrap().trim = Some(path.to_path_buf());
    if let Some(window) = app.get_webview_window(TRIM) {
        let _ = app.emit_to(TRIM, "recording://trim", ());
        let _ = window.unminimize();
        let _ = window.show();
        if !crate::window::no_focus() {
            let _ = window.set_focus();
        }
        return;
    }
    // Building a webview from a command can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, TRIM, WebviewUrl::App("recording.html#trim".into()))
            .title("Edit recording")
            .inner_size(940.0, 720.0)
            .min_inner_size(760.0, 600.0)
            .center()
            .focused(!crate::window::no_focus())
            .build();
    });
}

/// Serves the recording being trimmed to the page's video element, with range support. It serves
/// only that one file, whatever the address says.
pub fn media(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let reply = |status: StatusCode| -> Response<Cow<'static, [u8]>> {
        Response::builder().status(status).header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*").body(Cow::Borrowed(&[][..])).unwrap()
    };
    let Ok(path) = target(app) else { return reply(StatusCode::NOT_FOUND) };
    if is_gif(&path) {
        return reply(StatusCode::NOT_FOUND);
    }
    let Ok(mut file) = File::open(&path) else { return reply(StatusCode::NOT_FOUND) };
    let Ok(len) = file.metadata().map(|m| m.len()) else { return reply(StatusCode::NOT_FOUND) };
    let ranged = request.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).map(|h| byte_range(h, len));
    let (from, to, status) = match ranged {
        Some(Some((a, b))) => (a, b, StatusCode::PARTIAL_CONTENT),
        Some(None) => return reply(StatusCode::RANGE_NOT_SATISFIABLE),
        None if len == 0 => return reply(StatusCode::NOT_FOUND),
        None => (0, len - 1, StatusCode::OK),
    };
    let mut body = vec![0u8; (to - from + 1) as usize];
    if file.seek(SeekFrom::Start(from)).and_then(|_| file.read_exact(&mut body)).is_err() {
        return reply(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_LENGTH, body.len());
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {from}-{to}/{len}"));
    }
    builder.body(Cow::Owned(body)).unwrap()
}

// ---- GIF ----

fn trim_gif(src: &Path, dst: &Path, start_ms: u64, end_ms: u64) -> Result<(), String> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(BufReader::new(File::open(src).map_err(|e| format!("Could not open the GIF: {e}"))?)).map_err(|e| e.to_string())?;
    let (w, h) = (dec.width(), dec.height());
    let out = File::create(dst).map_err(|e| format!("Could not create {}: {e}", dst.display()))?;
    let mut enc = gif::Encoder::new(BufWriter::new(out), w, h, &[]).map_err(|e| e.to_string())?;
    enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
    // Frames can cover only part of the picture, so keep the picture built so far.
    let mut canvas = vec![0u8; w as usize * h as usize * 4];
    let (mut at, mut kept) = (0u64, 0u32);
    while let Some(frame) = dec.read_next_frame().map_err(|e| e.to_string())? {
        let (fw, fh, left, top) = (usize::from(frame.width), usize::from(frame.height), usize::from(frame.left), usize::from(frame.top));
        for row in 0..fh.min((h as usize).saturating_sub(top)) {
            for col in 0..fw.min((w as usize).saturating_sub(left)) {
                let from = (row * fw + col) * 4;
                if frame.buffer[from + 3] != 0 {
                    let to = ((top + row) * w as usize + left + col) * 4;
                    canvas[to..to + 4].copy_from_slice(&frame.buffer[from..from + 4]);
                }
            }
        }
        if let Some(delay) = frame_overlap_cs(at, frame.delay, start_ms, end_ms) {
            let mut rgba = canvas.clone();
            let mut keep = gif::Frame::from_rgba_speed(w, h, &mut rgba, 10);
            keep.delay = delay;
            enc.write_frame(&keep).map_err(|e| e.to_string())?;
            kept += 1;
        }
        at += u64::from(frame.delay) * 10;
        if at >= end_ms {
            break;
        }
    }
    if kept == 0 {
        return Err("No frames are left in that part.".into());
    }
    Ok(())
}

// ---- MP4 ----

#[cfg(windows)]
use super::export::render_mp4;

#[cfg(not(windows))]
fn render_mp4(_src: &Path, _dst: &Path, _spec: &EditSpec) -> Result<(), String> {
    Err("Editing a video is only built for Windows so far.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_is_ordered_and_inside_the_clip() {
        assert_eq!(clamp_range(1000.0, 4000.0, 10_000), Some((1000, 4000)));
        assert_eq!(clamp_range(4000.0, 1000.0, 10_000), Some((1000, 4000)));
        assert_eq!(clamp_range(-500.0, 99_999.0, 10_000), Some((0, 10_000)));
        assert_eq!(clamp_range(2000.0, 2050.0, 10_000), None);
        assert_eq!(clamp_range(12_000.0, 15_000.0, 10_000), None);
        assert_eq!(clamp_range(f64::NAN, 1000.0, 10_000), None);
    }

    #[test]
    fn gif_frames_keep_only_the_time_inside_the_range() {
        // Frames of 100 ms starting at 0, 100, 200 ...
        assert_eq!(frame_overlap_cs(0, 10, 250, 600), None);
        assert_eq!(frame_overlap_cs(200, 10, 250, 600), Some(5));
        assert_eq!(frame_overlap_cs(300, 10, 250, 600), Some(10));
        assert_eq!(frame_overlap_cs(550, 10, 250, 600), Some(5));
        assert_eq!(frame_overlap_cs(600, 10, 250, 600), None);
        assert_eq!(frame_overlap_cs(595, 1, 590, 600), Some(1));
    }

    #[test]
    fn trimmed_copy_never_reuses_a_name() {
        let src = Path::new("Z:/no-such-folder/Recording 1.mp4");
        assert_eq!(trimmed_path(src), Path::new("Z:/no-such-folder").join("Recording 1 (trimmed).mp4"));
    }

    #[test]
    fn byte_ranges_follow_http() {
        assert_eq!(byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(byte_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=500-5000", 1000), Some((500, 999)));
        assert_eq!(byte_range("bytes=1000-", 1000), None);
        assert_eq!(byte_range("bytes=50-10", 1000), None);
        assert_eq!(byte_range("nope", 1000), None);
        assert_eq!(byte_range("bytes=0-", 0), None);
    }
}
