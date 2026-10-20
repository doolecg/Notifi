use std::path::Path;
use std::process::Command;

use async_trait::async_trait;
use zbus::zvariant::{OwnedFd, OwnedValue, Value};
use zbus::Connection;

use crate::backend::Backend;
use crate::caps::{Capabilities, Feature, Support};
use crate::common::{system_watts, Guard};
use crate::error::{PlatformError, Result};
use crate::traits::*;
use crate::{gnome, kde, x11};

const APP_NAME: &str = "Notifi";

pub(crate) struct Linux {
    backend: Backend,
}

impl Linux {
    pub(crate) fn new(backend: Backend) -> Self {
        Self { backend }
    }
}

pub(crate) fn capabilities(backend: Backend) -> Capabilities {
    let mut caps = Capabilities::not_implemented();
    caps.set(Feature::KeepAwake, Support::Supported)
        .set(Feature::SensorsBattery, Support::Supported)
        .set(Feature::PowerProfile, Support::Supported)
        .set(Feature::Eject, Support::Supported);
    let dark = match backend {
        Backend::OtherWayland => Support::Partial("read through the desktop settings portal".into()),
        _ => Support::Supported,
    };
    caps.set(Feature::DarkModeRead, dark);
    caps
}

impl Audio for Linux {}
impl WindowManager for Linux {}
impl Input for Linux {}
impl Clipboard for Linux {}
impl Display for Linux {}
impl Capture for Linux {}
impl Packages for Linux {}

impl Sensors for Linux {
    fn battery(&self) -> Result<Option<BatteryReading>> {
        Ok(read_battery(Path::new("/sys/class/power_supply")))
    }
}

#[async_trait]
impl Power for Linux {
    async fn keep_awake(&self, request: KeepAwake) -> Result<Guard> {
        let logind = logind_inhibit("sleep:idle", &request.reason).await;
        if let Err(e) = &logind {
            tracing::warn!(error = %e, "logind inhibit failed");
        }
        let screensaver = if request.keep_display_on || logind.is_err() {
            let r = screensaver_inhibit(&request.reason).await;
            if let Err(e) = &r {
                tracing::warn!(error = %e, "ScreenSaver inhibit failed");
            }
            r.ok()
        } else {
            None
        };
        match (logind, screensaver) {
            (Err(e), None) => Err(e),
            (logind, screensaver) => {
                let fd = logind.ok();
                Ok(Guard::new(move || {
                    drop(fd);
                    drop(screensaver);
                }))
            }
        }
    }

    async fn power_profile(&self) -> Result<String> {
        active_profile().await
    }

    async fn power_profiles(&self) -> Result<Vec<PowerProfile>> {
        profile_list().await
    }

    async fn set_power_profile(&self, id: &str) -> Result<()> {
        set_profile(id).await
    }
}

#[async_trait]
impl Session for Linux {
    async fn eject(&self, volume_id: &str) -> Result<()> {
        let path = volume_id.to_string();
        std::thread::spawn(move || eject_blocking(&path))
            .join()
            .map_err(|_| PlatformError::backend("eject", "worker panicked"))?
    }

    async fn dark_mode(&self) -> Result<bool> {
        match self.backend {
            Backend::GnomeWayland => gnome::dark_mode().await,
            Backend::KdeWayland => kde::dark_mode().await,
            Backend::X11 => x11::dark_mode().await,
            _ => portal_dark_mode().await,
        }
    }
}

fn eject_blocking(mount_point: &str) -> Result<()> {
    let run = |program: &str, args: &[&str]| -> Result<std::result::Result<(), String>> {
        match Command::new(program).args(args).output() {
            Ok(o) if o.status.success() => Ok(Ok(())),
            Ok(o) => Ok(Err(String::from_utf8_lossy(&o.stderr).trim().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Err(format!("{program} not installed"))),
            Err(e) => Err(e.into()),
        }
    };
    let first = match run("gio", &["mount", "-e", mount_point])? {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    let device = mount_device(mount_point).unwrap_or_default();
    match run("udisksctl", &["unmount", "-b", &device])? {
        Ok(()) => Ok(()),
        Err(second) => Err(PlatformError::backend("eject", format!("{first}; {second}"))),
    }
}

/// Block device mounted at `mount_point`, from /proc/mounts.
fn mount_device(mount_point: &str) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let target = mount_point.trim_end_matches('/');
    mounts.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let dev = f.next()?;
        let at = f.next()?.replace("\\040", " ");
        (at.trim_end_matches('/') == target).then(|| dev.to_string())
    })
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_num(path: &Path) -> Option<f64> {
    read_trim(path)?.parse().ok()
}

/// First system battery under `root` (/sys/class/power_supply); None when the machine has none.
pub(crate) fn read_battery(root: &Path) -> Option<BatteryReading> {
    let mut entries: Vec<_> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).collect();
    entries.sort();
    let on_ac = entries.iter().any(|p| {
        read_trim(&p.join("type")).as_deref() == Some("Mains") && read_num(&p.join("online")) == Some(1.0)
    });
    let bat = entries.iter().find(|p| {
        read_trim(&p.join("type")).as_deref() == Some("Battery")
            && read_trim(&p.join("scope")).as_deref() != Some("Device")
    })?;
    let num = |name: &str| read_num(&bat.join(name));
    let status = read_trim(&bat.join("status")).unwrap_or_default();
    let energy = num("energy_now").is_some();
    let (now, full, design, rate) = if energy {
        (num("energy_now"), num("energy_full"), num("energy_full_design"), num("power_now"))
    } else {
        (num("charge_now"), num("charge_full"), num("charge_full_design"), num("current_now"))
    };
    let percent = num("capacity").or_else(|| match (now, full) {
        (Some(n), Some(f)) if f > 0.0 => Some(n / f * 100.0),
        _ => None,
    })?;
    let charging = status == "Charging";
    let seconds_remaining = match (now, full, rate) {
        (Some(n), _, Some(r)) if status == "Discharging" && r > 0.0 => Some((n / r * 3600.0) as u64),
        (Some(n), Some(f), Some(r)) if charging && r > 0.0 && f >= n => Some(((f - n) / r * 3600.0) as u64),
        _ => None,
    };
    let health_percent = match (full, design) {
        (Some(f), Some(d)) if d > 0.0 => Some((f / d * 100.0) as f32),
        _ => None,
    };
    let power_watts = num("power_now")
        .map(|p| (p.abs() / 1e6) as f32)
        .or_else(|| Some((num("current_now")?.abs() * num("voltage_now")? / 1e12) as f32));
    // Energy-based supplies report uWh, charge-based ones uAh (needing the voltage to become Wh).
    let volts = num("voltage_min_design").or(num("voltage_now"));
    let to_mwh = |v: Option<f64>| match (v, energy, volts) {
        (Some(v), true, _) => Some((v / 1000.0) as u32),
        (Some(v), false, Some(volts)) => Some((v * volts / 1e9) as u32),
        _ => None,
    };
    let state = match status.as_str() {
        "Charging" => BatteryState::Charging,
        "Discharging" => BatteryState::Discharging,
        "Full" => BatteryState::Full,
        _ if on_ac && percent >= 100.0 => BatteryState::Full,
        _ => BatteryState::NotCharging,
    };
    let on_ac = on_ac || charging;
    let adapter = entries.iter().find(|p| {
        matches!(read_trim(&p.join("type")).as_deref(), Some("Mains" | "USB" | "USB_C" | "USB_PD" | "USB_DCP"))
            && read_num(&p.join("online")) == Some(1.0)
    });
    let watts = |p: &Path, v: &str, c: &str| Some((read_num(&p.join(v))? * read_num(&p.join(c))? / 1e12) as f32);
    let adapter_watts = adapter.and_then(|p| watts(p, "voltage_now", "current_now")).filter(|w| *w > 0.0);
    let adapter_max_watts = adapter
        .and_then(|p| watts(p, "voltage_max", "current_max").or_else(|| Some((read_num(&p.join("power_max"))? / 1e6) as f32)))
        .filter(|w| *w > 0.0);
    Some(BatteryReading {
        percent: percent.clamp(0.0, 100.0) as f32,
        charging,
        on_ac,
        state,
        power_watts,
        seconds_remaining,
        health_percent,
        cycle_count: num("cycle_count").map(|c| c as u32).filter(|c| *c > 0),
        temperature_celsius: num("temp").map(|t| (t / 10.0) as f32),
        adapter_watts,
        adapter_max_watts,
        system_watts: system_watts(on_ac, charging, power_watts, adapter_watts),
        design_capacity_mwh: to_mwh(design),
        full_capacity_mwh: to_mwh(full),
    })
}

const PPD_SERVICES: [(&str, &str); 2] = [
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
];

/// A property of power-profiles-daemon, from whichever of its two bus names answers.
async fn ppd_property(conn: &Connection, property: &str) -> Result<OwnedValue> {
    let mut last = PlatformError::backend("power profile", "power-profiles-daemon not available");
    for (name, path) in PPD_SERVICES {
        let reply = conn
            .call_method(Some(name), path, Some("org.freedesktop.DBus.Properties"), "Get", &(name, property))
            .await;
        match reply {
            Ok(r) => return Ok(r.body().deserialize::<OwnedValue>()?),
            Err(e) => last = e.into(),
        }
    }
    Err(last)
}

fn platform_profile() -> Option<String> {
    read_trim(Path::new("/sys/firmware/acpi/platform_profile")).filter(|p| !p.is_empty())
}

/// power-profiles-daemon's ActiveProfile, falling back to the ACPI platform profile.
async fn active_profile() -> Result<String> {
    let daemon = match Connection::system().await {
        Ok(conn) => ppd_property(&conn, "ActiveProfile").await,
        Err(e) => Err(e.into()),
    };
    match daemon.map(|v| variant_str(&v).filter(|p| !p.is_empty())) {
        Ok(Some(p)) => Ok(p),
        other => platform_profile().ok_or_else(|| match other {
            Err(e) => e,
            _ => PlatformError::backend("power profile", "no profile source available"),
        }),
    }
}

async fn profile_list() -> Result<Vec<PowerProfile>> {
    let active = active_profile().await?;
    let from_daemon = async {
        let conn = Connection::system().await?;
        let value = ppd_property(&conn, "Profiles").await?;
        let entries: Vec<std::collections::HashMap<String, OwnedValue>> = Value::from(value)
            .try_into()
            .map_err(|_| PlatformError::backend("power profile", "Profiles has an unexpected type"))?;
        Result::Ok(entries.iter().filter_map(|e| variant_str(e.get("Profile")?)).collect::<Vec<_>>())
    };
    let ids = from_daemon.await.ok().filter(|ids| !ids.is_empty()).unwrap_or_else(|| vec![active.clone()]);
    Ok(ids
        .into_iter()
        .map(|id| PowerProfile { name: profile_name(&id), active: id == active, id })
        .collect())
}

fn profile_name(id: &str) -> String {
    let mut chars = id.replace(['-', '_'], " ").chars().collect::<Vec<_>>();
    if let Some(c) = chars.first_mut() {
        *c = c.to_ascii_uppercase();
    }
    chars.into_iter().collect()
}

async fn set_profile(id: &str) -> Result<()> {
    let conn = Connection::system().await?;
    let mut last = PlatformError::backend("power profile", "power-profiles-daemon not available");
    for (name, path) in PPD_SERVICES {
        let reply = conn
            .call_method(
                Some(name),
                path,
                Some("org.freedesktop.DBus.Properties"),
                "Set",
                &(name, "ActiveProfile", Value::from(id)),
            )
            .await;
        match reply {
            Ok(_) => return Ok(()),
            Err(e) => last = e.into(),
        }
    }
    Err(last)
}

fn variant_str(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.to_string()),
        Value::Value(inner) => variant_str(inner),
        _ => None,
    }
}

/// Holding the returned fd keeps the inhibitor; closing it releases.
async fn logind_inhibit(what: &str, why: &str) -> Result<OwnedFd> {
    let conn = Connection::system().await?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &(what, APP_NAME, why, "block"),
        )
        .await?;
    Ok(reply.body().deserialize::<OwnedFd>()?)
}

/// Inhibits idle blanking; releasing calls UnInhibit off-thread.
async fn screensaver_inhibit(why: &str) -> Result<Guard> {
    let conn = Connection::session().await?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.ScreenSaver"),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "Inhibit",
            &(APP_NAME, why),
        )
        .await?;
    let cookie: u32 = reply.body().deserialize()?;
    Ok(Guard::new(move || {
        std::thread::spawn(move || {
            let conn = zbus::blocking::Connection::from(conn);
            if let Err(e) = conn.call_method(
                Some("org.freedesktop.ScreenSaver"),
                "/org/freedesktop/ScreenSaver",
                Some("org.freedesktop.ScreenSaver"),
                "UnInhibit",
                &(cookie),
            ) {
                tracing::warn!(error = %e, "ScreenSaver UnInhibit failed");
            }
        });
    }))
}

/// org.freedesktop.appearance color-scheme from the settings portal: 1 = dark, 2 = light, 0 = no preference.
pub(crate) async fn portal_dark_mode() -> Result<bool> {
    let conn = Connection::session().await?;
    let call = |method: &'static str| {
        let conn = conn.clone();
        async move {
            conn.call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Settings"),
                method,
                &("org.freedesktop.appearance", "color-scheme"),
            )
            .await
        }
    };
    let reply = match call("ReadOne").await {
        Ok(r) => r,
        Err(_) => call("Read").await?,
    };
    let value: OwnedValue = reply.body().deserialize()?;
    match variant_u32(&value) {
        Some(scheme) => Ok(scheme == 1),
        None => Err(PlatformError::backend("settings portal", "color-scheme is not a u32")),
    }
}

fn variant_u32(v: &Value<'_>) -> Option<u32> {
    match v {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => variant_u32(inner),
        _ => None,
    }
}

/// $XDG_CONFIG_HOME or ~/.config.
pub(crate) fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(Into::into)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, files: &[(&str, &str)]) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        for (k, v) in files {
            std::fs::write(d.join(k), v).unwrap();
        }
    }

    #[test]
    fn battery_from_sysfs() {
        let root = std::env::temp_dir().join(format!("notifi-bat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write(&root, "AC", &[("type", "Mains\n"), ("online", "0\n")]);
        write(&root, "hidpp_battery_0", &[("type", "Battery\n"), ("scope", "Device\n"), ("capacity", "5\n")]);
        write(
            &root,
            "BAT0",
            &[
                ("type", "Battery\n"),
                ("status", "Discharging\n"),
                ("capacity", "50\n"),
                ("energy_now", "20000000\n"),
                ("energy_full", "40000000\n"),
                ("energy_full_design", "50000000\n"),
                ("power_now", "10000000\n"),
                ("cycle_count", "212\n"),
                ("temp", "315\n"),
            ],
        );
        let b = read_battery(&root).unwrap();
        assert_eq!(b.percent, 50.0);
        assert!(!b.charging && !b.on_ac);
        assert_eq!(b.seconds_remaining, Some(7200));
        assert_eq!(b.health_percent, Some(80.0));
        assert_eq!(b.power_watts, Some(10.0));
        assert_eq!(b.state, BatteryState::Discharging);
        assert_eq!(b.cycle_count, Some(212));
        assert_eq!(b.temperature_celsius, Some(31.5));
        assert_eq!(b.system_watts, Some(10.0));
        assert_eq!(b.full_capacity_mwh, Some(40000));
        let _ = std::fs::remove_dir_all(&root);
        assert!(read_battery(&root).is_none());
    }
}
