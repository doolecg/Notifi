//! Displays on Windows: GDI monitors and DisplayConfig for names, DDC/CI (Monitor Configuration API) for
//! external brightness, WMI (root\WMI WmiMonitorBrightness) for the built-in panel.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use windows::core::{w, BOOL, BSTR, PCWSTR};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply,
    QueryDisplayConfig, SetVCPFeature, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    PHYSICAL_MONITOR, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR, VT_I4, VT_UI1};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use crate::caps::Feature;
use crate::common::Rect;
use crate::error::{PlatformError, Result};
use crate::traits::*;
use crate::win::Win;
use crate::win_audio::{werr, Com};

const VCP_BRIGHTNESS: u8 = 0x10;

struct Monitor {
    handle: HMONITOR,
    device: String,
    bounds: Rect,
    primary: bool,
    scale: f32,
}

struct Target {
    name: String,
    internal: bool,
}

fn wide_str(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn is_internal(tech: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY) -> bool {
    tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
        || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
        || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
}

/// Display id: the GDI device name, plus "/k" for the k-th screen when several mirror one source.
fn display_id(device: &str, index: usize, count: usize) -> String {
    if count > 1 {
        format!("{device}/{index}")
    } else {
        device.to_string()
    }
}

fn parse_id(id: &str) -> (&str, usize) {
    match id.rsplit_once('/') {
        Some((dev, k)) => match k.parse() {
            Ok(k) => (dev, k),
            Err(_) => (id, 0),
        },
        None => (id, 0),
    }
}

fn percent_of(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }
    ((current.min(max) as f64 * 100.0 / max as f64).round() as u8).min(100)
}

fn value_for(percent: u8, max: u32) -> u32 {
    (percent.min(100) as f64 * max as f64 / 100.0).round() as u32
}

unsafe extern "system" fn collect(mon: HMONITOR, _dc: HDC, _r: *mut RECT, lparam: LPARAM) -> BOOL {
    (*(lparam.0 as *mut Vec<HMONITOR>)).push(mon);
    BOOL(1)
}

unsafe fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut handles as *mut Vec<HMONITOR> as isize));
    handles
        .into_iter()
        .filter_map(|handle| {
            let mut info = MONITORINFOEXW {
                monitorInfo: MONITORINFO { cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32, ..Default::default() },
                ..Default::default()
            };
            if !GetMonitorInfoW(handle, &mut info.monitorInfo).as_bool() {
                return None;
            }
            let r = info.monitorInfo.rcMonitor;
            let (mut dx, mut dy) = (96u32, 96u32);
            let scale = if GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() { dx as f32 / 96.0 } else { 1.0 };
            Some(Monitor {
                handle,
                device: wide_str(&info.szDevice),
                bounds: Rect { x: r.left, y: r.top, width: (r.right - r.left) as u32, height: (r.bottom - r.top) as u32 },
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                scale,
            })
        })
        .collect()
}

/// Active targets per GDI source name, from DisplayConfig.
unsafe fn targets() -> HashMap<String, Vec<Target>> {
    let mut out: HashMap<String, Vec<Target>> = HashMap::new();
    let (mut np, mut nm) = (0u32, 0u32);
    if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
        return out;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
    if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None) != ERROR_SUCCESS {
        return out;
    }
    for p in &paths[..np as usize] {
        let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: p.sourceInfo.adapterId,
                id: p.sourceInfo.id,
            },
            ..Default::default()
        };
        if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
            continue;
        }
        let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: p.targetInfo.adapterId,
                id: p.targetInfo.id,
            },
            ..Default::default()
        };
        let ok = DisplayConfigGetDeviceInfo(&mut tgt.header) == 0;
        let tech = if ok { tgt.outputTechnology } else { p.targetInfo.outputTechnology };
        out.entry(wide_str(&src.viewGdiDeviceName)).or_default().push(Target {
            name: if ok { wide_str(&tgt.monitorFriendlyDeviceName) } else { String::new() },
            internal: is_internal(tech),
        });
    }
    out
}

/// Physical monitors behind an HMONITOR, destroyed on drop.
struct Physical(Vec<PHYSICAL_MONITOR>);

impl Physical {
    unsafe fn of(handle: HMONITOR) -> Option<Self> {
        let mut n = 0u32;
        GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut n).ok()?;
        if n == 0 {
            return None;
        }
        let mut list = vec![PHYSICAL_MONITOR::default(); n as usize];
        GetPhysicalMonitorsFromHMONITOR(handle, &mut list).ok()?;
        Some(Self(list))
    }
}

impl Drop for Physical {
    fn drop(&mut self) {
        let _ = unsafe { DestroyPhysicalMonitors(&self.0) };
    }
}

/// Monitors that answered (or failed) a DDC probe, so unsupported ones aren't retried on every list.
static DDC_PROBED: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

unsafe fn vcp_get(physical: &PHYSICAL_MONITOR, code: u8) -> Option<VcpValue> {
    let (mut cur, mut max) = (0u32, 0u32);
    (GetVCPFeatureAndVCPFeatureReply(physical.hPhysicalMonitor, code, None, &mut cur, Some(&mut max)) != 0)
        .then(|| VcpValue { current: cur.min(u16::MAX as u32) as u16, max: max.min(u16::MAX as u32) as u16 })
}

unsafe fn with_physical<T>(id: &str, f: impl FnOnce(&PHYSICAL_MONITOR) -> Result<T>) -> Result<T> {
    let (device, index) = parse_id(id);
    let mon = monitors().into_iter().find(|m| m.device == device).ok_or_else(|| PlatformError::NotFound(format!("display {id}")))?;
    let phys = Physical::of(mon.handle).ok_or_else(|| PlatformError::unsupported(Feature::DisplayDdc, "no physical monitor"))?;
    let p = phys.0.get(index).or(phys.0.first()).ok_or_else(|| PlatformError::NotFound(format!("display {id}")))?;
    f(p)
}

fn ddc_failed(code: u8) -> PlatformError {
    PlatformError::unsupported(Feature::DisplayDdc, format!("the monitor did not answer DDC/CI VCP 0x{code:02x}"))
}

fn ddc_read(id: &str, code: u8) -> Result<VcpValue> {
    unsafe { with_physical(id, |p| vcp_get(p, code).ok_or_else(|| ddc_failed(code))) }
}

fn ddc_write(id: &str, code: u8, value: u32) -> Result<()> {
    unsafe {
        with_physical(id, |p| if SetVCPFeature(p.hPhysicalMonitor, code, value) != 0 { Ok(()) } else { Err(ddc_failed(code)) })
    }
}

fn ddc_supported(id: &str) -> bool {
    let mut guard = DDC_PROBED.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    *cache.entry(id.to_string()).or_insert_with(|| ddc_read(id, VCP_BRIGHTNESS).is_ok())
}

unsafe fn wmi() -> Result<IWbemServices> {
    let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(werr("WbemLocator"))?;
    let empty = BSTR::new();
    let svc = loc
        .ConnectServer(&BSTR::from(r"ROOT\WMI"), &empty, &empty, &empty, 0, &empty, None)
        .map_err(werr("WMI ConnectServer"))?;
    // RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE
    CoSetProxyBlanket(&svc, 10, 0, PCWSTR::null(), RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)
        .map_err(werr("CoSetProxyBlanket"))?;
    Ok(svc)
}

fn no_panel() -> PlatformError {
    PlatformError::unsupported(Feature::DisplayBrightness, "no WMI brightness control (no built-in panel)")
}

unsafe fn wmi_first(svc: &IWbemServices, query: &str) -> Result<IWbemClassObject> {
    let flags = WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0);
    let en = svc.ExecQuery(&BSTR::from("WQL"), &BSTR::from(query), flags, None).map_err(|_| no_panel())?;
    let mut objs = [None];
    let mut n = 0u32;
    en.Next(WBEM_INFINITE, &mut objs, &mut n).ok().map_err(|_| no_panel())?;
    objs[0].take().filter(|_| n > 0).ok_or_else(no_panel)
}

unsafe fn wmi_get_brightness() -> Result<u8> {
    let _com = Com::init();
    let svc = wmi()?;
    let obj = wmi_first(&svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active = TRUE")?;
    let mut v = VARIANT::default();
    obj.Get(w!("CurrentBrightness"), 0, &mut v, None, None).map_err(werr("WmiMonitorBrightness"))?;
    let out = (v.Anonymous.Anonymous.vt == VT_UI1).then(|| v.Anonymous.Anonymous.Anonymous.bVal);
    let _ = VariantClear(&mut v);
    out.map(|b| b.min(100)).ok_or_else(|| PlatformError::backend("WmiMonitorBrightness", "unexpected value type"))
}

unsafe fn variant_i4(value: i32) -> VARIANT {
    let mut v = VARIANT::default();
    (*v.Anonymous.Anonymous).vt = VT_I4;
    (*v.Anonymous.Anonymous).Anonymous.lVal = value;
    v
}

unsafe fn variant_u8(value: u8) -> VARIANT {
    let mut v = VARIANT::default();
    (*v.Anonymous.Anonymous).vt = VT_UI1;
    (*v.Anonymous.Anonymous).Anonymous.bVal = value;
    v
}

unsafe fn wmi_set_brightness(percent: u8) -> Result<()> {
    let _com = Com::init();
    let svc = wmi()?;
    let obj = wmi_first(&svc, "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active = TRUE")?;
    let mut path = VARIANT::default();
    obj.Get(w!("__PATH"), 0, &mut path, None, None).map_err(werr("WmiMonitorBrightnessMethods"))?;
    let path_str = if path.Anonymous.Anonymous.vt == VT_BSTR {
        (*path.Anonymous.Anonymous.Anonymous.bstrVal).clone()
    } else {
        BSTR::new()
    };
    let _ = VariantClear(&mut path);
    let mut class = None;
    svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), WBEM_GENERIC_FLAG_TYPE(0), None, Some(&mut class), None)
        .map_err(werr("WMI GetObject"))?;
    let class = class.ok_or_else(no_panel)?;
    let (mut sig_in, mut sig_out) = (None, None);
    class.GetMethod(w!("WmiSetBrightness"), 0, &mut sig_in, &mut sig_out).map_err(werr("WMI GetMethod"))?;
    let params = sig_in.ok_or_else(no_panel)?.SpawnInstance(0).map_err(werr("WMI SpawnInstance"))?;
    let mut timeout = variant_i4(0);
    let mut level = variant_u8(percent.min(100));
    params.Put(w!("Timeout"), 0, &timeout, 0).map_err(werr("WMI Put Timeout"))?;
    params.Put(w!("Brightness"), 0, &level, 0).map_err(werr("WMI Put Brightness"))?;
    let _ = VariantClear(&mut timeout);
    let _ = VariantClear(&mut level);
    svc.ExecMethod(&path_str, &BSTR::from("WmiSetBrightness"), WBEM_GENERIC_FLAG_TYPE(0), None, &params, None, None)
        .map_err(werr("WmiSetBrightness"))
}

fn is_internal_id(id: &str) -> Result<bool> {
    let (device, index) = parse_id(id);
    let all = unsafe { targets() };
    let list = all.get(device).ok_or_else(|| PlatformError::NotFound(format!("display {id}")))?;
    Ok(list.get(index).or(list.first()).is_some_and(|t| t.internal))
}

fn list_displays() -> Vec<DisplayInfo> {
    let mons = unsafe { monitors() };
    let all = unsafe { targets() };
    let mut out = Vec::new();
    let mut external_n = 0;
    for m in mons {
        let none = Vec::new();
        let ts = all.get(&m.device).unwrap_or(&none);
        let count = ts.len().max(1);
        for k in 0..count {
            let t = ts.get(k);
            let internal = t.is_some_and(|t| t.internal);
            let id = display_id(&m.device, k, count);
            let name = match t.map(|t| t.name.trim()).filter(|n| !n.is_empty()) {
                Some(n) => n.to_string(),
                None if internal => "Built-in display".into(),
                None => {
                    external_n += 1;
                    format!("Display {external_n}")
                }
            };
            out.push(DisplayInfo {
                ddc: !internal && ddc_supported(&id),
                id,
                name,
                bounds: m.bounds,
                scale: m.scale,
                primary: m.primary,
                internal,
            });
        }
    }
    out
}

#[async_trait]
impl Display for Win {
    async fn list(&self) -> Result<Vec<DisplayInfo>> {
        Ok(list_displays())
    }

    async fn brightness(&self, display_id: &str) -> Result<u8> {
        if is_internal_id(display_id)? {
            unsafe { wmi_get_brightness() }
        } else {
            let v = ddc_read(display_id, VCP_BRIGHTNESS)?;
            Ok(percent_of(v.current as u32, v.max as u32))
        }
    }

    async fn set_brightness(&self, display_id: &str, percent: u8) -> Result<()> {
        if is_internal_id(display_id)? {
            unsafe { wmi_set_brightness(percent) }
        } else {
            let v = ddc_read(display_id, VCP_BRIGHTNESS)?;
            ddc_write(display_id, VCP_BRIGHTNESS, value_for(percent, v.max as u32))
        }
    }

    async fn ddc_get(&self, display_id: &str, vcp: u8) -> Result<VcpValue> {
        ddc_read(display_id, vcp)
    }

    async fn ddc_set(&self, display_id: &str, vcp: u8, value: u16) -> Result<()> {
        ddc_write(display_id, vcp, value as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        assert_eq!(display_id(r"\\.\DISPLAY1", 0, 1), r"\\.\DISPLAY1");
        assert_eq!(display_id(r"\\.\DISPLAY2", 1, 2), r"\\.\DISPLAY2/1");
        assert_eq!(parse_id(r"\\.\DISPLAY2/1"), (r"\\.\DISPLAY2", 1));
        assert_eq!(parse_id(r"\\.\DISPLAY1"), (r"\\.\DISPLAY1", 0));
    }

    #[test]
    fn percent_scaling() {
        assert_eq!(percent_of(50, 100), 50);
        assert_eq!(percent_of(127, 255), 50);
        assert_eq!(percent_of(300, 255), 100);
        assert_eq!(percent_of(5, 0), 0);
        assert_eq!(value_for(50, 255), 128);
        assert_eq!(value_for(100, 100), 100);
        assert_eq!(value_for(150, 100), 100);
    }

    #[test]
    fn lists_displays_with_one_primary() {
        let list = crate::block_on(Display::list(&Win)).unwrap();
        assert!(!list.is_empty());
        assert!(list.iter().any(|d| d.primary));
    }
}
