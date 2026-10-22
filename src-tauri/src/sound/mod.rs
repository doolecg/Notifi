//! Phase 2, Sound: the volume mixer, per-app output, output switcher, device priority,
//! microphone tools, volume overlay and the music app blocker (spec/sound/*.md).

mod blocker;
mod icon;
mod logic;
pub(crate) mod osd;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{mpsc::Receiver, Mutex},
    time::{Duration, Instant},
};

use notifi_platform::{AudioDevice, AudioEvent, AudioSession, DeviceDirection, DeviceKind, Feature, Platform, Support};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::settings::{Settings, Store};
use logic::{gain_to_perceived, perceived_to_gain};

/// Shortcut actions run by [`run_action`]; bound through `keybinds::FEATURE_SHORTCUTS`.
pub const ACTIONS: &[&str] = &["cycleOutput", "muteMicrophones", "openMixer"];

const ACTIVE_FOR: Duration = Duration::from_secs(10);
const RECENT_FOR: Duration = Duration::from_secs(300);
const QUIET_FOR: Duration = Duration::from_millis(900);
const SWITCH_SETTLE: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct SoundState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    saved: Saved,
    loaded: bool,
    last_active: HashMap<String, Instant>,
    applied: HashSet<String>,
    icons: HashMap<String, Option<String>>,
    devices: HashMap<String, DeviceInfo>,
    default_output: Option<String>,
    default_changed: Option<Instant>,
    quiet_until: Option<Instant>,
    mic_muted: bool,
}

#[derive(Clone)]
struct DeviceInfo {
    kind: DeviceKind,
}

/// What survives a restart: each app's gain, its chosen output and the row order.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    levels: HashMap<String, f32>,
    routes: HashMap<String, String>,
    order: Vec<String>,
}

fn saved_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("sound.json"))
}

fn persist(app: &AppHandle, saved: &Saved) {
    if let Some(path) = saved_path(app) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(saved) {
            let _ = fs::write(path, text);
        }
    }
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<Store>().get()
}

fn flag(s: &Settings, ns: &str, key: &str, default: bool) -> bool {
    s.option(ns, key).and_then(Value::as_bool).unwrap_or(default)
}

fn number(s: &Settings, ns: &str, key: &str, default: f64) -> f64 {
    s.option(ns, key).and_then(Value::as_f64).unwrap_or(default)
}

fn strings(s: &Settings, ns: &str, key: &str) -> Vec<String> {
    s.option(ns, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn supported(platform: &Platform, feature: Feature) -> bool {
    !matches!(platform.capabilities().get(feature), Support::Unsupported(_))
}

/// The loudest a slider may go, as perceived loudness where 1.0 is "100%".
fn boost_cap(platform: &Platform, s: &Settings) -> f32 {
    if supported(platform, Feature::AudioBoost) && flag(s, "volumeMixer", "allowBoost", true) {
        (number(s, "volumeMixer", "boostCap", 200.0) as f32 / 100.0).max(1.0)
    } else {
        1.0
    }
}

fn kind_icon(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Headphones | DeviceKind::Headset => "headphones",
        DeviceKind::Display => "display",
        DeviceKind::Microphone => "mic",
        _ => "speakers",
    }
}

fn quiet(app: &AppHandle) {
    app.state::<SoundState>().inner.lock().unwrap().quiet_until = Some(Instant::now() + QUIET_FOR);
}

// ---- Mixer rows ----

struct Group {
    key: String,
    name: String,
    icon_path: Option<String>,
    ids: Vec<String>,
    volume: f32,
    muted: bool,
    peak: f32,
    device: Option<String>,
}

fn session_key(s: &AudioSession) -> String {
    s.app_id.as_deref().filter(|a| !a.is_empty()).unwrap_or(&s.app_name).to_lowercase()
}

/// One group per app, however many audio sessions it has.
fn group(sessions: Vec<AudioSession>) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for s in sessions {
        let key = session_key(&s);
        match out.iter_mut().find(|g| g.key == key) {
            Some(g) => {
                g.ids.push(s.id);
                g.volume = g.volume.max(s.volume);
                g.muted &= s.muted;
                g.peak = g.peak.max(s.peak);
                g.device = g.device.take().or(s.device_id);
                g.icon_path = g.icon_path.take().or(s.icon_path);
            }
            None => out.push(Group {
                key,
                name: s.app_name,
                icon_path: s.icon_path,
                ids: vec![s.id],
                volume: s.volume,
                muted: s.muted,
                peak: s.peak,
                device: s.device_id,
            }),
        }
    }
    out
}

/// Lists the app groups and re-applies saved levels and routes to sessions not seen before.
fn refresh(app: &AppHandle, platform: &Platform) -> Result<Vec<Group>, String> {
    let sessions = platform.audio().sessions().map_err(|e| e.to_string())?;
    let s = settings(app);
    let groups = group(sessions);
    let remember_levels = flag(&s, "volumeMixer", "rememberLevels", true);
    let remember_routes = flag(&s, "perAppOutput", "rememberDevice", true);
    let cap = boost_cap(platform, &s);
    let outputs: HashSet<String> =
        platform.audio().devices(DeviceDirection::Output).map(|d| d.into_iter().map(|d| d.id).collect()).unwrap_or_default();
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    if !inner.loaded {
        inner.loaded = true;
        inner.saved = saved_path(app).and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    }
    let now = Instant::now();
    let live: HashSet<&String> = groups.iter().flat_map(|g| &g.ids).collect();
    inner.applied.retain(|id| live.contains(id));
    for g in &groups {
        if g.peak > 0.001 || !inner.last_active.contains_key(&g.key) {
            inner.last_active.insert(g.key.clone(), now);
        }
        for id in g.ids.iter().filter(|id| !inner.applied.contains(*id)).cloned().collect::<Vec<_>>() {
            if remember_levels {
                if let Some(gain) = inner.saved.levels.get(&g.key) {
                    let _ = platform.audio().set_session_volume(&id, gain.min(perceived_to_gain(cap)));
                }
            }
            if remember_routes {
                if let Some(device) = inner.saved.routes.get(&g.key).filter(|d| outputs.contains(*d)) {
                    let _ = platform.audio().set_session_output(&id, device);
                }
            }
            inner.applied.insert(id);
        }
        if let Some(device) = g.device.as_ref().filter(|d| !outputs.contains(*d)) {
            let _ = device;
            for id in &g.ids {
                let _ = platform.audio().set_session_output(id, "");
            }
        }
    }
    Ok(groups)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    key: String,
    name: String,
    icon: Option<String>,
    /// Perceived loudness in percent; the slider runs 0 to `max_level`.
    level: f32,
    muted: bool,
    peak: f32,
    device_id: Option<String>,
    route_missing: bool,
    pinned: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    id: String,
    name: String,
    kind: DeviceKind,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    id: String,
    name: String,
    kind: DeviceKind,
    volume: f32,
    muted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    supported: bool,
    per_app: bool,
    boost: bool,
    max_level: f32,
    apps: Vec<Row>,
    outputs: Vec<Output>,
    master: Option<Level>,
    mic: Option<Level>,
    mic_muted: bool,
}

fn level_of(d: AudioDevice) -> Option<Level> {
    d.has_volume.then(|| Level { id: d.id, name: d.name, kind: d.kind, volume: d.volume.clamp(0.0, 1.0), muted: d.muted })
}

fn pinned_index(pinned: &[String], g: &Group) -> Option<usize> {
    pinned.iter().position(|p| {
        let p = p.trim().to_lowercase();
        !p.is_empty() && (p == g.key || p == g.name.to_lowercase())
    })
}

#[tauri::command]
pub async fn mixer_state(app: AppHandle) -> MixerState {
    let platform = app.state::<Platform>().inner().clone();
    let s = settings(&app);
    let cap = boost_cap(&platform, &s);
    let sessions_ok = supported(&platform, Feature::AudioSessions);
    let mut groups = if sessions_ok { refresh(&app, &platform).unwrap_or_default() } else { Vec::new() };

    let pinned = strings(&s, "volumeMixer", "pinnedApps");
    let recent = s.option("volumeMixer", "showApps").and_then(Value::as_str) != Some("active");
    let window = if recent { RECENT_FOR } else { ACTIVE_FOR };
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    let now = Instant::now();
    groups.retain(|g| pinned_index(&pinned, g).is_some() || inner.last_active.get(&g.key).is_some_and(|t| now.duration_since(*t) <= window));
    let order = inner.saved.order.clone();
    groups.sort_by_key(|g| match pinned_index(&pinned, g) {
        Some(i) => (0, i, g.name.to_lowercase()),
        None => (1, order.iter().position(|k| *k == g.key).unwrap_or(usize::MAX), g.name.to_lowercase()),
    });

    let outputs_list = platform.audio().devices(DeviceDirection::Output).unwrap_or_default();
    let ids: HashSet<&String> = outputs_list.iter().map(|d| &d.id).collect();
    let apps = groups
        .iter()
        .map(|g| {
            let source = g.icon_path.clone();
            let icon = inner.icons.entry(g.key.clone()).or_insert_with(|| source.as_deref().and_then(icon::load)).clone();
            let routed = inner.saved.routes.get(&g.key);
            Row {
                key: g.key.clone(),
                name: g.name.clone(),
                icon,
                level: (gain_to_perceived(g.volume) * 100.0).round(),
                muted: g.muted,
                peak: g.peak,
                device_id: g.device.clone(),
                route_missing: g.device.is_none() && routed.is_some_and(|d| !ids.contains(d)),
                pinned: pinned_index(&pinned, g).is_some(),
            }
        })
        .collect();
    let mic_muted = inner.mic_muted;
    drop(inner);

    let audio = platform.audio();
    MixerState {
        supported: sessions_ok,
        per_app: supported(&platform, Feature::AudioPerAppOutput),
        boost: cap > 1.0,
        max_level: cap * 100.0,
        apps,
        outputs: outputs_list
            .into_iter()
            .map(|d| Output { id: d.id, name: d.name, kind: d.kind, is_default: d.is_default })
            .collect(),
        master: audio.default_device(DeviceDirection::Output).ok().flatten().and_then(level_of),
        mic: audio.default_device(DeviceDirection::Input).ok().flatten().and_then(level_of),
        mic_muted,
    }
}

fn with_group<T>(app: &AppHandle, key: &str, f: impl FnOnce(&Platform, &Group) -> T) -> Result<T, String> {
    let platform = app.state::<Platform>().inner().clone();
    let groups = refresh(app, &platform)?;
    let g = groups.iter().find(|g| g.key == key).ok_or("That app is no longer playing sound")?;
    Ok(f(&platform, g))
}

fn save_level(app: &AppHandle, key: &str, gain: f32) {
    if !flag(&settings(app), "volumeMixer", "rememberLevels", true) {
        return;
    }
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    inner.saved.levels.insert(key.to_string(), gain);
    persist(app, &inner.saved);
}

/// Sets an app to `level` percent of perceived loudness. Zero also mutes; anything above unmutes.
#[tauri::command]
pub async fn mixer_set_app(app: AppHandle, key: String, level: f32) -> Result<(), String> {
    let cap = boost_cap(app.state::<Platform>().inner(), &settings(&app));
    let gain = perceived_to_gain((level / 100.0).clamp(0.0, cap));
    with_group(&app, &key, |p, g| {
        for id in &g.ids {
            p.audio().set_session_volume(id, gain).map_err(|e| e.to_string())?;
            let _ = p.audio().set_session_mute(id, gain <= 0.0);
        }
        Ok::<(), String>(())
    })??;
    save_level(&app, &key, gain);
    Ok(())
}

#[tauri::command]
pub async fn mixer_mute_app(app: AppHandle, key: String, muted: bool) -> Result<(), String> {
    with_group(&app, &key, |p, g| {
        g.ids.iter().try_for_each(|id| p.audio().set_session_mute(id, muted).map_err(|e| e.to_string()))
    })?
}

#[tauri::command]
pub async fn mixer_reset_app(app: AppHandle, key: String) -> Result<(), String> {
    with_group(&app, &key, |p, g| {
        for id in &g.ids {
            p.audio().set_session_volume(id, 1.0).map_err(|e| e.to_string())?;
            let _ = p.audio().set_session_mute(id, false);
        }
        Ok::<(), String>(())
    })??;
    save_level(&app, &key, 1.0);
    Ok(())
}

/// Sends an app to `device_id`, or back to the system default when it is empty.
#[tauri::command]
pub async fn mixer_set_route(app: AppHandle, key: String, device_id: String) -> Result<(), String> {
    with_group(&app, &key, |p, g| {
        g.ids.iter().try_for_each(|id| p.audio().set_session_output(id, &device_id).map_err(|e| e.to_string()))
    })??;
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    if device_id.is_empty() {
        inner.saved.routes.remove(&key);
    } else if flag(&settings(&app), "perAppOutput", "rememberDevice", true) {
        inner.saved.routes.insert(key, device_id);
    }
    persist(&app, &inner.saved);
    Ok(())
}

#[tauri::command]
pub async fn reset_routes(app: AppHandle) -> Result<(), String> {
    let platform = app.state::<Platform>().inner().clone();
    for g in refresh(&app, &platform).unwrap_or_default() {
        for id in &g.ids {
            let _ = platform.audio().set_session_output(id, "");
        }
    }
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    inner.saved.routes.clear();
    persist(&app, &inner.saved);
    Ok(())
}

#[tauri::command]
pub async fn mixer_pin(app: AppHandle, store: State<'_, Store>, key: String, name: String, pinned: bool) -> Result<(), String> {
    store
        .update(&app, |s| {
            let values = s.features.entry("volumeMixer".into()).or_default();
            let mut list: Vec<String> = values
                .get("pinnedApps")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            list.retain(|p| !p.eq_ignore_ascii_case(&key) && !p.eq_ignore_ascii_case(&name));
            if pinned {
                list.push(key);
            }
            values.insert("pinnedApps".into(), list.into());
            Ok(())
        })
        .map(|_| ())
}

/// Moves a row one place up or down among the rows shown (`delta` of -1 or 1).
#[tauri::command]
pub async fn mixer_move(app: AppHandle, store: State<'_, Store>, keys: Vec<String>, key: String, delta: i32) -> Result<(), String> {
    let Some(at) = keys.iter().position(|k| *k == key) else { return Ok(()) };
    let to = at as i32 + delta;
    if to < 0 || to as usize >= keys.len() {
        return Ok(());
    }
    let mut keys = keys;
    keys.swap(at, to as usize);
    let pinned: Vec<String> = strings(&settings(&app), "volumeMixer", "pinnedApps");
    let was_pinned = pinned.iter().any(|p| p.eq_ignore_ascii_case(&key));
    if was_pinned {
        let order: Vec<String> = keys.iter().filter(|k| pinned.iter().any(|p| p.eq_ignore_ascii_case(k))).cloned().collect();
        let mut next = order;
        next.extend(pinned.iter().filter(|p| !next_contains(&keys, p)).cloned());
        store
            .update(&app, |s| {
                s.features.entry("volumeMixer".into()).or_default().insert("pinnedApps".into(), next.into());
                Ok(())
            })
            .map(|_| ())
    } else {
        let state = app.state::<SoundState>();
        let mut inner = state.inner.lock().unwrap();
        inner.saved.order = keys;
        persist(&app, &inner.saved);
        Ok(())
    }
}

fn next_contains(keys: &[String], p: &str) -> bool {
    keys.iter().any(|k| k.eq_ignore_ascii_case(p))
}

#[tauri::command]
pub async fn mixer_set_master(app: AppHandle, volume: f32) -> Result<(), String> {
    set_default_level(&app, DeviceDirection::Output, volume)
}

#[tauri::command]
pub async fn mixer_mute_master(app: AppHandle, muted: bool) -> Result<(), String> {
    quiet(&app);
    let p = app.state::<Platform>().inner().clone();
    let device = p.audio().default_device(DeviceDirection::Output).map_err(|e| e.to_string())?.ok_or("No output device")?;
    p.audio().set_device_mute(&device.id, muted).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mixer_set_mic(app: AppHandle, volume: f32) -> Result<(), String> {
    set_default_level(&app, DeviceDirection::Input, volume)
}

#[tauri::command]
pub async fn mixer_mute_mic(app: AppHandle, muted: bool) -> Result<(), String> {
    let p = app.state::<Platform>().inner().clone();
    let device = p.audio().default_device(DeviceDirection::Input).map_err(|e| e.to_string())?.ok_or("No microphone")?;
    p.audio().set_device_mute(&device.id, muted).map_err(|e| e.to_string())?;
    refresh_mic(&app, &p);
    Ok(())
}

#[tauri::command]
pub async fn mixer_mute_all_mics(app: AppHandle) -> Result<bool, String> {
    mute_all_microphones(&app)
}

/// Sets the default device's level; zero also mutes it, anything above unmutes.
fn set_default_level(app: &AppHandle, direction: DeviceDirection, volume: f32) -> Result<(), String> {
    quiet(app);
    let p = app.state::<Platform>().inner().clone();
    let device = p.audio().default_device(direction).map_err(|e| e.to_string())?.ok_or("No device")?;
    let volume = volume.clamp(0.0, 1.0);
    p.audio().set_device_volume(&device.id, volume).map_err(|e| e.to_string())?;
    if (volume <= 0.0) != device.muted {
        let _ = p.audio().set_device_mute(&device.id, volume <= 0.0);
    }
    if direction == DeviceDirection::Input {
        refresh_mic(app, &p);
    }
    Ok(())
}

// ---- Shortcut actions ----

pub fn run_action(app: &AppHandle, action: &str) {
    let app = app.clone();
    let action = action.to_string();
    std::thread::spawn(move || match action.as_str() {
        "cycleOutput" => cycle_output(&app),
        "muteMicrophones" => {
            let _ = mute_all_microphones(&app);
        }
        "openMixer" => crate::panel::show_by_tray(&app, Some("mixer".into())),
        _ => {}
    });
}

fn name_matches(device: &str, wanted: &str) -> bool {
    let wanted = wanted.trim().to_lowercase();
    !wanted.is_empty() && device.to_lowercase().contains(&wanted)
}

fn cycle_output(app: &AppHandle) {
    let p = app.state::<Platform>().inner().clone();
    let s = settings(app);
    let Ok(outputs) = p.audio().devices(DeviceDirection::Output) else { return };
    let wanted = strings(&s, "outputSwitcher", "cycleDevices");
    let eligible: Vec<&AudioDevice> =
        outputs.iter().filter(|d| wanted.is_empty() || wanted.iter().any(|w| name_matches(&d.name, w))).collect();
    let current = outputs.iter().find(|d| d.is_default).map(|d| d.id.as_str());
    if eligible.len() < 2 {
        osd::show(app, osd::text("speakers", "Only one output to switch to".into()));
        return;
    }
    let ids: Vec<String> = eligible.iter().map(|d| d.id.clone()).collect();
    let Some(next) = logic::next_in_cycle(&ids, current) else { return };
    quiet(app);
    if p.audio().set_default_device(&next).is_ok() {
        if let Some(d) = outputs.iter().find(|d| d.id == next) {
            osd::show(app, osd::text(kind_icon(d.kind), d.name.clone()));
        }
    }
}

/// Mutes every microphone, or unmutes them if they are all muted. Returns the new state.
fn mute_all_microphones(app: &AppHandle) -> Result<bool, String> {
    let p = app.state::<Platform>().inner().clone();
    let target = !current_mic_muted(&p);
    let result = p.audio().mute_all_microphones(target);
    refresh_mic(app, &p);
    let s = settings(app);
    if flag(&s, "microphone", "showMuteNotices", true) {
        let notice = match &result {
            Ok(()) if target => osd::text("mic-off", "Microphones muted".into()),
            Ok(()) => osd::text("mic", "Microphones on".into()),
            Err(_) => osd::text("mic-off", "Some microphones could not be changed".into()),
        };
        osd::show(app, notice);
    }
    result.map(|_| target).map_err(|e| e.to_string())
}

fn current_mic_muted(p: &Platform) -> bool {
    p.audio().devices(DeviceDirection::Input).is_ok_and(|d| !d.is_empty() && d.iter().all(|d| d.muted))
}

/// Re-reads whether the microphones are muted and shows or hides the indicator.
fn refresh_mic(app: &AppHandle, p: &Platform) {
    let muted = current_mic_muted(p);
    app.state::<SoundState>().inner.lock().unwrap().mic_muted = muted;
    let s = settings(app);
    osd::mic_indicator(app, muted && s.enabled("microphone") && flag(&s, "microphone", "mutedIndicator", true));
}

// ---- Device events ----

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    blocker::start(app);
    let platform = app.state::<Platform>().inner().clone();
    let events = platform.audio().events();
    let app = app.clone();
    let _ = std::thread::Builder::new().name("notifi-sound".into()).spawn(move || {
        refresh_cache(&app, &platform);
        refresh_mic(&app, &platform);
        std::thread::sleep(Duration::from_secs(3));
        apply_priority(&app, &platform);
        if let Ok(rx) = events {
            listen(&app, &platform, rx);
        }
    });
    Ok(())
}

pub fn on_settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    let touched = |ns: &str| old.features.get(ns) != new.features.get(ns);
    if touched("microphone") {
        let (app, p) = (app.clone(), app.state::<Platform>().inner().clone());
        std::thread::spawn(move || {
            refresh_mic(&app, &p);
            apply_priority(&app, &p);
        });
    }
    if touched("devicePriority") {
        let (app, p) = (app.clone(), app.state::<Platform>().inner().clone());
        std::thread::spawn(move || apply_priority(&app, &p));
    }
}

fn refresh_cache(app: &AppHandle, p: &Platform) {
    let state = app.state::<SoundState>();
    let mut inner = state.inner.lock().unwrap();
    for direction in [DeviceDirection::Output, DeviceDirection::Input] {
        for d in p.audio().devices(direction).unwrap_or_default() {
            if d.is_default && direction == DeviceDirection::Output {
                inner.default_output = Some(d.id.clone());
            }
            inner.devices.insert(d.id, DeviceInfo { kind: d.kind });
        }
    }
}

fn listen(app: &AppHandle, p: &Platform, rx: Receiver<AudioEvent>) {
    while let Ok(event) = rx.recv() {
        match event {
            AudioEvent::DeviceAdded(id) => {
                let known = app.state::<SoundState>().inner.lock().unwrap().devices.get(&id).cloned();
                refresh_cache(app, p);
                let info = known.or_else(|| app.state::<SoundState>().inner.lock().unwrap().devices.get(&id).cloned());
                let (app, p) = (app.clone(), p.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(SWITCH_SETTLE);
                    on_device_added(&app, &p, &id, info);
                });
            }
            AudioEvent::DeviceRemoved(id) => {
                let removed = app.state::<SoundState>().inner.lock().unwrap().devices.get(&id).cloned();
                let (app, p) = (app.clone(), p.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(500));
                    refresh_cache(&app, &p);
                    if let Some(info) = removed {
                        on_headphones_removed(&app, &p, info.kind);
                    }
                    apply_priority(&app, &p);
                    refresh_mic(&app, &p);
                });
            }
            AudioEvent::DefaultChanged { direction, device_id } => {
                let state = app.state::<SoundState>();
                let mut inner = state.inner.lock().unwrap();
                if direction == DeviceDirection::Output {
                    inner.default_output = Some(device_id);
                    inner.default_changed = Some(Instant::now());
                }
            }
            AudioEvent::VolumeChanged { device_id, volume, muted } => on_volume_changed(app, p, &device_id, volume, muted),
            AudioEvent::SessionsChanged => {
                let _ = refresh(app, p);
            }
        }
    }
}

fn on_volume_changed(app: &AppHandle, p: &Platform, device_id: &str, volume: f32, muted: bool) {
    let (info, is_default, hold) = {
        let state = app.state::<SoundState>();
        let inner = state.inner.lock().unwrap();
        let now = Instant::now();
        let hold = inner.quiet_until.is_some_and(|t| now < t) || inner.default_changed.is_some_and(|t| now.duration_since(t) < Duration::from_millis(1200));
        (inner.devices.get(device_id).cloned(), inner.default_output.as_deref() == Some(device_id), hold)
    };
    let Some(info) = info else { return };
    if info.kind == DeviceKind::Microphone {
        refresh_mic(app, p);
        return;
    }
    let s = settings(app);
    if is_default && !hold && flag(&s, "volumeMixer", "showVolumeOsd", true) {
        let level = if muted { 0.0 } else { volume.clamp(0.0, 1.0) };
        osd::show(app, osd::volume(kind_icon(info.kind), format!("{}%", (level * 100.0).round()), level, muted || level <= 0.0));
    }
}

fn on_device_added(app: &AppHandle, p: &Platform, id: &str, info: Option<DeviceInfo>) {
    let s = settings(app);
    let is_input = p.audio().devices(DeviceDirection::Input).is_ok_and(|d| d.iter().any(|d| d.id == id));
    if is_input {
        if app.state::<SoundState>().inner.lock().unwrap().mic_muted {
            let _ = p.audio().mute_all_microphones(true);
        }
    } else if let Some(_) = info.filter(|i| logic::is_headphones(i.kind)) {
        if s.enabled("outputSwitcher") && flag(&s, "outputSwitcher", "setOnConnect", false) {
            quiet(app);
            let _ = p.audio().set_device_volume(id, (number(&s, "outputSwitcher", "volumeOnConnect", 50.0) / 100.0).clamp(0.0, 1.0) as f32);
        }
    }
    apply_priority(app, p);
    refresh_mic(app, p);
}

fn on_headphones_removed(app: &AppHandle, p: &Platform, removed: DeviceKind) {
    let s = settings(app);
    if !s.enabled("outputSwitcher") || !flag(&s, "outputSwitcher", "setOnDisconnect", true) {
        return;
    }
    let Ok(Some(now)) = p.audio().default_device(DeviceDirection::Output) else { return };
    if !logic::lower_on_disconnect(removed, Some(now.kind)) {
        return;
    }
    let volume = (number(&s, "outputSwitcher", "volumeOnDisconnect", 0.0) / 100.0).clamp(0.0, 1.0) as f32;
    quiet(app);
    let _ = p.audio().set_device_volume(&now.id, volume);
    if volume <= 0.0 {
        let _ = p.audio().set_device_mute(&now.id, true);
    }
}

/// Makes the preferred or highest-ranked connected device the default.
fn apply_priority(app: &AppHandle, p: &Platform) {
    let s = settings(app);
    let ranked = s.enabled("devicePriority");
    let ignore_virtual = flag(&s, "devicePriority", "ignoreVirtual", true);
    let mut plans: Vec<(DeviceDirection, Vec<String>)> = Vec::new();
    if ranked {
        plans.push((DeviceDirection::Output, strings(&s, "devicePriority", "outputRanking")));
    }
    let preferred = s.option("microphone", "preferredMicrophone").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    if s.enabled("microphone") && !preferred.is_empty() {
        plans.push((DeviceDirection::Input, vec![preferred]));
    } else if ranked {
        plans.push((DeviceDirection::Input, strings(&s, "devicePriority", "microphoneRanking")));
    }
    for (direction, ranking) in plans {
        let Ok(devices) = p.audio().devices(direction) else { continue };
        let candidates: Vec<logic::Candidate> = devices.iter().map(|d| logic::Candidate { id: d.id.clone(), name: d.name.clone() }).collect();
        let Some(best) = logic::choose_by_priority(&ranking, &candidates, ignore_virtual) else { continue };
        if devices.iter().any(|d| d.is_default && d.id == best) {
            continue;
        }
        quiet(app);
        let _ = p.audio().set_default_device(&best);
    }
}
