//! Pure capture logic: selector geometry, file names, text joining, QR links, OCR scaling.

use std::path::{Path, PathBuf};

use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let (x, y) = (self.x.max(other.x), self.y.max(other.y));
        let (r, b) = (self.right().min(other.right()), self.bottom().min(other.bottom()));
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// The whole-pixel box that covers two corner points given in fractional pixels: left and top round
/// down, right and bottom round up, so the cut always matches the source pixels.
pub fn box_from_corners(a: (f64, f64), b: (f64, f64)) -> Rect {
    let left = a.0.min(b.0).floor();
    let top = a.1.min(b.1).floor();
    let right = a.0.max(b.0).ceil();
    let bottom = a.1.max(b.1).ceil();
    Rect::new(left as i32, top as i32, (right - left) as i32, (bottom - top) as i32)
}

/// The window drawn on top at a point. `windows` is ordered from the top of the stack down.
pub fn topmost_at(windows: &[Rect], x: i32, y: i32) -> Option<Rect> {
    windows.iter().find(|w| w.contains(x, y)).copied()
}

/// Position in `windows` of the window drawn on top at a point.
pub fn topmost_index(windows: &[Rect], x: i32, y: i32) -> Option<usize> {
    windows.iter().position(|w| w.contains(x, y))
}

/// The monitor holding a point, or the closest one when the point is in a gap between monitors.
pub fn monitor_at(monitors: &[Rect], x: i32, y: i32) -> Option<Rect> {
    if let Some(m) = monitors.iter().find(|m| m.contains(x, y)) {
        return Some(*m);
    }
    let distance = |m: &Rect| {
        let dx = (m.x - x).max(0).max(x - (m.right() - 1)) as i64;
        let dy = (m.y - y).max(0).max(y - (m.bottom() - 1)) as i64;
        dx * dx + dy * dy
    };
    monitors.iter().min_by_key(|m| distance(m)).copied()
}

/// Where a capture was asked for, in pixels of the selector window.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Area { ax: f64, ay: f64, bx: f64, by: f64 },
    Window { x: f64, y: f64 },
    Full { x: f64, y: f64 },
}

/// The box a target means in screen pixels, given the windows (top first) and monitors on screen
/// and where the selector window sits on the virtual desktop.
pub fn resolve_target(target: Target, origin: (i32, i32), windows: &[Rect], monitors: &[Rect]) -> Option<Rect> {
    let (ox, oy) = origin;
    let rect = match target {
        Target::Area { ax, ay, bx, by } => box_from_corners((ax + f64::from(ox), ay + f64::from(oy)), (bx + f64::from(ox), by + f64::from(oy))),
        Target::Window { x, y } => topmost_at(windows, x.floor() as i32 + ox, y.floor() as i32 + oy)?,
        Target::Full { x, y } => monitor_at(monitors, x.floor() as i32 + ox, y.floor() as i32 + oy)?,
    };
    (!rect.is_empty()).then_some(rect)
}

// ---- Screen frames ----

/// A snapshot of part of the virtual desktop in physical pixels, RGBA, top row first.
pub struct Frame {
    pub origin: (i32, i32),
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn bounds(&self) -> Rect {
        Rect::new(self.origin.0, self.origin.1, self.w as i32, self.h as i32)
    }

    /// The part of the frame inside `r` (screen pixels), clipped to the frame.
    pub fn crop(&self, r: &Rect) -> Option<RgbaImage> {
        let r = r.intersect(&self.bounds())?;
        let (x0, y0) = ((r.x - self.origin.0) as usize, (r.y - self.origin.1) as usize);
        let (w, h) = (r.w as usize, r.h as usize);
        let stride = self.w as usize * 4;
        let mut out = Vec::with_capacity(w * h * 4);
        for row in y0..y0 + h {
            let start = row * stride + x0 * 4;
            out.extend_from_slice(self.rgba.get(start..start + w * 4)?);
        }
        RgbaImage::from_raw(w as u32, h as u32, out)
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        let (lx, ly) = (x - self.origin.0, y - self.origin.1);
        if lx < 0 || ly < 0 || lx >= self.w as i32 || ly >= self.h as i32 {
            return None;
        }
        let i = (ly as usize * self.w as usize + lx as usize) * 4;
        Some([self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]])
    }
}

// ---- File names ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// UTC date and time of a Unix timestamp.
pub fn civil_from_unix(secs: i64) -> DateParts {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    DateParts { year, month, day, hour: (rem / 3600) as u32, minute: (rem % 3600 / 60) as u32, second: (rem % 60) as u32 }
}

/// A file name (no extension) from the template. `{date}` is 2026-10-20, `{time}` is 14.05.09
/// (colons are not allowed in names); `{year}`, `{month}`, `{day}`, `{hour}`, `{minute}` and
/// `{second}` give the single parts.
pub fn file_stem(template: &str, d: &DateParts) -> String {
    let text = template
        .replace("{date}", &format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))
        .replace("{time}", &format!("{:02}.{:02}.{:02}", d.hour, d.minute, d.second))
        .replace("{year}", &format!("{:04}", d.year))
        .replace("{month}", &format!("{:02}", d.month))
        .replace("{day}", &format!("{:02}", d.day))
        .replace("{hour}", &format!("{:02}", d.hour))
        .replace("{minute}", &format!("{:02}", d.minute))
        .replace("{second}", &format!("{:02}", d.second));
    let cleaned: String = text.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '-' } else { c }).collect();
    let trimmed: String = cleaned.trim().trim_end_matches('.').trim().chars().take(120).collect();
    if trimmed.is_empty() {
        "Screenshot".into()
    } else {
        trimmed
    }
}

/// `dir/stem.ext`, or `dir/stem (2).ext` and so on when that name is taken.
pub fn unique_path(dir: &Path, stem: &str, ext: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !exists(&first) {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !exists(p))
        .expect("an unused name exists")
}

// ---- Text ----

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xFF00..=0xFFEF)
}

/// Recognised lines as the text to copy. Joining merges them into one paragraph: no space between
/// two CJK characters, a normal space otherwise, and a hyphen at a line end is dropped when the
/// word carries on in lower case. Without joining, line breaks are kept or turned into spaces.
pub fn join_text(lines: &[String], join: bool, keep_breaks: bool) -> String {
    let lines: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if join {
        let mut out = String::new();
        for line in lines {
            let (Some(last), Some(first)) = (out.chars().last(), line.chars().next()) else {
                out.push_str(line);
                continue;
            };
            let word_hyphen = last == '-' && out.chars().rev().nth(1).is_some_and(char::is_alphabetic) && first.is_lowercase();
            if word_hyphen {
                out.pop();
            } else if !(is_cjk(last) && is_cjk(first)) {
                out.push(' ');
            }
            out.push_str(line);
        }
        out
    } else if keep_breaks {
        lines.join("\n")
    } else {
        lines.join(" ")
    }
}

/// The text when it is a single web link and nothing else.
pub fn web_link(text: &str) -> Option<String> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    ((lower.starts_with("http://") || lower.starts_with("https://")) && !t.chars().any(char::is_whitespace) && t.len() > 8).then(|| t.to_string())
}

/// A short form of copied text for a notice.
pub fn shorten(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}...", flat.chars().take(max).collect::<String>().trim_end())
    }
}

/// How much to scale an image before recognition: small areas grow so the text is big enough, and
/// nothing goes past the engine's largest side.
pub fn ocr_scale(w: u32, h: u32, max_side: u32) -> f64 {
    const WANT: f64 = 160.0;
    let short = f64::from(w.min(h)).max(1.0);
    let mut scale = if short < WANT { (WANT / short).min(4.0) } else { 1.0 };
    let long = f64::from(w.max(h));
    if long * scale > f64::from(max_side) {
        scale = f64::from(max_side) / long;
    }
    scale
}

/// Greys the image, flips light-on-dark text to dark-on-light and stretches the contrast.
pub fn normalise_contrast(img: &mut RgbaImage) {
    let luma = |p: &image::Rgba<u8>| (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
    let (mut sum, mut lo, mut hi, mut n) = (0u64, 255u32, 0u32, 0u64);
    for p in img.pixels() {
        let l = luma(p);
        sum += u64::from(l);
        lo = lo.min(l);
        hi = hi.max(l);
        n += 1;
    }
    if n == 0 {
        return;
    }
    let invert = sum / n < 110;
    let span = (hi.saturating_sub(lo)).max(1);
    for p in img.pixels_mut() {
        let mut v = (luma(p).saturating_sub(lo) * 255 / span).min(255) as u8;
        if invert {
            v = 255 - v;
        }
        *p = image::Rgba([v, v, v, 255]);
    }
}

/// Blends a coverage mask (0 to 255 per pixel, `mw` wide) onto the image at (x, y) in one grey level.
pub fn blend_mask(img: &mut RgbaImage, mask: &[u8], mw: u32, mh: u32, x: i32, y: i32, level: u8, opacity: f32) {
    for my in 0..mh {
        for mx in 0..mw {
            let coverage = f32::from(mask[(my * mw + mx) as usize]) / 255.0 * opacity;
            let (px, py) = (x + mx as i32, y + my as i32);
            if coverage <= 0.0 || px < 0 || py < 0 || px >= img.width() as i32 || py >= img.height() as i32 {
                continue;
            }
            let dst = img.get_pixel_mut(px as u32, py as u32);
            for c in 0..3 {
                dst[c] = (f32::from(dst[c]) * (1.0 - coverage) + f32::from(level) * coverage).round() as u8;
            }
        }
    }
}

// ---- Editor crop and pins ----

/// The crop box the editor sent ("x,y,w,h" in image pixels), snapped to whole pixels and kept inside
/// an image of `iw` by `ih`. None when nothing of the box is left or the text is not four numbers.
pub fn clamp_crop(text: &str, iw: u32, ih: u32) -> Option<(u32, u32, u32, u32)> {
    let v: Vec<f64> = text.split(',').map(|p| p.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
    let &[x, y, w, h] = v.as_slice() else { return None };
    if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
        return None;
    }
    let left = x.round().clamp(0.0, f64::from(iw));
    let top = y.round().clamp(0.0, f64::from(ih));
    let right = (x + w).round().clamp(0.0, f64::from(iw));
    let bottom = (y + h).round().clamp(0.0, f64::from(ih));
    (right > left && bottom > top).then(|| (left as u32, top as u32, (right - left) as u32, (bottom - top) as u32))
}

/// Size of a pinned image after wheel notches (positive grows): 10 percent per notch, the aspect
/// ratio kept, width between `min_w` and `max_w`.
pub fn pin_size(width: u32, height: u32, notches: f64, min_w: u32, max_w: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width, height);
    }
    let w = (f64::from(width) * 1.1f64.powf(notches)).round().clamp(f64::from(min_w), f64::from(max_w.max(min_w)));
    let h = (w * f64::from(height) / f64::from(width)).round().max(1.0);
    (w as u32, h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn corners_round_outwards_and_swap() {
        assert_eq!(box_from_corners((10.4, 20.6), (50.2, 60.0)), Rect::new(10, 20, 41, 40));
        assert_eq!(box_from_corners((50.2, 60.0), (10.4, 20.6)), Rect::new(10, 20, 41, 40));
        assert!(box_from_corners((5.0, 5.0), (5.0, 5.0)).is_empty());
    }

    #[test]
    fn rects_intersect_and_contain() {
        let a = Rect::new(0, 0, 100, 100);
        assert_eq!(a.intersect(&Rect::new(50, 60, 100, 100)), Some(Rect::new(50, 60, 50, 40)));
        assert_eq!(a.intersect(&Rect::new(100, 0, 10, 10)), None);
        assert!(a.contains(0, 0) && a.contains(99, 99) && !a.contains(100, 50));
    }

    #[test]
    fn window_hit_follows_stacking_order() {
        let windows = [Rect::new(100, 100, 200, 200), Rect::new(0, 0, 400, 400)];
        assert_eq!(topmost_at(&windows, 150, 150), Some(windows[0]));
        assert_eq!(topmost_at(&windows, 10, 10), Some(windows[1]));
        assert_eq!(topmost_at(&windows, 500, 10), None);
    }

    #[test]
    fn monitor_falls_back_to_the_nearest() {
        let monitors = [Rect::new(0, 0, 1920, 1080), Rect::new(1930, 0, 1920, 1080)];
        assert_eq!(monitor_at(&monitors, 100, 100), Some(monitors[0]));
        assert_eq!(monitor_at(&monitors, 1922, 100), Some(monitors[0]));
        assert_eq!(monitor_at(&monitors, 1928, 100), Some(monitors[1]));
        assert_eq!(monitor_at(&[], 0, 0), None);
    }

    #[test]
    fn targets_resolve_on_the_virtual_desktop() {
        let origin = (-1920, 0);
        let windows = [Rect::new(-1500, 100, 400, 300)];
        let monitors = [Rect::new(-1920, 0, 1920, 1080), Rect::new(0, 0, 2560, 1440)];
        let area = resolve_target(Target::Area { ax: 10.5, ay: 10.0, bx: 110.5, by: 60.0 }, origin, &windows, &monitors);
        assert_eq!(area, Some(Rect::new(-1910, 10, 101, 50)));
        assert_eq!(resolve_target(Target::Window { x: 500.0, y: 150.0 }, origin, &windows, &monitors), Some(windows[0]));
        assert_eq!(resolve_target(Target::Window { x: 5.0, y: 5.0 }, origin, &windows, &monitors), None);
        assert_eq!(resolve_target(Target::Full { x: 2000.0, y: 10.0 }, origin, &windows, &monitors), Some(monitors[1]));
        assert_eq!(resolve_target(Target::Area { ax: 3.0, ay: 3.0, bx: 3.0, by: 9.0 }, origin, &windows, &monitors), None);
    }

    #[test]
    fn frame_crop_is_clipped_to_the_frame() {
        let rgba: Vec<u8> = (0..4 * 4 * 4).map(|i| (i / 4) as u8).collect();
        let f = Frame { origin: (10, 20), w: 4, h: 4, rgba };
        let img = f.crop(&Rect::new(11, 21, 2, 2)).unwrap();
        assert_eq!(img.dimensions(), (2, 2));
        assert_eq!(img.get_pixel(0, 0)[0], 5);
        assert_eq!(img.get_pixel(1, 1)[0], 10);
        let clipped = f.crop(&Rect::new(12, 22, 10, 10)).unwrap();
        assert_eq!(clipped.dimensions(), (2, 2));
        assert!(f.crop(&Rect::new(0, 0, 5, 5)).is_none());
        assert_eq!(f.pixel(11, 21), Some([5, 5, 5]));
        assert_eq!(f.pixel(9, 21), None);
    }

    #[test]
    fn unix_time_converts() {
        assert_eq!(civil_from_unix(0), DateParts { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0 });
        assert_eq!(civil_from_unix(1_000_000_000), DateParts { year: 2001, month: 9, day: 9, hour: 1, minute: 46, second: 40 });
        assert_eq!(civil_from_unix(1_709_164_800), DateParts { year: 2024, month: 2, day: 29, hour: 0, minute: 0, second: 0 });
    }

    #[test]
    fn name_templates() {
        let d = DateParts { year: 2026, month: 10, day: 20, hour: 14, minute: 5, second: 9 };
        assert_eq!(file_stem("Screenshot {date} {time}", &d), "Screenshot 2026-10-20 14.05.09");
        assert_eq!(file_stem("{year}{month}{day}_{hour}{minute}{second}", &d), "20261020_140509");
        assert_eq!(file_stem("a/b:c?", &d), "a-b-c-");
        assert_eq!(file_stem("  ", &d), "Screenshot");
        assert_eq!(file_stem("shot.", &d), "shot");
    }

    #[test]
    fn taken_names_get_a_number() {
        let taken = [PathBuf::from("d/a.png"), PathBuf::from("d/a (2).png")];
        let p = unique_path(Path::new("d"), "a", "png", |p| taken.iter().any(|t| t == p));
        assert_eq!(p, PathBuf::from("d/a (3).png"));
        assert_eq!(unique_path(Path::new("d"), "b", "png", |_| false), PathBuf::from("d/b.png"));
    }

    #[test]
    fn lines_keep_or_flatten() {
        let l = lines(&["first line", "  ", "second line"]);
        assert_eq!(join_text(&l, false, true), "first line\nsecond line");
        assert_eq!(join_text(&l, false, false), "first line second line");
    }

    #[test]
    fn lines_join_into_a_paragraph() {
        assert_eq!(join_text(&lines(&["Hello", "world"]), true, true), "Hello world");
        assert_eq!(join_text(&lines(&["an exam-", "ple of it"]), true, true), "an example of it");
        assert_eq!(join_text(&lines(&["well -", "known"]), true, true), "well - known");
        assert_eq!(join_text(&lines(&["pre-", "Paris"]), true, true), "pre- Paris");
        assert_eq!(join_text(&lines(&["日本語の", "文章です"]), true, true), "日本語の文章です");
        assert_eq!(join_text(&lines(&["日本語", "text"]), true, true), "日本語 text");
        assert_eq!(join_text(&[], true, true), "");
    }

    #[test]
    fn only_plain_links_count() {
        assert_eq!(web_link(" https://example.com/a?b=1 "), Some("https://example.com/a?b=1".into()));
        assert_eq!(web_link("HTTP://x.org"), Some("HTTP://x.org".into()));
        assert_eq!(web_link("see https://example.com"), None);
        assert_eq!(web_link("ftp://example.com"), None);
        assert_eq!(web_link("https://"), None);
    }

    #[test]
    fn notices_are_short() {
        assert_eq!(shorten("a  b\n c", 10), "a b c");
        assert_eq!(shorten("abcdefghij", 4), "abcd...");
    }

    #[test]
    fn small_areas_grow_and_huge_ones_shrink() {
        assert_eq!(ocr_scale(400, 80, 2600), 2.0);
        assert_eq!(ocr_scale(400, 400, 2600), 1.0);
        assert_eq!(ocr_scale(100, 10, 2600), 4.0);
        assert_eq!(ocr_scale(5200, 1000, 2600), 0.5);
        assert_eq!(ocr_scale(2000, 50, 2600), 1.3);
    }

    #[test]
    fn masks_blend_and_clip() {
        let mut img = RgbaImage::from_pixel(3, 1, image::Rgba([100, 100, 100, 255]));
        blend_mask(&mut img, &[255, 0, 255], 3, 1, -1, 0, 200, 0.5);
        assert_eq!(img.get_pixel(0, 0)[0], 100);
        assert_eq!(img.get_pixel(1, 0)[0], 150);
        assert_eq!(img.get_pixel(2, 0)[0], 100);
    }

    #[test]
    fn dark_text_backgrounds_are_flipped() {
        let mut dark = RgbaImage::from_pixel(4, 1, image::Rgba([20, 20, 20, 255]));
        dark.put_pixel(0, 0, image::Rgba([240, 240, 240, 255]));
        normalise_contrast(&mut dark);
        assert_eq!(dark.get_pixel(0, 0)[0], 0);
        assert_eq!(dark.get_pixel(1, 0)[0], 255);
        let mut light = RgbaImage::from_pixel(4, 1, image::Rgba([250, 250, 250, 255]));
        light.put_pixel(0, 0, image::Rgba([10, 10, 10, 255]));
        normalise_contrast(&mut light);
        assert_eq!(light.get_pixel(0, 0)[0], 0);
        assert_eq!(light.get_pixel(1, 0)[0], 255);
    }

    #[test]
    fn topmost_index_picks_the_first_window_under_the_point() {
        let windows = [Rect::new(0, 0, 50, 50), Rect::new(0, 0, 200, 200)];
        assert_eq!(topmost_index(&windows, 10, 10), Some(0));
        assert_eq!(topmost_index(&windows, 100, 100), Some(1));
        assert_eq!(topmost_index(&windows, 300, 10), None);
    }

    #[test]
    fn crop_snaps_and_stays_inside_the_image() {
        assert_eq!(clamp_crop("10.4,20.6,100.2,50", 400, 300), Some((10, 21, 101, 50)));
        assert_eq!(clamp_crop("-20,-10,100,100", 400, 300), Some((0, 0, 80, 90)));
        assert_eq!(clamp_crop("350,250,200,200", 400, 300), Some((350, 250, 50, 50)));
        assert_eq!(clamp_crop("500,10,50,50", 400, 300), None);
        assert_eq!(clamp_crop("10,10,0.2,50", 400, 300), None);
        assert_eq!(clamp_crop("10,10,50", 400, 300), None);
        assert_eq!(clamp_crop("a,b,c,d", 400, 300), None);
        assert_eq!(clamp_crop("NaN,0,10,10", 400, 300), None);
    }

    #[test]
    fn pin_zoom_keeps_the_aspect_and_the_limits() {
        assert_eq!(pin_size(200, 100, 0.0, 80, 2000), (200, 100));
        assert_eq!(pin_size(200, 100, 1.0, 80, 2000), (220, 110));
        assert_eq!(pin_size(200, 100, -30.0, 80, 2000), (80, 40));
        assert_eq!(pin_size(200, 100, 60.0, 80, 500), (500, 250));
        assert_eq!(pin_size(0, 0, 1.0, 80, 500), (0, 0));
    }
}
