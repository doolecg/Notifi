//! Scrolling capture: finds how far a page moved between two frames and joins the frames into one
//! tall image. Rows that stay put at the top or bottom (sticky bars) are kept once.

use image::RgbaImage;

/// A match must cover at least this share of the rows that could overlap.
const MIN_SCORE: f64 = 0.97;
/// Fewest rows two frames must share to trust a match.
const MIN_OVERLAP: usize = 12;

#[derive(Debug, PartialEq, Eq)]
pub struct Offset {
    /// Rows the content moved up.
    pub shift: usize,
    /// Rows at the top that did not move.
    pub header: usize,
    /// Rows at the bottom that did not move.
    pub footer: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Frames are identical: the page did not move.
    Same,
    /// The new frame added this many rows.
    Added(usize),
    /// No overlap was found.
    Lost,
}

pub fn row_hashes(img: &RgbaImage) -> Vec<u64> {
    let stride = img.width() as usize * 4;
    if stride == 0 {
        return Vec::new();
    }
    img.as_raw()
        .chunks_exact(stride)
        .map(|row| row.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)))
        .collect()
}

/// How far the content of `next` moved up from `prev`, or `None` when the frames share nothing.
/// Both are row hashes of equally tall frames and must differ.
pub fn find_offset(prev: &[u64], next: &[u64]) -> Option<Offset> {
    let h = prev.len();
    if h != next.len() || h < MIN_OVERLAP * 2 {
        return None;
    }
    let cap = h / 3;
    let header = prev.iter().zip(next).take_while(|(a, b)| a == b).count().min(cap);
    let footer = prev.iter().rev().zip(next.iter().rev()).take_while(|(a, b)| a == b).count().min(cap);
    let content = h - header - footer;
    if content <= MIN_OVERLAP {
        return None;
    }
    let mut best: Option<(f64, usize)> = None;
    for shift in 1..=content - MIN_OVERLAP {
        let overlap = content - shift;
        let hits = (0..overlap).filter(|i| next[header + i] == prev[header + shift + i]).count();
        let score = hits as f64 / overlap as f64;
        if score >= MIN_SCORE && best.is_none_or(|(s, _)| score >= s) {
            best = Some((score, shift));
        }
    }
    best.map(|(_, shift)| Offset { shift, header, footer })
}

pub struct Stitcher {
    width: usize,
    rows: Vec<u8>,
    height: usize,
    last: RgbaImage,
    last_hashes: Vec<u64>,
}

impl Stitcher {
    pub fn new(first: RgbaImage) -> Self {
        Self {
            width: first.width() as usize,
            rows: first.as_raw().clone(),
            height: first.height() as usize,
            last_hashes: row_hashes(&first),
            last: first,
        }
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Adds a frame taken after the page moved. A frame of another size is lost.
    pub fn push(&mut self, frame: RgbaImage) -> Step {
        if frame.dimensions() != self.last.dimensions() {
            return Step::Lost;
        }
        let hashes = row_hashes(&frame);
        if hashes == self.last_hashes {
            return Step::Same;
        }
        let Some(Offset { shift, footer, .. }) = find_offset(&self.last_hashes, &hashes) else {
            return Step::Lost;
        };
        let stride = self.width * 4;
        let h = hashes.len();
        // The last frame's bottom bar is already at the end; drop it and take the new rows plus the bar again.
        self.rows.truncate((self.height - footer) * stride);
        self.rows.extend_from_slice(&frame.as_raw()[(h - footer - shift) * stride..]);
        self.height = self.rows.len() / stride;
        self.last = frame;
        self.last_hashes = hashes;
        Step::Added(shift)
    }

    pub fn finish(self) -> Option<RgbaImage> {
        RgbaImage::from_raw(self.width as u32, self.height as u32, self.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 3;

    fn row(seed: u32) -> [u8; 12] {
        let mut out = [0u8; 12];
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
        for b in out.iter_mut() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *b = (x >> 24) as u8;
        }
        out
    }

    fn image(rows: &[u32]) -> RgbaImage {
        let raw: Vec<u8> = rows.iter().flat_map(|r| row(*r)).collect();
        RgbaImage::from_raw(W, rows.len() as u32, raw).unwrap()
    }

    /// A frame showing page rows `start..` between a fixed header and footer.
    fn frame(start: u32, header: u32, footer: u32, height: u32) -> Vec<u32> {
        let content = height - header - footer;
        let mut rows: Vec<u32> = (0..header).map(|i| 10_000 + i).collect();
        rows.extend((start..start + content).map(|i| 1000 + i));
        rows.extend((0..footer).map(|i| 20_000 + i));
        rows
    }

    #[test]
    fn finds_a_plain_scroll() {
        let a = row_hashes(&image(&frame(0, 0, 0, 60)));
        let b = row_hashes(&image(&frame(25, 0, 0, 60)));
        assert_eq!(find_offset(&a, &b), Some(Offset { shift: 25, header: 0, footer: 0 }));
    }

    #[test]
    fn sees_sticky_bars() {
        let a = row_hashes(&image(&frame(0, 6, 5, 60)));
        let b = row_hashes(&image(&frame(20, 6, 5, 60)));
        assert_eq!(find_offset(&a, &b), Some(Offset { shift: 20, header: 6, footer: 5 }));
    }

    #[test]
    fn unrelated_frames_have_no_offset() {
        let a = row_hashes(&image(&frame(0, 0, 0, 60)));
        let b = row_hashes(&image(&frame(500, 0, 0, 60)));
        assert_eq!(find_offset(&a, &b), None);
    }

    #[test]
    fn stitches_a_page_with_sticky_bars_once() {
        let (header, footer, height) = (6, 5, 60);
        let mut stitcher = Stitcher::new(image(&frame(0, header, footer, height)));
        assert_eq!(stitcher.push(image(&frame(0, header, footer, height))), Step::Same);
        for start in [20, 40, 60] {
            assert_eq!(stitcher.push(image(&frame(start, header, footer, height))), Step::Added(20));
        }
        let content = height - header - footer;
        let mut expected: Vec<u32> = (0..header).map(|i| 10_000 + i).collect();
        expected.extend((0..60 + content).map(|i| 1000 + i));
        expected.extend((0..footer).map(|i| 20_000 + i));
        assert_eq!(stitcher.height(), expected.len());
        assert_eq!(stitcher.finish().unwrap().as_raw(), image(&expected).as_raw());
    }

    #[test]
    fn a_lost_frame_keeps_what_was_captured() {
        let mut stitcher = Stitcher::new(image(&frame(0, 0, 0, 60)));
        assert_eq!(stitcher.push(image(&frame(30, 0, 0, 60))), Step::Added(30));
        assert_eq!(stitcher.push(image(&frame(900, 0, 0, 60))), Step::Lost);
        assert_eq!(stitcher.push(image(&frame(0, 0, 0, 40))), Step::Lost);
        assert_eq!(stitcher.finish().unwrap().height(), 90);
    }
}
