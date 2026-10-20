//! The rules of the recording editor: what an edit is, how the kept parts map onto the new timeline,
//! the picture effects drawn into each frame (zoom, blur, arrows, text) and the sound handling
//! (speed change that keeps the pitch, volume). Nothing here touches Media Foundation, so it all
//! runs in tests; `export.rs` feeds it frames and sound.

#![cfg_attr(not(windows), allow(dead_code))]

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use serde::Deserialize;

use super::pure::{blend_px, zoom_blit, zoom_rect, SAMPLE_RATE};

/// A kept part must be at least this long, and so must a timed effect be (half of it).
pub const MIN_PART_MS: f64 = 100.0;
const MIN_EFFECT_MS: f64 = 50.0;
pub const SPEED_MIN: f64 = 0.25;
pub const SPEED_MAX: f64 = 4.0;
const ZOOM_LEVEL: (f64, f64) = (1.1, 6.0);
const ZOOM_EASE_MS: f64 = 300.0;
const VOLUME_MAX: f64 = 2.0;

// ---- The edit ----

/// A part of the original that is kept, played at `speed` times.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: f64,
    pub end_ms: f64,
    pub speed: f64,
}

/// Zooms toward the point `x`, `y` (0 to 1 of the frame) between the two times, easing in and out.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ZoomRegion {
    pub start_ms: f64,
    pub end_ms: f64,
    pub x: f64,
    pub y: f64,
    pub level: f64,
}

/// A rectangle (0 to 1 of the frame) that is blurred, or pixelated, between the two times.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlurRegion {
    pub start_ms: f64,
    pub end_ms: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub pixelate: bool,
}

/// An arrow from `x1, y1` to `x2, y2` (0 to 1 of the frame); `width` is a fraction of the frame height.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Arrow {
    pub start_ms: f64,
    pub end_ms: f64,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: [u8; 3],
    pub width: f64,
}

/// Text drawn by the page as a PNG at the video's own size; its top-left corner sits at `x, y`.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub start_ms: f64,
    pub end_ms: f64,
    pub x: f64,
    pub y: f64,
    pub png: Vec<u8>,
}

/// Everything the editor sends back. All times are on the original's timeline, so cutting and speed
/// changes never move an effect off the moment it was placed on.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EditSpec {
    pub duration_ms: f64,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub zooms: Vec<ZoomRegion>,
    #[serde(default)]
    pub blurs: Vec<BlurRegion>,
    #[serde(default)]
    pub arrows: Vec<Arrow>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Volume of each sound track (0 drops the track). Tracks not listed stay at 1.
    #[serde(default)]
    pub volumes: Vec<f64>,
}

impl EditSpec {
    /// A copy that keeps only `start_ms..end_ms` of the original, unchanged.
    pub fn trim(duration_ms: f64, start_ms: f64, end_ms: f64) -> Self {
        Self {
            duration_ms,
            segments: vec![Segment { start_ms, end_ms, speed: 1.0 }],
            zooms: Vec::new(),
            blurs: Vec::new(),
            arrows: Vec::new(),
            labels: Vec::new(),
            volumes: Vec::new(),
        }
    }

    /// The edit with times and positions held inside their limits, or why it cannot be used.
    pub fn check(mut self) -> Result<Self, String> {
        const INCOMPLETE: &str = "That edit is not complete, so nothing was saved.";
        let d = self.duration_ms;
        if !d.is_finite() || d < MIN_PART_MS {
            return Err(INCOMPLETE.into());
        }
        if self.segments.is_empty() {
            return Err("Keep at least one part of the video.".into());
        }
        let mut earliest = 0.0;
        for s in &mut self.segments {
            if !(s.start_ms.is_finite() && s.end_ms.is_finite() && s.speed.is_finite()) {
                return Err(INCOMPLETE.into());
            }
            s.end_ms = s.end_ms.min(d);
            if s.start_ms < earliest || s.end_ms - s.start_ms < MIN_PART_MS || !(SPEED_MIN..=SPEED_MAX).contains(&s.speed) {
                return Err("A kept part is out of order, too short, or has a speed outside 0.25x to 4x.".into());
            }
            earliest = s.end_ms;
        }
        for z in &mut self.zooms {
            span(&mut z.start_ms, &mut z.end_ms, d)?;
            z.x = unit(z.x)?;
            z.y = unit(z.y)?;
            if !z.level.is_finite() {
                return Err(INCOMPLETE.into());
            }
            z.level = z.level.clamp(ZOOM_LEVEL.0, ZOOM_LEVEL.1);
        }
        for b in &mut self.blurs {
            span(&mut b.start_ms, &mut b.end_ms, d)?;
            b.x = unit(b.x)?;
            b.y = unit(b.y)?;
            b.w = unit(b.w)?.min(1.0 - b.x);
            b.h = unit(b.h)?.min(1.0 - b.y);
            if b.w < 0.005 || b.h < 0.005 {
                return Err("A blur area is too small to use.".into());
            }
        }
        for a in &mut self.arrows {
            span(&mut a.start_ms, &mut a.end_ms, d)?;
            (a.x1, a.y1, a.x2, a.y2) = (unit(a.x1)?, unit(a.y1)?, unit(a.x2)?, unit(a.y2)?);
            if !a.width.is_finite() {
                return Err(INCOMPLETE.into());
            }
            a.width = a.width.clamp(0.002, 0.05);
        }
        for l in &mut self.labels {
            span(&mut l.start_ms, &mut l.end_ms, d)?;
            l.x = unit(l.x)?;
            l.y = unit(l.y)?;
            if l.png.is_empty() {
                return Err("A text label is empty.".into());
            }
        }
        for v in &mut self.volumes {
            if !v.is_finite() {
                return Err(INCOMPLETE.into());
            }
            *v = v.clamp(0.0, VOLUME_MAX);
        }
        Ok(self)
    }
}

/// Puts a timed effect inside `0..=duration`; an effect left with almost no time is refused.
fn span(start: &mut f64, end: &mut f64, duration: f64) -> Result<(), String> {
    if !start.is_finite() || !end.is_finite() {
        return Err("That edit is not complete, so nothing was saved.".into());
    }
    *start = start.clamp(0.0, duration);
    *end = end.clamp(0.0, duration);
    if *end - *start < MIN_EFFECT_MS {
        return Err("An effect lasts less than a twentieth of a second, so nothing was saved.".into());
    }
    Ok(())
}

/// A position in the frame, held to `0..=1`.
fn unit(v: f64) -> Result<f64, String> {
    if !v.is_finite() || !(-0.01..=1.01).contains(&v) {
        return Err("An effect sits outside the picture, so nothing was saved.".into());
    }
    Ok(v.clamp(0.0, 1.0))
}

fn active(start: f64, end: f64, t: f64) -> bool {
    t >= start && t < end
}

// ---- Timeline ----

/// How long the edited video lasts.
pub fn output_ms(segments: &[Segment]) -> f64 {
    segments.iter().map(|s| (s.end_ms - s.start_ms) / s.speed).sum()
}

/// The moment of the original that is on screen `out_ms` into the edited video; `None` past its end.
pub fn out_to_src(segments: &[Segment], out_ms: f64) -> Option<f64> {
    if out_ms < 0.0 {
        return None;
    }
    let mut left = out_ms;
    for s in segments {
        let len = (s.end_ms - s.start_ms) / s.speed;
        if left < len {
            return Some(s.start_ms + left * s.speed);
        }
        left -= len;
    }
    None
}

/// Walks the frames of the edited video in order and says which frame of the original each one shows,
/// so a cut or a speed change becomes dropped or repeated frames at a steady output rate.
pub struct FrameMapper<'a> {
    segments: &'a [Segment],
    fps: u32,
    next: u64,
    total: u64,
}

impl<'a> FrameMapper<'a> {
    pub fn new(segments: &'a [Segment], fps: u32) -> Self {
        let total = (output_ms(segments) * f64::from(fps) / 1000.0).floor() as u64;
        Self { segments, fps: fps.max(1), next: 0, total }
    }

    pub fn done(&self) -> bool {
        self.next >= self.total
    }

    /// The output frames (index and original time) that show something earlier than `limit_ms` of the
    /// original, and so belong to the source frame that ends there. Each is handed out once.
    pub fn take_until(&mut self, limit_ms: f64) -> Vec<(u64, f64)> {
        let mut out = Vec::new();
        while self.next < self.total {
            let Some(src) = out_to_src(self.segments, self.next as f64 * 1000.0 / f64::from(self.fps)) else {
                self.next = self.total;
                break;
            };
            if src >= limit_ms {
                break;
            }
            out.push((self.next, src));
            self.next += 1;
        }
        out
    }
}

// ---- Picture ----

/// Zoom level and focus (0 to 1) at the original's time `t`, easing in and out; `None` when no zoom is on.
pub fn zoom_at(zooms: &[ZoomRegion], t: f64) -> Option<(f64, f64, f64)> {
    let z = zooms.iter().find(|z| active(z.start_ms, z.end_ms, t))?;
    let ease = ZOOM_EASE_MS.min((z.end_ms - z.start_ms) / 2.0);
    let p = ((t - z.start_ms) / ease).min((z.end_ms - t) / ease).clamp(0.0, 1.0);
    Some((1.0 + (z.level - 1.0) * p * p * (3.0 - 2.0 * p), z.x, z.y))
}

/// Blurs one line of pixels (blue, green, red) with a window of `2 * r + 1`, repeating the edge pixels.
fn blur_line(line: &mut [[u8; 3]], copy: &mut Vec<[u8; 3]>, r: usize) {
    let n = line.len() as isize;
    copy.clear();
    copy.extend_from_slice(line);
    let at = |i: isize| copy[i.clamp(0, n - 1) as usize];
    let (r, win) = (r as isize, (2 * r + 1) as u32);
    let mut sum = [0u32; 3];
    for k in -r..=r {
        let p = at(k);
        for c in 0..3 {
            sum[c] += u32::from(p[c]);
        }
    }
    for i in 0..n {
        for c in 0..3 {
            line[i as usize][c] = ((sum[c] + win / 2) / win) as u8;
        }
        let (add, sub) = (at(i + r + 1), at(i - r));
        for c in 0..3 {
            sum[c] = sum[c] + u32::from(add[c]) - u32::from(sub[c]);
        }
    }
}

/// A box blur (two passes each way) of the rectangle `x0..x1` by `y0..y1` of a BGRA frame `w` wide.
pub fn blur_rect(buf: &mut [u8], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize), radius: usize) {
    if x1 <= x0 + 1 || y1 <= y0 + 1 || radius == 0 {
        return;
    }
    let mut line: Vec<[u8; 3]> = Vec::new();
    let mut copy = Vec::new();
    for _ in 0..2 {
        for y in y0..y1 {
            line.clear();
            line.extend((x0..x1).map(|x| {
                let i = (y * w + x) * 4;
                [buf[i], buf[i + 1], buf[i + 2]]
            }));
            blur_line(&mut line, &mut copy, radius);
            for (k, p) in line.iter().enumerate() {
                let i = (y * w + x0 + k) * 4;
                buf[i..i + 3].copy_from_slice(p);
            }
        }
        for x in x0..x1 {
            line.clear();
            line.extend((y0..y1).map(|y| {
                let i = (y * w + x) * 4;
                [buf[i], buf[i + 1], buf[i + 2]]
            }));
            blur_line(&mut line, &mut copy, radius);
            for (k, p) in line.iter().enumerate() {
                let i = ((y0 + k) * w + x) * 4;
                buf[i..i + 3].copy_from_slice(p);
            }
        }
    }
}

/// Replaces each `block` by `block` square of the rectangle with its average colour.
pub fn pixelate_rect(buf: &mut [u8], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize), block: usize) {
    let block = block.max(2);
    for by in (y0..y1).step_by(block) {
        for bx in (x0..x1).step_by(block) {
            let (ex, ey) = ((bx + block).min(x1), (by + block).min(y1));
            let mut sum = [0u64; 3];
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * w + x) * 4;
                    for c in 0..3 {
                        sum[c] += u64::from(buf[i + c]);
                    }
                }
            }
            let n = ((ex - bx) * (ey - by)) as u64;
            for y in by..ey {
                for x in bx..ex {
                    let i = (y * w + x) * 4;
                    for c in 0..3 {
                        buf[i + c] = (sum[c] / n) as u8;
                    }
                }
            }
        }
    }
}

/// An arrow with a filled head at `to`, drawn into a BGRA frame with soft edges.
pub fn draw_arrow(buf: &mut [u8], w: i32, h: i32, from: (f64, f64), to: (f64, f64), width: f64, rgb: [u8; 3]) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = dx.hypot(dy);
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (width * 4.0).max(12.0).min(len);
    let half_head = head * 0.55;
    let base = (to.0 - ux * head, to.1 - uy * head);
    let pad = head + width + 2.0;
    let (min_x, max_x) = (from.0.min(to.0) - pad, from.0.max(to.0) + pad);
    let (min_y, max_y) = (from.1.min(to.1) - pad, from.1.max(to.1) + pad);
    for y in (min_y.floor().max(0.0) as i32)..=(max_y.ceil().min(f64::from(h - 1)) as i32) {
        for x in (min_x.floor().max(0.0) as i32)..=(max_x.ceil().min(f64::from(w - 1)) as i32) {
            let (px, py) = (f64::from(x) + 0.5 - base.0, f64::from(y) + 0.5 - base.1);
            let along = px * ux + py * uy;
            let across = (px * -uy + py * ux).abs();
            // The shaft runs from the start to the base of the head; the head is a triangle.
            let shaft = if len > head {
                let (qx, qy) = (f64::from(x) + 0.5 - from.0, f64::from(y) + 0.5 - from.1);
                let t = (qx * ux + qy * uy).clamp(0.0, len - head);
                (width / 2.0 + 0.5 - (qx - ux * t).hypot(qy - uy * t)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let tri = if along > -0.5 && along < head + 0.5 {
                let limit = half_head * (1.0 - along.max(0.0) / head);
                (limit - across + 0.5).clamp(0.0, 1.0) * (along + 0.5).clamp(0.0, 1.0) * (head - along + 0.5).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cover = shaft.max(tri);
            if cover > 0.0 {
                blend_px(buf, w, h, x, y, rgb, cover as f32);
            }
        }
    }
}

/// Lays an RGBA picture over a BGRA frame with its top-left corner at `dx, dy`.
pub fn blend_rgba(buf: &mut [u8], w: usize, h: usize, img: &[u8], iw: usize, ih: usize, dx: i64, dy: i64) {
    for iy in 0..ih {
        let y = dy + iy as i64;
        if y < 0 || y >= h as i64 {
            continue;
        }
        for ix in 0..iw {
            let x = dx + ix as i64;
            if x < 0 || x >= w as i64 {
                continue;
            }
            let s = (iy * iw + ix) * 4;
            let a = u32::from(img[s + 3]);
            if a == 0 {
                continue;
            }
            let d = (y as usize * w + x as usize) * 4;
            for (c, from) in [(0, s + 2), (1, s + 1), (2, s)] {
                let old = u32::from(buf[d + c]);
                buf[d + c] = ((old * (255 - a) + u32::from(img[from]) * a + 127) / 255) as u8;
            }
        }
    }
}

/// A text label ready to draw.
pub struct LabelImage {
    start_ms: f64,
    end_ms: f64,
    x: f64,
    y: f64,
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

pub fn decode_labels(spec: &EditSpec) -> Result<Vec<LabelImage>, String> {
    spec.labels
        .iter()
        .map(|l| -> Result<LabelImage, String> {
            let img = image::load_from_memory_with_format(&l.png, image::ImageFormat::Png)
                .map_err(|_| "A text label could not be read, so nothing was saved.".to_string())?
                .to_rgba8();
            Ok(LabelImage { start_ms: l.start_ms, end_ms: l.end_ms, x: l.x, y: l.y, w: img.width() as usize, h: img.height() as usize, rgba: img.into_raw() })
        })
        .collect()
}

/// The pixel box of a 0 to 1 rectangle in a `w` by `h` frame, held inside it.
fn pixel_box(x: f64, y: f64, bw: f64, bh: f64, w: usize, h: usize) -> (usize, usize, usize, usize) {
    let edge = |v: f64, n: usize| ((v * n as f64).round().max(0.0) as usize).min(n);
    (edge(x, w), edge(y, h), edge(x + bw, w), edge(y + bh, h))
}

/// Draws the effects that are on at the original's time `t` into a BGRA frame: blurs, arrows, text,
/// then the zoom over the lot. `scratch` is working space of any size.
pub fn render_frame(spec: &EditSpec, labels: &[LabelImage], frame: &mut Vec<u8>, scratch: &mut Vec<u8>, w: usize, h: usize, t: f64) {
    for b in spec.blurs.iter().filter(|b| active(b.start_ms, b.end_ms, t)) {
        let rect = pixel_box(b.x, b.y, b.w, b.h, w, h);
        if b.pixelate {
            pixelate_rect(frame, w, rect, ((rect.2 - rect.0).min(rect.3 - rect.1) / 14).max(6));
        } else {
            blur_rect(frame, w, rect, (w.min(h) / 60).max(3));
        }
    }
    let (fw, fh) = (w as f64, h as f64);
    for a in spec.arrows.iter().filter(|a| active(a.start_ms, a.end_ms, t)) {
        draw_arrow(frame, w as i32, h as i32, (a.x1 * fw, a.y1 * fh), (a.x2 * fw, a.y2 * fh), a.width * fh, a.color);
    }
    for l in labels.iter().filter(|l| active(l.start_ms, l.end_ms, t)) {
        blend_rgba(frame, w, h, &l.rgba, l.w, l.h, (l.x * fw).round() as i64, (l.y * fh).round() as i64);
    }
    if let Some((level, cx, cy)) = zoom_at(&spec.zooms, t) {
        if level > 1.001 {
            scratch.resize(frame.len(), 0);
            zoom_blit(frame, w, h, zoom_rect(fw, fh, level, cx * fw, cy * fh), scratch);
            std::mem::swap(frame, scratch);
        }
    }
}

/// Copies the rows of a decoded frame into a top-down BGRA buffer. A negative `stride` means the
/// picture is stored bottom-up. `false` when the data is too short for the size.
pub fn rows_to_bgra(src: &[u8], stride: i32, w: usize, h: usize, out: &mut Vec<u8>) -> bool {
    let step = stride.unsigned_abs() as usize;
    if step < w * 4 || src.len() < step * h {
        return false;
    }
    out.clear();
    for row in 0..h {
        let from = if stride < 0 { (h - 1 - row) * step } else { row * step };
        out.extend_from_slice(&src[from..from + w * 4]);
    }
    true
}

// ---- Sound ----

/// One sound track of the original at 48 kHz, 16-bit stereo, laid out by the time each piece was
/// stamped with, so a gap in the file stays a gap.
#[derive(Default)]
pub struct TrackBuf {
    pub pcm: Vec<i16>,
}

/// Nothing a recording can hold lies beyond this; a stamp past it is damaged data.
const MAX_TRACK_FRAMES: usize = SAMPLE_RATE as usize * 3600 * 12;

impl TrackBuf {
    pub fn place(&mut self, ts_100ns: i64, data: &[i16]) {
        let frame = (ts_100ns.max(0) as u128 * u128::from(SAMPLE_RATE) / 10_000_000) as usize;
        if frame > MAX_TRACK_FRAMES {
            return;
        }
        let (start, end) = (frame * 2, frame * 2 + data.len());
        if self.pcm.len() < end {
            self.pcm.resize(end, 0);
        }
        self.pcm[start..end].copy_from_slice(data);
    }

    fn slice(&self, start_ms: f64, end_ms: f64) -> &[i16] {
        let at = |ms: f64| ((ms * f64::from(SAMPLE_RATE) / 1000.0).round() as usize * 2).min(self.pcm.len());
        let (a, b) = (at(start_ms), at(end_ms));
        if a >= b {
            &[]
        } else {
            &self.pcm[a..b]
        }
    }
}

/// Speeds interleaved stereo sound up or slows it down by `speed` without changing its pitch
/// (waveform-similarity overlap-add). The result lasts `len / speed`.
pub fn stretch(pcm: &[i16], speed: f64) -> Vec<i16> {
    const GRAIN: usize = 2048;
    const HOP: usize = GRAIN / 2;
    const SEARCH: usize = 256;
    const MATCH: usize = 512;
    let n = pcm.len() / 2;
    let out_len = (n as f64 / speed).round() as usize;
    if n == 0 || out_len == 0 {
        return Vec::new();
    }
    let padded = n + GRAIN + 2 * SEARCH + MATCH + HOP;
    let mut input = pcm.to_vec();
    input.resize(padded * 2, 0);
    let mono: Vec<f32> = (0..padded).map(|i| (f32::from(input[i * 2]) + f32::from(input[i * 2 + 1])) * 0.5).collect();
    let window: Vec<f32> = (0..GRAIN).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / GRAIN as f32).cos()).collect();
    let mut acc = vec![0f32; (out_len + GRAIN) * 2];
    let (mut prev, mut k) = (0usize, 0usize);
    while k * HOP < out_len {
        let pos = if k == 0 {
            0
        } else {
            let nominal = (k as f64 * HOP as f64 * speed).round() as usize;
            // What would follow the previous grain unchanged; pick the nearby grain start that joins it best.
            let want = prev + HOP;
            let (lo, hi) = (nominal.saturating_sub(SEARCH), (nominal + SEARCH).min(n + SEARCH));
            let mut best = (lo, f32::MAX);
            for c in lo..=hi {
                let miss: f32 = (0..MATCH).step_by(2).map(|j| (mono[c + j] - mono[want + j]).abs()).sum();
                if miss < best.1 {
                    best = (c, miss);
                }
            }
            best.0
        };
        for i in 0..GRAIN {
            // The very first grain starts at full level so the sound does not fade in.
            let gain = if k == 0 && i < HOP { 1.0 } else { window[i] };
            let (o, s) = ((k * HOP + i) * 2, (pos + i) * 2);
            acc[o] += gain * f32::from(input[s]);
            acc[o + 1] += gain * f32::from(input[s + 1]);
        }
        prev = pos;
        k += 1;
    }
    acc[..out_len * 2].iter().map(|v| v.round().clamp(-32768.0, 32767.0) as i16).collect()
}

/// The sound of the edited video for one track: each kept part, sped up or slowed down, then set to
/// `volume`. Parts the file has no sound for are silent, so the track stays in step with the picture.
pub fn build_track(track: &TrackBuf, segments: &[Segment], volume: f64) -> Vec<i16> {
    let mut out = Vec::new();
    for s in segments {
        let frames = ((s.end_ms - s.start_ms) * f64::from(SAMPLE_RATE) / 1000.0).round() as usize;
        let mut part = track.slice(s.start_ms, s.end_ms).to_vec();
        part.resize(frames * 2, 0);
        if (s.speed - 1.0).abs() < 1e-3 {
            out.extend(part);
        } else {
            out.extend(stretch(&part, s.speed));
        }
    }
    if (volume - 1.0).abs() > 1e-3 {
        for v in &mut out {
            *v = (f64::from(*v) * volume).round().clamp(-32768.0, 32767.0) as i16;
        }
    }
    out
}

/// `n` frames of stereo sound from `start` on, padded with silence past the end of the track.
pub fn chunk(track: &[i16], start: usize, n: usize) -> Vec<i16> {
    let mut out: Vec<i16> = track.get(start * 2..).unwrap_or(&[]).iter().take(n * 2).copied().collect();
    out.resize(n * 2, 0);
    out
}

// ---- MP4 sound tracks ----

/// The boxes directly inside `data` with this name, as their contents.
fn boxes<'a>(data: &'a [u8], name: &[u8; 4]) -> Vec<&'a [u8]> {
    let mut found = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if size < 8 || pos + size > data.len() {
            break;
        }
        if &data[pos + 4..pos + 8] == name {
            found.push(&data[pos + 8..pos + size]);
        }
        pos += size;
    }
    found
}

fn count_sound_tracks(moov: &[u8]) -> usize {
    boxes(moov, b"trak")
        .into_iter()
        .flat_map(|t| boxes(t, b"mdia"))
        .flat_map(|m| boxes(m, b"hdlr"))
        // Version and flags, a reserved word, then the four-letter kind of the track.
        .filter(|h| h.len() >= 12 && &h[8..12] == b"soun")
        .count()
}

/// How many sound tracks an MP4 file holds (0 when it has none or cannot be read).
pub fn sound_tracks(path: &Path) -> usize {
    const MAX_MOOV: u64 = 64 * 1024 * 1024;
    let Ok(mut file) = File::open(path) else { return 0 };
    let Ok(len) = file.metadata().map(|m| m.len()) else { return 0 };
    let mut pos = 0u64;
    while pos + 8 <= len {
        let mut head = [0u8; 16];
        if file.seek(SeekFrom::Start(pos)).is_err() || file.read(&mut head).is_err() {
            return 0;
        }
        let mut size = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let mut header = 8;
        if size == 1 {
            size = u64::from_be_bytes(head[8..16].try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = len - pos;
        }
        if size < header || pos + size > len {
            return 0;
        }
        if &head[4..8] == b"moov" {
            if size - header > MAX_MOOV {
                return 0;
            }
            let mut body = vec![0u8; (size - header) as usize];
            if file.seek(SeekFrom::Start(pos + header)).and_then(|_| file.read_exact(&mut body)).is_err() {
                return 0;
            }
            return count_sound_tracks(&body);
        }
        pos += size;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: f64, b: f64, speed: f64) -> Segment {
        Segment { start_ms: a, end_ms: b, speed }
    }

    #[test]
    fn output_length_follows_cuts_and_speed() {
        assert_eq!(output_ms(&[seg(0.0, 10_000.0, 1.0)]), 10_000.0);
        // A middle section cut out: 0 to 2 s and 5 to 8 s remain.
        assert_eq!(output_ms(&[seg(0.0, 2000.0, 1.0), seg(5000.0, 8000.0, 1.0)]), 5000.0);
        assert_eq!(output_ms(&[seg(0.0, 4000.0, 2.0)]), 2000.0);
        assert_eq!(output_ms(&[seg(0.0, 1000.0, 0.5)]), 2000.0);
    }

    #[test]
    fn edited_time_maps_back_to_the_original() {
        let parts = [seg(0.0, 2000.0, 1.0), seg(5000.0, 9000.0, 2.0)];
        assert_eq!(out_to_src(&parts, 0.0), Some(0.0));
        assert_eq!(out_to_src(&parts, 1500.0), Some(1500.0));
        // The second part starts 2 s in; at 2x, 1 s of output is 2 s of original.
        assert_eq!(out_to_src(&parts, 2000.0), Some(5000.0));
        assert_eq!(out_to_src(&parts, 3000.0), Some(7000.0));
        assert_eq!(out_to_src(&parts, 4000.0), None);
        assert_eq!(out_to_src(&parts, -1.0), None);
    }

    #[test]
    fn frames_of_a_cut_video_skip_the_cut_and_repeat_when_slowed() {
        let parts = [seg(0.0, 100.0, 1.0), seg(1000.0, 1100.0, 1.0)];
        let mut m = FrameMapper::new(&parts, 30);
        // 200 ms at 30 fps is 6 frames; the first three show 0 to 100 ms, the rest 1000 to 1100 ms.
        let all = m.take_until(f64::INFINITY);
        assert_eq!(all.len(), 6);
        assert!(all[..3].iter().all(|(_, s)| *s < 100.0));
        assert!(all[3..].iter().all(|(_, s)| *s >= 1000.0));
        assert!(m.done());

        // Slowed to half speed, 100 ms of original lasts 200 ms and the frames repeat.
        let slow = [seg(0.0, 200.0, 0.5)];
        let mut m = FrameMapper::new(&slow, 30);
        assert_eq!(m.take_until(f64::INFINITY).len(), 12);
    }

    #[test]
    fn frames_are_handed_out_as_source_frames_arrive() {
        let parts = [seg(0.0, 1000.0, 1.0)];
        let mut m = FrameMapper::new(&parts, 30);
        // Output frames sit at 0, 33.3, 66.7 ms; a source frame that ends at 33 ms owns only the first.
        let a = m.take_until(33.0);
        assert_eq!(a.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0]);
        let b = m.take_until(66.0);
        assert_eq!(b.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1]);
        assert!(m.take_until(66.0).is_empty());
    }

    #[test]
    fn a_zoom_eases_in_and_out() {
        let z = [ZoomRegion { start_ms: 1000.0, end_ms: 3000.0, x: 0.25, y: 0.75, level: 3.0 }];
        assert_eq!(zoom_at(&z, 500.0), None);
        assert_eq!(zoom_at(&z, 3000.0), None);
        assert_eq!(zoom_at(&z, 1000.0), Some((1.0, 0.25, 0.75)));
        let (mid, x, y) = zoom_at(&z, 2000.0).unwrap();
        assert!((mid - 3.0).abs() < 1e-9 && x == 0.25 && y == 0.75);
        let (early, ..) = zoom_at(&z, 1150.0).unwrap();
        assert!(early > 1.0 && early < 3.0);
        // A short zoom never reaches its full level, but still returns to 1 at its end.
        let short = [ZoomRegion { start_ms: 0.0, end_ms: 200.0, x: 0.5, y: 0.5, level: 2.0 }];
        assert!((zoom_at(&short, 100.0).unwrap().0 - 2.0).abs() < 1e-9);
        assert!(zoom_at(&short, 199.0).unwrap().0 < 1.01);
    }

    #[test]
    fn checks_refuse_an_incomplete_edit() {
        let ok = EditSpec::trim(10_000.0, 1000.0, 4000.0);
        assert!(ok.clone().check().is_ok());
        assert!(EditSpec::trim(10_000.0, 1000.0, 1050.0).check().is_err());
        assert!(EditSpec::trim(10_000.0, f64::NAN, 4000.0).check().is_err());
        let mut bad_speed = ok.clone();
        bad_speed.segments[0].speed = 8.0;
        assert!(bad_speed.check().is_err());
        let mut overlap = ok.clone();
        overlap.segments.push(seg(2000.0, 6000.0, 1.0));
        assert!(overlap.check().is_err());
        let mut none = ok.clone();
        none.segments.clear();
        assert!(none.check().is_err());
        // An end past the clip is held to its length.
        let held = EditSpec::trim(10_000.0, 9000.0, 99_000.0).check().unwrap();
        assert_eq!(held.segments[0].end_ms, 10_000.0);
    }

    #[test]
    fn checks_hold_effects_inside_the_picture_and_the_clip() {
        let mut spec = EditSpec::trim(10_000.0, 0.0, 10_000.0);
        spec.zooms.push(ZoomRegion { start_ms: -500.0, end_ms: 99_000.0, x: 0.5, y: 0.5, level: 50.0 });
        spec.blurs.push(BlurRegion { start_ms: 0.0, end_ms: 1000.0, x: 0.8, y: 0.1, w: 0.5, h: 0.2, pixelate: false });
        let spec = spec.check().unwrap();
        assert_eq!((spec.zooms[0].start_ms, spec.zooms[0].end_ms, spec.zooms[0].level), (0.0, 10_000.0, ZOOM_LEVEL.1));
        assert!((spec.blurs[0].w - 0.2).abs() < 1e-9);

        let mut off = EditSpec::trim(10_000.0, 0.0, 10_000.0);
        off.arrows.push(Arrow { start_ms: 0.0, end_ms: 1000.0, x1: 2.0, y1: 0.0, x2: 0.5, y2: 0.5, color: [255, 0, 0], width: 0.01 });
        assert!(off.check().is_err());
        let mut brief = EditSpec::trim(10_000.0, 0.0, 10_000.0);
        brief.labels.push(Label { start_ms: 100.0, end_ms: 110.0, x: 0.1, y: 0.1, png: vec![1] });
        assert!(brief.check().is_err());
    }

    fn frame(w: usize, h: usize, v: u8) -> Vec<u8> {
        vec![v; w * h * 4]
    }

    #[test]
    fn blur_smooths_only_inside_the_rectangle() {
        let (w, h) = (20, 20);
        let mut f = frame(w, h, 0);
        for y in 0..h {
            for x in 10..w {
                f[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(200);
            }
        }
        blur_rect(&mut f, w, (5, 5, 15, 15), 2);
        let px = |x: usize, y: usize| f[(y * w + x) * 4];
        assert!(px(10, 10) > 0 && px(10, 10) < 200, "edge softened, got {}", px(10, 10));
        assert_eq!(px(2, 2), 0);
        assert_eq!(px(18, 18), 200);
        assert_eq!(px(10, 3), 200);
    }

    #[test]
    fn pixelate_averages_blocks() {
        let (w, h) = (8, 2);
        let mut f = frame(w, h, 0);
        f[0] = 100;
        f[4] = 200;
        pixelate_rect(&mut f, w, (0, 0, 8, 1), 4);
        assert_eq!(f[0], 75);
        assert_eq!(f[3 * 4], 75);
        assert_eq!(f[4 * 4], 0);
        assert_eq!(f[w * 4], 0);
    }

    #[test]
    fn arrows_mark_their_line_and_head() {
        let (w, h) = (100, 40);
        let mut f = frame(w, h, 0);
        draw_arrow(&mut f, w as i32, h as i32, (10.0, 20.0), (80.0, 20.0), 4.0, [255, 0, 0]);
        let red = |x: usize, y: usize| f[(y * w + x) * 4 + 2];
        assert!(red(40, 20) > 200, "shaft");
        assert!(red(78, 20) > 200, "tip");
        assert_eq!(red(40, 5), 0);
        assert!(red(70, 24) > 100, "head is wider than the shaft");
        assert_eq!(red(90, 20), 0);
    }

    #[test]
    fn pictures_blend_by_their_alpha_and_stay_inside() {
        let (w, h) = (4, 4);
        let mut f = frame(w, h, 0);
        // One opaque red pixel and one half-transparent white pixel, placed with part off the frame.
        let img = [255, 0, 0, 255, 255, 255, 255, 128];
        blend_rgba(&mut f, w, h, &img, 2, 1, 3, 0);
        assert_eq!(&f[(0 * w + 3) * 4..(0 * w + 3) * 4 + 3], &[0, 0, 255]);
        blend_rgba(&mut f, w, h, &img, 2, 1, -1, 1);
        let p = (1 * w) * 4;
        assert!((i32::from(f[p]) - 128).abs() <= 1);
    }

    #[test]
    fn zoom_crops_toward_the_focus() {
        let (w, h) = (16, 16);
        let mut spec = EditSpec::trim(10_000.0, 0.0, 10_000.0);
        spec.zooms.push(ZoomRegion { start_ms: 0.0, end_ms: 4000.0, x: 0.0, y: 0.0, level: 2.0 });
        let mut f = frame(w, h, 10);
        for y in 0..h / 2 {
            for x in 0..w / 2 {
                f[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(250);
            }
        }
        let mut scratch = Vec::new();
        render_frame(&spec, &[], &mut f, &mut scratch, w, h, 2000.0);
        // Zoomed 2x toward the top-left corner, the bright quarter fills the whole frame.
        assert!(f[(h - 1) * w * 4] > 150 && f[(w - 1) * 4] > 150 && f[(8 * w + 8) * 4] > 240);
        let mut early = frame(w, h, 10);
        render_frame(&spec, &[], &mut early, &mut scratch, w, h, 5000.0);
        assert_eq!(early, frame(w, h, 10));
    }

    #[test]
    fn decoded_rows_follow_the_stride_direction() {
        // Two rows of two pixels; the first value of each row tells them apart.
        let top_down: Vec<u8> = vec![1, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0];
        let mut out = Vec::new();
        assert!(rows_to_bgra(&top_down, 8, 2, 2, &mut out));
        assert_eq!((out[0], out[8]), (1, 2));
        assert!(rows_to_bgra(&top_down, -8, 2, 2, &mut out));
        assert_eq!((out[0], out[8]), (2, 1));
        // Rows with padding at the end are trimmed to the picture.
        let padded: Vec<u8> = (0..24).collect();
        assert!(rows_to_bgra(&padded, 12, 2, 2, &mut out));
        assert_eq!(out.len(), 16);
        assert_eq!(out[8], 12);
        assert!(!rows_to_bgra(&top_down, 8, 2, 3, &mut out));
        assert!(!rows_to_bgra(&top_down, 4, 2, 2, &mut out));
    }

    fn sine(frames: usize, hz: f64) -> Vec<i16> {
        (0..frames).flat_map(|i| {
            let v = (8000.0 * (std::f64::consts::TAU * hz * i as f64 / f64::from(SAMPLE_RATE)).sin()) as i16;
            [v, v]
        }).collect()
    }

    fn crossings(pcm: &[i16]) -> usize {
        pcm.chunks_exact(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|p| (p[0] < 0) != (p[1] < 0)).count()
    }

    fn rms(pcm: &[i16]) -> f64 {
        (pcm.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt()
    }

    #[test]
    fn stretching_changes_the_length_and_keeps_the_pitch() {
        let input = sine(48_000, 440.0);
        for speed in [2.0, 0.5, 1.5] {
            let out = stretch(&input, speed);
            let frames = out.len() / 2;
            assert_eq!(frames, (48_000.0 / speed).round() as usize);
            // Zero crossings a second stay about the same, so the pitch did not move.
            let rate = crossings(&out) as f64 / (frames as f64 / 48_000.0);
            assert!((rate - 880.0).abs() < 880.0 * 0.08, "speed {speed}: {rate} crossings a second");
            assert!((rms(&out) / rms(&input) - 1.0).abs() < 0.2, "speed {speed}: level changed");
        }
        assert!(stretch(&[], 2.0).is_empty());
    }

    #[test]
    fn a_track_is_cut_sped_and_padded_to_match_the_picture() {
        let mut track = TrackBuf::default();
        track.place(0, &sine(48_000, 440.0));
        let parts = [seg(0.0, 250.0, 1.0), seg(500.0, 1000.0, 2.0)];
        let out = build_track(&track, &parts, 1.0);
        // 250 ms plus 500 ms / 2 is 500 ms of sound.
        assert_eq!(out.len() / 2, 24_000);
        // Volume scales and clips.
        let quiet = build_track(&track, &[seg(0.0, 100.0, 1.0)], 0.5);
        let loud = build_track(&track, &[seg(0.0, 100.0, 1.0)], 2.0);
        assert!((rms(&quiet) * 4.0 - rms(&loud)).abs() < rms(&loud) * 0.02);
        // A part past the end of the track is silent but still takes its time.
        let past = build_track(&track, &[seg(2000.0, 2500.0, 1.0)], 1.0);
        assert_eq!(past.len() / 2, 24_000);
        assert!(past.iter().all(|v| *v == 0));
    }

    #[test]
    fn pieces_are_placed_by_their_time() {
        let mut t = TrackBuf::default();
        t.place(10_000_000, &[5, 5, 6, 6]);
        assert_eq!(t.pcm.len(), 48_000 * 2 + 4);
        assert_eq!(&t.pcm[96_000..], &[5, 5, 6, 6]);
        assert!(t.pcm[..96_000].iter().all(|v| *v == 0));
        t.place(i64::MAX, &[1, 1]);
        assert_eq!(t.pcm.len(), 96_004);
        assert_eq!(chunk(&[1, 2, 3, 4], 1, 3), vec![3, 4, 0, 0, 0, 0]);
        assert_eq!(chunk(&[1, 2], 5, 1), vec![0, 0]);
    }

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(body);
        b
    }

    fn track_of(kind: &[u8; 4]) -> Vec<u8> {
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(kind);
        hdlr.extend_from_slice(&[0; 12]);
        mp4_box(b"trak", &mp4_box(b"mdia", &mp4_box(b"hdlr", &hdlr)))
    }

    #[test]
    fn sound_tracks_are_counted_from_the_file_header() {
        let moov = mp4_box(b"moov", &[track_of(b"vide"), track_of(b"soun"), track_of(b"soun")].concat());
        let mut file = mp4_box(b"ftyp", b"isom0000");
        file.extend(mp4_box(b"mdat", &[0u8; 64]));
        file.extend(&moov);
        let path = std::env::temp_dir().join(format!("notifi-tracks-{}.mp4", std::process::id()));
        std::fs::write(&path, &file).unwrap();
        assert_eq!(sound_tracks(&path), 2);
        let _ = std::fs::remove_file(&path);
        assert_eq!(sound_tracks(Path::new("Z:/no-such-folder/none.mp4")), 0);
        assert_eq!(count_sound_tracks(&track_of(b"vide")), 0);
    }
}
