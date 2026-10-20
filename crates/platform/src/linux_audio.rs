//! Audio on Linux through `pactl` (PulseAudio or pipewire-pulse) and its JSON output (pactl 16+).
//! Every call runs a short subprocess, so call from a worker thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use crate::caps::Feature;
use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::linux_common::Linux;
use crate::traits::*;

/// Pulse's 100%.
const NORM: f64 = 65536.0;
const MAX_VOLUME: f32 = 4.0;

fn spawn_error(e: std::io::Error) -> PlatformError {
    match e.kind() {
        std::io::ErrorKind::NotFound => PlatformError::unsupported(
            Feature::AudioDevices,
            "pactl not found; install pulseaudio-utils (it also drives PipeWire)",
        ),
        _ => e.into(),
    }
}

fn old_pactl() -> PlatformError {
    PlatformError::unsupported(
        Feature::AudioDevices,
        "pactl gave no usable JSON; pactl from PulseAudio 16 or newer is needed",
    )
}

fn pactl(args: &[&str]) -> Result<String> {
    let out = Command::new("pactl").args(args).stdin(Stdio::null()).output().map_err(spawn_error)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.contains("option") && args.first() == Some(&"-f") {
            return Err(old_pactl());
        }
        let msg = if err.is_empty() { format!("pactl {} failed", args.join(" ")) } else { err };
        return Err(PlatformError::backend("pactl", msg));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn pactl_json(args: &[&str]) -> Result<Value> {
    let mut full = vec!["-f", "json"];
    full.extend_from_slice(args);
    serde_json::from_str(&pactl(&full)?).map_err(|_| old_pactl())
}

fn prop<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get("properties")?.get(key)?.as_str()
}

/// Mean of the channel volumes, 1.0 = 100%; None when the object has no usable volume.
pub(crate) fn average_volume(v: &Value) -> Option<f32> {
    let channels = v.get("volume")?.as_object()?;
    let vals: Vec<f64> = channels.values().filter_map(|c| c.get("value")?.as_f64()).collect();
    if vals.is_empty() {
        return None;
    }
    let avg = (vals.iter().sum::<f64>() / vals.len() as f64 / NORM) as f32;
    avg.is_finite().then_some(avg.max(0.0))
}

pub(crate) fn is_monitor(source: &Value) -> bool {
    prop(source, "device.class") == Some("monitor")
        || source.get("monitor_of_sink").and_then(Value::as_str).is_some_and(|s| !s.is_empty() && s != "n/a")
}

pub(crate) fn device_kind(direction: DeviceDirection, dev: &Value) -> DeviceKind {
    if direction == DeviceDirection::Input {
        return DeviceKind::Microphone;
    }
    let p = |k: &str| prop(dev, k).unwrap_or("").to_ascii_lowercase();
    match p("device.form_factor").as_str() {
        "headphone" => return DeviceKind::Headphones,
        "headset" | "hands-free" | "handset" => return DeviceKind::Headset,
        "tv" => return DeviceKind::Display,
        "speaker" | "internal" | "computer" | "hifi" | "portable" | "car" => return DeviceKind::Speakers,
        _ => {}
    }
    let active = dev.get("active_port").and_then(Value::as_str).unwrap_or("");
    let port_type = dev
        .get("ports")
        .and_then(Value::as_array)
        .and_then(|ports| ports.iter().find(|p| p.get("name").and_then(Value::as_str) == Some(active)))
        .and_then(|p| p.get("type")?.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match port_type.as_str() {
        "headphones" => return DeviceKind::Headphones,
        "headset" | "handsfree" => return DeviceKind::Headset,
        "hdmi" | "tv" => return DeviceKind::Display,
        "speaker" => return DeviceKind::Speakers,
        _ => {}
    }
    let name = dev.get("name").and_then(Value::as_str).unwrap_or("");
    let text = format!("{name} {active} {}", p("device.profile.name")).to_ascii_lowercase();
    if text.contains("hdmi") || text.contains("displayport") {
        return DeviceKind::Display;
    }
    let bluez = p("device.api") == "bluez5" || p("device.bus") == "bluetooth" || name.starts_with("bluez_");
    if bluez {
        let call = ["headset", "handsfree", "hands-free", "hfp", "hsp"].iter().any(|k| text.contains(k));
        return if call { DeviceKind::Headset } else { DeviceKind::Headphones };
    }
    if text.contains("headphone") {
        DeviceKind::Headphones
    } else if text.contains("headset") {
        DeviceKind::Headset
    } else if text.contains("speaker") || text.contains("analog-output") {
        DeviceKind::Speakers
    } else {
        DeviceKind::Other
    }
}

fn prefix(direction: DeviceDirection) -> &'static str {
    match direction {
        DeviceDirection::Output => "sink",
        DeviceDirection::Input => "source",
    }
}

/// Devices from `pactl -f json list sinks|sources`; monitor sources are left out.
pub(crate) fn parse_devices(list: &Value, direction: DeviceDirection, default_name: &str) -> Vec<AudioDevice> {
    let Some(items) = list.as_array() else { return Vec::new() };
    items
        .iter()
        .filter(|d| direction == DeviceDirection::Output || !is_monitor(d))
        .filter_map(|d| {
            let name = d.get("name")?.as_str()?;
            let volume = average_volume(d);
            Some(AudioDevice {
                id: format!("{}:{name}", prefix(direction)),
                name: d.get("description").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(name).to_string(),
                direction,
                kind: device_kind(direction, d),
                is_default: name == default_name,
                volume: volume.unwrap_or(0.0),
                muted: d.get("mute").and_then(Value::as_bool).unwrap_or(false),
                has_volume: volume.is_some(),
            })
        })
        .collect()
}

/// Sessions from `pactl -f json list sink-inputs`; `sinks` resolves the sink index to its name.
pub(crate) fn parse_sessions(inputs: &Value, sinks: &Value, default_sink: &str) -> Vec<AudioSession> {
    let sink_names: HashMap<u64, &str> = sinks
        .as_array()
        .map(|a| a.iter().filter_map(|s| Some((s.get("index")?.as_u64()?, s.get("name")?.as_str()?))).collect())
        .unwrap_or_default();
    let Some(items) = inputs.as_array() else { return Vec::new() };
    items
        .iter()
        .filter_map(|i| {
            let index = i.get("index")?.as_u64()?;
            let sink = i.get("sink").and_then(Value::as_u64).and_then(|s| sink_names.get(&s).copied());
            let owned = |k: &str| prop(i, k).filter(|s| !s.is_empty()).map(str::to_string);
            Some(AudioSession {
                id: index.to_string(),
                pid: prop(i, "application.process.id").and_then(|p| p.parse().ok()),
                app_id: owned("application.process.binary").or_else(|| owned("application.id")),
                app_name: owned("application.name").or_else(|| owned("media.name")).unwrap_or_else(|| "Unknown".into()),
                icon_path: owned("application.icon_name"),
                volume: average_volume(i).unwrap_or(0.0),
                muted: i.get("mute").and_then(Value::as_bool).unwrap_or(false),
                peak: 0.0,
                device_id: sink.filter(|s| *s != default_sink).map(|s| format!("sink:{s}")),
            })
        })
        .collect()
}

struct Defaults {
    sink: String,
    source: String,
}

fn defaults() -> Result<Defaults> {
    let info = pactl_json(&["info"])?;
    let get = |k: &str| info.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Defaults { sink: get("default_sink_name"), source: get("default_source_name") })
}

fn list_devices(direction: DeviceDirection, default_name: &str) -> Result<Vec<AudioDevice>> {
    let kind = match direction {
        DeviceDirection::Output => "sinks",
        DeviceDirection::Input => "sources",
    };
    Ok(parse_devices(&pactl_json(&["list", kind])?, direction, default_name))
}

fn split_id(id: &str) -> Result<(&'static str, &str)> {
    if let Some(n) = id.strip_prefix("sink:") {
        Ok(("sink", n))
    } else if let Some(n) = id.strip_prefix("source:") {
        Ok(("source", n))
    } else {
        Err(PlatformError::NotFound(format!("audio device {id}")))
    }
}

fn session_index(id: &str) -> Result<&str> {
    id.parse::<u32>().map(|_| id).map_err(|_| PlatformError::NotFound(format!("audio session {id}")))
}

fn raw_volume(volume: f32) -> String {
    let v = if volume.is_finite() { volume.clamp(0.0, MAX_VOLUME) } else { 0.0 };
    ((v as f64 * NORM).round() as u32).to_string()
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

/// What `events()` compares between two looks at the server.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Snapshot {
    /// Device id to (volume, muted).
    pub devices: BTreeMap<String, (f32, bool)>,
    pub default_sink: String,
    pub default_source: String,
}

fn snapshot() -> Result<Snapshot> {
    let d = defaults()?;
    let mut devices = BTreeMap::new();
    for dir in [DeviceDirection::Output, DeviceDirection::Input] {
        let def = if dir == DeviceDirection::Output { &d.sink } else { &d.source };
        for dev in list_devices(dir, def)? {
            devices.insert(dev.id, (dev.volume, dev.muted));
        }
    }
    Ok(Snapshot { devices, default_sink: format!("sink:{}", d.sink), default_source: format!("source:{}", d.source) })
}

pub(crate) fn diff_ids<'a>(old: &'a BTreeSet<&'a str>, new: &'a BTreeSet<&'a str>) -> (Vec<&'a str>, Vec<&'a str>) {
    (new.difference(old).copied().collect(), old.difference(new).copied().collect())
}

pub(crate) fn snapshot_events(old: &Snapshot, new: &Snapshot) -> Vec<AudioEvent> {
    let old_ids: BTreeSet<&str> = old.devices.keys().map(String::as_str).collect();
    let new_ids: BTreeSet<&str> = new.devices.keys().map(String::as_str).collect();
    let (added, removed) = diff_ids(&old_ids, &new_ids);
    let mut out: Vec<AudioEvent> = added.into_iter().map(|id| AudioEvent::DeviceAdded(id.into())).collect();
    out.extend(removed.into_iter().map(|id| AudioEvent::DeviceRemoved(id.into())));
    for (direction, was, now) in [
        (DeviceDirection::Output, &old.default_sink, &new.default_sink),
        (DeviceDirection::Input, &old.default_source, &new.default_source),
    ] {
        if was != now {
            out.push(AudioEvent::DefaultChanged { direction, device_id: now.clone() });
        } else if let (Some(a), Some(b)) = (old.devices.get(now), new.devices.get(now)) {
            if a != b {
                out.push(AudioEvent::VolumeChanged { device_id: now.clone(), volume: b.0, muted: b.1 });
            }
        }
    }
    out
}

/// `Event 'change' on sink #53` gives `("change", "sink", Some(53))`.
pub(crate) fn parse_subscribe_line(line: &str) -> Option<(&str, &str, Option<u32>)> {
    let rest = line.trim().strip_prefix("Event '")?;
    let (kind, rest) = rest.split_once("' on ")?;
    match rest.split_once(" #") {
        Some((facility, index)) => Some((kind, facility, Some(index.trim().parse().ok()?))),
        None => Some((kind, rest, None)),
    }
}

impl Audio for Linux {
    fn sessions(&self) -> Result<Vec<AudioSession>> {
        let inputs = pactl_json(&["list", "sink-inputs"])?;
        let sinks = pactl_json(&["list", "sinks"])?;
        Ok(parse_sessions(&inputs, &sinks, &defaults()?.sink))
    }

    fn set_session_volume(&self, session_id: &str, volume: f32) -> Result<()> {
        pactl(&["set-sink-input-volume", session_index(session_id)?, &raw_volume(volume)]).map(drop)
    }

    fn set_session_mute(&self, session_id: &str, muted: bool) -> Result<()> {
        pactl(&["set-sink-input-mute", session_index(session_id)?, flag(muted)]).map(drop)
    }

    fn set_session_output(&self, session_id: &str, device_id: &str) -> Result<()> {
        let sink = if device_id.is_empty() {
            "@DEFAULT_SINK@"
        } else {
            match split_id(device_id)? {
                ("sink", name) => name,
                _ => return Err(PlatformError::NotFound(format!("output device {device_id}"))),
            }
        };
        pactl(&["move-sink-input", session_index(session_id)?, sink]).map(drop)
    }

    fn devices(&self, direction: DeviceDirection) -> Result<Vec<AudioDevice>> {
        let d = defaults()?;
        let def = if direction == DeviceDirection::Output { d.sink } else { d.source };
        list_devices(direction, &def)
    }

    fn default_device(&self, direction: DeviceDirection) -> Result<Option<AudioDevice>> {
        Ok(self.devices(direction)?.into_iter().find(|d| d.is_default))
    }

    fn set_default_device(&self, device_id: &str) -> Result<()> {
        let (kind, name) = split_id(device_id)?;
        pactl(&[&format!("set-default-{kind}"), name]).map(drop)
    }

    fn set_device_volume(&self, device_id: &str, volume: f32) -> Result<()> {
        let (kind, name) = split_id(device_id)?;
        pactl(&[&format!("set-{kind}-volume"), name, &raw_volume(volume)]).map(drop)
    }

    fn set_device_mute(&self, device_id: &str, muted: bool) -> Result<()> {
        let (kind, name) = split_id(device_id)?;
        pactl(&[&format!("set-{kind}-mute"), name, flag(muted)]).map(drop)
    }

    fn mute_all_microphones(&self, muted: bool) -> Result<()> {
        let sources = list_devices(DeviceDirection::Input, "")?;
        let failed: Vec<&str> = sources
            .iter()
            .filter(|d| {
                let name = d.id.strip_prefix("source:").unwrap_or(&d.id);
                pactl(&["set-source-mute", name, flag(muted)]).is_err()
            })
            .map(|d| d.name.as_str())
            .collect();
        if failed.is_empty() {
            return Ok(());
        }
        let verb = if muted { "mute" } else { "unmute" };
        Err(PlatformError::backend("microphones", format!("could not {verb} {}", failed.join(", "))))
    }

    fn events(&self) -> Result<EventStream<AudioEvent>> {
        let mut state = snapshot()?;
        let mut child = Command::new("pactl")
            .arg("subscribe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(spawn_error)?;
        let stdout = child.stdout.take().ok_or_else(|| PlatformError::backend("pactl", "no stdout"))?;
        let (raw_tx, raw_rx) = mpsc::channel::<(bool, bool)>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Some((_, facility, _)) = parse_subscribe_line(&line) else { continue };
                let device = matches!(facility, "sink" | "source" | "server" | "card");
                if raw_tx.send((device, facility == "sink-input")).is_err() {
                    break;
                }
            }
        });
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Bursts (a dragged slider, a device appearing) are folded into one look at the server.
            while let Ok(first) = raw_rx.recv() {
                let (mut device, mut sessions) = first;
                let mut n = 0;
                while let Ok((d, s)) = raw_rx.recv_timeout(Duration::from_millis(50)) {
                    device |= d;
                    sessions |= s;
                    n += 1;
                    if n > 256 {
                        break;
                    }
                }
                let mut out = Vec::new();
                if device {
                    match snapshot() {
                        Ok(new) => {
                            out = snapshot_events(&state, &new);
                            state = new;
                        }
                        Err(e) => tracing::debug!(error = %e, "pactl snapshot failed"),
                    }
                }
                if sessions {
                    out.push(AudioEvent::SessionsChanged);
                }
                if out.into_iter().any(|ev| tx.send(ev).is_err()) {
                    let _ = child.kill();
                    break;
                }
            }
            let _ = child.wait();
        });
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vol(values: &[u64]) -> Value {
        let names = ["front-left", "front-right", "rear-left", "rear-right"];
        Value::Object(values.iter().zip(names).map(|(v, n)| (n.to_string(), json!({ "value": v }))).collect())
    }

    #[test]
    fn devices_average_channels_and_skip_monitors() {
        let sinks = json!([
            { "index": 1, "name": "alsa_output.hdmi", "description": "HDMI", "mute": false,
              "volume": vol(&[65536, 32768]), "properties": { "device.class": "sound" } },
            { "index": 2, "name": "bluez_output.AA", "description": "Buds", "mute": true,
              "volume": vol(&[131072, 131072]), "properties": { "device.api": "bluez5" } }
        ]);
        let devs = parse_devices(&sinks, DeviceDirection::Output, "bluez_output.AA");
        assert_eq!(devs.len(), 2);
        assert_eq!(devs[0].id, "sink:alsa_output.hdmi");
        assert!((devs[0].volume - 0.75).abs() < 1e-6);
        assert_eq!(devs[0].kind, DeviceKind::Display);
        assert!(!devs[0].is_default && devs[1].is_default);
        assert_eq!(devs[1].volume, 2.0);
        assert!(devs[1].muted);
        assert_eq!(devs[1].kind, DeviceKind::Headphones);

        let sources = json!([
            { "name": "alsa_output.hdmi.monitor", "monitor_of_sink": "alsa_output.hdmi", "volume": vol(&[65536]) },
            { "name": "alsa_input.mic", "monitor_of_sink": "n/a", "description": "Mic", "volume": {} },
            { "name": "other.monitor", "monitor_of_sink": null, "properties": { "device.class": "monitor" } }
        ]);
        let mics = parse_devices(&sources, DeviceDirection::Input, "alsa_input.mic");
        assert_eq!(mics.len(), 1);
        assert_eq!(mics[0].id, "source:alsa_input.mic");
        assert_eq!(mics[0].kind, DeviceKind::Microphone);
        assert!(!mics[0].has_volume && mics[0].is_default);
    }

    #[test]
    fn kind_mapping() {
        let k = |v: Value| device_kind(DeviceDirection::Output, &v);
        assert_eq!(k(json!({ "properties": { "device.form_factor": "headset" } })), DeviceKind::Headset);
        assert_eq!(k(json!({ "properties": { "device.form_factor": "internal" } })), DeviceKind::Speakers);
        assert_eq!(
            k(json!({ "active_port": "p", "ports": [{ "name": "p", "type": "Headphones" }] })),
            DeviceKind::Headphones
        );
        assert_eq!(k(json!({ "name": "alsa_output.pci.analog-stereo", "active_port": "analog-output-speaker" })), DeviceKind::Speakers);
        assert_eq!(
            k(json!({ "name": "bluez_output.X.1", "properties": { "device.profile.name": "headset-head-unit" } })),
            DeviceKind::Headset
        );
        assert_eq!(k(json!({ "name": "alsa_output.usb-dac" })), DeviceKind::Other);
        assert_eq!(device_kind(DeviceDirection::Input, &json!({})), DeviceKind::Microphone);
    }

    #[test]
    fn sessions_from_sink_inputs() {
        let sinks = json!([{ "index": 1, "name": "speakers" }, { "index": 2, "name": "headset" }]);
        let inputs = json!([
            { "index": 7, "sink": 1, "mute": false, "volume": vol(&[98304, 98304]),
              "properties": { "application.name": "Firefox", "application.process.binary": "firefox",
                              "application.process.id": "4242", "application.icon_name": "firefox" } },
            { "index": 8, "sink": 2, "mute": true, "volume": vol(&[65536]), "properties": { "media.name": "Call" } }
        ]);
        let s = parse_sessions(&inputs, &sinks, "speakers");
        assert_eq!(s[0].id, "7");
        assert_eq!(s[0].pid, Some(4242));
        assert_eq!(s[0].app_id.as_deref(), Some("firefox"));
        assert_eq!(s[0].icon_path.as_deref(), Some("firefox"));
        assert_eq!(s[0].volume, 1.5);
        assert_eq!(s[0].device_id, None);
        assert_eq!(s[1].app_name, "Call");
        assert_eq!(s[1].device_id.as_deref(), Some("sink:headset"));
        assert!(s[1].muted && s[1].peak == 0.0);
    }

    #[test]
    fn subscribe_lines() {
        assert_eq!(parse_subscribe_line("Event 'change' on sink #53"), Some(("change", "sink", Some(53))));
        assert_eq!(parse_subscribe_line("Event 'new' on sink-input #120\n"), Some(("new", "sink-input", Some(120))));
        assert_eq!(parse_subscribe_line("Event 'change' on server"), Some(("change", "server", None)));
        assert_eq!(parse_subscribe_line("garbage"), None);
        assert_eq!(parse_subscribe_line("Event 'remove' on source #x"), None);
    }

    #[test]
    fn snapshot_diffs() {
        let snap = |devs: &[(&str, f32, bool)], sink: &str, source: &str| Snapshot {
            devices: devs.iter().map(|(id, v, m)| (id.to_string(), (*v, *m))).collect(),
            default_sink: sink.into(),
            default_source: source.into(),
        };
        let a = snap(&[("sink:a", 0.5, false), ("source:m", 1.0, false)], "sink:a", "source:m");
        let b = snap(&[("sink:a", 0.6, false), ("sink:b", 1.0, false), ("source:m", 1.0, false)], "sink:a", "source:m");
        assert_eq!(
            snapshot_events(&a, &b),
            vec![
                AudioEvent::DeviceAdded("sink:b".into()),
                AudioEvent::VolumeChanged { device_id: "sink:a".into(), volume: 0.6, muted: false },
            ]
        );
        let c = snap(&[("sink:b", 1.0, false), ("source:m", 1.0, true)], "sink:b", "source:m");
        assert_eq!(
            snapshot_events(&b, &c),
            vec![
                AudioEvent::DeviceRemoved("sink:a".into()),
                AudioEvent::DefaultChanged { direction: DeviceDirection::Output, device_id: "sink:b".into() },
                AudioEvent::VolumeChanged { device_id: "source:m".into(), volume: 1.0, muted: true },
            ]
        );
        assert!(snapshot_events(&c, &c).is_empty());
        let old: BTreeSet<&str> = ["x", "y"].into();
        let new: BTreeSet<&str> = ["y", "z"].into();
        assert_eq!(diff_ids(&old, &new), (vec!["z"], vec!["x"]));
    }

    #[test]
    fn volume_and_ids() {
        assert_eq!(raw_volume(1.0), "65536");
        assert_eq!(raw_volume(9.0), "262144");
        assert_eq!(raw_volume(-1.0), "0");
        assert_eq!(raw_volume(f32::NAN), "0");
        assert_eq!(split_id("source:mic").unwrap(), ("source", "mic"));
        assert!(split_id("mic").is_err());
        assert!(session_index("--help").is_err());
    }
}
