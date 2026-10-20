//! The parts of screen recording that need no operating system: file names, frame pacing, bitrate,
//! the pause-aware clock, zoom easing, audio maths and the drawing done into the frames.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub type Stereo = (f32, f32);

pub const SAMPLE_RATE: u32 = 48_000;
/// Audio is written this far behind the clock so a capture buffer that arrives late is not cut short.
const AUDIO_LATENCY: Duration = Duration::from_millis(100);

/// A rectangle of the desktop in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Region {
    /// Video encoders want even sizes.
    pub fn even(self) -> Self {
        Self { w: (self.w & !1).max(2), h: (self.h & !1).max(2), ..self }
    }

    pub fn center(self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    /// The part of `self` inside `bounds`, or `None` when they do not overlap.
    pub fn within(self, bounds: Region) -> Option<Region> {
        let x = self.x.max(bounds.x);
        let y = self.y.max(bounds.y);
        let right = (self.x + self.w).min(bounds.x + bounds.w);
        let bottom = (self.y + self.h).min(bounds.y + bounds.h);
        (right > x && bottom > y).then_some(Region { x, y, w: right - x, h: bottom - y })
    }
}

// ---- Names ----

/// A file name (no extension) from the template: `{date}` is YYYY-MM-DD and `{time}` is HH.MM.SS.
/// Characters Windows does not allow in names become `-`.
pub fn file_stem(template: &str, date: (u16, u16, u16), time: (u16, u16, u16)) -> String {
    let d = format!("{:04}-{:02}-{:02}", date.0, date.1, date.2);
    let t = format!("{:02}.{:02}.{:02}", time.0, time.1, time.2);
    let raw = template.replace("{date}", &d).replace("{time}", &t);
    let clean: String = raw
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c })
        .collect();
    let clean: String = clean.trim().trim_end_matches('.').trim_end().chars().take(120).collect();
    if clean.is_empty() {
        format!("Recording {d} {t}")
    } else {
        clean
    }
}

/// `dir/stem.ext`, or `dir/stem (2).ext` and so on when that name is taken. Never an existing file.
pub fn unique_path(dir: &Path, stem: &str, ext: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !exists(&first) {
        return first;
    }
    let mut n = 2u32;
    loop {
        let p = dir.join(format!("{stem} ({n}).{ext}"));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

// ---- Pacing, bitrate, clock ----

pub fn fps_from(choice: &str) -> u32 {
    if choice == "60" {
        60
    } else {
        30
    }
}

/// Which frame slot the recording is in after `elapsed` of recorded (not paused) time.
pub fn frame_index(elapsed: Duration, fps: u32) -> u64 {
    (elapsed.as_nanos() * u128::from(fps) / 1_000_000_000) as u64
}

pub fn frame_time(index: u64, fps: u32) -> Duration {
    Duration::from_nanos((u128::from(index) * 1_000_000_000 / u128::from(fps.max(1))) as u64)
}

/// Sample time of a frame slot in 100 ns units, as Media Foundation counts.
pub fn frame_ts(index: u64, fps: u32) -> i64 {
    (u128::from(index) * 10_000_000 / u128::from(fps.max(1))) as i64
}

/// H.264 bits per second from the quality choice: bits per pixel per frame, kept in a sane range.
pub fn bitrate(quality: &str, w: u32, h: u32, fps: u32) -> u32 {
    let bpp = match quality {
        "small" => 0.05,
        "high" => 0.18,
        _ => 0.10,
    };
    (f64::from(w) * f64::from(h) * f64::from(fps) * bpp).clamp(1_500_000.0, 80_000_000.0).round() as u32
}

/// Recorded time: wall time minus every pause.
pub struct ActiveClock {
    start: Instant,
    paused: Duration,
    paused_at: Option<Instant>,
}

impl ActiveClock {
    pub fn new(now: Instant) -> Self {
        Self { start: now, paused: Duration::ZERO, paused_at: None }
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        let end = self.paused_at.unwrap_or(now);
        end.saturating_duration_since(self.start).saturating_sub(self.paused)
    }

    pub fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
    }

    pub fn resume(&mut self, now: Instant) {
        if let Some(at) = self.paused_at.take() {
            self.paused += now.saturating_duration_since(at);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }
}

// ---- Typing zoom ----

pub const TYPING_ZOOM: f64 = 1.8;
/// How long the zoom stays in after the last key.
const TYPING_HOLD: f64 = 1.5;

/// Moves `cur` toward `target` so the gap closes smoothly whatever the frame time is.
pub fn ease_toward(cur: f64, target: f64, dt: f64, rate: f64) -> f64 {
    cur + (target - cur) * (1.0 - (-rate * dt.max(0.0)).exp())
}

/// The part of a `w` x `h` frame shown at `level`, centred on (cx, cy) but never leaving the frame.
pub fn zoom_rect(w: f64, h: f64, level: f64, cx: f64, cy: f64) -> (f64, f64, f64, f64) {
    let level = level.max(1.0);
    let (cw, ch) = (w / level, h / level);
    ((cx - cw / 2.0).clamp(0.0, w - cw), (cy - ch / 2.0).clamp(0.0, h - ch), cw, ch)
}

pub struct Zoom {
    pub level: f64,
    pub cx: f64,
    pub cy: f64,
    last_typed: Option<f64>,
}

impl Zoom {
    pub fn new(w: f64, h: f64) -> Self {
        Self { level: 1.0, cx: w / 2.0, cy: h / 2.0, last_typed: None }
    }

    pub fn typed(&mut self, now: f64) {
        self.last_typed = Some(now);
    }

    /// Advances by `dt` seconds toward the zoomed or the full view and follows `focus` while zoomed.
    pub fn step(&mut self, now: f64, dt: f64, focus: (f64, f64)) {
        let active = self.last_typed.is_some_and(|t| now - t < TYPING_HOLD);
        self.level = ease_toward(self.level, if active { TYPING_ZOOM } else { 1.0 }, dt, 5.0);
        if active || self.level > 1.01 {
            self.cx = ease_toward(self.cx, focus.0, dt, 8.0);
            self.cy = ease_toward(self.cy, focus.1, dt, 8.0);
        }
    }

    pub fn rect(&self, w: f64, h: f64) -> (f64, f64, f64, f64) {
        zoom_rect(w, h, self.level, self.cx, self.cy)
    }
}

/// Scales `rect` of the BGRA frame `src` up to fill `dst` (same size), bilinear.
pub fn zoom_blit(src: &[u8], w: usize, h: usize, rect: (f64, f64, f64, f64), dst: &mut [u8]) {
    let (rx, ry, rw, rh) = rect;
    let axis = |i: usize, origin: f64, span: f64, n: usize| {
        let s = (origin + (i as f64 + 0.5) * span / n as f64 - 0.5).clamp(0.0, (n - 1) as f64);
        let a = s.floor() as usize;
        (a, (a + 1).min(n - 1), ((s - a as f64) * 256.0) as u32)
    };
    let xs: Vec<(usize, usize, u32)> = (0..w).map(|x| axis(x, rx, rw, w)).collect();
    for y in 0..h {
        let (y0, y1, fy) = axis(y, ry, rh, h);
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            for c in 0..4 {
                let p = |yy: usize, xx: usize| u32::from(src[(yy * w + xx) * 4 + c]);
                let top = p(y0, x0) * (256 - fx) + p(y0, x1) * fx;
                let bottom = p(y1, x0) * (256 - fx) + p(y1, x1) * fx;
                dst[(y * w + x) * 4 + c] = ((top * (256 - fy) + bottom * fy) >> 16) as u8;
            }
        }
    }
}

// ---- Cursor effects ----

pub const HIGHLIGHT_RADIUS: f32 = 26.0;
pub const HIGHLIGHT: [u8; 3] = [255, 214, 64];
pub const LEFT_RING: [u8; 3] = [64, 160, 255];
pub const RIGHT_RING: [u8; 3] = [255, 140, 60];
const RIPPLE_MS: u64 = 550;

pub struct Ripple {
    pub x: i32,
    pub y: i32,
    pub born_ms: u64,
    pub right: bool,
}

/// Radius and opacity of a click ring this many milliseconds after the click; `None` once it is gone.
pub fn ripple_look(age_ms: u64) -> Option<(f32, f32)> {
    if age_ms >= RIPPLE_MS {
        return None;
    }
    let t = age_ms as f32 / RIPPLE_MS as f32;
    let out = 1.0 - (1.0 - t).powi(3);
    Some((10.0 + 36.0 * out, 0.9 * (1.0 - t)))
}

pub fn blend_px(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, rgb: [u8; 3], a: f32) {
    if x < 0 || y < 0 || x >= w || y >= h || a <= 0.0 {
        return;
    }
    let i = (y * w + x) as usize * 4;
    for (k, c) in [rgb[2], rgb[1], rgb[0]].into_iter().enumerate() {
        let old = f32::from(buf[i + k]);
        buf[i + k] = (old + (f32::from(c) - old) * a).round() as u8;
    }
}

/// A filled circle with a one-pixel soft edge, blended over the BGRA frame.
pub fn fill_disc(buf: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, radius: f32, rgb: [u8; 3], alpha: f32) {
    let r = radius.ceil() as i32 + 1;
    for y in (cy - r).max(0)..(cy + r + 1).min(h) {
        for x in (cx - r).max(0)..(cx + r + 1).min(w) {
            let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
            blend_px(buf, w, h, x, y, rgb, alpha * (radius + 0.5 - d).clamp(0.0, 1.0));
        }
    }
}

/// A ring of the given line width, blended over the BGRA frame.
pub fn stroke_ring(buf: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, radius: f32, width: f32, rgb: [u8; 3], alpha: f32) {
    let r = (radius + width).ceil() as i32 + 1;
    for y in (cy - r).max(0)..(cy + r + 1).min(h) {
        for x in (cx - r).max(0)..(cx + r + 1).min(w) {
            let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
            blend_px(buf, w, h, x, y, rgb, alpha * (width / 2.0 + 0.5 - (d - radius).abs()).clamp(0.0, 1.0));
        }
    }
}

// ---- Colour conversion and GIF sizing ----

/// BGRA to NV12 (BT.709, limited range), the layout H.264 encoders take.
pub fn bgra_to_nv12(src: &[u8], w: usize, h: usize, out: &mut Vec<u8>) {
    out.resize(w * h * 3 / 2, 0);
    let (luma, chroma) = out.split_at_mut(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (b, g, r) = (i32::from(src[i]), i32::from(src[i + 1]), i32::from(src[i + 2]));
            luma[y * w + x] = (((47 * r + 157 * g + 16 * b + 128) >> 8) + 16) as u8;
        }
    }
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let i = ((y + dy).min(h - 1) * w + (x + dx).min(w - 1)) * 4;
                b += i32::from(src[i]);
                g += i32::from(src[i + 1]);
                r += i32::from(src[i + 2]);
            }
            let (r, g, b) = (r >> 2, g >> 2, b >> 2);
            let o = (y / 2) * w + x;
            chroma[o] = (((-26 * r - 87 * g + 113 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            chroma[o + 1] = (((112 * r - 102 * g - 10 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
}

/// GIF size: at most `max_w` wide, same shape.
pub fn gif_dims(w: u32, h: u32, max_w: u32) -> (u32, u32) {
    if w <= max_w {
        return (w, h);
    }
    (max_w, (u64::from(h) * u64::from(max_w) / u64::from(w)).max(1) as u32)
}

/// RGBA at `gw` x `gh` from a BGRA frame, nearest neighbour.
pub fn bgra_to_rgba_scaled(src: &[u8], w: usize, h: usize, gw: usize, gh: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(gw * gh * 4);
    for y in 0..gh {
        let sy = y * h / gh;
        for x in 0..gw {
            let i = (sy * w + x * w / gw) * 4;
            out.extend_from_slice(&[src[i + 2], src[i + 1], src[i], 255]);
        }
    }
    out
}

/// GIF frame delay in hundredths of a second between two sample times (100 ns units).
pub fn gif_delay_cs(from_ts: i64, to_ts: i64) -> u16 {
    ((to_ts - from_ts) / 100_000).clamp(2, 6000) as u16
}

// ---- Audio ----

/// One interleaved frame down to stereo, channels in WASAPI order: FL FR FC LFE BL BR SL SR.
/// Nothing is scaled down, so a many-channel device is not quieter than a stereo one.
pub fn downmix(frame: &[f32]) -> Stereo {
    const SIDE: f32 = 0.707;
    match frame {
        [] => (0.0, 0.0),
        [m] => (*m, *m),
        [l, r] => (*l, *r),
        _ => {
            let (mut l, mut r) = (frame[0], frame[1]);
            if let Some(c) = frame.get(2) {
                l += c * SIDE;
                r += c * SIDE;
            }
            for (i, s) in frame.iter().enumerate().skip(4) {
                if i % 2 == 0 {
                    l += s * SIDE;
                } else {
                    r += s * SIDE;
                }
            }
            (l, r)
        }
    }
}

/// Stereo frames from a capture buffer of 16, 24 or 32-bit integers or 32-bit floats.
pub fn decode_frames(bytes: &[u8], channels: usize, bits: u16, float: bool) -> Vec<Stereo> {
    let width = usize::from(bits / 8);
    if channels == 0 || width == 0 {
        return Vec::new();
    }
    let sample = |b: &[u8]| -> f32 {
        match (width, float) {
            (4, true) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (4, false) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            (3, _) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (2, _) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            _ => 0.0,
        }
    };
    bytes
        .chunks_exact(channels * width)
        .map(|frame| {
            let s: Vec<f32> = frame.chunks_exact(width).map(sample).collect();
            downmix(&s)
        })
        .collect()
}

/// Linear resampler that carries its position between chunks, so the seams do not click.
pub struct Resampler {
    step: f64,
    pos: f64,
    tail: Option<Stereo>,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self { step: f64::from(from_rate) / f64::from(to_rate.max(1)), pos: 0.0, tail: None }
    }

    pub fn push(&mut self, input: &[Stereo], out: &mut Vec<Stereo>) {
        if input.is_empty() {
            return;
        }
        let mut seq = Vec::with_capacity(input.len() + 1);
        seq.extend(self.tail);
        seq.extend_from_slice(input);
        while self.pos + 1.0 < seq.len() as f64 {
            let i = self.pos as usize;
            let f = (self.pos - i as f64) as f32;
            let (a, b) = (seq[i], seq[i + 1]);
            out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
            self.pos += self.step;
        }
        let skip = self.pos as usize;
        match seq.get(skip) {
            Some(&t) => {
                self.tail = Some(t);
                self.pos -= skip as f64;
            }
            None => {
                self.tail = None;
                self.pos -= seq.len() as f64;
            }
        }
    }
}

fn pcm16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// `frames` interleaved stereo 16-bit frames of the two sources added together; a source that has
/// fewer frames than asked for is silent for the rest.
pub fn mix_pcm(system: &[Stereo], mic: &[Stereo], frames: usize, out: &mut Vec<i16>) {
    for i in 0..frames {
        let (a, b) = system.get(i).copied().unwrap_or((0.0, 0.0));
        let (c, d) = mic.get(i).copied().unwrap_or((0.0, 0.0));
        out.push(pcm16(a + c));
        out.push(pcm16(b + d));
    }
}

/// How many audio tracks a recording gets: system sound and the microphone are kept apart when both
/// are on, so they can be balanced or removed in the editor.
pub fn track_count(system: bool, mic: bool) -> u8 {
    match (system, mic) {
        (true, true) => 2,
        (false, false) => 0,
        _ => 1,
    }
}

/// Two interleaved 16-bit tracks added together (for a file that can only hold one sound track).
pub fn mix_tracks(a: &[i16], b: &[i16]) -> Vec<i16> {
    (0..a.len().max(b.len()))
        .map(|i| (i32::from(a.get(i).copied().unwrap_or(0)) + i32::from(b.get(i).copied().unwrap_or(0))).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
        .collect()
}

/// How many audio frames to write now: the ones the clock has reached, less the latency margin.
pub fn audio_frames_due(elapsed: Duration, written: u64) -> u64 {
    let target = (elapsed.saturating_sub(AUDIO_LATENCY).as_nanos() * u128::from(SAMPLE_RATE) / 1_000_000_000) as u64;
    target.saturating_sub(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fill_the_template_and_drop_bad_characters() {
        assert_eq!(file_stem("Recording {date} {time}", (2026, 3, 9), (14, 5, 7)), "Recording 2026-03-09 14.05.07");
        assert_eq!(file_stem("a/b:c*d?", (2026, 1, 1), (0, 0, 0)), "a-b-c-d-");
        assert_eq!(file_stem("  ...  ", (2026, 1, 1), (1, 2, 3)), "Recording 2026-01-01 01.02.03");
        assert_eq!(file_stem("", (2026, 1, 1), (1, 2, 3)), "Recording 2026-01-01 01.02.03");
        assert_eq!(file_stem(&"x".repeat(300), (2026, 1, 1), (0, 0, 0)).chars().count(), 120);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let taken = [PathBuf::from("d/r.mp4"), PathBuf::from("d/r (2).mp4")];
        let p = unique_path(Path::new("d"), "r", "mp4", |p| taken.iter().any(|t| t == p));
        assert_eq!(p, PathBuf::from("d/r (3).mp4"));
        assert_eq!(unique_path(Path::new("d"), "new", "gif", |_| false), PathBuf::from("d/new.gif"));
    }

    #[test]
    fn frames_follow_the_clock() {
        assert_eq!(frame_index(Duration::from_millis(0), 30), 0);
        assert_eq!(frame_index(Duration::from_millis(33), 30), 0);
        assert_eq!(frame_index(Duration::from_millis(34), 30), 1);
        assert_eq!(frame_index(Duration::from_secs(2), 60), 120);
        assert_eq!(frame_time(30, 30), Duration::from_secs(1));
        assert_eq!(frame_ts(30, 30), 10_000_000);
        assert_eq!(frame_ts(1, 60), 166_666);
        assert_eq!(fps_from("60"), 60);
        assert_eq!(fps_from("whatever"), 30);
    }

    #[test]
    fn bitrate_follows_quality_and_stays_in_range() {
        let balanced = bitrate("balanced", 1920, 1080, 30);
        assert_eq!(balanced, 6_220_800);
        assert!(bitrate("small", 1920, 1080, 30) < balanced);
        assert!(bitrate("high", 1920, 1080, 30) > balanced);
        assert_eq!(bitrate("small", 320, 200, 30), 1_500_000);
        assert_eq!(bitrate("high", 7680, 4320, 60), 80_000_000);
    }

    #[test]
    fn paused_time_does_not_count() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut c = ActiveClock::new(t0);
        assert_eq!(c.elapsed(at(1000)), Duration::from_millis(1000));
        c.pause(at(1000));
        c.pause(at(1500));
        assert!(c.is_paused());
        assert_eq!(c.elapsed(at(3000)), Duration::from_millis(1000));
        c.resume(at(3000));
        assert_eq!(c.elapsed(at(3500)), Duration::from_millis(1500));
        c.resume(at(4000));
        assert_eq!(c.elapsed(at(4000)), Duration::from_millis(2000));
    }

    #[test]
    fn easing_closes_the_gap_without_overshoot() {
        let mut v = 1.0;
        for _ in 0..10 {
            let next = ease_toward(v, 2.0, 0.033, 5.0);
            assert!(next > v && next < 2.0);
            v = next;
        }
        assert_eq!(ease_toward(1.0, 2.0, 0.0, 5.0), 1.0);
        assert!((ease_toward(1.0, 2.0, 100.0, 5.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_goes_in_while_typing_and_back_out_after() {
        let mut z = Zoom::new(1000.0, 500.0);
        z.typed(0.0);
        for i in 1..=60 {
            z.step(i as f64 * 0.016, 0.016, (900.0, 100.0));
        }
        assert!(z.level > 1.5 && z.level < TYPING_ZOOM + 1e-9);
        assert!(z.cx > 600.0);
        for i in 61..=400 {
            z.step(i as f64 * 0.016, 0.016, (900.0, 100.0));
        }
        assert!(z.level < 1.05);
    }

    #[test]
    fn the_zoomed_view_stays_inside_the_frame() {
        let (x, y, w, h) = zoom_rect(1000.0, 500.0, 2.0, 990.0, 5.0);
        assert_eq!((w, h), (500.0, 250.0));
        assert_eq!((x, y), (500.0, 0.0));
        let (x, y, w, h) = zoom_rect(1000.0, 500.0, 0.5, 0.0, 0.0);
        assert_eq!((x, y, w, h), (0.0, 0.0, 1000.0, 500.0));
    }

    #[test]
    fn a_full_frame_zoom_changes_nothing() {
        let src: Vec<u8> = (0..4 * 4 * 4).map(|i| (i * 3) as u8).collect();
        let mut dst = vec![0u8; src.len()];
        zoom_blit(&src, 4, 4, (0.0, 0.0, 4.0, 4.0), &mut dst);
        assert_eq!(src, dst);
    }

    #[test]
    fn ripples_grow_and_fade() {
        let (r0, a0) = ripple_look(0).unwrap();
        let (r1, a1) = ripple_look(300).unwrap();
        assert!(r1 > r0 && a1 < a0);
        assert!(ripple_look(RIPPLE_MS).is_none());
    }

    #[test]
    fn drawing_stays_inside_the_frame_and_blends() {
        let (w, h) = (20, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        fill_disc(&mut buf, w, h, 10, 10, 4.0, [200, 100, 50], 1.0);
        let i = (10 * w + 10) as usize * 4;
        assert_eq!(&buf[i..i + 3], &[50, 100, 200]);
        assert_eq!(&buf[0..3], &[0, 0, 0]);
        stroke_ring(&mut buf, w, h, -5, -5, 30.0, 3.0, [255, 255, 255], 1.0);
        fill_disc(&mut buf, w, h, 19, 19, 50.0, [1, 1, 1], 0.5);
    }

    #[test]
    fn nv12_black_and_white() {
        let mut out = Vec::new();
        bgra_to_nv12(&[0, 0, 0, 255].repeat(16), 4, 4, &mut out);
        assert_eq!(out.len(), 24);
        assert!(out[..16].iter().all(|&y| y == 16));
        assert!(out[16..].iter().all(|&c| c == 128));
        bgra_to_nv12(&[255, 255, 255, 255].repeat(16), 4, 4, &mut out);
        assert!(out[..16].iter().all(|&y| y == 235));
        assert!(out[16..].iter().all(|&c| c == 128));
    }

    #[test]
    fn gif_sizes_and_delays() {
        assert_eq!(gif_dims(800, 600, 960), (800, 600));
        assert_eq!(gif_dims(1920, 1080, 960), (960, 540));
        assert_eq!(gif_delay_cs(0, 666_666), 6);
        assert_eq!(gif_delay_cs(0, 1), 2);
        let rgba = bgra_to_rgba_scaled(&[1, 2, 3, 255, 4, 5, 6, 255], 2, 1, 1, 1);
        assert_eq!(rgba, [3, 2, 1, 255]);
    }

    #[test]
    fn many_channels_are_not_quieter() {
        assert_eq!(downmix(&[0.5, -0.5]), (0.5, -0.5));
        assert_eq!(downmix(&[0.5]), (0.5, 0.5));
        let eight = downmix(&[1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(eight, (1.0, 1.0));
        let (l, r) = downmix(&[0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
        assert!((l - 0.707 * 2.0).abs() < 1e-6 && (r - 0.707 * 2.0).abs() < 1e-6, "{l} {r}");
    }

    #[test]
    fn capture_buffers_decode() {
        let f = decode_frames(&[0x00, 0x40, 0x00, 0xc0], 2, 16, false);
        assert_eq!(f, [(0.5, -0.5)]);
        let bytes: Vec<u8> = [0.25f32, 0.75].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(decode_frames(&bytes, 2, 32, true), [(0.25, 0.75)]);
        assert!(decode_frames(&[1, 2, 3], 0, 16, false).is_empty());
    }

    #[test]
    fn resampling_keeps_the_rate_and_the_ramp() {
        let ramp: Vec<Stereo> = (0..100).map(|i| (i as f32, i as f32)).collect();
        let mut same = Resampler::new(48_000, 48_000);
        let mut out = Vec::new();
        same.push(&ramp[..50], &mut out);
        same.push(&ramp[50..], &mut out);
        assert_eq!(out.len(), 99);
        assert!(out.iter().enumerate().all(|(i, s)| (s.0 - i as f32).abs() < 1e-4));

        let mut half = Resampler::new(96_000, 48_000);
        let mut out = Vec::new();
        half.push(&ramp, &mut out);
        assert!(out.len() >= 49 && out.len() <= 50);
        assert!(out.iter().enumerate().all(|(i, s)| (s.0 - 2.0 * i as f32).abs() < 1e-4));

        let mut up = Resampler::new(24_000, 48_000);
        let mut out = Vec::new();
        up.push(&ramp, &mut out);
        assert!(out.len() > 190 && out[1].0 > 0.0 && out[1].0 < 1.0);
    }

    #[test]
    fn mixing_adds_clips_and_pads_with_silence() {
        let mut out = Vec::new();
        mix_pcm(&[(0.5, 0.5), (1.0, 1.0)], &[(0.25, -0.25)], 3, &mut out);
        assert_eq!(out.len(), 6);
        assert_eq!(out[0], pcm16(0.75));
        assert_eq!(out[1], pcm16(0.25));
        assert_eq!(out[2], 32767);
        assert_eq!((out[4], out[5]), (0, 0));
    }

    #[test]
    fn audio_is_written_up_to_the_clock_minus_the_margin() {
        assert_eq!(audio_frames_due(Duration::from_millis(50), 0), 0);
        assert_eq!(audio_frames_due(Duration::from_millis(1100), 0), 48_000);
        assert_eq!(audio_frames_due(Duration::from_millis(1100), 48_000), 0);
        assert_eq!(audio_frames_due(Duration::from_millis(1100), 50_000), 0);
    }

    #[test]
    fn regions_clip_to_a_display() {
        let display = Region { x: 0, y: 0, w: 1920, h: 1080 };
        let r = Region { x: 1800, y: 1000, w: 300, h: 300 };
        assert_eq!(r.within(display), Some(Region { x: 1800, y: 1000, w: 120, h: 80 }));
        assert_eq!(Region { x: 3000, y: 0, w: 10, h: 10 }.within(display), None);
        assert_eq!(Region { x: 0, y: 0, w: 101, h: 55 }.even(), Region { x: 0, y: 0, w: 100, h: 54 });
        assert_eq!(Region { x: 0, y: 0, w: 1, h: 1 }.even(), Region { x: 0, y: 0, w: 2, h: 2 });
    }

    #[test]
    fn sound_tracks_follow_the_sources() {
        assert_eq!(track_count(true, true), 2);
        assert_eq!(track_count(true, false), 1);
        assert_eq!(track_count(false, true), 1);
        assert_eq!(track_count(false, false), 0);
    }

    #[test]
    fn mixing_two_tracks_clips_and_pads() {
        assert_eq!(mix_tracks(&[100, 200, 300], &[1, 2]), vec![101, 202, 300]);
        assert_eq!(mix_tracks(&[30_000], &[30_000]), vec![i16::MAX]);
        assert_eq!(mix_tracks(&[-30_000], &[-30_000]), vec![i16::MIN]);
        assert!(mix_tracks(&[], &[]).is_empty());
    }
}
