//! GPU load, video memory, temperature and power. Windows reads the "GPU Engine" and
//! "GPU Adapter Memory" performance counters plus DXGI for names and sizes; Linux reads
//! /sys/class/drm (amdgpu and i915/xe files) and nvidia-smi when it is installed.

use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub id: String,
    pub name: String,
    /// Busiest engine, 0-100; `None` when the driver doesn't say.
    pub load: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub temp: Option<f32>,
    pub power: Option<f32>,
    pub integrated: bool,
    /// Powered down; not woken just to read it.
    pub idle: bool,
}

/// The GPU the tray and the System tab follow: "busiest", "integrated" or "discrete".
pub fn pick<'a>(gpus: &'a [Gpu], follow: &str) -> Option<&'a Gpu> {
    let busiest = |list: &mut dyn Iterator<Item = &'a Gpu>| list.max_by(|a, b| a.load.unwrap_or(-1.0).total_cmp(&b.load.unwrap_or(-1.0)));
    let wanted = match follow {
        "integrated" => busiest(&mut gpus.iter().filter(|g| g.integrated)),
        "discrete" => busiest(&mut gpus.iter().filter(|g| !g.integrated)),
        _ => None,
    };
    wanted.or_else(|| busiest(&mut gpus.iter()))
}

fn clamp_pct(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 100.0) as f32
    } else {
        0.0
    }
}

/// `pid_1234_luid_0x00000000_0x0000D1A3_phys_0_eng_3_engtype_3D` as (pid, luid, engine).
pub fn parse_engine(instance: &str) -> Option<(u32, String, String)> {
    let rest = instance.strip_prefix("pid_")?;
    let (pid, rest) = rest.split_once('_')?;
    let rest = rest.strip_prefix("luid_")?;
    let (luid, rest) = rest.split_once("_phys_")?;
    let engine = rest.split_once("_engtype_").map_or(rest, |(eng, _)| eng);
    Some((pid.parse().ok()?, luid.to_ascii_lowercase(), engine.to_string()))
}

/// `luid_0x00000000_0x0000D1A3_phys_0` as its luid.
pub fn parse_adapter(instance: &str) -> Option<String> {
    let rest = instance.strip_prefix("luid_")?;
    Some(rest.split_once("_phys_").map_or(rest, |(l, _)| l).to_ascii_lowercase())
}

/// Per adapter: the busiest engine, summed over processes. Per process: its busiest engine.
pub fn engine_loads(samples: &[(String, f64)]) -> (HashMap<String, f32>, HashMap<u32, f32>) {
    let mut engines: HashMap<(String, String), f64> = HashMap::new();
    let mut procs: HashMap<(u32, String, String), f64> = HashMap::new();
    for (name, value) in samples {
        let Some((pid, luid, engine)) = parse_engine(name) else { continue };
        if !value.is_finite() || *value < 0.0 {
            continue;
        }
        *engines.entry((luid.clone(), engine.clone())).or_default() += value;
        *procs.entry((pid, luid, engine)).or_default() += value;
    }
    let mut adapters: HashMap<String, f32> = HashMap::new();
    for ((luid, _), v) in engines {
        let slot = adapters.entry(luid).or_default();
        *slot = slot.max(clamp_pct(v));
    }
    let mut per_pid: HashMap<u32, f32> = HashMap::new();
    for ((pid, _, _), v) in procs {
        let slot = per_pid.entry(pid).or_default();
        *slot = slot.max(clamp_pct(v));
    }
    per_pid.retain(|_, v| *v > 0.0);
    (adapters, per_pid)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw --format=csv,noheader,nounits`
pub fn parse_nvidia_smi(csv: &str) -> Vec<Gpu> {
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    csv.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 7 {
                return None;
            }
            const MIB: f64 = 1024.0 * 1024.0;
            Some(Gpu {
                id: format!("nvidia{}", f[0].trim()),
                name: f[1].trim().to_string(),
                load: num(f[2]).map(clamp_pct),
                vram_used: num(f[3]).map(|v| (v * MIB) as u64),
                vram_total: num(f[4]).map(|v| (v * MIB) as u64),
                temp: num(f[5]).map(|v| v as f32),
                power: num(f[6]).map(|v| v as f32),
                integrated: false,
                idle: false,
            })
        })
        .collect()
}

pub struct Sampler {
    #[cfg(windows)]
    adapters: Option<Vec<win::Adapter>>,
    /// `None` until tried; `Some(false)` once nvidia-smi was not found, so it isn't tried again.
    #[cfg_attr(windows, allow(dead_code))]
    nvidia: Option<bool>,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            adapters: None,
            nvidia: None,
        }
    }

    /// `engine` and `memory` are the PDH "GPU Engine" and "GPU Adapter Memory" instances.
    /// Returns the GPUs and the per-process load.
    #[cfg(windows)]
    pub fn sample(&mut self, engine: &[(String, f64)], memory: &[(String, f64)]) -> (Vec<Gpu>, HashMap<u32, f32>) {
        let adapters = self.adapters.get_or_insert_with(win::adapters);
        let (loads, per_pid) = engine_loads(engine);
        let used: HashMap<String, u64> =
            memory.iter().filter_map(|(name, v)| Some((parse_adapter(name)?, v.max(0.0) as u64))).collect();
        let gpus = adapters
            .iter()
            .map(|a| Gpu {
                id: a.luid.clone(),
                name: a.name.clone(),
                load: Some(loads.get(&a.luid).copied().unwrap_or(0.0)),
                vram_used: used.get(&a.luid).copied(),
                vram_total: (a.dedicated > 0).then_some(a.dedicated),
                integrated: a.integrated,
                ..Default::default()
            })
            .collect();
        (gpus, per_pid)
    }

    #[cfg(target_os = "linux")]
    pub fn sample(&mut self) -> (Vec<Gpu>, HashMap<u32, f32>) {
        let mut gpus = linux::read_drm(std::path::Path::new("/sys/class/drm"));
        if self.nvidia != Some(false) && gpus.iter().any(|g| g.id.starts_with("nvidia-drm") || g.name.starts_with("NVIDIA")) {
            match linux::nvidia_smi() {
                Some(rows) => {
                    self.nvidia = Some(true);
                    gpus.retain(|g| !g.name.starts_with("NVIDIA"));
                    gpus.extend(rows);
                }
                None => self.nvidia = Some(false),
            }
        }
        (gpus, HashMap::new())
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn sample(&mut self) -> (Vec<Gpu>, HashMap<u32, f32>) {
        (Vec::new(), HashMap::new())
    }
}

/// A graphics card with 2 GB or more of its own memory, for effects that cost GPU time.
pub fn strong() -> bool {
    #[cfg(windows)]
    return win::adapters().iter().any(|a| a.dedicated >= 2 << 30);
    #[cfg(not(windows))]
    false
}

#[cfg(windows)]
mod win {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};

    pub struct Adapter {
        pub luid: String,
        pub name: String,
        pub dedicated: u64,
        pub integrated: bool,
    }

    pub fn adapters() -> Vec<Adapter> {
        let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return Vec::new() };
        let mut out: Vec<Adapter> = Vec::new();
        for i in 0.. {
            let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else { break };
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..len]).trim().to_string();
            if name.starts_with("Microsoft Basic") {
                continue;
            }
            let luid = format!("0x{:08x}_0x{:08x}", desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart);
            if out.iter().any(|a| a.luid == luid) {
                continue;
            }
            let dedicated = desc.DedicatedVideoMemory as u64;
            // Integrated GPUs carve out little or no dedicated memory.
            out.push(Adapter { luid, name, dedicated, integrated: dedicated < 1 << 30 });
        }
        out
    }
}

#[cfg(any(target_os = "linux", test))]
mod linux {
    use std::path::Path;

    use super::Gpu;

    fn read(path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    }

    fn num(path: &Path) -> Option<f64> {
        read(path)?.parse().ok()
    }

    pub fn read_drm(root: &Path) -> Vec<Gpu> {
        let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
        let mut cards: Vec<_> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.strip_prefix("card").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())))
            .collect();
        cards.sort();
        cards
            .into_iter()
            .filter_map(|card| {
                let dev = root.join(&card).join("device");
                let vendor = read(&dev.join("vendor"))?;
                let driver = std::fs::read_link(dev.join("driver"))
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_default();
                let make = match vendor.as_str() {
                    "0x1002" => "AMD",
                    "0x8086" => "Intel",
                    "0x10de" => "NVIDIA",
                    _ => "GPU",
                };
                let name = if driver.is_empty() { make.to_string() } else { format!("{make} ({driver})") };
                let idle = read(&dev.join("power/runtime_status")).as_deref() == Some("suspended");
                let mut gpu = Gpu { id: card, name, integrated: vendor == "0x8086", idle, ..Default::default() };
                if idle {
                    return Some(gpu);
                }
                gpu.load = num(&dev.join("gpu_busy_percent")).map(super::clamp_pct);
                gpu.vram_used = num(&dev.join("mem_info_vram_used")).map(|v| v as u64);
                gpu.vram_total = num(&dev.join("mem_info_vram_total")).map(|v| v as u64);
                if vendor == "0x1002" {
                    gpu.integrated = gpu.vram_total.is_some_and(|t| t < 1 << 30);
                }
                if let Some(hwmon) = std::fs::read_dir(dev.join("hwmon")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()) {
                    let h = hwmon.path();
                    gpu.temp = num(&h.join("temp1_input")).map(|v| (v / 1000.0) as f32).filter(|t| *t > 0.0 && *t < 150.0);
                    gpu.power = num(&h.join("power1_average"))
                        .or_else(|| num(&h.join("power1_input")))
                        .map(|v| (v / 1e6) as f32);
                }
                Some(gpu)
            })
            .collect()
    }

    #[cfg(target_os = "linux")]
    pub fn nvidia_smi() -> Option<Vec<Gpu>> {
        let out = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        Some(super::parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_counter_instances() {
        let (pid, luid, eng) = parse_engine("pid_1234_luid_0x00000000_0x0000D1A3_phys_0_eng_3_engtype_3D").unwrap();
        assert_eq!((pid, luid.as_str(), eng.as_str()), (1234, "0x00000000_0x0000d1a3", "0_eng_3"));
        assert_eq!(parse_engine("_Total"), None);
        assert_eq!(parse_adapter("luid_0x00000000_0x0000D1A3_phys_0").as_deref(), Some("0x00000000_0x0000d1a3"));
    }

    #[test]
    fn busiest_engine_sums_processes_and_clamps() {
        let s = |n: &str, v: f64| (n.to_string(), v);
        let samples = [
            s("pid_1_luid_0x0_0xa_phys_0_eng_0_engtype_3D", 30.0),
            s("pid_2_luid_0x0_0xa_phys_0_eng_0_engtype_3D", 25.0),
            s("pid_2_luid_0x0_0xa_phys_0_eng_1_engtype_Copy", 70.0),
            s("pid_3_luid_0x0_0xb_phys_0_eng_0_engtype_3D", 140.0),
            s("pid_4_luid_0x0_0xb_phys_0_eng_0_engtype_3D", -5.0),
        ];
        let (adapters, pids) = engine_loads(&samples);
        assert_eq!(adapters["0x0_0xa"], 70.0);
        assert_eq!(adapters["0x0_0xb"], 100.0);
        assert_eq!(pids[&2], 70.0);
        assert_eq!(pids[&3], 100.0);
        assert!(!pids.contains_key(&4));
    }

    #[test]
    fn picks_the_followed_gpu() {
        let gpu = |id: &str, load: f32, integrated: bool| Gpu { id: id.into(), load: Some(load), integrated, ..Default::default() };
        let list = [gpu("igpu", 50.0, true), gpu("dgpu", 10.0, false)];
        assert_eq!(pick(&list, "busiest").unwrap().id, "igpu");
        assert_eq!(pick(&list, "discrete").unwrap().id, "dgpu");
        assert_eq!(pick(&list[..1], "discrete").unwrap().id, "igpu");
        assert!(pick(&[], "busiest").is_none());
    }

    #[test]
    fn parses_nvidia_smi() {
        let rows = parse_nvidia_smi("0, NVIDIA GeForce RTX 3060, 37, 1024, 12288, 55, 42.10\n1, Broken, [N/A], [N/A], [N/A], [N/A], [N/A]\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].load, Some(37.0));
        assert_eq!(rows[0].vram_total, Some(12288 * 1024 * 1024));
        assert_eq!(rows[0].temp, Some(55.0));
        assert_eq!(rows[1].load, None);
    }

    #[test]
    fn reads_drm_sysfs() {
        let root = std::env::temp_dir().join(format!("notifi-drm-{}", std::process::id()));
        let dev = root.join("card0/device");
        std::fs::create_dir_all(dev.join("hwmon/hwmon3")).unwrap();
        std::fs::create_dir_all(root.join("card0-DP-1")).unwrap();
        std::fs::create_dir_all(root.join("card1/device/power")).unwrap();
        let w = |p: std::path::PathBuf, v: &str| std::fs::write(p, v).unwrap();
        w(dev.join("vendor"), "0x1002\n");
        w(dev.join("gpu_busy_percent"), "42\n");
        w(dev.join("mem_info_vram_used"), "1073741824\n");
        w(dev.join("mem_info_vram_total"), "8589934592\n");
        w(dev.join("hwmon/hwmon3/temp1_input"), "61000\n");
        w(dev.join("hwmon/hwmon3/power1_average"), "35000000\n");
        w(root.join("card1/device/vendor"), "0x8086\n");
        w(root.join("card1/device/power/runtime_status"), "suspended\n");
        let gpus = linux::read_drm(&root);
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].load, Some(42.0));
        assert_eq!(gpus[0].temp, Some(61.0));
        assert_eq!(gpus[0].power, Some(35.0));
        assert!(!gpus[0].integrated);
        assert!(gpus[1].idle && gpus[1].integrated && gpus[1].load.is_none());
    }
}
