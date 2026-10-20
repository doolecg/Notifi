//! Readings for the ends of the resting pill when nothing live is showing: CPU, memory, battery
//! and the Claude session share. Read on the Island's own tick, only for the parts switched on
//! and only while the pill is resting.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use notifi_platform::Platform;
use serde::Serialize;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, System};

use super::{config::IslandConfig, usage};

const SYSTEM_EVERY: Duration = Duration::from_secs(2);
const SLOW_EVERY: Duration = Duration::from_secs(15);

/// Whole percents, `None` when switched off or not readable.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Idle {
    pub cpu: Option<u8>,
    pub mem: Option<u8>,
    /// `None` also when the machine has no battery.
    pub battery: Option<u8>,
    /// Charging or on mains power.
    pub charging: bool,
    /// Share of the Claude 5-hour session used: of the plan limit when one is set, else of the time.
    pub ai: Option<u8>,
}

pub struct Sampler {
    sys: Option<System>,
    next_system: Instant,
    next_slow: Instant,
    last: Idle,
}

impl Sampler {
    pub fn new() -> Self {
        let now = Instant::now();
        Self { sys: None, next_system: now, next_slow: now, last: Idle::default() }
    }

    pub fn tick(&mut self, c: &IslandConfig, platform: &Platform, history: &Mutex<usage::History>, resting: bool, now_ms: u64) -> Idle {
        if !resting {
            return self.last;
        }
        let now = Instant::now();
        if now >= self.next_slow {
            self.next_slow = now + SLOW_EVERY;
            let b = if c.idle_battery { platform.sensors().battery().ok().flatten() } else { None };
            self.last.battery = b.as_ref().map(|b| percent(b.percent));
            self.last.charging = b.is_some_and(|b| b.charging || b.on_ac);
            // Skipped while the usage thread holds the history; the last share stays.
            if let Some(h) = c.idle_ai_usage.then(|| history.try_lock().ok()).flatten() {
                self.last.ai = h.claude_window(now_ms).map(|(_, resets_at, cost_usd, _)| {
                    let share = if c.claude_plan_limit_usd > 0.0 {
                        cost_usd / c.claude_plan_limit_usd
                    } else {
                        usage::pace(resets_at, usage::CLAUDE_WINDOW_MS, now_ms)
                    };
                    percent((share * 100.0) as f32)
                });
            }
        }
        if !c.idle_battery {
            self.last.battery = None;
            self.last.charging = false;
        }
        if !c.idle_ai_usage {
            self.last.ai = None;
        }

        // CPU stands in for the battery on machines without one.
        let need_cpu = c.idle_cpu_ram || (c.idle_battery && self.last.battery.is_none());
        if !need_cpu {
            self.sys = None;
            self.last.cpu = None;
            self.last.mem = None;
        } else if now >= self.next_system {
            self.next_system = now + SYSTEM_EVERY;
            let fresh = self.sys.is_none();
            let sys = self.sys.get_or_insert_with(System::new);
            sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
            // The first CPU reading has nothing to compare against.
            self.last.cpu = (!fresh).then(|| percent(sys.global_cpu_usage()));
            self.last.mem = c.idle_cpu_ram.then(|| {
                sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
                let total = sys.total_memory();
                percent(if total == 0 { 0.0 } else { (sys.used_memory() as f64 / total as f64 * 100.0) as f32 })
            });
        }
        self.last
    }
}

fn percent(v: f32) -> u8 {
    if v.is_finite() { v.round().clamp(0.0, 100.0) as u8 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(41.6), 42);
        assert_eq!(percent(-3.0), 0);
        assert_eq!(percent(250.0), 100);
        assert_eq!(percent(f32::NAN), 0);
    }
}
