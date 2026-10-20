//! Fixed-size sample history for the graphs, and the round top value each graph scales to.

use std::collections::VecDeque;

use serde::Serialize;

/// Samples kept per graph.
pub const LEN: usize = 60;

#[derive(Clone, Debug)]
pub struct History {
    buf: VecDeque<f32>,
    cap: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(LEN)
    }
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self { buf: VecDeque::with_capacity(cap), cap: cap.max(1) }
    }

    /// Adds a sample, dropping the oldest when full. Non-finite values are ignored.
    pub fn push(&mut self, v: f32) {
        if !v.is_finite() {
            return;
        }
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }

    pub fn values(&self) -> Vec<f32> {
        self.buf.iter().copied().collect()
    }

    pub fn peak(&self) -> f32 {
        self.buf.iter().copied().fold(0.0, f32::max)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// The graph for the UI: the samples and the top of its scale.
    pub fn series(&self, fixed_max: Option<f64>, floor: f64) -> Series {
        let scale = fixed_max.unwrap_or_else(|| nice_ceiling(self.peak() as f64, floor));
        Series { values: self.values(), scale }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Series {
    pub values: Vec<f32>,
    pub scale: f64,
}

/// The smallest 1, 2 or 5 times a power of ten that is at least `peak`, and at least `floor`.
pub fn nice_ceiling(peak: f64, floor: f64) -> f64 {
    let target = peak.max(floor);
    if !target.is_finite() || target <= 0.0 {
        return floor.max(1.0);
    }
    let mut magnitude = 10f64.powf(target.log10().floor());
    loop {
        for step in [1.0, 2.0, 5.0] {
            let v = step * magnitude;
            if v >= target * (1.0 - 1e-9) {
                return v;
            }
        }
        magnitude *= 10.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_drops_oldest() {
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0, 4.0] {
            h.push(v);
        }
        assert_eq!(h.values(), vec![2.0, 3.0, 4.0]);
        h.push(f32::NAN);
        h.push(f32::INFINITY);
        assert_eq!(h.len(), 3);
        assert_eq!(h.peak(), 4.0);
    }

    #[test]
    fn default_holds_a_minute() {
        let mut h = History::default();
        for i in 0..100 {
            h.push(i as f32);
        }
        assert_eq!(h.len(), LEN);
        assert_eq!(h.values()[0], 40.0);
    }

    #[test]
    fn scale_is_a_round_number() {
        assert_eq!(nice_ceiling(0.0, 1000.0), 1000.0);
        assert_eq!(nice_ceiling(1200.0, 1000.0), 2000.0);
        assert_eq!(nice_ceiling(2000.0, 1.0), 2000.0);
        assert_eq!(nice_ceiling(3_400_000.0, 1.0), 5_000_000.0);
        assert_eq!(nice_ceiling(7.0, 1.0), 10.0);
        assert_eq!(nice_ceiling(0.3, 0.0), 0.5);
        let mut h = History::new(5);
        h.push(42.0);
        assert_eq!(h.series(Some(100.0), 0.0).scale, 100.0);
        assert_eq!(h.series(None, 10.0).scale, 50.0);
    }
}
