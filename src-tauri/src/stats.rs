//! Live readings for the tray panel. The sampler thread sleeps on a channel while the panel
//! is hidden and, while it's visible, refreshes once a second only the parts the open tab shows.

use std::{
    collections::HashMap,
    net::UdpSocket,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use notifi_platform::{BatteryReading, Platform, PowerProfile};
use serde::Serialize;
use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate, System,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::panel::PanelState;

const STATS_EVENT: &str = "panel://stats";
const INTERVAL: Duration = Duration::from_secs(1);

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    tab: String,
    system: Option<SystemSnap>,
    network: Option<NetSnap>,
    storage: Option<Vec<DiskSnap>>,
    power: Option<PowerSnap>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSnap {
    cpu: f32,
    cores: Vec<f32>,
    mem_used: u64,
    mem_total: u64,
    uptime: u64,
    top: Vec<ProcSnap>,
    temps: Temps,
    battery: Option<BatteryReading>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Temps {
    cpu: Option<f32>,
    gpu: Option<f32>,
    battery: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcSnap {
    name: String,
    cpu: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetSnap {
    down: f64,
    up: f64,
    down_total: u64,
    up_total: u64,
    ip: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskSnap {
    name: String,
    mount: String,
    used: u64,
    total: u64,
    removable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerSnap {
    battery: Option<BatteryReading>,
    top: Vec<ProcSnap>,
    profiles: Vec<PowerProfile>,
}

struct Sampler {
    platform: Platform,
    sys: System,
    networks: Networks,
    components: Option<Components>,
    last_net: Instant,
    down_total: u64,
    up_total: u64,
    ip: Option<(Instant, Option<String>)>,
}

impl Sampler {
    fn new(platform: Platform) -> Self {
        Self {
            platform,
            sys: System::new(),
            networks: Networks::new_with_refreshed_list(),
            components: None,
            last_net: Instant::now(),
            down_total: 0,
            up_total: 0,
            ip: None,
        }
    }

    fn sample(&mut self, tab: &str) -> Snapshot {
        let mut snap = Snapshot { tab: tab.to_string(), ..Default::default() };
        match tab {
            "system" => snap.system = Some(self.system()),
            "network" => snap.network = Some(self.network()),
            "storage" => snap.storage = Some(storage()),
            "power" => snap.power = Some(self.power()),
            _ => {}
        }
        snap
    }

    fn battery(&self) -> Option<BatteryReading> {
        self.platform.sensors().battery().ok().flatten()
    }

    fn system(&mut self) -> SystemSnap {
        self.sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let components = self.components.get_or_insert_with(Components::new_with_refreshed_list);
        components.refresh(false);
        let temps = temps(components);
        SystemSnap {
            cpu: self.sys.global_cpu_usage(),
            cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            uptime: System::uptime(),
            top: self.top_processes(),
            temps,
            battery: self.battery(),
        }
    }

    /// Top 5 apps by CPU, processes of the same app added together, as a share of the whole CPU.
    fn top_processes(&mut self) -> Vec<ProcSnap> {
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu());
        let cores = self.sys.cpus().len().max(1) as f32;
        let mut by_name: HashMap<String, f32> = HashMap::new();
        for process in self.sys.processes().values() {
            let name = process.name().to_string_lossy();
            let name = name.strip_suffix(".exe").unwrap_or(&name);
            if name.is_empty() || name == "System Idle Process" || name == "Idle" {
                continue;
            }
            *by_name.entry(name.to_string()).or_default() += process.cpu_usage() / cores;
        }
        let mut top: Vec<ProcSnap> = by_name.into_iter().map(|(name, cpu)| ProcSnap { name, cpu }).collect();
        top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        top.truncate(5);
        top
    }

    fn network(&mut self) -> NetSnap {
        self.networks.refresh(true);
        let secs = self.last_net.elapsed().as_secs_f64().max(0.001);
        self.last_net = Instant::now();
        let (mut down, mut up) = (0, 0);
        for (name, data) in self.networks.list() {
            if is_loopback(name) {
                continue;
            }
            down += data.received();
            up += data.transmitted();
        }
        self.down_total += down;
        self.up_total += up;
        NetSnap {
            down: down as f64 / secs,
            up: up as f64 / secs,
            down_total: self.down_total,
            up_total: self.up_total,
            ip: self.local_ip(),
        }
    }

    fn local_ip(&mut self) -> Option<String> {
        match &self.ip {
            Some((at, ip)) if at.elapsed() < Duration::from_secs(10) => ip.clone(),
            _ => {
                let ip = local_ip();
                self.ip = Some((Instant::now(), ip.clone()));
                ip
            }
        }
    }

    fn power(&mut self) -> PowerSnap {
        let profiles = tauri::async_runtime::block_on(self.platform.power().power_profiles()).unwrap_or_default();
        PowerSnap { battery: self.battery(), top: self.top_processes(), profiles }
    }
}

fn is_loopback(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "lo" || name.contains("loopback")
}

/// The address the OS would route outward from; connecting a UDP socket sends nothing.
fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| ip.to_string())
}

fn temps(components: &Components) -> Temps {
    let mut out = Temps::default();
    let mut other = None;
    for component in components.list() {
        let Some(celsius) = component.temperature().filter(|c| c.is_finite() && *c > 0.0) else { continue };
        let label = component.label().to_ascii_lowercase();
        let slot = if ["gpu", "amdgpu", "nouveau", "radeon", "nvidia", "edge", "junction"].iter().any(|k| label.contains(k)) {
            &mut out.gpu
        } else if label.contains("bat") {
            &mut out.battery
        } else if ["cpu", "package", "tctl", "tdie", "coretemp", "k10temp", "zenpower", "core"].iter().any(|k| label.contains(k)) {
            &mut out.cpu
        } else {
            &mut other
        };
        *slot = Some(slot.map_or(celsius, |c: f32| c.max(celsius)));
    }
    // A plain thermal zone is the only CPU-ish reading some machines give.
    out.cpu = out.cpu.or(other);
    out
}

fn storage() -> Vec<DiskSnap> {
    let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());
    let mut seen = std::collections::HashSet::new();
    disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter(|d| seen.insert(d.mount_point().to_path_buf()))
        .map(|d| DiskSnap {
            name: d.name().to_string_lossy().into_owned(),
            mount: d.mount_point().to_string_lossy().into_owned(),
            used: d.total_space().saturating_sub(d.available_space()),
            total: d.total_space(),
            removable: d.is_removable(),
        })
        .collect()
}

pub fn start(app: &AppHandle) {
    let Some(wake) = app.state::<PanelState>().take_wake() else { return };
    let platform = app.state::<Platform>().inner().clone();
    let app = app.clone();
    std::thread::Builder::new()
        .name("notifi-panel-stats".into())
        .spawn(move || run(app, platform, wake))
        .expect("spawn stats thread");
}

fn run(app: AppHandle, platform: Platform, wake: Receiver<()>) {
    let mut sampler = Sampler::new(platform);
    let mut next: Option<Instant> = None;
    loop {
        let woke = match next {
            // Hidden: block until the panel opens.
            None => wake.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => wake.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        if woke == Err(RecvTimeoutError::Disconnected) {
            return;
        }
        let state = app.state::<PanelState>();
        if !state.visible() {
            next = None;
            continue;
        }
        let snap = sampler.sample(&state.tab());
        let _ = app.emit_to(crate::panel::PANEL, STATS_EVENT, &snap);
        next = Some(Instant::now() + INTERVAL);
    }
}

#[tauri::command]
pub async fn eject_volume(app: AppHandle, mount: String) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    platform.session().eject(&mount).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_power_profile(app: AppHandle, id: String) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    platform.power().set_power_profile(&id).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_only_the_open_tab() {
        let mut s = Sampler::new(Platform::detect());
        let snap = s.sample("network");
        assert!(snap.network.is_some() && snap.system.is_none() && snap.storage.is_none());
        let snap = s.sample("storage");
        assert!(snap.storage.is_some_and(|d| !d.is_empty()));
        assert!(s.sample("controls").system.is_none());
    }

    #[test]
    fn system_has_cores_and_memory() {
        let mut s = Sampler::new(Platform::detect());
        let sys = s.sample("system").system.unwrap();
        assert!(!sys.cores.is_empty());
        assert!(sys.mem_total > 0 && sys.top.len() <= 5);
    }
}
