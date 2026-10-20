//! Displays on Linux: connectors from /sys/class/drm, positions from RandR on X11, laptop panels
//! through the backlight class (set via logind), external monitors through `ddcutil`.
//! Calls block on sysfs and subprocesses, so call from a worker.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use zbus::Connection;

use crate::backend::Backend;
use crate::caps::Feature;
use crate::common::Rect;
use crate::error::{PlatformError, Result};
use crate::traits::*;
use crate::x11;

const DRM: &str = "/sys/class/drm";
const BACKLIGHT: &str = "/sys/class/backlight";
const DDC_REFRESH: Duration = Duration::from_secs(30);
const BRIGHTNESS_VCP: u8 = 0x10;

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// `card1-eDP-1` gives `eDP-1`; None when the name isn't a connector.
pub(crate) fn connector_part(id: &str) -> Option<&str> {
    let rest = id.strip_prefix("card")?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let conn = rest[digits..].strip_prefix('-')?;
    (digits > 0 && !conn.is_empty()).then_some(conn)
}

/// Connector type and number: `HDMI-A-1` gives ("HDMI", Some(1)), `DisplayPort-0` ("DP", Some(0)), `eDP` ("EDP", None).
pub(crate) fn split_connector(name: &str) -> (String, Option<u32>) {
    let mut ty = Vec::new();
    let mut num = None;
    for part in name.split('-') {
        match part.parse::<u32>() {
            Ok(n) => {
                num = Some(n);
                break;
            }
            Err(_) => ty.push(part.to_ascii_uppercase()),
        }
    }
    let ty = match ty.join("-").as_str() {
        "HDMI-A" | "HDMI-B" => "HDMI".to_string(),
        "DVI-D" | "DVI-I" | "DVI-A" => "DVI".to_string(),
        "DISPLAYPORT" => "DP".to_string(),
        t => t.to_string(),
    };
    (ty, num)
}

pub(crate) fn is_internal(connector: &str) -> bool {
    matches!(split_connector(connector).0.as_str(), "EDP" | "LVDS" | "DSI")
}

/// The monitor name descriptor (tag 0xFC) of a base EDID block.
pub(crate) fn edid_name(edid: &[u8]) -> Option<String> {
    const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
    if edid.len() < 128 || edid[..8] != HEADER {
        return None;
    }
    [54, 72, 90, 108].iter().find_map(|&o| {
        let d = &edid[o..o + 18];
        if d[0] != 0 || d[1] != 0 || d[3] != 0xFC {
            return None;
        }
        let text: String = d[5..].iter().take_while(|&&b| b != 0x0A && b != 0).map(|&b| b as char).collect();
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    })
}

/// For each DRM connector, the index of the RandR output it most likely is. RandR names follow the
/// X driver (`HDMI-1`, `HDMI-A-0`, `DisplayPort-0`), so match by type and number, allowing for drivers
/// that count from 0, then pair what is left by type in order.
pub(crate) fn match_outputs(connectors: &[&str], outputs: &[&str]) -> Vec<Option<usize>> {
    let conns: Vec<_> = connectors.iter().map(|c| split_connector(c)).collect();
    let outs: Vec<_> = outputs.iter().map(|o| split_connector(o)).collect();
    let zero_based = |ty: &str| outs.iter().any(|(t, n)| t == ty && *n == Some(0));
    let mut used = vec![false; outputs.len()];
    let mut result = vec![None; connectors.len()];
    for (ci, (ty, num)) in conns.iter().enumerate() {
        let shift = u32::from(zero_based(ty));
        let hit = outs.iter().enumerate().position(|(oi, (t, n))| {
            !used[oi] && t == ty && num.is_some() && n.map(|n| n + shift) == *num
        });
        if let Some(oi) = hit {
            used[oi] = true;
            result[ci] = Some(oi);
        }
    }
    for (ci, (ty, _)) in conns.iter().enumerate() {
        if result[ci].is_some() {
            continue;
        }
        if let Some(oi) = outs.iter().enumerate().position(|(oi, (t, _))| !used[oi] && t == ty) {
            used[oi] = true;
            result[ci] = Some(oi);
        }
    }
    result
}

fn connected_ids() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(DRM) else { return Vec::new() };
    let mut ids: Vec<String> = dir
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| connector_part(n).is_some())
        .filter(|n| read_trim(&Path::new(DRM).join(n).join("status")).as_deref() == Some("connected"))
        .collect();
    ids.sort();
    ids
}

/// Connected displays without the DDC probe; RandR positions when `x11`.
pub(crate) fn connected_displays(x11: bool) -> Vec<DisplayInfo> {
    let mut list: Vec<DisplayInfo> = connected_ids()
        .into_iter()
        .map(|id| {
            let conn = connector_part(&id).unwrap_or(&id).to_string();
            let name = std::fs::read(Path::new(DRM).join(&id).join("edid")).ok().and_then(|e| edid_name(&e));
            DisplayInfo {
                name: name.unwrap_or_else(|| conn.clone()),
                internal: is_internal(&conn),
                id,
                bounds: Rect::default(),
                scale: 1.0,
                primary: false,
                ddc: false,
            }
        })
        .collect();
    if x11 {
        match x11::randr_outputs() {
            Ok(outputs) => {
                let conns: Vec<&str> = list.iter().map(|d| connector_part(&d.id).unwrap_or(&d.id)).collect();
                let names: Vec<&str> = outputs.iter().map(|o| o.name.as_str()).collect();
                let matched = match_outputs(&conns, &names);
                for (d, m) in list.iter_mut().zip(matched) {
                    if let Some(o) = m.map(|i| &outputs[i]) {
                        d.bounds = o.bounds;
                        d.primary = o.primary;
                    }
                }
            }
            Err(e) => tracing::debug!(error = %e, "RandR query failed"),
        }
    }
    if !list.iter().any(|d| d.primary) {
        if let Some(d) = list.iter().position(|d| d.internal).or((!list.is_empty()).then_some(0)) {
            list[d].primary = true;
        }
    }
    list
}

pub(crate) fn list(backend: Backend) -> Vec<DisplayInfo> {
    let mut list = connected_displays(backend == Backend::X11);
    let ddc = ddc_displays(true);
    for d in &mut list {
        d.ddc = ddc_bus(&ddc, &d.id).is_some();
    }
    list
}

// ---- DDC/CI ----

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DdcDisplay {
    pub bus: u32,
    pub connector: Option<String>,
}

/// Valid displays from `ddcutil detect --brief`; "Invalid display" blocks are skipped.
pub(crate) fn parse_ddcutil_detect(text: &str) -> Vec<DdcDisplay> {
    let mut out = Vec::new();
    let mut current: Option<DdcDisplay> = None;
    let mut valid = false;
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            out.extend(current.take().filter(|d| d.bus != u32::MAX));
            valid = line.starts_with("Display ");
            if valid {
                current = Some(DdcDisplay { bus: u32::MAX, connector: None });
            }
            continue;
        }
        let (Some(d), true) = (current.as_mut(), valid) else { continue };
        let Some((key, value)) = line.trim().split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "I2C bus" => {
                if let Some(n) = value.rsplit_once("i2c-").and_then(|(_, n)| n.parse().ok()) {
                    d.bus = n;
                }
            }
            "DRM connector" => d.connector = Some(value.to_string()).filter(|c| !c.is_empty()),
            _ => {}
        }
    }
    out.extend(current.filter(|d| d.bus != u32::MAX));
    out
}

/// `VCP 10 C 50 100`, `VCP 60 SNC x0f` or `VCP 14 CNC x00 x0b x00 x05` from `getvcp --brief`.
pub(crate) fn parse_getvcp_brief(text: &str) -> Option<VcpValue> {
    let line = text.lines().find(|l| l.trim_start().starts_with("VCP "))?;
    let t: Vec<&str> = line.split_whitespace().collect();
    let hex = |s: &str| u16::from_str_radix(s.trim_start_matches('x'), 16).ok();
    match t.get(2).copied()? {
        "C" => Some(VcpValue { current: t.get(3)?.parse().ok()?, max: t.get(4)?.parse().ok()? }),
        "SNC" => Some(VcpValue { current: hex(t.get(3)?)?, max: 0 }),
        "CNC" => {
            let b: Vec<u16> = t.get(3..7)?.iter().map(|s| hex(s)).collect::<Option<_>>()?;
            Some(VcpValue { current: b[2] << 8 | b[3], max: b[0] << 8 | b[1] })
        }
        _ => None,
    }
}

fn ddcutil(args: &[&str]) -> Result<String> {
    let out = Command::new("ddcutil").args(args).stdin(Stdio::null()).output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => PlatformError::unsupported(Feature::DisplayDdc, "ddcutil is not installed"),
        _ => e.into(),
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(stdout);
    }
    let msg = format!("{} {}", stdout.trim(), String::from_utf8_lossy(&out.stderr).trim()).trim().to_string();
    if msg.to_ascii_lowercase().contains("permission") {
        return Err(PlatformError::PermissionDenied { what: "i2c device access for ddcutil (i2c group or udev rule)".into() });
    }
    Err(PlatformError::backend("ddcutil", if msg.is_empty() { "failed".to_string() } else { msg }))
}

/// `ddcutil detect` is slow, so its result is kept; `refresh` re-runs it when older than 30 s.
fn ddc_displays(refresh: bool) -> Vec<DdcDisplay> {
    static CACHE: Mutex<Option<(Instant, Vec<DdcDisplay>)>> = Mutex::new(None);
    if let Some((at, list)) = &*CACHE.lock().unwrap_or_else(|e| e.into_inner()) {
        if !refresh || at.elapsed() < DDC_REFRESH {
            return list.clone();
        }
    }
    let list = match ddcutil(&["detect", "--brief"]) {
        Ok(text) => parse_ddcutil_detect(&text),
        Err(e) => {
            tracing::debug!(error = %e, "ddcutil detect failed");
            Vec::new()
        }
    };
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), list.clone()));
    list
}

/// The i2c bus of a connector: as ddcutil names it, else through the connector's `ddc` link.
fn ddc_bus(ddc: &[DdcDisplay], id: &str) -> Option<u32> {
    if let Some(d) = ddc.iter().find(|d| d.connector.as_deref() == Some(id)) {
        return Some(d.bus);
    }
    let link = std::fs::read_link(Path::new(DRM).join(id).join("ddc")).ok()?;
    let bus: u32 = link.file_name()?.to_str()?.strip_prefix("i2c-")?.parse().ok()?;
    ddc.iter().any(|d| d.bus == bus && d.connector.is_none()).then_some(bus)
}

fn display_path(id: &str) -> Result<PathBuf> {
    let path = Path::new(DRM).join(id);
    if connector_part(id).is_none() || id.contains('/') || !path.exists() {
        return Err(PlatformError::NotFound(format!("display {id}")));
    }
    Ok(path)
}

fn bus_for(id: &str) -> Result<String> {
    display_path(id)?;
    ddc_bus(&ddc_displays(false), id)
        .map(|b| b.to_string())
        .ok_or_else(|| PlatformError::unsupported(Feature::DisplayDdc, "this display doesn't answer DDC/CI"))
}

pub(crate) fn ddc_get(id: &str, vcp: u8) -> Result<VcpValue> {
    let bus = bus_for(id)?;
    let text = ddcutil(&["--bus", &bus, "getvcp", &format!("{vcp:02x}"), "--brief"])?;
    parse_getvcp_brief(&text).ok_or_else(|| PlatformError::backend("ddcutil", format!("no value for VCP {vcp:02x}: {}", text.trim())))
}

pub(crate) fn ddc_set(id: &str, vcp: u8, value: u16) -> Result<()> {
    let bus = bus_for(id)?;
    ddcutil(&["--bus", &bus, "setvcp", &format!("{vcp:02x}"), &value.to_string()]).map(drop)
}

// ---- Backlight ----

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Backlight {
    pub name: String,
    /// `firmware`, `platform` or `raw`.
    pub kind: String,
    /// The DRM connector its `device` link resolves to, when it is one.
    pub connector: Option<String>,
}

/// The one linked to `connector`, else firmware before platform before raw.
pub(crate) fn pick_backlight<'a>(list: &'a [Backlight], connector: &str) -> Option<&'a Backlight> {
    let rank = |k: &str| match k {
        "firmware" => 0,
        "platform" => 1,
        "raw" => 2,
        _ => 3,
    };
    list.iter().min_by_key(|b| (b.connector.as_deref() != Some(connector), rank(&b.kind), b.name.clone()))
}

pub(crate) fn percent_of(value: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }
    ((value.min(max) as f64 * 100.0 / max as f64).round()) as u8
}

/// Raw backlight value for `percent`, never below 1% of max so the panel stays readable.
pub(crate) fn backlight_value(percent: u8, max: u32) -> u32 {
    let floor = max.div_ceil(100).max(1).min(max);
    ((percent.min(100) as f64 * max as f64 / 100.0).round() as u32).clamp(floor, max)
}

fn backlights() -> Vec<Backlight> {
    let Ok(dir) = std::fs::read_dir(BACKLIGHT) else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let path = e.path();
            let connector = std::fs::canonicalize(path.join("device"))
                .ok()
                .and_then(|p| p.file_name()?.to_str().map(str::to_string))
                .filter(|n| connector_part(n).is_some());
            Some(Backlight { kind: read_trim(&path.join("type")).unwrap_or_default(), name, connector })
        })
        .collect()
}

fn backlight_for(id: &str) -> Option<Backlight> {
    let conn = connector_part(id)?;
    if !is_internal(conn) {
        return None;
    }
    pick_backlight(&backlights(), id).cloned()
}

fn read_backlight(b: &Backlight) -> Result<(u32, u32)> {
    let dir = Path::new(BACKLIGHT).join(&b.name);
    let num = |f: &str| -> Result<u32> {
        read_trim(&dir.join(f))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| PlatformError::backend("backlight", format!("cannot read {}/{f}", b.name)))
    };
    Ok((num("brightness")?, num("max_brightness")?))
}

pub(crate) fn brightness(id: &str) -> Result<u8> {
    display_path(id)?;
    if let Some(b) = backlight_for(id) {
        let (cur, max) = read_backlight(&b)?;
        return Ok(percent_of(cur, max));
    }
    let v = ddc_get(id, BRIGHTNESS_VCP)?;
    Ok(percent_of(v.current.into(), v.max.into()))
}

pub(crate) async fn set_brightness(id: &str, percent: u8) -> Result<()> {
    display_path(id)?;
    if let Some(b) = backlight_for(id) {
        let (_, max) = read_backlight(&b)?;
        return set_backlight(&b.name, backlight_value(percent, max)).await;
    }
    let max = ddc_get(id, BRIGHTNESS_VCP)?.max;
    let value = (percent.min(100) as f64 * max as f64 / 100.0).round() as u16;
    ddc_set(id, BRIGHTNESS_VCP, value)
}

/// logind's SetBrightness needs no extra rights for the active session; writing sysfs directly is the fallback.
async fn set_backlight(name: &str, value: u32) -> Result<()> {
    let logind = async {
        let conn = Connection::system().await?;
        conn.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1/session/auto",
            Some("org.freedesktop.login1.Session"),
            "SetBrightness",
            &("backlight", name, value),
        )
        .await?;
        Result::Ok(())
    };
    let Err(e) = logind.await else { return Ok(()) };
    std::fs::write(Path::new(BACKLIGHT).join(name).join("brightness"), value.to_string()).map_err(|io| {
        PlatformError::PermissionDenied { what: format!("backlight {name} (logind: {e}; sysfs: {io})") }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_with_name(name: &[u8]) -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        e[54] = 0x01; // a detailed timing descriptor first
        e[72..77].copy_from_slice(&[0, 0, 0, 0xFF, 0]);
        e[90..95].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        let mut text = [0x20u8; 13];
        text[..name.len()].copy_from_slice(name);
        if name.len() < 13 {
            text[name.len()] = 0x0A;
        }
        e[95..108].copy_from_slice(&text);
        e
    }

    #[test]
    fn edid_monitor_name() {
        assert_eq!(edid_name(&edid_with_name(b"DELL U2415")).as_deref(), Some("DELL U2415"));
        assert_eq!(edid_name(&edid_with_name(b"ABCDEFGHIJKLM")).as_deref(), Some("ABCDEFGHIJKLM"));
        let mut bad = edid_with_name(b"X");
        bad[0] = 1;
        assert_eq!(edid_name(&bad), None);
        assert_eq!(edid_name(&[0u8; 16]), None);
    }

    #[test]
    fn connector_names() {
        assert_eq!(connector_part("card1-eDP-1"), Some("eDP-1"));
        assert_eq!(connector_part("card12-HDMI-A-2"), Some("HDMI-A-2"));
        assert_eq!(connector_part("card0"), None);
        assert_eq!(connector_part("renderD128"), None);
        assert_eq!(split_connector("HDMI-A-1"), ("HDMI".into(), Some(1)));
        assert_eq!(split_connector("DisplayPort-0"), ("DP".into(), Some(0)));
        assert_eq!(split_connector("eDP"), ("EDP".into(), None));
        assert!(is_internal("eDP-1") && is_internal("LVDS-1") && !is_internal("DP-2"));
    }

    #[test]
    fn randr_matching() {
        // modesetting driver
        assert_eq!(match_outputs(&["eDP-1", "HDMI-A-1"], &["HDMI-1", "eDP-1", "DP-1"]), vec![Some(1), Some(0)]);
        // amdgpu counts from 0
        assert_eq!(
            match_outputs(&["DP-1", "DP-2", "eDP-1"], &["eDP", "DisplayPort-0", "DisplayPort-1"]),
            vec![Some(1), Some(2), Some(0)]
        );
        // no counterpart
        assert_eq!(match_outputs(&["DSI-1"], &["HDMI-1"]), vec![None]);
    }

    #[test]
    fn ddcutil_detect() {
        let text = "Display 1\n   I2C bus:  /dev/i2c-4\n   DRM connector:           card1-DP-1\n   Monitor:   DEL:DELL U2415:ABC\n\n\
                    Invalid display\n   I2C bus:  /dev/i2c-7\n   DRM connector:  card1-eDP-1\n\n\
                    Display 2\n   I2C bus:  /dev/i2c-5\n   Monitor:  GSM:LG:123\n";
        assert_eq!(
            parse_ddcutil_detect(text),
            vec![
                DdcDisplay { bus: 4, connector: Some("card1-DP-1".into()) },
                DdcDisplay { bus: 5, connector: None },
            ]
        );
        assert!(parse_ddcutil_detect("No displays found.\n").is_empty());
    }

    #[test]
    fn getvcp_brief() {
        assert_eq!(parse_getvcp_brief("VCP 10 C 50 100\n"), Some(VcpValue { current: 50, max: 100 }));
        assert_eq!(parse_getvcp_brief("VCP 60 SNC x0f"), Some(VcpValue { current: 15, max: 0 }));
        assert_eq!(parse_getvcp_brief("VCP 14 CNC x00 x0b x00 x05"), Some(VcpValue { current: 5, max: 11 }));
        assert_eq!(parse_getvcp_brief("VCP 10 ERR"), None);
    }

    #[test]
    fn backlight_choice_and_values() {
        let b = |name: &str, kind: &str, conn: Option<&str>| Backlight {
            name: name.into(),
            kind: kind.into(),
            connector: conn.map(Into::into),
        };
        let list = [b("intel_backlight", "raw", Some("card1-eDP-1")), b("acpi_video0", "firmware", None)];
        assert_eq!(pick_backlight(&list, "card1-eDP-1").unwrap().name, "intel_backlight");
        assert_eq!(pick_backlight(&list, "card0-eDP-1").unwrap().name, "acpi_video0");
        assert!(pick_backlight(&[], "card0-eDP-1").is_none());
        assert_eq!(backlight_value(0, 1000), 10);
        assert_eq!(backlight_value(50, 1000), 500);
        assert_eq!(backlight_value(100, 7), 7);
        assert_eq!(backlight_value(0, 7), 1);
        assert_eq!(percent_of(500, 1000), 50);
        assert_eq!(percent_of(5, 0), 0);
    }
}
