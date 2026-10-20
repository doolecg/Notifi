//! Per-app resource use. Processes are added up per app, and helper processes (children
//! started from the same app folder) are counted under the app that started them.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    /// Folder the executable lives in, when readable.
    pub dir: Option<PathBuf>,
    /// Percent of one core, as sysinfo reports it.
    pub cpu: f32,
    pub mem: u64,
    pub gpu: f32,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub name: String,
    /// Share of the whole CPU, 0-100.
    pub cpu: f32,
    pub mem: u64,
    pub gpu: Option<f32>,
    pub processes: usize,
}

pub fn display_name(name: &str) -> &str {
    name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")).unwrap_or(name)
}

fn is_idle(name: &str) -> bool {
    name.is_empty() || name == "System Idle Process" || name == "Idle"
}

/// Folders shared by unrelated programs, where a common folder says nothing about ownership.
pub fn shared_folder(dir: &Path) -> bool {
    let lower = dir.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()).to_ascii_lowercase().replace('\\', "/");
    lower.starts_with(&windows)
        || ["/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/local/bin", "/usr/libexec", "/usr/lib/systemd"]
            .iter()
            .any(|d| lower == *d || lower.starts_with(&format!("{d}/")))
}

/// The app a process belongs to: its highest ancestor started from the same folder.
fn owner<'a>(proc: &'a Proc, by_pid: &HashMap<u32, &'a Proc>) -> &'a Proc {
    let Some(dir) = proc.dir.as_deref().filter(|d| !shared_folder(d)) else { return proc };
    let mut top = proc;
    for _ in 0..32 {
        let Some(parent) = top.parent.and_then(|p| by_pid.get(&p)) else { break };
        if parent.pid == top.pid || parent.dir.as_deref() != Some(dir) {
            break;
        }
        top = parent;
    }
    top
}

/// Apps by CPU, busiest first, with CPU normalised to the whole machine and capped at 100%.
pub fn group(procs: &[Proc], cores: usize, group_helpers: bool, limit: usize) -> Vec<AppUsage> {
    let cores = cores.max(1) as f32;
    let by_pid: HashMap<u32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut apps: HashMap<String, AppUsage> = HashMap::new();
    for p in procs {
        if is_idle(&p.name) {
            continue;
        }
        let key = if group_helpers { owner(p, &by_pid) } else { p };
        let name = display_name(&key.name).to_string();
        let app = apps.entry(name.clone()).or_insert(AppUsage { name, cpu: 0.0, mem: 0, gpu: None, processes: 0 });
        app.cpu += p.cpu.max(0.0) / cores;
        app.mem += p.mem;
        if p.gpu > 0.0 {
            app.gpu = Some(app.gpu.unwrap_or(0.0) + p.gpu);
        }
        app.processes += 1;
    }
    let mut out: Vec<AppUsage> = apps
        .into_values()
        .map(|mut a| {
            a.cpu = a.cpu.min(100.0);
            a.gpu = a.gpu.map(|g| g.min(100.0));
            a
        })
        .collect();
    out.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| b.mem.cmp(&a.mem)));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: Option<u32>, name: &str, dir: &str, cpu: f32) -> Proc {
        Proc { pid, parent, name: name.into(), dir: Some(PathBuf::from(dir)), cpu, mem: 100, gpu: 0.0 }
    }

    #[test]
    fn helpers_group_under_their_app() {
        let procs = [
            p(1, None, "explorer.exe", "C:\\Windows", 4.0),
            p(10, Some(1), "Code.exe", "C:\\Apps\\Code", 40.0),
            p(11, Some(10), "Code Helper.exe", "C:\\Apps\\Code", 40.0),
            p(12, Some(11), "rg.exe", "C:\\Apps\\Code\\bin", 8.0),
            p(20, Some(1), "svchost.exe", "C:\\Windows\\System32", 8.0),
            p(21, Some(20), "dllhost.exe", "C:\\Windows\\System32", 8.0),
            p(0, None, "System Idle Process", "", 300.0),
        ];
        let apps = group(&procs, 4, true, 10);
        assert_eq!(apps[0].name, "Code");
        assert_eq!(apps[0].cpu, 20.0);
        assert_eq!(apps[0].processes, 2);
        assert!(apps.iter().any(|a| a.name == "rg"));
        assert!(apps.iter().any(|a| a.name == "dllhost"), "system folders never group");
        assert!(!apps.iter().any(|a| a.name.contains("Idle")));

        let flat = group(&procs, 4, false, 10);
        assert!(flat.iter().any(|a| a.name == "Code Helper"));
    }

    #[test]
    fn caps_and_limits() {
        let mut procs: Vec<Proc> = (0..8).map(|i| p(100 + i, None, "busy", "/opt/busy", 400.0)).collect();
        procs[0].gpu = 80.0;
        procs[1].gpu = 70.0;
        procs.push(p(5, None, "a", "/opt/a", 1.0));
        procs.push(p(6, None, "b", "/opt/b", 2.0));
        let apps = group(&procs, 2, true, 2);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].cpu, 100.0);
        assert_eq!(apps[0].gpu, Some(100.0));
        assert_eq!(apps[1].name, "b");
    }

    #[test]
    fn parent_loops_end() {
        let procs = [p(1, Some(2), "x", "/opt/x", 1.0), p(2, Some(1), "y", "/opt/x", 1.0)];
        let apps = group(&procs, 1, true, 5);
        assert_eq!(apps.iter().map(|a| a.processes).sum::<usize>(), 2);
    }
}
