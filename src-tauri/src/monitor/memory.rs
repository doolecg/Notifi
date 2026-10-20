//! Memory details beyond used/total (cached, compressed, commit charge) and the pressure level.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum Pressure {
    #[default]
    Normal,
    Elevated,
    High,
    Critical,
}

impl Pressure {
    pub fn label(self) -> &'static str {
        match self {
            Pressure::Normal => "Normal",
            Pressure::Elevated => "Elevated",
            Pressure::High => "High",
            Pressure::Critical => "Critical",
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Extra {
    pub cached: Option<u64>,
    pub compressed: Option<u64>,
    /// Committed bytes and the commit limit (Windows).
    pub commit: Option<(u64, u64)>,
    /// Linux PSI "some avg10": share of the last 10 s some task waited on memory.
    pub stall: Option<f32>,
}

/// Pressure from how much memory is still available, how full the commit charge is and,
/// on Linux, how long tasks have been stalling on memory.
pub fn pressure(available: u64, total: u64, extra: &Extra) -> Pressure {
    let free = if total > 0 { available as f64 / total as f64 } else { 1.0 };
    let commit = extra.commit.filter(|(_, limit)| *limit > 0).map(|(used, limit)| used as f64 / limit as f64).unwrap_or(0.0);
    let stall = extra.stall.unwrap_or(0.0) as f64;
    if free < 0.05 || commit >= 0.95 || stall >= 40.0 {
        Pressure::Critical
    } else if free < 0.10 || commit >= 0.90 || stall >= 20.0 {
        Pressure::High
    } else if free < 0.20 || commit >= 0.80 || stall >= 5.0 {
        Pressure::Elevated
    } else {
        Pressure::Normal
    }
}

#[cfg(windows)]
pub fn extra(compressed: Option<u64>) -> Extra {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    let mut info = PERFORMANCE_INFORMATION { cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32, ..Default::default() };
    if unsafe { GetPerformanceInfo(&mut info, info.cb) }.is_err() {
        return Extra { compressed, ..Default::default() };
    }
    let page = info.PageSize as u64;
    Extra {
        cached: Some(info.SystemCache as u64 * page),
        compressed,
        commit: Some((info.CommitTotal as u64 * page, info.CommitLimit as u64 * page)),
        stall: None,
    }
}

#[cfg(target_os = "linux")]
pub fn extra(_compressed: Option<u64>) -> Extra {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let psi = std::fs::read_to_string("/proc/pressure/memory").unwrap_or_default();
    Extra { cached: parse_cached(&meminfo), compressed: zram_compressed(), commit: None, stall: parse_psi_some(&psi) }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn extra(compressed: Option<u64>) -> Extra {
    Extra { compressed, ..Default::default() }
}

/// Page cache plus buffers, from /proc/meminfo, in bytes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_cached(meminfo: &str) -> Option<u64> {
    let field = |name: &str| {
        meminfo.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?.strip_prefix(':')?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    let cached = field("Cached")?;
    Some((cached + field("Buffers").unwrap_or(0)) * 1024)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_psi_some(psi: &str) -> Option<f32> {
    let line = psi.lines().find(|l| l.starts_with("some "))?;
    line.split_whitespace().find_map(|kv| kv.strip_prefix("avg10=")?.parse().ok())
}

#[cfg(target_os = "linux")]
fn zram_compressed() -> Option<u64> {
    let mut total = None;
    for entry in std::fs::read_dir("/sys/block").ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("zram") {
            continue;
        }
        let stat = std::fs::read_to_string(entry.path().join("mm_stat")).unwrap_or_default();
        if let Some(bytes) = stat.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()) {
            *total.get_or_insert(0) += bytes;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn pressure_levels() {
        let none = Extra::default();
        assert_eq!(pressure(8 * GB, 16 * GB, &none), Pressure::Normal);
        assert_eq!(pressure(3 * GB, 16 * GB, &none), Pressure::Elevated);
        assert_eq!(pressure(GB, 16 * GB, &none), Pressure::High);
        assert_eq!(pressure(GB / 2, 16 * GB, &none), Pressure::Critical);
        let commit = Extra { commit: Some((91, 100)), ..Default::default() };
        assert_eq!(pressure(8 * GB, 16 * GB, &commit), Pressure::High);
        let stall = Extra { stall: Some(6.5), ..Default::default() };
        assert_eq!(pressure(8 * GB, 16 * GB, &stall), Pressure::Elevated);
        assert_eq!(pressure(0, 0, &none), Pressure::Normal);
        assert!(Pressure::Critical > Pressure::High);
    }

    #[test]
    fn parses_linux_files() {
        let meminfo = "MemTotal:       16000000 kB\nBuffers:          100 kB\nCached:          2000 kB\nSwapCached:  5 kB\n";
        assert_eq!(parse_cached(meminfo), Some(2100 * 1024));
        assert_eq!(parse_cached("MemTotal: 1 kB"), None);
        let psi = "some avg10=1.25 avg60=0.50 avg300=0.10 total=123\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        assert_eq!(parse_psi_some(psi), Some(1.25));
        assert_eq!(parse_psi_some(""), None);
    }
}
