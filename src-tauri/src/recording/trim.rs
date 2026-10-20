//! After-recording trimmer: keeps the part of a finished recording between a start and an end time
//! and saves it as a new file next to the original (the original is never changed). GIFs are
//! re-written frame by frame; MP4 files are decoded and re-encoded through Media Foundation, so the
//! trimmed copy goes through the same H.264 and AAC encoders as a recording.

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

use super::{pure::unique_path, RecordingState};

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

/// `Recording (trimmed).mp4`, or `Recording (trimmed 2).mp4` and so on. Never an existing file.
pub fn trimmed_path(src: &Path) -> PathBuf {
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Recording".into());
    let ext = src.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mp4".into());
    let first = dir.join(format!("{stem} (trimmed).{ext}"));
    if !first.exists() {
        return first;
    }
    unique_path(dir, &format!("{stem} (trimmed)"), &ext, |p| p.exists())
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
        path: path.display().to_string(),
    })
}

/// Saves the part between `start_ms` and `end_ms` as a new file and returns its path.
#[tauri::command]
pub async fn recording_trim_save(app: AppHandle, start_ms: f64, end_ms: f64, duration_ms: f64) -> Result<String, String> {
    let src = target(&app)?;
    let (start, end) = clamp_range(start_ms, end_ms, duration_ms.max(0.0) as u64).ok_or("Pick a part that is at least a tenth of a second long.")?;
    let dst = trimmed_path(&src);
    let (from, to) = (src.clone(), dst.clone());
    let done = tauri::async_runtime::spawn_blocking(move || if is_gif(&from) { trim_gif(&from, &to, start, end) } else { trim_mp4(&from, &to, start, end) })
        .await
        .map_err(|e| e.to_string())?;
    if let Err(e) = done {
        let _ = fs::remove_file(&dst);
        return Err(e);
    }
    crate::capture::recent::remember(&app, &dst, "video");
    crate::island::notice(&app, format!("Trimmed copy saved \u{b7} {}", dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
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
            .title("Trim recording")
            .inner_size(560.0, 460.0)
            .min_inner_size(420.0, 380.0)
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

#[cfg(not(windows))]
fn trim_mp4(_src: &Path, _dst: &Path, _start_ms: u64, _end_ms: u64) -> Result<(), String> {
    Err("Trimming a video is only built for Windows so far.".into())
}

#[cfg(windows)]
fn trim_mp4(src: &Path, dst: &Path, start_ms: u64, end_ms: u64) -> Result<(), String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let result = trim_mp4_inner(src, dst, start_ms, end_ms);
    unsafe { CoUninitialize() };
    result
}

#[cfg(windows)]
fn trim_mp4_inner(src: &Path, dst: &Path, start_ms: u64, end_ms: u64) -> Result<(), String> {
    use std::collections::HashMap;

    use windows::{
        core::HSTRING,
        Win32::Media::MediaFoundation::{
            IMFAttributes, IMFSample, IMFSourceReader, MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
            MFMediaType_Audio, MFMediaType_Video, MFShutdown, MFStartup, MFVideoFormat_NV12, MFSTARTUP_FULL, MF_MT_AUDIO_BITS_PER_SAMPLE,
            MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
            MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ANY_STREAM, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
            MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
        },
    };

    use super::{
        encode::{open_writer, Config},
        pure::{bitrate, SAMPLE_RATE},
    };

    let err = |what: &'static str| move |e: windows::core::Error| format!("{what}: {e}");
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(err("Media Foundation"))?;
    let out = (|| -> Result<(), String> {
        let mut attrs: Option<IMFAttributes> = None;
        unsafe { MFCreateAttributes(&mut attrs, 1) }.map_err(err("Media Foundation"))?;
        let attrs = attrs.ok_or("Media Foundation gave no settings object")?;
        unsafe { attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1) }.map_err(err("Media Foundation"))?;
        let url = HSTRING::from(src.to_string_lossy().as_ref());
        let reader: IMFSourceReader = unsafe { MFCreateSourceReaderFromURL(&url, &attrs) }.map_err(err("Opening the recording"))?;

        let video_index = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let audio_index = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        unsafe { reader.SetStreamSelection(MF_SOURCE_READER_ANY_STREAM.0 as u32, false) }.map_err(err("Opening the recording"))?;
        unsafe { reader.SetStreamSelection(video_index, true) }.map_err(|_| "The recording has no video".to_string())?;
        let native = unsafe { reader.GetNativeMediaType(video_index, 0) }.map_err(err("Reading the video format"))?;
        let size = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(err("Reading the video size"))?;
        let rate = unsafe { native.GetUINT64(&MF_MT_FRAME_RATE) }.unwrap_or((30u64 << 32) | 1);
        let (w, h) = ((size >> 32) as u32, size as u32);
        let fps = (((rate >> 32) as f64) / f64::from((rate as u32).max(1))).round().clamp(1.0, 120.0) as u32;

        let want = unsafe { MFCreateMediaType() }.map_err(err("Video format"))?;
        unsafe {
            want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err("Video format"))?;
            want.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(err("Video format"))?;
            reader.SetCurrentMediaType(video_index, None, &want).map_err(err("Reading the video"))?;
        }

        // A recording made without sound has no audio stream.
        let audio = unsafe {
            reader.SetStreamSelection(audio_index, true).is_ok() && {
                let pcm = MFCreateMediaType().map_err(err("Audio format"))?;
                pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(err("Audio format"))?;
                pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE).map_err(err("Audio format"))?;
                pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(err("Audio format"))?;
                reader.SetCurrentMediaType(audio_index, None, &pcm).is_ok()
            }
        };
        if !audio {
            unsafe { reader.SetStreamSelection(audio_index, false) }.ok();
        }

        let cfg = Config { path: dst.to_path_buf(), w, h, fps, bitrate: bitrate("balanced", w, h, fps), audio, gif: false };
        let writer = open_writer(&cfg)?;

        let (start, end) = (start_ms as i64 * 10_000, end_ms as i64 * 10_000);
        // Reader stream numbers to the kind of stream: 0 video, 1 audio.
        let mut kinds: HashMap<u32, usize> = HashMap::new();
        let mut finished = [false, !audio];
        let mut written = 0u32;
        let mut empty_reads = 0u32;
        while !(finished[0] && finished[1]) {
            let (mut index, mut flags, mut stamp) = (0u32, 0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            unsafe { reader.ReadSample(MF_SOURCE_READER_ANY_STREAM.0 as u32, 0, Some(&mut index), Some(&mut flags), Some(&mut stamp), Some(&mut sample)) }
                .map_err(err("Reading the recording"))?;
            let kind = match kinds.get(&index) {
                Some(k) => *k,
                None => {
                    let major = unsafe { reader.GetCurrentMediaType(index).and_then(|t| t.GetGUID(&MF_MT_MAJOR_TYPE)) };
                    let k = usize::from(major.map_or(true, |g| g != MFMediaType_Video));
                    kinds.insert(index, k);
                    k
                }
            };
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                finished[kind] = true;
            }
            let Some(sample) = sample else {
                empty_reads += 1;
                if empty_reads > 100_000 {
                    return Err("The recording stopped giving frames.".into());
                }
                continue;
            };
            empty_reads = 0;
            let length = unsafe { sample.GetSampleDuration() }.unwrap_or(0);
            if stamp >= end {
                finished[kind] = true;
                continue;
            }
            if stamp + length <= start {
                continue;
            }
            unsafe { sample.SetSampleTime((stamp - start).max(0)) }.map_err(err("Writing the trimmed copy"))?;
            let stream = if kind == 0 { writer.video } else { writer.audio.ok_or("No audio stream")? };
            unsafe { writer.writer.WriteSample(stream, &sample) }.map_err(err("Writing the trimmed copy"))?;
            if kind == 0 {
                written += 1;
            }
        }
        unsafe { writer.writer.Finalize() }.map_err(err("Finishing the file"))?;
        if written == 0 {
            return Err("No frames are left in that part.".into());
        }
        Ok(())
    })();
    unsafe {
        let _ = MFShutdown();
    }
    out
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
