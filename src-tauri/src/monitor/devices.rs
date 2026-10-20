//! Accessory batteries, connected devices and drive health for the System and Storage tabs.
//! Windows reads them with one PowerShell query, cached for a short while; other systems report nothing.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::settings::Store;

const CACHE: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Accessory {
    pub name: String,
    pub percent: u8,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub name: String,
    pub kind: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveHealth {
    pub name: String,
    pub media: String,
    pub health: String,
    pub temperature: Option<f64>,
    pub wear: Option<f64>,
    pub power_on_hours: Option<f64>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Devices {
    pub accessories: Vec<Accessory>,
    pub devices: Vec<Device>,
    pub drives: Vec<DriveHealth>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Want {
    accessories: bool,
    devices: bool,
    drives: bool,
}

static CACHED: Mutex<Option<(Instant, Want, Devices)>> = Mutex::new(None);

/// The PowerShell that prints one JSON object with the parts asked for.
fn script(want: Want) -> String {
    let mut parts = Vec::new();
    if want.accessories {
        parts.push(
            "$a = @(Get-PnpDevice -Class Bluetooth -PresentOnly | Where-Object { $_.FriendlyName } | ForEach-Object { \
             $p = Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName '{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2'; \
             if ($p -and $null -ne $p.Data) { [pscustomobject]@{ name = $_.FriendlyName; percent = [int]$p.Data } } })",
        );
    } else {
        parts.push("$a = @()");
    }
    if want.devices {
        parts.push(
            "$d = @(Get-PnpDevice -PresentOnly | Where-Object { $_.FriendlyName -and $_.InstanceId -like 'USB\\VID_*' -and \
             $_.FriendlyName -notmatch 'Hub|Host Controller|Root|Composite' } | ForEach-Object { \
             [pscustomobject]@{ name = $_.FriendlyName; kind = [string]$_.Class } })",
        );
    } else {
        parts.push("$d = @()");
    }
    if want.drives {
        parts.push(
            "$s = @(Get-PhysicalDisk | ForEach-Object { $r = $_ | Get-StorageReliabilityCounter; \
             [pscustomobject]@{ name = $_.FriendlyName; media = [string]$_.MediaType; health = [string]$_.HealthStatus; \
             temperature = $r.Temperature; wear = $r.Wear; powerOnHours = $r.PowerOnHours } })",
        );
    } else {
        parts.push("$s = @()");
    }
    parts.push("@{ accessories = $a; devices = $d; drives = $s } | ConvertTo-Json -Compress -Depth 4");
    format!("$ErrorActionPreference = 'SilentlyContinue'; {}", parts.join("; "))
}

/// PowerShell prints a one-item list as a bare object and an empty one as nothing; both mean a list.
fn items(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::Object(_)) => vec![v],
        _ => Vec::new(),
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0)
}

pub fn parse(json: &str) -> Devices {
    let Ok(root) = serde_json::from_str::<Value>(json.trim()) else { return Devices::default() };
    let mut out = Devices::default();
    for v in items(root.get("accessories")) {
        let (name, percent) = (text(v, "name"), number(v, "percent"));
        if let (false, Some(p)) = (name.is_empty(), percent) {
            if !out.accessories.iter().any(|a| a.name == name) {
                out.accessories.push(Accessory { name, percent: p.min(100.0) as u8 });
            }
        }
    }
    for v in items(root.get("devices")) {
        let name = text(v, "name");
        if !name.is_empty() && !out.devices.iter().any(|d| d.name == name) {
            out.devices.push(Device { name, kind: text(v, "kind") });
        }
    }
    for v in items(root.get("drives")) {
        let name = text(v, "name");
        if name.is_empty() {
            continue;
        }
        out.drives.push(DriveHealth {
            name,
            media: text(v, "media"),
            health: text(v, "health"),
            temperature: number(v, "temperature").filter(|t| *t > 0.0 && *t < 150.0),
            wear: number(v, "wear").filter(|w| *w <= 100.0),
            power_on_hours: number(v, "powerOnHours"),
        });
    }
    out
}

#[cfg(windows)]
fn query(want: Want) -> Devices {
    use std::{os::windows::process::CommandExt, process::Command};
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(script(want))
        .creation_flags(0x0800_0000)
        .output();
    match out {
        Ok(o) if o.status.success() => parse(&String::from_utf8_lossy(&o.stdout)),
        _ => Devices::default(),
    }
}

#[cfg(not(windows))]
fn query(_want: Want) -> Devices {
    Devices::default()
}

fn wanted(app: &AppHandle) -> Want {
    let s = app.state::<Store>().get();
    let on = |key: &str, default: bool| s.option("monitor", key).and_then(Value::as_bool).unwrap_or(default);
    Want { accessories: on("accessoryBatteries", true), devices: on("connectedDevices", false), drives: on("showSmart", true) }
}

/// What the Storage and System tabs show beyond the plain readings; only the parts that are switched on are read.
#[tauri::command]
pub async fn monitor_devices(app: AppHandle) -> Devices {
    let want = wanted(&app);
    if let Some((at, cached, devices)) = CACHED.lock().unwrap().as_ref() {
        if *cached == want && at.elapsed() < CACHE {
            return devices.clone();
        }
    }
    let devices = tauri::async_runtime::spawn_blocking(move || query(want)).await.unwrap_or_default();
    *CACHED.lock().unwrap() = Some((Instant::now(), want, devices.clone()));
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_and_single_objects() {
        let json = r#"{"accessories":{"name":"MX Keys","percent":64},"devices":[{"name":"Webcam","kind":"Camera"},{"name":"Webcam","kind":"Camera"}],"drives":[{"name":"Samsung SSD","media":"SSD","health":"Healthy","temperature":41,"wear":3,"powerOnHours":1200}]}"#;
        let d = parse(json);
        assert_eq!(d.accessories, vec![Accessory { name: "MX Keys".into(), percent: 64 }]);
        assert_eq!(d.devices.len(), 1, "a device seen twice is listed once");
        assert_eq!(d.drives[0].temperature, Some(41.0));
        assert_eq!(d.drives[0].power_on_hours, Some(1200.0));
    }

    #[test]
    fn odd_values_do_not_break_parsing() {
        let json = r#"{"accessories":[{"name":"","percent":50},{"name":"Mouse","percent":null},{"name":"Pad","percent":250}],"devices":null,"drives":[{"name":"D","media":null,"health":"Unknown","temperature":0,"wear":null}]}"#;
        let d = parse(json);
        assert_eq!(d.accessories, vec![Accessory { name: "Pad".into(), percent: 100 }]);
        assert!(d.devices.is_empty());
        assert_eq!(d.drives[0].temperature, None);
        assert_eq!(d.drives[0].media, "");
        assert_eq!(parse("").accessories.len(), 0);
        assert_eq!(parse("not json"), Devices::default());
    }

    #[test]
    fn script_only_asks_for_what_is_wanted() {
        let s = script(Want { accessories: false, devices: true, drives: false });
        assert!(s.contains("$a = @()") && s.contains("$s = @()"));
        assert!(s.contains("Get-PnpDevice -PresentOnly") && !s.contains("Get-PhysicalDisk"));
    }
}
