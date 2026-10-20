//! Media tools (spec/capture/media-tools.md): convert and shrink images with the `image` crate,
//! compress video and make GIFs through an `ffmpeg` found on PATH. Everything runs on this computer.

use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use image::{imageops::FilterType, DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::{settings::Store, window};

pub const WINDOW: &str = "mediatools";
const PROGRESS_EVENT: &str = "media://progress";
const FINISHED_EVENT: &str = "media://finished";
const AUDIO_KBPS: f64 = 128.0;

#[derive(Default)]
pub struct MediaState {
    cancel: Arc<AtomicBool>,
    running: Mutex<bool>,
}

#[derive(Clone)]
struct Opts {
    folder: String,
    subfolder: bool,
    template: String,
    overwrite: String,
    keep_date: bool,
    keep_meta: bool,
    format: String,
    quality: u8,
    max_side: u32,
    preset: String,
    target_mb: f64,
    gif_loop: bool,
    gif_fps: u32,
    gif_width: u32,
}

impl Opts {
    fn load(app: &AppHandle) -> Self {
        let s = app.state::<Store>().get();
        let get = |k: &str| s.option("mediaTools", k);
        let text = |k: &str, d: &str| get(k).and_then(|v| v.as_str()).unwrap_or(d).to_string();
        let flag = |k: &str, d: bool| get(k).and_then(|v| v.as_bool()).unwrap_or(d);
        let num = |k: &str, d: f64| get(k).and_then(|v| v.as_f64()).filter(|v| v.is_finite()).unwrap_or(d);
        Self {
            folder: text("folder", "").trim().to_string(),
            subfolder: flag("subfolder", true),
            template: text("nameTemplate", "{name}-converted"),
            overwrite: text("overwrite", "never"),
            keep_date: flag("keepModifiedDate", false),
            keep_meta: flag("keepMetadata", false),
            format: text("imageFormat", "webp"),
            quality: num("imageQuality", 85.0).clamp(10.0, 100.0) as u8,
            max_side: num("maxSide", 0.0).clamp(0.0, 20000.0) as u32,
            preset: text("videoPreset", "balanced"),
            target_mb: num("targetSize", 0.0).max(0.0),
            gif_loop: flag("gifLoop", true),
            gif_fps: num("gifFrameRate", 15.0).clamp(1.0, 60.0) as u32,
            gif_width: num("gifWidth", 640.0).clamp(16.0, 4096.0) as u32,
        }
    }
}

// ---- Pure logic ----

pub struct NameCtx<'a> {
    pub name: &'a str,
    pub index: usize,
    pub date: String,
    pub time: String,
    pub width: u32,
    pub height: u32,
    pub format: &'a str,
}

/// Expands `{name} {index} {date} {time} {width} {height} {format}` and makes the result a safe file name.
pub fn render_name(template: &str, c: &NameCtx) -> String {
    let template = if template.trim().is_empty() { "{name}" } else { template };
    let out = template
        .replace("{name}", c.name)
        .replace("{index}", &c.index.to_string())
        .replace("{date}", &c.date)
        .replace("{time}", &c.time)
        .replace("{width}", &c.width.to_string())
        .replace("{height}", &c.height.to_string())
        .replace("{format}", c.format);
    let clean: String = out.chars().map(|ch| if r#"<>:"/\|?*"#.contains(ch) || ch.is_control() { '_' } else { ch }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() {
        c.name.to_string()
    } else {
        clean
    }
}

/// `name (2).ext`, `name (3).ext` and so on until the path is free.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Where unfinished output lives until it is complete, so a cancelled job leaves nothing behind.
pub fn part_path(out: &Path) -> PathBuf {
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("tmp");
    out.with_file_name(format!("{stem}.notifi-part.{ext}"))
}

/// (year, month, day) for a count of days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `(YYYY-MM-DD, HH-MM-SS)` for a Unix time in seconds and a clock offset in minutes.
pub fn date_time(unix_s: i64, offset_min: i32) -> (String, String) {
    let t = unix_s + i64::from(offset_min) * 60;
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    (format!("{y:04}-{m:02}-{d:02}"), format!("{:02}-{:02}-{:02}", s / 3600, s % 3600 / 60, s % 60))
}

/// Video bitrate in kbps that lands a clip of `seconds` near `target_mb`, or None when it would be unwatchable.
pub fn video_kbps(target_mb: f64, seconds: f64, audio_kbps: f64) -> Option<u32> {
    if target_mb <= 0.0 || seconds <= 0.0 {
        return None;
    }
    // Five percent is left for the container.
    let total = target_mb * 1024.0 * 1024.0 * 8.0 * 0.95 / seconds / 1000.0;
    let video = total - audio_kbps;
    (video >= 60.0).then_some(video as u32)
}

/// One step smaller for a GIF that is over its size target: (width, frame rate), or None at the floor.
pub fn gif_shrink(width: u32, fps: u32) -> Option<(u32, u32)> {
    let next = (f64::from(width) * 0.8) as u32;
    (next >= 100).then(|| (next, fps.saturating_sub(2).max(5)))
}

#[derive(Debug, Default, PartialEq)]
pub struct Probe {
    pub seconds: Option<f64>,
    pub size: Option<(u32, u32)>,
}

/// Reads the length and picture size from the text `ffmpeg -i file` prints.
pub fn parse_probe(text: &str) -> Probe {
    let mut p = Probe::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Duration:") {
            let stamp = rest.split(',').next().unwrap_or_default().trim();
            let parts: Vec<f64> = stamp.split(':').filter_map(|x| x.parse().ok()).collect();
            if let [h, m, s] = parts[..] {
                p.seconds = Some(h * 3600.0 + m * 60.0 + s);
            }
        } else if line.contains("Video:") && p.size.is_none() {
            p.size = line.split(|c: char| c == ' ' || c == ',').find_map(|tok| {
                let (w, h) = tok.split_once('x')?;
                let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
                (w >= 16 && h >= 16).then_some((w, h))
            });
        }
    }
    p
}

/// The EXIF block (APP1 segment, marker included) of a JPEG file.
pub fn exif_segment(jpeg: &[u8]) -> Option<&[u8]> {
    if jpeg.len() < 4 || jpeg[..2] != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            return None;
        }
        let marker = jpeg[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = usize::from(u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]));
        let end = i + 2 + len;
        if len < 2 || end > jpeg.len() {
            return None;
        }
        if marker == 0xE1 && jpeg[i + 4..end].starts_with(b"Exif\0\0") {
            return Some(&jpeg[i..end]);
        }
        i = end;
    }
    None
}

/// Puts a segment straight after the start-of-image marker.
pub fn insert_segment(jpeg: &mut Vec<u8>, segment: &[u8]) {
    if jpeg.len() >= 2 {
        jpeg.splice(2..2, segment.iter().copied());
    }
}

fn extension_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

pub fn is_image(path: &Path) -> bool {
    matches!(extension_of(path).as_str(), "png" | "jpg" | "jpeg" | "bmp" | "webp")
}

pub fn is_video(path: &Path) -> bool {
    matches!(extension_of(path).as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "gif")
}

// ---- Jobs ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    files: Vec<String>,
    /// "image", "video" or "gif".
    action: String,
    format: Option<String>,
    quality: Option<u8>,
    max_side: Option<u32>,
    /// Replaces the page's "If the file exists" for this run (used by "Replace" on a skipped file).
    overwrite: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct Progress {
    index: usize,
    /// "running", "done", "exists", "error" or "cancelled".
    status: &'static str,
    percent: Option<u8>,
    output: Option<String>,
    bytes: Option<u64>,
    message: Option<String>,
}

enum Failure {
    Exists,
    Error(String),
}

impl<E: std::fmt::Display> From<E> for Failure {
    fn from(e: E) -> Self {
        Failure::Error(e.to_string())
    }
}

struct Done {
    path: PathBuf,
    bytes: u64,
    note: Option<String>,
}

struct Run<'a> {
    app: &'a AppHandle,
    o: &'a Opts,
    job: &'a Job,
    cancel: &'a AtomicBool,
    index: usize,
    batch: bool,
}

impl Run<'_> {
    fn overwrite(&self) -> &str {
        self.job.overwrite.as_deref().unwrap_or(&self.o.overwrite)
    }

    fn emit(&self, status: &'static str, percent: Option<u8>) {
        let _ = self.app.emit_to(WINDOW, PROGRESS_EVENT, Progress { index: self.index, status, percent, ..Default::default() });
    }

    /// The path to write `name.ext` to, made from the page's folder, subfolder and collision rules.
    fn target(&self, src: &Path, ctx: &NameCtx, ext: &str) -> Result<PathBuf, Failure> {
        let mut dir = if self.o.folder.is_empty() {
            src.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            PathBuf::from(&self.o.folder)
        };
        if self.o.subfolder && self.batch {
            dir.push("Converted");
        }
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.{ext}", render_name(&self.o.template, ctx)));
        match self.overwrite() {
            _ if !path.exists() => Ok(path),
            "replace" => Ok(path),
            "ask" => Err(Failure::Exists),
            _ => Ok(unique_path(&path)),
        }
    }

    fn name_ctx<'a>(&self, src: &'a Path, size: (u32, u32), format: &'a str) -> NameCtx<'a> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let (date, time) = date_time(now, crate::island::local_offset_min());
        NameCtx {
            name: src.file_stem().and_then(|s| s.to_str()).unwrap_or("file"),
            index: self.index + 1,
            date,
            time,
            width: size.0,
            height: size.1,
            format,
        }
    }

    fn finish(&self, src: &Path, part: &Path, out: &Path) -> Result<Done, Failure> {
        if out.exists() {
            fs::remove_file(out)?;
        }
        fs::rename(part, out)?;
        if self.o.keep_date {
            if let Ok(modified) = fs::metadata(src).and_then(|m| m.modified()) {
                let _ = fs::File::options().write(true).open(out).and_then(|f| f.set_modified(modified));
            }
        }
        Ok(Done { path: out.to_path_buf(), bytes: fs::metadata(out).map_or(0, |m| m.len()), note: None })
    }
}

fn image_job(r: &Run, src: &Path) -> Result<Done, Failure> {
    let format = r.job.format.clone().unwrap_or_else(|| r.o.format.clone());
    let quality = r.job.quality.unwrap_or(r.o.quality).clamp(10, 100);
    let max_side = r.job.max_side.unwrap_or(r.o.max_side);
    let source_jpeg = matches!(extension_of(src).as_str(), "jpg" | "jpeg");
    let original = fs::read(src)?;
    let keep_exif = r.o.keep_meta && source_jpeg && format == "jpeg";
    let exif = if keep_exif { exif_segment(&original) } else { None };

    let mut decoder = ImageReader::new(std::io::Cursor::new(&original))
        .with_guessed_format()?
        .into_decoder()
        .map_err(|_| Failure::Error("This image type isn't supported.".into()))?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    // Re-encoding drops the rotation tag, so turn the pixels unless the tag is being kept.
    if exif.is_none() {
        img.apply_orientation(orientation);
    }
    if max_side > 0 && (img.width() > max_side || img.height() > max_side) {
        img = img.resize(max_side, max_side, FilterType::Lanczos3);
    }

    let ext = if format == "jpeg" { "jpg" } else { format.as_str() };
    let out = r.target(src, &r.name_ctx(src, (img.width(), img.height()), ext), ext)?;
    let mut bytes = Vec::new();
    match format.as_str() {
        "jpeg" => {
            let rgb = flatten(&img);
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
            DynamicImage::ImageRgb8(rgb).write_with_encoder(enc)?;
            if let Some(segment) = exif {
                insert_segment(&mut bytes, segment);
            }
        }
        "png" => img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)?,
        "webp" => DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::WebP)?,
        "avif" => {
            let enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut bytes, 6, quality);
            DynamicImage::ImageRgba8(img.to_rgba8()).write_with_encoder(enc)?;
        }
        other => return Err(Failure::Error(format!("Can't write {other} files."))),
    }
    let part = part_path(&out);
    if let Err(e) = fs::write(&part, &bytes) {
        let _ = fs::remove_file(&part);
        return Err(e.into());
    }
    let mut done = r.finish(src, &part, &out)?;
    if format == "webp" {
        done.note = Some("WebP is saved without loss, so the quality setting doesn't apply.".into());
    }
    Ok(done)
}

/// Transparent areas become white, as JPEG has no transparency.
fn flatten(img: &DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let mix = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        out.put_pixel(x, y, image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
    }
    out
}

// ---- ffmpeg ----

fn ffmpeg() -> Command {
    let mut cmd = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn ffmpeg_present() -> bool {
    ffmpeg().arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn probe(src: &Path) -> Probe {
    let text = ffmpeg()
        .args(["-hide_banner", "-i"])
        .arg(src)
        .stdout(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .unwrap_or_default();
    parse_probe(&text)
}

/// Runs ffmpeg with `-progress` on stdout; returns its last error lines when it fails.
fn encode(r: &Run, args: &[String], seconds: Option<f64>, part: &Path) -> Result<(), Failure> {
    let mut child = ffmpeg()
        .args(["-y", "-hide_banner", "-nostats", "-progress", "pipe:1"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Failure::Error("Needs ffmpeg. Install it and make sure `ffmpeg` runs from a terminal.".into()))?;
    let tail = child.stderr.take().map(|mut e| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = e.read_to_string(&mut text);
            text.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ")
        })
    });
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if r.cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                break;
            }
            if let (Some(us), Some(total)) = (line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()), seconds) {
                r.emit("running", Some((us / 1_000_000.0 / total * 100.0).clamp(0.0, 99.0) as u8));
            }
        }
    }
    let status = child.wait()?;
    let message = tail.and_then(|t| t.join().ok()).unwrap_or_default();
    if r.cancel.load(Ordering::SeqCst) || !status.success() {
        let _ = fs::remove_file(part);
    }
    if r.cancel.load(Ordering::SeqCst) {
        return Err(Failure::Error("Cancelled.".into()));
    }
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Error(if message.is_empty() { "ffmpeg couldn't process this file.".into() } else { message }))
    }
}

fn video_job(r: &Run, src: &Path, gif: bool) -> Result<Done, Failure> {
    if !ffmpeg_present() {
        return Err(Failure::Error("Needs ffmpeg. Install it and make sure `ffmpeg` runs from a terminal.".into()));
    }
    let info = probe(src);
    let ext = if gif { "gif" } else { "mp4" };
    let out = r.target(src, &r.name_ctx(src, info.size.unwrap_or((0, 0)), ext), ext)?;
    let part = part_path(&out);
    let input = src.to_string_lossy().into_owned();
    let metadata = if r.o.keep_meta { "0" } else { "-1" };
    let target_bytes = (r.o.target_mb * 1024.0 * 1024.0) as u64;
    let mut note = None;

    if gif {
        let (mut width, mut fps) = (r.o.gif_width.min(info.size.map_or(u32::MAX, |s| s.0)), r.o.gif_fps);
        let loops = if r.o.gif_loop { "0" } else { "-1" };
        loop {
            let filter = format!(
                "fps={fps},scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse"
            );
            let args = vec![
                "-i".into(),
                input.clone(),
                "-vf".into(),
                filter,
                "-loop".into(),
                loops.into(),
                "-map_metadata".into(),
                metadata.into(),
                part.to_string_lossy().into_owned(),
            ];
            encode(r, &args, info.seconds, &part)?;
            let size = fs::metadata(&part).map_or(0, |m| m.len());
            if target_bytes == 0 || size <= target_bytes {
                break;
            }
            match gif_shrink(width, fps) {
                Some((w, f)) => (width, fps) = (w, f),
                None => {
                    note = Some(format!("The target size wasn't reachable. The smallest result is {:.1} MB.", size as f64 / 1_048_576.0));
                    break;
                }
            }
        }
    } else {
        let (preset, crf) = match r.o.preset.as_str() {
            "small" => ("slow", "30"),
            "high" => ("medium", "20"),
            _ => ("medium", "26"),
        };
        let mut args: Vec<String> = vec!["-i".into(), input, "-c:v".into(), "libx264".into(), "-preset".into(), preset.into()];
        if target_bytes > 0 {
            let seconds = info.seconds.ok_or_else(|| Failure::Error("Couldn't read the video's length to aim for a size.".into()))?;
            let kbps = video_kbps(r.o.target_mb, seconds, AUDIO_KBPS)
                .ok_or_else(|| Failure::Error("The target size is too small for a video this long.".into()))?;
            args.extend(["-b:v".into(), format!("{kbps}k"), "-maxrate".into(), format!("{}k", kbps * 3 / 2), "-bufsize".into(), format!("{}k", kbps * 2)]);
        } else {
            args.extend(["-crf".into(), crf.into()]);
        }
        args.extend([
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{AUDIO_KBPS}k"),
            "-movflags".into(),
            "+faststart".into(),
            "-map_metadata".into(),
            metadata.into(),
            part.to_string_lossy().into_owned(),
        ]);
        encode(r, &args, info.seconds, &part)?;
        let size = fs::metadata(&part).map_or(0, |m| m.len());
        if target_bytes > 0 && size > target_bytes {
            note = Some(format!("The result is {:.1} MB, above the {} MB target.", size as f64 / 1_048_576.0, r.o.target_mb));
        }
    }
    let mut done = r.finish(src, &part, &out)?;
    done.note = note;
    Ok(done)
}

fn work(app: &AppHandle, job: &Job, cancel: &AtomicBool) {
    let o = Opts::load(app);
    let batch = job.files.len() > 1;
    for (index, file) in job.files.iter().enumerate() {
        let run = Run { app, o: &o, job, cancel, index, batch };
        if cancel.load(Ordering::SeqCst) {
            run.emit("cancelled", None);
            continue;
        }
        run.emit("running", Some(0));
        let src = Path::new(file);
        let result = match job.action.as_str() {
            "image" if is_image(src) => image_job(&run, src),
            "image" => Err(Failure::Error("This isn't an image type that can be converted.".into())),
            "gif" | "video" if is_video(src) => video_job(&run, src, job.action == "gif"),
            _ => Err(Failure::Error("This file can't be used for that.".into())),
        };
        let progress = match result {
            Ok(done) => Progress {
                index,
                status: "done",
                percent: Some(100),
                output: Some(done.path.to_string_lossy().into_owned()),
                bytes: Some(done.bytes),
                message: done.note,
            },
            Err(Failure::Exists) => Progress { index, status: "exists", message: Some("A file with that name already exists.".into()), ..Default::default() },
            Err(Failure::Error(_)) if cancel.load(Ordering::SeqCst) => Progress { index, status: "cancelled", ..Default::default() },
            Err(Failure::Error(message)) => Progress { index, status: "error", message: Some(message), ..Default::default() },
        };
        let _ = app.emit_to(WINDOW, PROGRESS_EVENT, progress);
    }
    let _ = app.emit_to(WINDOW, FINISHED_EVENT, ());
}

// ---- Commands and window ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    ffmpeg: bool,
    format: String,
    quality: u8,
    max_side: u32,
    target_mb: f64,
}

#[tauri::command]
pub async fn media_status(app: AppHandle) -> Status {
    let o = Opts::load(&app);
    let ffmpeg = tauri::async_runtime::spawn_blocking(ffmpeg_present).await.unwrap_or(false);
    Status { ffmpeg, format: o.format, quality: o.quality, max_side: o.max_side, target_mb: o.target_mb }
}

#[tauri::command]
pub fn media_run(app: AppHandle, state: State<MediaState>, job: Job) -> Result<(), String> {
    if job.files.is_empty() {
        return Err("No files to work on.".into());
    }
    {
        let mut running = state.running.lock().unwrap();
        if *running {
            return Err("A job is already running.".into());
        }
        *running = true;
    }
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    std::thread::Builder::new()
        .name("notifi-media".into())
        .spawn(move || {
            work(&app, &job, &cancel);
            *app.state::<MediaState>().running.lock().unwrap() = false;
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn media_cancel(state: State<MediaState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn media_open(app: AppHandle) {
    open(&app);
}

/// Shows the Media window, building it the first time.
pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        if !window::no_focus() {
            let _ = w.set_focus();
        }
        return;
    }
    // Building a webview from an event handler can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::App("mediatools.html".into()))
            .title("Notifi Media Tools")
            .inner_size(700.0, 600.0)
            .min_inner_size(520.0, 420.0)
            .center()
            .focused(!window::no_focus())
            .build();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(name: &str) -> NameCtx<'_> {
        NameCtx { name, index: 3, date: "2026-10-20".into(), time: "08-05-09".into(), width: 800, height: 600, format: "webp" }
    }

    #[test]
    fn name_templates() {
        assert_eq!(render_name("{name}-converted", &ctx("photo")), "photo-converted");
        assert_eq!(render_name("{name}_{index}_{width}x{height}.{format}", &ctx("a")), "a_3_800x600.webp");
        assert_eq!(render_name("{date} {time}", &ctx("a")), "2026-10-20 08-05-09");
        assert_eq!(render_name("  ", &ctx("keep")), "keep");
        assert_eq!(render_name("a/b:c?", &ctx("x")), "a_b_c_");
        assert_eq!(render_name("???.", &ctx("x")), "___");
    }

    #[test]
    fn collisions_get_a_number() {
        let dir = std::env::temp_dir().join(format!("notifi-media-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("a.png");
        assert_eq!(unique_path(&first), first);
        fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&first), dir.join("a (2).png"));
        fs::write(dir.join("a (2).png"), b"x").unwrap();
        assert_eq!(unique_path(&first), dir.join("a (3).png"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(date_time(1_700_000_000, 0), ("2023-11-14".into(), "22-13-20".into()));
        assert_eq!(date_time(1_700_000_000, 120), ("2023-11-15".into(), "00-13-20".into()));
    }

    #[test]
    fn size_targets() {
        // 10 MB over 100 s: about 797 kbps in total, 668 of it video.
        assert_eq!(video_kbps(10.0, 100.0, 128.0), Some(668));
        assert_eq!(video_kbps(1.0, 600.0, 128.0), None);
        assert_eq!(video_kbps(0.0, 100.0, 128.0), None);
        assert_eq!(gif_shrink(640, 15), Some((512, 13)));
        assert_eq!(gif_shrink(120, 6), None);
        assert_eq!(gif_shrink(300, 5), Some((240, 5)));
    }

    #[test]
    fn reads_ffmpeg_info() {
        let text = "Input #0, mov,mp4, from 'a.mp4':\n  Duration: 00:01:30.50, start: 0.0, bitrate: 900 kb/s\n  Stream #0:0: Video: h264 (High), yuv420p, 1280x720 [SAR 1:1], 30 fps\n";
        assert_eq!(parse_probe(text), Probe { seconds: Some(90.5), size: Some((1280, 720)) });
        assert_eq!(parse_probe("nothing"), Probe::default());
    }

    #[test]
    fn exif_is_copied_between_jpegs() {
        let exif = [0xFF, 0xE1, 0x00, 0x0A, b'E', b'x', b'i', b'f', 0, 0, 1, 2];
        let mut source = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0];
        source.extend_from_slice(&exif);
        source.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0xFF, 0xD9]);
        assert_eq!(exif_segment(&source), Some(&exif[..]));
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02];
        insert_segment(&mut out, &exif);
        assert_eq!(&out[..2], &[0xFF, 0xD8]);
        assert_eq!(&out[2..14], &exif);
        assert_eq!(exif_segment(&out), Some(&exif[..]));
        assert_eq!(exif_segment(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02]), None);
    }

    #[test]
    fn file_kinds() {
        assert!(is_image(Path::new("a.JPG")));
        assert!(!is_image(Path::new("a.mp4")));
        assert!(is_video(Path::new("a.mkv")));
        assert_eq!(part_path(Path::new("D:/x/a.mp4")), Path::new("D:/x/a.notifi-part.mp4"));
    }
}
