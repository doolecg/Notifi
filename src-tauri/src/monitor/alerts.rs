//! Alert rules: a value must stay past its limit for the sustained time before it notifies,
//! and the same alert repeats no more often than the repeat interval (never under 2 minutes).

use std::time::{Duration, Instant};

use serde_json::Value;

use super::{memory::Pressure, units};
use crate::settings::Settings;

pub const MIN_REPEAT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cpu,
    CpuTemp,
    Memory,
    Disk,
    Battery,
    BatteryTemp,
}

const KINDS: [Kind; 6] = [Kind::Cpu, Kind::CpuTemp, Kind::Memory, Kind::Disk, Kind::Battery, Kind::BatteryTemp];

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub on: [bool; 6],
    pub cpu_limit: f32,
    pub cpu_temp_limit: f32,
    pub memory_level: Pressure,
    pub disk_free: f32,
    pub battery_low: f32,
    pub battery_temp_limit: f32,
    pub sustained: Duration,
    pub repeat: Duration,
    pub fahrenheit: bool,
    pub sound: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            on: [false; 6],
            cpu_limit: 90.0,
            cpu_temp_limit: 90.0,
            memory_level: Pressure::High,
            disk_free: 10.0,
            battery_low: 15.0,
            battery_temp_limit: 45.0,
            sustained: Duration::from_secs(60),
            repeat: Duration::from_secs(600),
            fahrenheit: false,
            sound: false,
        }
    }
}

impl Config {
    pub fn from_settings(s: &Settings) -> Self {
        let d = Self::default();
        let num = |key: &str, default: f32| s.option("alerts", key).and_then(Value::as_f64).map_or(default, |v| v as f32);
        let flag = |key: &str| s.option("alerts", key).and_then(Value::as_bool).unwrap_or(false);
        let master = s.enabled("monitor") && s.enabled("alerts");
        let on = |key: &str| master && flag(key);
        let repeat_min = s.option("alerts", "repeat").and_then(Value::as_str).and_then(|v| v.parse::<u64>().ok()).unwrap_or(10);
        Self {
            on: [on("cpu"), on("cpuTemp"), on("memory"), on("disk"), on("battery"), on("batteryTemp")],
            cpu_limit: num("cpuLimit", d.cpu_limit),
            cpu_temp_limit: num("cpuTempLimit", d.cpu_temp_limit),
            memory_level: match s.option("alerts", "memoryLevel").and_then(Value::as_str) {
                Some("critical") => Pressure::Critical,
                _ => Pressure::High,
            },
            disk_free: num("diskFree", d.disk_free),
            battery_low: num("batteryLow", d.battery_low),
            battery_temp_limit: num("batteryTempLimit", d.battery_temp_limit),
            sustained: Duration::from_secs_f32(num("sustained", 60.0).max(0.0)),
            repeat: Duration::from_secs(repeat_min * 60).max(MIN_REPEAT),
            fahrenheit: s.option("temperatures", "unit").and_then(Value::as_str) == Some("fahrenheit"),
            sound: flag("sound"),
        }
    }

    pub fn any(&self) -> bool {
        self.on.iter().any(|on| *on)
    }

    fn enabled(&self, kind: Kind) -> bool {
        self.on[kind as usize]
    }
}

#[derive(Default, Clone, Debug)]
pub struct Reading {
    pub cpu: f32,
    pub top_app: Option<String>,
    pub cpu_temp: Option<f32>,
    pub pressure: Pressure,
    /// Free space on the followed disk, percent, and its name.
    pub disk_free: Option<(f32, String)>,
    /// Charge percent and whether it's on mains power or charging.
    pub battery: Option<(f32, bool)>,
    pub battery_temp: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub kind: Kind,
    pub title: String,
    pub body: String,
}

#[derive(Default, Clone, Copy)]
struct Rule {
    since: Option<Instant>,
    fired: Option<Instant>,
}

#[derive(Default)]
pub struct Engine {
    rules: [Rule; 6],
    last: Option<Instant>,
}

impl Engine {
    /// Checks every rule against `r`. A gap longer than `max_gap` since the last check (sleep)
    /// restarts the sustained timers.
    pub fn check(&mut self, now: Instant, cfg: &Config, r: &Reading, max_gap: Duration) -> Vec<Alert> {
        if self.last.is_some_and(|last| now.saturating_duration_since(last) > max_gap) {
            for rule in &mut self.rules {
                rule.since = None;
            }
        }
        self.last = Some(now);
        let mut out = Vec::new();
        for kind in KINDS {
            let rule = &mut self.rules[kind as usize];
            let alert = if cfg.enabled(kind) { breach(kind, cfg, r) } else { None };
            let Some(alert) = alert else {
                rule.since = None;
                continue;
            };
            let since = *rule.since.get_or_insert(now);
            let sustained = now.saturating_duration_since(since) >= cfg.sustained;
            let rested = rule.fired.is_none_or(|t| now.saturating_duration_since(t) >= cfg.repeat.max(MIN_REPEAT));
            if sustained && rested {
                rule.fired = Some(now);
                out.push(alert);
            }
        }
        out
    }
}

fn breach(kind: Kind, cfg: &Config, r: &Reading) -> Option<Alert> {
    let f = cfg.fahrenheit;
    let alert = |title: &str, body: String| Some(Alert { kind, title: title.to_string(), body });
    match kind {
        Kind::Cpu if r.cpu >= cfg.cpu_limit => {
            let by = r.top_app.as_ref().map(|a| format!(" Busiest app: {a}.")).unwrap_or_default();
            alert("High CPU load", format!("CPU at {:.0}% (limit {:.0}%).{by}", r.cpu, cfg.cpu_limit))
        }
        Kind::CpuTemp => {
            let t = r.cpu_temp?;
            (units::to_unit(t, f) >= cfg.cpu_temp_limit).then_some(())?;
            let limit = format!("{:.0}°{}", cfg.cpu_temp_limit, if f { 'F' } else { 'C' });
            alert("CPU is hot", format!("CPU at {} (limit {limit}).", units::temperature(t, f)))
        }
        Kind::Memory if r.pressure >= cfg.memory_level => {
            alert("Memory pressure", format!("Memory pressure is {} (limit {}).", r.pressure.label(), cfg.memory_level.label()))
        }
        Kind::Disk => {
            let (free, name) = r.disk_free.as_ref()?;
            (*free < cfg.disk_free).then_some(())?;
            alert("Disk nearly full", format!("{name} has {free:.0}% free (limit {:.0}%).", cfg.disk_free))
        }
        Kind::Battery => {
            let (percent, powered) = r.battery?;
            (!powered && percent <= cfg.battery_low).then_some(())?;
            alert("Low battery", format!("Battery at {percent:.0}% (limit {:.0}%).", cfg.battery_low))
        }
        Kind::BatteryTemp => {
            let t = r.battery_temp?;
            (units::to_unit(t, f) >= cfg.battery_temp_limit).then_some(())?;
            let limit = format!("{:.0}°{}", cfg.battery_temp_limit, if f { 'F' } else { 'C' });
            alert("Battery is hot", format!("Battery at {} (limit {limit}).", units::temperature(t, f)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: Duration = Duration::from_secs(30);

    fn cfg() -> Config {
        Config { on: [true; 6], sustained: Duration::from_secs(60), repeat: Duration::from_secs(600), ..Default::default() }
    }

    fn hot_cpu() -> Reading {
        Reading { cpu: 95.0, top_app: Some("Code".into()), ..Default::default() }
    }

    #[test]
    fn fires_only_after_the_sustained_time() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let c = cfg();
        assert!(e.check(t0, &c, &hot_cpu(), GAP).is_empty());
        assert!(e.check(t0 + Duration::from_secs(30), &c, &hot_cpu(), GAP).is_empty());
        let fired = e.check(t0 + Duration::from_secs(60), &c, &hot_cpu(), GAP);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, Kind::Cpu);
        assert!(fired[0].body.contains("Code"));
    }

    #[test]
    fn short_bursts_reset_the_timer() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let c = cfg();
        e.check(t0, &c, &hot_cpu(), GAP);
        e.check(t0 + Duration::from_secs(20), &c, &Reading::default(), GAP);
        assert!(e.check(t0 + Duration::from_secs(40), &c, &hot_cpu(), GAP).is_empty());
        assert!(e.check(t0 + Duration::from_secs(70), &c, &hot_cpu(), GAP).is_empty());
        assert_eq!(e.check(t0 + Duration::from_secs(100), &c, &hot_cpu(), GAP).len(), 1);
    }

    #[test]
    fn repeats_no_sooner_than_the_interval_and_never_under_two_minutes() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let mut c = cfg();
        c.sustained = Duration::ZERO;
        c.repeat = Duration::from_secs(10);
        let at = |s: u64| t0 + Duration::from_secs(s);
        assert_eq!(e.check(at(0), &c, &hot_cpu(), GAP).len(), 1);
        for s in (10..120).step_by(10) {
            assert!(e.check(at(s), &c, &hot_cpu(), GAP).is_empty(), "fired again at {s}s");
        }
        assert_eq!(e.check(at(120), &c, &hot_cpu(), GAP).len(), 1);
    }

    #[test]
    fn sleep_restarts_the_sustained_timer() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let c = cfg();
        e.check(t0, &c, &hot_cpu(), GAP);
        assert!(e.check(t0 + Duration::from_secs(600), &c, &hot_cpu(), GAP).is_empty());
        assert_eq!(e.check(t0 + Duration::from_secs(620), &c, &hot_cpu(), GAP).len(), 0);
        assert_eq!(e.check(t0 + Duration::from_secs(640), &c, &hot_cpu(), GAP).len(), 0);
        assert_eq!(e.check(t0 + Duration::from_secs(660), &c, &hot_cpu(), GAP).len(), 1);
    }

    #[test]
    fn thresholds_per_kind() {
        let mut c = cfg();
        c.sustained = Duration::ZERO;
        let r = Reading {
            cpu: 10.0,
            cpu_temp: Some(92.0),
            pressure: Pressure::Elevated,
            disk_free: Some((5.0, "C:".into())),
            battery: Some((10.0, true)),
            battery_temp: Some(40.0),
            ..Default::default()
        };
        let kinds: Vec<Kind> = Engine::default().check(Instant::now(), &c, &r, GAP).into_iter().map(|a| a.kind).collect();
        assert_eq!(kinds, vec![Kind::CpuTemp, Kind::Disk], "charging hides low battery");

        let r = Reading { battery: Some((10.0, false)), pressure: Pressure::Critical, ..Default::default() };
        let kinds: Vec<Kind> = Engine::default().check(Instant::now(), &c, &r, GAP).into_iter().map(|a| a.kind).collect();
        assert_eq!(kinds, vec![Kind::Memory, Kind::Battery]);

        c.fahrenheit = true;
        c.cpu_temp_limit = 190.0;
        let r = Reading { cpu_temp: Some(85.0), ..Default::default() };
        assert!(Engine::default().check(Instant::now(), &c, &r, GAP).is_empty(), "85°C is 185°F");
        c.on = [false; 6];
        assert!(Engine::default().check(Instant::now(), &c, &hot_cpu(), GAP).is_empty());
    }

    #[test]
    fn reads_settings() {
        let mut s = Settings::default();
        s.features.get_mut("monitor").unwrap().insert("enabled".into(), true.into());
        let a = s.features.get_mut("alerts").unwrap();
        a.insert("enabled".into(), true.into());
        a.insert("cpu".into(), true.into());
        a.insert("repeat".into(), "2".into());
        a.insert("memoryLevel".into(), "critical".into());
        let c = Config::from_settings(&s);
        assert!(c.on[0] && !c.on[1] && c.any());
        assert_eq!(c.repeat, MIN_REPEAT);
        assert_eq!(c.memory_level, Pressure::Critical);
        assert_eq!(c.cpu_limit, 90.0);
        assert!(!Config::from_settings(&Settings::default()).any());
    }
}
