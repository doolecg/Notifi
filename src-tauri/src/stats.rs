//! Live readings. One sampler thread serves the tray panel (while it's open), the tray
//! readouts and the alerts, at the shortest interval any of them asks for, and reads only
//! what they show. With none of them active it blocks until the panel opens or a setting
//! changes.

use std::{
    collections::HashMap,
    net::UdpSocket,
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use notifi_platform::{BatteryReading, Platform, PowerProfile};
use serde::Serialize;
use serde_json::Value;
use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate, System,
    UpdateKind,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    monitor::{
        alerts,
        gpu::{self, Gpu},
        history::{History, Series},
        memory::{self, Pressure},
        notify,
        procs::{self, AppUsage},
        readouts::{self, Reading},
    },
    panel::PanelState,
    settings::{Settings, Store},
};

const STATS_EVENT: &str = "panel://stats";
const MIN_GAP: Duration = Duration::from_millis(250);
const TOP_APPS: usize = 5;
/// Faster than this is a counter glitch, not traffic (about 100 Gbit/s).
const MAX_RATE: f64 = 12.5e9;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    tab: String,
    history: HistorySnap,
    system: Option<SystemSnap>,
    network: Option<NetSnap>,
    storage: Option<Vec<DiskSnap>>,
    power: Option<PowerSnap>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct HistorySnap {
    cpu: Series,
    mem: Series,
    battery: Series,
    down: Series,
    up: Series,
    watts: Series,
    gpu: Series,
    disk_read: Series,
    disk_write: Series,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSnap {
    cpu: f32,
    cores: Vec<f32>,
    mem_used: u64,
    mem_total: u64,
    mem_available: u64,
    swap_used: u64,
    swap_total: u64,
    cached: Option<u64>,
    compressed: Option<u64>,
    pressure: Pressure,
    uptime: u64,
    top: Vec<AppUsage>,
    temps: Temps,
    battery: Option<BatteryReading>,
    gpus: Vec<Gpu>,
    /// Id of the GPU the settings follow.
    gpu: Option<String>,
}

#[derive(Serialize, Default, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct Temps {
    cpu: Option<f32>,
    gpu: Option<f32>,
    battery: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetSnap {
    down: f64,
    up: f64,
    down_total: u64,
    up_total: u64,
    ip: Option<String>,
    connected: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DiskSnap {
    name: String,
    mount: String,
    fs: String,
    used: u64,
    total: u64,
    removable: bool,
    system: bool,
    /// Bytes per second; `None` until two readings exist.
    read: Option<f64>,
    write: Option<f64>,
    read_total: u64,
    write_total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerSnap {
    battery: Option<BatteryReading>,
    top: Vec<AppUsage>,
    profiles: Vec<PowerProfile>,
}

#[derive(Default)]
struct Histories {
    cpu: History,
    mem: History,
    battery: History,
    down: History,
    up: History,
    watts: History,
    gpu: History,
    disk_read: History,
    disk_write: History,
}

impl Histories {
    fn snap(&self) -> HistorySnap {
        HistorySnap {
            cpu: self.cpu.series(Some(100.0), 0.0),
            mem: self.mem.series(Some(100.0), 0.0),
            battery: self.battery.series(Some(100.0), 0.0),
            down: self.down.series(None, 10_000.0),
            up: self.up.series(None, 10_000.0),
            watts: self.watts.series(None, 10.0),
            gpu: self.gpu.series(Some(100.0), 0.0),
            disk_read: self.disk_read.series(None, 100_000.0),
            disk_write: self.disk_write.series(None, 100_000.0),
        }
    }
}

struct Sampler {
    platform: Platform,
    sys: System,
    networks: Networks,
    disks: Option<Disks>,
    components: Option<Components>,
    gpu: gpu::Sampler,
    #[cfg(windows)]
    mem_query: Option<Option<crate::monitor::pdh::Query>>,
    #[cfg(windows)]
    gpu_query: Option<Option<crate::monitor::pdh::Query>>,
    pid_gpu: HashMap<u32, f32>,
    last_tick: Option<Instant>,
    last_net: Option<Instant>,
    last_disk: Option<Instant>,
    down_total: u64,
    up_total: u64,
    disk_totals: HashMap<String, (u64, u64)>,
    ip: Option<(Instant, Option<String>)>,
    history: Histories,
    alerts: alerts::Engine,
    tray: readouts::Tray,
}

/// What this tick has to read beyond CPU, memory, network and battery.
#[derive(Default, Clone, Copy)]
struct Needs {
    temps: bool,
    gpu: bool,
    disks: bool,
    procs: bool,
}

/// One tick's readings.
#[derive(Default)]
struct Readings {
    down: f64,
    up: f64,
    battery: Option<BatteryReading>,
    extra: memory::Extra,
    pressure: Pressure,
    temps: Temps,
    gpus: Vec<Gpu>,
    disks: Vec<DiskSnap>,
    top: Option<Vec<AppUsage>>,
}

impl Sampler {
    fn new(platform: Platform) -> Self {
        Self {
            platform,
            sys: System::new(),
            networks: Networks::new_with_refreshed_list(),
            disks: None,
            components: None,
            gpu: gpu::Sampler::new(),
            #[cfg(windows)]
            mem_query: None,
            #[cfg(windows)]
            gpu_query: None,
            pid_gpu: HashMap::new(),
            last_tick: None,
            last_net: None,
            last_disk: None,
            down_total: 0,
            up_total: 0,
            disk_totals: HashMap::new(),
            ip: None,
            history: Histories::default(),
            alerts: alerts::Engine::default(),
            tray: readouts::Tray::default(),
        }
    }

    /// Reads what the open tab, the readouts and the alerts need, updates the graphs, the tray
    /// and the alerts, and returns the panel snapshot when the panel is open.
    fn tick(&mut self, app: Option<&AppHandle>, s: &Settings, tab: Option<&str>, tray: Option<&readouts::Config>, alert_cfg: &alerts::Config, interval: Duration) -> Option<Snapshot> {
        let now = Instant::now();
        let resumed = self.last_tick.is_some_and(|t| now.duration_since(t) > interval * 3 + Duration::from_secs(5));
        self.last_tick = Some(now);
        let wants = |r: Reading| tray.is_some_and(|c| c.readings.contains(&r));
        let alert_on = |k: alerts::Kind| alert_cfg.on[k as usize];
        let t = tab.unwrap_or("");
        let needs = Needs {
            temps: t == "system" || wants(Reading::Temperature) || alert_on(alerts::Kind::CpuTemp) || alert_on(alerts::Kind::BatteryTemp),
            gpu: t == "system" || wants(Reading::Gpu),
            disks: t == "storage" || wants(Reading::Disk) || wants(Reading::DiskActivity) || alert_on(alerts::Kind::Disk),
            procs: t == "system" || t == "power",
        };
        let group_helpers = s.option("monitor", "groupHelpers").and_then(Value::as_bool).unwrap_or(true);

        let mut r = self.read(s, needs, resumed);
        if needs.procs {
            r.top = Some(self.top_apps(group_helpers));
        }
        let followed_disk = follow_disk(&r.disks, s);
        let followed_gpu = gpu::pick(&r.gpus, s.option("gpu", "follow").and_then(Value::as_str).unwrap_or("busiest")).cloned();
        self.record(&r, resumed, followed_gpu.as_ref());

        if let Some(app) = app {
            match tray {
                Some(cfg) if self.tray.due(cfg) => {
                    let v = self.readout_values(&r, followed_disk.as_ref(), followed_gpu.as_ref());
                    self.tray.update(app, Some(cfg), &v);
                }
                Some(_) => {}
                None => self.tray.clear(app),
            }
        }

        if alert_cfg.any() {
            let cpu = self.sys.global_cpu_usage();
            if r.top.is_none() && alert_on(alerts::Kind::Cpu) && cpu >= alert_cfg.cpu_limit {
                r.top = Some(self.top_apps(group_helpers));
            }
            let reading = alerts::Reading {
                cpu,
                top_app: r.top.as_ref().and_then(|t| t.first()).map(|a| a.name.clone()),
                cpu_temp: r.temps.cpu,
                pressure: r.pressure,
                disk_free: followed_disk.as_ref().map(|d| (100.0 - share(d.used, d.total), disk_title(d))),
                battery: r.battery.as_ref().map(|b| (b.percent, b.on_ac || b.charging)),
                battery_temp: r.battery.as_ref().and_then(|b| b.temperature_celsius).or(r.temps.battery),
            };
            let max_gap = interval * 3 + Duration::from_secs(10);
            for alert in self.alerts.check(now, alert_cfg, &reading, max_gap) {
                if !(alert_cfg.quiet_focus && notify::do_not_disturb()) {
                    notify::send(alert.title, alert.body, alert_cfg.sound);
                }
            }
        }

        let tab = tab?;
        let mut snap = Snapshot { tab: tab.to_string(), history: self.history.snap(), ..Default::default() };
        match tab {
            "system" => snap.system = Some(self.system_snap(&r, followed_gpu.map(|g| g.id))),
            "network" => snap.network = Some(self.net_snap(&r)),
            "storage" => snap.storage = Some(r.disks.clone()),
            "power" => {
                let profiles = tauri::async_runtime::block_on(self.platform.power().power_profiles()).unwrap_or_default();
                snap.power = Some(PowerSnap { battery: r.battery.clone(), top: r.top.clone().unwrap_or_default(), profiles });
            }
            _ => {}
        }
        Some(snap)
    }

    fn read(&mut self, s: &Settings, needs: Needs, resumed: bool) -> Readings {
        self.sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram().with_swap());
        let mut r = Readings { battery: self.platform.sensors().battery().ok().flatten(), ..Default::default() };
        (r.down, r.up) = self.network(s, resumed);
        r.extra = memory::extra(self.compressed());
        r.pressure = memory::pressure(self.sys.available_memory(), self.sys.total_memory(), &r.extra);
        if needs.temps {
            let components = self.components.get_or_insert_with(Components::new_with_refreshed_list);
            components.refresh(false);
            r.temps = temps(components, s.option("temperatures", "cpuValue").and_then(Value::as_str) == Some("hottest"));
        }
        if needs.gpu {
            r.gpus = self.gpus();
            if r.temps.gpu.is_none() {
                r.temps.gpu = r.gpus.iter().filter_map(|g| g.temp).reduce(f32::max);
            }
        }
        if needs.disks {
            r.disks = self.disks(resumed);
        }
        r
    }

    fn record(&mut self, r: &Readings, resumed: bool, gpu: Option<&Gpu>) {
        let h = &mut self.history;
        h.cpu.push(self.sys.global_cpu_usage());
        h.mem.push(share(self.sys.used_memory(), self.sys.total_memory()));
        if let Some(b) = &r.battery {
            h.battery.push(b.percent);
            if let Some(w) = b.system_watts.or(b.power_watts) {
                h.watts.push(w);
            }
        }
        if !resumed {
            h.down.push(r.down as f32);
            h.up.push(r.up as f32);
        }
        if let Some(load) = gpu.and_then(|g| g.load) {
            h.gpu.push(load);
        }
        if r.disks.iter().any(|d| d.read.is_some()) {
            h.disk_read.push(r.disks.iter().filter_map(|d| d.read).sum::<f64>() as f32);
            h.disk_write.push(r.disks.iter().filter_map(|d| d.write).sum::<f64>() as f32);
        }
    }

    fn readout_values(&self, r: &Readings, disk: Option<&DiskSnap>, gpu: Option<&Gpu>) -> readouts::Values {
        readouts::Values {
            cpu: self.sys.global_cpu_usage(),
            mem: share(self.sys.used_memory(), self.sys.total_memory()),
            pressure: r.pressure,
            gpu: gpu.and_then(|g| g.load),
            disk: disk.map(|d| (share(d.used, d.total), d.total.saturating_sub(d.used), d.used)),
            disk_io: disk.and_then(|d| Some(d.read? + d.write?)),
            down: r.down,
            up: r.up,
            battery: r.battery.as_ref().map(|b| (b.percent, b.seconds_remaining, b.charging)),
            power: r.battery.as_ref().and_then(|b| b.system_watts.or(b.power_watts)),
            fan: None,
            temp: r.temps.cpu,
        }
    }

    fn system_snap(&self, r: &Readings, gpu: Option<String>) -> SystemSnap {
        SystemSnap {
            cpu: self.sys.global_cpu_usage(),
            cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            mem_available: self.sys.available_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            cached: r.extra.cached,
            compressed: r.extra.compressed.filter(|c| *c > 0),
            pressure: r.pressure,
            uptime: System::uptime(),
            top: r.top.clone().unwrap_or_default(),
            temps: r.temps,
            battery: r.battery.clone(),
            gpus: r.gpus.clone(),
            gpu,
        }
    }

    fn net_snap(&mut self, r: &Readings) -> NetSnap {
        let ip = self.local_ip();
        NetSnap { down: r.down, up: r.up, down_total: self.down_total, up_total: self.up_total, connected: ip.is_some(), ip }
    }

    /// Download and upload rates in bytes per second; zero for the first reading after a gap.
    fn network(&mut self, s: &Settings, resumed: bool) -> (f64, f64) {
        self.networks.refresh(true);
        let now = Instant::now();
        let elapsed = self.last_net.map(|t| now.duration_since(t).as_secs_f64().max(0.001));
        self.last_net = Some(now);
        let only = s.option("network", "interface").and_then(Value::as_str).map(str::trim).unwrap_or("");
        let (mut down, mut up) = (0u64, 0u64);
        for (name, data) in self.networks.list() {
            if counted(name, only) {
                down += data.received();
                up += data.transmitted();
            }
        }
        let Some(secs) = elapsed else { return (0.0, 0.0) };
        let (down_rate, up_rate) = (down as f64 / secs, up as f64 / secs);
        if down_rate > MAX_RATE || up_rate > MAX_RATE {
            return (0.0, 0.0);
        }
        self.down_total += down;
        self.up_total += up;
        if resumed {
            (0.0, 0.0)
        } else {
            (down_rate, up_rate)
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

    fn disks(&mut self, resumed: bool) -> Vec<DiskSnap> {
        let kind = DiskRefreshKind::nothing().with_storage().with_io_usage();
        let first = self.disks.is_none();
        let disks = self.disks.get_or_insert_with(|| Disks::new_with_refreshed_list_specifics(kind));
        if !first {
            disks.refresh_specifics(true, kind);
        }
        let now = Instant::now();
        let elapsed = self.last_disk.map(|t| now.duration_since(t).as_secs_f64().max(0.001)).filter(|_| !first && !resumed);
        self.last_disk = Some(now);
        let system = system_mount();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for d in disks.list().iter().filter(|d| d.total_space() > 0) {
            let mount = d.mount_point().to_string_lossy().into_owned();
            if !seen.insert(mount.clone()) {
                continue;
            }
            let usage = d.usage();
            let totals = self.disk_totals.entry(mount.clone()).or_default();
            if !first {
                totals.0 += usage.read_bytes;
                totals.1 += usage.written_bytes;
            }
            let is_system = mount.eq_ignore_ascii_case(&system);
            out.push(DiskSnap {
                name: d.name().to_string_lossy().into_owned(),
                fs: d.file_system().to_string_lossy().into_owned(),
                used: d.total_space().saturating_sub(d.available_space()),
                total: d.total_space(),
                removable: d.is_removable() && !is_system,
                system: is_system,
                read: elapsed.map(|s| usage.read_bytes as f64 / s),
                write: elapsed.map(|s| usage.written_bytes as f64 / s),
                read_total: totals.0,
                write_total: totals.1,
                mount,
            });
        }
        out
    }

    fn top_apps(&mut self, group_helpers: bool) -> Vec<AppUsage> {
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet);
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let list: Vec<procs::Proc> = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| procs::Proc {
                pid: pid.as_u32(),
                parent: p.parent().map(|p| p.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                dir: p.exe().and_then(Path::parent).map(Path::to_path_buf),
                cpu: p.cpu_usage(),
                mem: p.memory(),
                gpu: self.pid_gpu.get(&pid.as_u32()).copied().unwrap_or(0.0),
            })
            .collect();
        procs::group(&list, self.sys.cpus().len(), group_helpers, TOP_APPS)
    }

    #[cfg(windows)]
    fn compressed(&mut self) -> Option<u64> {
        let query = self.mem_query.get_or_insert_with(|| crate::monitor::pdh::Query::open(&["\\Process(Memory Compression)\\Working Set"])).as_ref()?;
        if !query.collect() {
            return None;
        }
        query.values(0).first().map(|(_, v)| *v as u64)
    }

    #[cfg(not(windows))]
    fn compressed(&mut self) -> Option<u64> {
        None
    }

    #[cfg(windows)]
    fn gpus(&mut self) -> Vec<Gpu> {
        let query = self.gpu_query.get_or_insert_with(|| {
            crate::monitor::pdh::Query::open(&["\\GPU Engine(*)\\Utilization Percentage", "\\GPU Adapter Memory(*)\\Dedicated Usage"])
        });
        let (engine, memory) = match query {
            Some(q) if q.collect() => (q.values(0), q.values(1)),
            _ => (Vec::new(), Vec::new()),
        };
        let (gpus, per_pid) = self.gpu.sample(&engine, &memory);
        self.pid_gpu = per_pid;
        gpus
    }

    #[cfg(not(windows))]
    fn gpus(&mut self) -> Vec<Gpu> {
        let (gpus, per_pid) = self.gpu.sample();
        self.pid_gpu = per_pid;
        gpus
    }
}

fn share(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
    }
}

/// Interfaces added into the totals: the one the settings name, or every physical one.
/// Loopback, virtual switches, VPN tunnels and filter layers would count traffic twice.
fn counted(name: &str, only: &str) -> bool {
    if !only.is_empty() {
        return name.eq_ignore_ascii_case(only);
    }
    let n = name.to_ascii_lowercase();
    let virtual_part = [
        "loopback", "veth", "docker", "virbr", "vethernet", "vmware", "virtualbox", "hyper-v", "isatap", "teredo", "tailscale",
        "wireguard", "zerotier", "filter-", "-wfp", "qos packet", "npcap", "pseudo", "vpn",
    ];
    let virtual_prefix = ["br-", "tun", "tap", "wg", "utun", "lxc", "vnet", "cni", "flannel"];
    !(n == "lo" || virtual_part.iter().any(|p| n.contains(p)) || virtual_prefix.iter().any(|p| n.starts_with(p)))
}

fn system_mount() -> String {
    if cfg!(windows) {
        format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()))
    } else {
        "/".into()
    }
}

/// The disk the settings follow (a mount point or name), else the system disk.
fn follow_disk(disks: &[DiskSnap], s: &Settings) -> Option<DiskSnap> {
    let want = s.option("monitor", "diskToFollow").and_then(Value::as_str).map(str::trim).unwrap_or("");
    let norm = |m: &str| m.trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let picked = (!want.is_empty())
        .then(|| disks.iter().find(|d| norm(&d.mount) == norm(want) || d.name.eq_ignore_ascii_case(want)))
        .flatten();
    picked.or_else(|| disks.iter().find(|d| d.system)).or(disks.first()).cloned()
}

fn disk_title(d: &DiskSnap) -> String {
    if d.name.is_empty() || d.name.starts_with("/dev/") {
        d.mount.clone()
    } else {
        format!("{} ({})", d.name, d.mount.trim_end_matches(['\\', '/']))
    }
}

/// The address the OS would route outward from; connecting a UDP socket sends nothing.
fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| ip.to_string())
}

fn temps(components: &Components, hottest_cpu: bool) -> Temps {
    let mut out = Temps::default();
    let mut other = None;
    let mut cpu_readings = Vec::new();
    for component in components.list() {
        let Some(celsius) = component.temperature().filter(|c| c.is_finite() && *c > 0.0 && *c < 150.0) else { continue };
        let label = component.label().to_ascii_lowercase();
        let is_cpu = !["gpu", "amdgpu", "nouveau", "radeon", "nvidia", "edge", "junction"].iter().any(|k| label.contains(k))
            && !label.contains("bat")
            && ["cpu", "package", "tctl", "tdie", "coretemp", "k10temp", "zenpower", "core"].iter().any(|k| label.contains(k));
        if is_cpu {
            cpu_readings.push((label.clone(), celsius));
            continue;
        }
        let slot = if ["gpu", "amdgpu", "nouveau", "radeon", "nvidia", "edge", "junction"].iter().any(|k| label.contains(k)) {
            &mut out.gpu
        } else if label.contains("bat") {
            &mut out.battery
        } else {
            &mut other
        };
        *slot = Some(slot.map_or(celsius, |c: f32| c.max(celsius)));
    }
    // A plain thermal zone is the only CPU-ish reading some machines give.
    out.cpu = crate::monitor::sensors::cpu_temperature(&cpu_readings, hottest_cpu).or(other);
    out
}

pub fn start(app: &AppHandle) {
    let Some(wake) = app.state::<PanelState>().take_wake() else { return };
    let platform = app.state::<Platform>().inner().clone();
    let app = app.clone();
    std::thread::Builder::new()
        .name("notifi-monitor".into())
        .spawn(move || run(app, platform, wake))
        .expect("spawn stats thread");
}

/// Re-reads the settings at once (readouts or alerts may have been switched on or off).
pub fn settings_changed(app: &AppHandle) {
    app.state::<PanelState>().wake();
}

fn seconds(s: &Settings, key: &str, default: f64) -> Duration {
    Duration::from_secs_f64(s.option("monitor", key).and_then(Value::as_f64).unwrap_or(default).max(1.0))
}

fn run(app: AppHandle, platform: Platform, wake: Receiver<()>) {
    let mut sampler = Sampler::new(platform);
    loop {
        let settings = app.state::<Store>().get();
        let state = app.state::<PanelState>();
        let tab = state.visible().then(|| state.tab());
        let tray = readouts::Config::from_settings(&settings);
        let alert_cfg = alerts::Config::from_settings(&settings);
        let interval = [
            tab.as_ref().map(|_| seconds(&settings, "interval", 2.0)),
            tray.as_ref().map(|c| c.interval),
            alert_cfg.any().then(|| seconds(&settings, "backgroundInterval", 5.0)),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(interval) = interval else {
            sampler.tray.clear(&app);
            if wake.recv().is_err() {
                return;
            }
            continue;
        };
        if let Some(since) = sampler.last_tick.map(|t| t.elapsed()).filter(|e| *e < MIN_GAP) {
            std::thread::sleep(MIN_GAP - since);
        }
        if let Some(snap) = sampler.tick(Some(&app), &settings, tab.as_deref(), tray.as_ref(), &alert_cfg, interval) {
            let _ = app.emit_to(crate::panel::PANEL, STATS_EVENT, &snap);
        }
        if wake.recv_timeout(interval) == Err(RecvTimeoutError::Disconnected) {
            return;
        }
    }
}

#[tauri::command]
pub async fn eject_volume(app: AppHandle, mount: String) -> Result<(), String> {
    if mount.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(system_mount().trim_end_matches(['\\', '/'])) {
        return Err("The system disk can't be ejected.".into());
    }
    let platform = app.state::<Platform>().inner().clone();
    platform.session().eject(&mount).await.map_err(|e| e.to_string())
}

/// Ejects every removable disk; reports the ones that refused.
#[tauri::command]
pub async fn eject_all(app: AppHandle, mounts: Vec<String>) -> Result<(), String> {
    let mut failed = Vec::new();
    for mount in mounts {
        if let Err(e) = eject_volume(app.clone(), mount.clone()).await {
            failed.push(format!("{mount}: {e}"));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

#[tauri::command]
pub async fn set_power_profile(app: AppHandle, id: String) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    platform.power().set_power_profile(&id).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(s: &mut Sampler, tab: &str) -> Snapshot {
        let settings = Settings::default();
        s.tick(None, &settings, Some(tab), None, &alerts::Config::default(), Duration::from_secs(1)).unwrap()
    }

    #[test]
    fn samples_only_the_open_tab() {
        let mut s = Sampler::new(Platform::detect());
        let snap = tick(&mut s, "network");
        assert!(snap.network.is_some() && snap.system.is_none() && snap.storage.is_none());
        let snap = tick(&mut s, "storage");
        assert!(snap.storage.is_some_and(|d| !d.is_empty()));
        assert!(tick(&mut s, "controls").system.is_none());
        assert_eq!(snap.history.cpu.values.len(), 2);
        assert_eq!(snap.history.cpu.scale, 100.0);
    }

    #[test]
    fn system_has_cores_memory_and_pressure() {
        let mut s = Sampler::new(Platform::detect());
        let sys = tick(&mut s, "system").system.unwrap();
        assert!(!sys.cores.is_empty());
        assert!(sys.mem_total > 0 && sys.mem_available <= sys.mem_total && sys.top.len() <= TOP_APPS);
        if cfg!(windows) {
            assert!(sys.cached.is_some());
        }
    }

    #[test]
    fn no_panel_means_no_snapshot() {
        let mut s = Sampler::new(Platform::detect());
        let settings = Settings::default();
        assert!(s.tick(None, &settings, None, None, &alerts::Config::default(), Duration::from_secs(1)).is_none());
    }

    #[test]
    fn virtual_interfaces_are_not_counted() {
        for name in ["lo", "Loopback Pseudo-Interface 1", "docker0", "veth12ab", "vEthernet (WSL)", "tun0", "wg0", "Ethernet-WFP Native MAC Layer LightWeight Filter-0000"] {
            assert!(!counted(name, ""), "{name}");
        }
        for name in ["eth0", "Ethernet", "Wi-Fi", "wlp3s0", "enp0s31f6"] {
            assert!(counted(name, ""), "{name}");
        }
        assert!(counted("tun0", "TUN0"));
        assert!(!counted("eth0", "wlan0"));
    }

    #[test]
    fn follows_the_chosen_disk() {
        let d = |name: &str, mount: &str, system: bool| DiskSnap {
            name: name.into(),
            mount: mount.into(),
            fs: String::new(),
            used: 1,
            total: 2,
            removable: false,
            system,
            read: None,
            write: None,
            read_total: 0,
            write_total: 0,
        };
        let disks = [d("Data", "D:\\", false), d("OS", "C:\\", true)];
        let mut s = Settings::default();
        assert_eq!(follow_disk(&disks, &s).unwrap().mount, "C:\\");
        s.features.get_mut("monitor").unwrap().insert("diskToFollow".into(), "d:".into());
        assert_eq!(follow_disk(&disks, &s).unwrap().mount, "D:\\");
        assert_eq!(disk_title(&disks[0]), "Data (D:)");
        assert!(follow_disk(&[], &s).is_none());
    }
}
