use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Running,
    Paused,
    Done,
}

/// A countdown. Times are unix ms passed in by the caller, so the logic is testable.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub phase: Phase,
    pub total_ms: u64,
    /// When a running timer ends.
    pub ends_at: u64,
    /// Left on a paused timer.
    pub remaining_ms: u64,
}

pub const MAX_MS: u64 = 24 * 3_600_000;

impl Timer {
    pub fn start(&mut self, now: u64, ms: u64) {
        let ms = ms.clamp(1000, MAX_MS);
        *self = Timer { phase: Phase::Running, total_ms: ms, ends_at: now + ms, remaining_ms: ms };
    }

    pub fn pause(&mut self, now: u64) {
        if self.phase == Phase::Running {
            self.remaining_ms = self.ends_at.saturating_sub(now);
            self.phase = Phase::Paused;
        }
    }

    pub fn resume(&mut self, now: u64) {
        if self.phase == Phase::Paused {
            self.ends_at = now + self.remaining_ms;
            self.phase = Phase::Running;
        }
    }

    pub fn cancel(&mut self) {
        *self = Timer::default();
    }

    /// Adds time to a running or paused timer; on an idle or finished one it starts a new countdown.
    pub fn add(&mut self, now: u64, ms: u64) {
        match self.phase {
            Phase::Running => {
                let left = self.ends_at.saturating_sub(now);
                let add = ms.min(MAX_MS.saturating_sub(left));
                self.ends_at += add;
                self.total_ms += add;
            }
            Phase::Paused => {
                let add = ms.min(MAX_MS.saturating_sub(self.remaining_ms));
                self.remaining_ms += add;
                self.total_ms += add;
            }
            Phase::Idle | Phase::Done => self.start(now, ms),
        }
    }

    pub fn remaining(&self, now: u64) -> u64 {
        match self.phase {
            Phase::Running => self.ends_at.saturating_sub(now),
            Phase::Paused => self.remaining_ms,
            Phase::Idle | Phase::Done => 0,
        }
    }

    /// Moves a running timer to `Done` once its time is up; true when that just happened.
    pub fn tick(&mut self, now: u64) -> bool {
        if self.phase == Phase::Running && now >= self.ends_at {
            self.phase = Phase::Done;
            self.remaining_ms = 0;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_down_and_finishes_once() {
        let mut t = Timer::default();
        t.start(1_000, 60_000);
        assert_eq!(t.remaining(31_000), 30_000);
        assert!(!t.tick(60_999));
        assert!(t.tick(61_000));
        assert_eq!(t.phase, Phase::Done);
        assert!(!t.tick(70_000));
        assert_eq!(t.remaining(70_000), 0);
    }

    #[test]
    fn pause_keeps_the_rest() {
        let mut t = Timer::default();
        t.start(0, 10_000);
        t.pause(4_000);
        assert_eq!(t.remaining(100_000), 6_000);
        assert!(!t.tick(100_000));
        t.resume(100_000);
        assert_eq!(t.ends_at, 106_000);
        t.resume(101_000);
        assert_eq!(t.ends_at, 106_000);
    }

    #[test]
    fn add_extends_or_starts() {
        let mut t = Timer::default();
        t.add(0, 60_000);
        assert_eq!(t.phase, Phase::Running);
        t.add(10_000, 60_000);
        assert_eq!((t.remaining(10_000), t.total_ms), (110_000, 120_000));
        t.pause(20_000);
        t.add(20_000, 1_000);
        assert_eq!(t.remaining(0), 101_000);
        t.cancel();
        assert_eq!(t, Timer::default());
    }

    #[test]
    fn clamps_length() {
        let mut t = Timer::default();
        t.start(0, 5);
        assert_eq!(t.total_ms, 1000);
        t.start(0, u64::MAX / 2);
        assert_eq!(t.total_ms, MAX_MS);
        t.add(0, MAX_MS);
        assert_eq!(t.remaining(0), MAX_MS);
    }
}
