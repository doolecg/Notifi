use std::sync::mpsc;

use async_trait::async_trait;
use windows::core::w;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::core::{GUID, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows::Win32::Foundation::{CloseHandle, LocalFree, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Power::{
    BatteryInformation, BatteryTemperature, GetSystemPowerStatus, PowerEnumerate, PowerGetActiveScheme,
    PowerReadFriendlyName, PowerSetActiveScheme, SetThreadExecutionState, ACCESS_SCHEME, BATTERY_CAPACITY_RELATIVE,
    BATTERY_INFORMATION, BATTERY_QUERY_INFORMATION, BATTERY_STATUS, BATTERY_UNKNOWN_RATE, BATTERY_WAIT_STATUS,
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, EXECUTION_STATE, GUID_DEVICE_BATTERY,
    IOCTL_BATTERY_QUERY_INFORMATION, IOCTL_BATTERY_QUERY_STATUS, IOCTL_BATTERY_QUERY_TAG, SYSTEM_POWER_STATUS,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

use crate::caps::{Capabilities, Feature, Support};
use crate::common::{system_watts, Guard};
use crate::error::{PlatformError, Result};
use crate::traits::*;

pub(crate) struct Win;

pub(crate) fn capabilities() -> Capabilities {
    let mut caps = Capabilities::not_implemented();
    caps.set(Feature::KeepAwake, Support::Supported)
        .set(Feature::DarkModeRead, Support::Supported)
        .set(Feature::SensorsBattery, Support::Supported)
        .set(Feature::PowerProfile, Support::Supported)
        .set(Feature::Eject, Support::Supported)
        .set(Feature::AudioSessions, Support::Supported)
        .set(
            Feature::AudioPerAppOutput,
            Support::Partial("uses an undocumented Windows interface that may change in a future build".into()),
        )
        .set(Feature::AudioDevices, Support::Supported)
        .set(Feature::AudioMicrophone, Support::Supported)
        .set(Feature::AudioEvents, Support::Supported)
        .set(
            Feature::AudioBoost,
            Support::Unsupported("boost past 100% needs a system audio driver (APO) such as Equalizer APO".into()),
        )
        .set(Feature::WindowList, Support::Supported)
        .set(Feature::WindowControl, Support::Supported)
        .set(Feature::WindowEvents, Support::Supported)
        .set(Feature::DisplayList, Support::Supported)
        .set(
            Feature::DisplayBrightness,
            Support::Partial("external monitors need DDC/CI switched on in the monitor's menu".into()),
        )
        .set(Feature::DisplayDdc, Support::Partial("only monitors that answer DDC/CI".into()));
    caps
}

impl Input for Win {}
impl Clipboard for Win {}
impl Capture for Win {}
impl Packages for Win {}

impl Sensors for Win {
    fn battery(&self) -> Result<Option<BatteryReading>> {
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut status) }
            .map_err(|e| PlatformError::backend("GetSystemPowerStatus", e))?;
        let Some(mut reading) =
            battery_from_status(status.ACLineStatus, status.BatteryFlag, status.BatteryLifePercent, status.BatteryLifeTime)
        else {
            return Ok(None);
        };
        if let Some(dev) = unsafe { query_battery_device() } {
            apply_device(&mut reading, &dev);
        }
        Ok(Some(reading))
    }
}

fn battery_from_status(ac_line: u8, flag: u8, percent: u8, life_secs: u32) -> Option<BatteryReading> {
    // 128 = no battery; 255 = unknown, which with an unknown percent means none either.
    if flag & 128 != 0 || (flag == 255 && percent == 255) {
        return None;
    }
    let percent = if percent <= 100 { percent as f32 } else { 0.0 };
    let charging = flag != 255 && flag & 8 != 0;
    let on_ac = ac_line == 1;
    let state = match (charging, on_ac) {
        (true, _) => BatteryState::Charging,
        (false, true) if percent >= 100.0 => BatteryState::Full,
        (false, true) => BatteryState::NotCharging,
        (false, false) => BatteryState::Discharging,
    };
    Some(BatteryReading {
        percent,
        charging,
        on_ac,
        state,
        power_watts: None,
        seconds_remaining: (life_secs != u32::MAX).then_some(life_secs as u64),
        health_percent: None,
        cycle_count: None,
        temperature_celsius: None,
        adapter_watts: None,
        adapter_max_watts: None,
        system_watts: None,
        design_capacity_mwh: None,
        full_capacity_mwh: None,
    })
}

#[derive(Default)]
struct BatteryDevice {
    design_mwh: Option<u32>,
    full_mwh: Option<u32>,
    cycles: Option<u32>,
    temperature_c: Option<f32>,
    rate_mw: Option<i32>,
    power_state: Option<u32>,
}

fn apply_device(r: &mut BatteryReading, d: &BatteryDevice) {
    r.design_capacity_mwh = d.design_mwh;
    r.full_capacity_mwh = d.full_mwh;
    r.health_percent = match (d.full_mwh, d.design_mwh) {
        (Some(f), Some(de)) if de > 0 => Some(f as f32 / de as f32 * 100.0),
        _ => None,
    };
    r.cycle_count = d.cycles;
    r.temperature_celsius = d.temperature_c;
    r.power_watts = d.rate_mw.map(|w| w.unsigned_abs() as f32 / 1000.0);
    if let Some(ps) = d.power_state {
        r.charging = ps & 4 != 0;
        r.state = if ps & 4 != 0 {
            BatteryState::Charging
        } else if ps & 2 != 0 {
            BatteryState::Discharging
        } else if r.percent >= 100.0 {
            BatteryState::Full
        } else {
            BatteryState::NotCharging
        };
        r.on_ac = r.on_ac || ps & 1 != 0;
    }
    r.system_watts = system_watts(r.on_ac, r.charging, r.power_watts, r.adapter_watts);
}

unsafe fn ioctl<I, O>(handle: HANDLE, code: u32, input: &I) -> Option<O> {
    let mut out: O = std::mem::zeroed();
    let mut returned = 0u32;
    DeviceIoControl(
        handle,
        code,
        Some(input as *const I as *const _),
        std::mem::size_of::<I>() as u32,
        Some(&mut out as *mut O as *mut _),
        std::mem::size_of::<O>() as u32,
        Some(&mut returned),
        None,
    )
    .ok()?;
    Some(out)
}

/// First present battery's readings through the battery class IOCTLs; anything unreadable stays `None`.
unsafe fn query_battery_device() -> Option<BatteryDevice> {
    let class = GUID_DEVICE_BATTERY;
    let set = SetupDiGetClassDevsW(Some(&class), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE).ok()?;
    let mut found = None;
    let mut index = 0;
    loop {
        let mut iface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiEnumDeviceInterfaces(set, None, &class, index, &mut iface).is_err() {
            break;
        }
        index += 1;
        let mut need = 0u32;
        let _ = SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut need), None);
        if need == 0 {
            continue;
        }
        // u32 storage keeps the detail struct aligned.
        let mut buf = vec![0u32; (need as usize).div_ceil(4)];
        let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        if SetupDiGetDeviceInterfaceDetailW(set, &iface, Some(detail), need, None, None).is_err() {
            continue;
        }
        let path = PCWSTR((*detail).DevicePath.as_ptr());
        let Ok(handle) = CreateFileW(
            path,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        ) else {
            continue;
        };
        found = read_battery_device(handle);
        let _ = CloseHandle(handle);
        if found.is_some() {
            break;
        }
    }
    let _ = SetupDiDestroyDeviceInfoList(set);
    found
}

unsafe fn read_battery_device(handle: HANDLE) -> Option<BatteryDevice> {
    let tag: u32 = ioctl(handle, IOCTL_BATTERY_QUERY_TAG, &0u32).filter(|t| *t != 0)?;
    let query = |level| BATTERY_QUERY_INFORMATION { BatteryTag: tag, InformationLevel: level, AtRate: 0 };
    let mut dev = BatteryDevice::default();
    if let Some(info) = ioctl::<_, BATTERY_INFORMATION>(handle, IOCTL_BATTERY_QUERY_INFORMATION, &query(BatteryInformation)) {
        // Relative capacities carry no mWh meaning.
        if info.Capabilities & BATTERY_CAPACITY_RELATIVE == 0 {
            let known = |v: u32| (v != 0 && v != u32::MAX).then_some(v);
            dev.design_mwh = known(info.DesignedCapacity);
            dev.full_mwh = known(info.FullChargedCapacity);
        }
        dev.cycles = (info.CycleCount != 0).then_some(info.CycleCount);
    }
    // Tenths of a kelvin.
    dev.temperature_c = ioctl::<_, u32>(handle, IOCTL_BATTERY_QUERY_INFORMATION, &query(BatteryTemperature))
        .filter(|t| *t != 0)
        .map(|t| t as f32 / 10.0 - 273.15);
    let wait = BATTERY_WAIT_STATUS { BatteryTag: tag, ..Default::default() };
    if let Some(st) = ioctl::<_, BATTERY_STATUS>(handle, IOCTL_BATTERY_QUERY_STATUS, &wait) {
        dev.power_state = Some(st.PowerState);
        dev.rate_mw = (st.Rate != BATTERY_UNKNOWN_RATE as i32).then_some(st.Rate);
    }
    Some(dev)
}

#[async_trait]
impl Power for Win {
    async fn keep_awake(&self, request: KeepAwake) -> Result<Guard> {
        // The execution state belongs to the calling thread, so a dedicated thread holds it.
        let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
        if request.keep_display_on {
            flags |= ES_DISPLAY_REQUIRED;
        }
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        std::thread::Builder::new().name("notifi-keep-awake".into()).spawn(move || {
            let previous = unsafe { SetThreadExecutionState(flags) };
            let ok = previous != EXECUTION_STATE(0);
            let _ = ready_tx.send(ok);
            if ok {
                let _ = stop_rx.recv();
                unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            }
        })?;
        if !ready_rx.recv().unwrap_or(false) {
            return Err(PlatformError::backend("SetThreadExecutionState", "call failed"));
        }
        tracing::debug!(reason = %request.reason, display = request.keep_display_on, "keep awake on");
        Ok(Guard::new(move || drop(stop_tx)))
    }

    async fn power_profile(&self) -> Result<String> {
        unsafe { read_scheme_name(&active_scheme()?) }
    }

    async fn power_profiles(&self) -> Result<Vec<PowerProfile>> {
        unsafe {
            let active = active_scheme()?;
            let mut out = Vec::new();
            for index in 0.. {
                let mut guid = GUID::zeroed();
                let mut size = std::mem::size_of::<GUID>() as u32;
                let status =
                    PowerEnumerate(None, None, None, ACCESS_SCHEME, index, Some(&mut guid as *mut GUID as *mut u8), &mut size);
                if status != ERROR_SUCCESS {
                    break;
                }
                let name = read_scheme_name(&guid).unwrap_or_else(|_| guid_string(&guid));
                out.push(PowerProfile { id: guid_string(&guid), name, active: guid == active });
            }
            Ok(out)
        }
    }

    async fn set_power_profile(&self, id: &str) -> Result<()> {
        let guid = GUID::try_from(id.trim_matches(|c| c == '{' || c == '}'))
            .map_err(|_| PlatformError::NotFound(format!("power plan {id}")))?;
        let status = unsafe { PowerSetActiveScheme(None, Some(&guid)) };
        if status != ERROR_SUCCESS {
            return Err(PlatformError::backend("PowerSetActiveScheme", format!("error {}", status.0)));
        }
        Ok(())
    }
}

fn guid_string(g: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2..].iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

unsafe fn active_scheme() -> Result<GUID> {
    let mut scheme: *mut GUID = std::ptr::null_mut();
    let status = PowerGetActiveScheme(None, &mut scheme);
    if status != ERROR_SUCCESS || scheme.is_null() {
        return Err(PlatformError::backend("PowerGetActiveScheme", format!("error {}", status.0)));
    }
    let guid = *scheme;
    let _ = LocalFree(Some(HLOCAL(scheme as *mut _)));
    Ok(guid)
}

unsafe fn read_scheme_name(scheme: *const GUID) -> Result<String> {
    let mut size = 0u32;
    let status = PowerReadFriendlyName(None, Some(scheme), None, None, None, &mut size);
    if (status != ERROR_SUCCESS && status.0 != 234) || size == 0 {
        return Err(PlatformError::backend("PowerReadFriendlyName", format!("error {}", status.0)));
    }
    let mut buf = vec![0u8; size as usize];
    let status = PowerReadFriendlyName(None, Some(scheme), None, None, Some(buf.as_mut_ptr()), &mut size);
    if status != ERROR_SUCCESS {
        return Err(PlatformError::backend("PowerReadFriendlyName", format!("error {}", status.0)));
    }
    let wide: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    Ok(String::from_utf16_lossy(&wide))
}

#[async_trait]
impl Session for Win {
    async fn dark_mode(&self) -> Result<bool> {
        let mut value: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        match status {
            ERROR_SUCCESS => Ok(value == 0),
            // Never set means the default light theme.
            ERROR_FILE_NOT_FOUND => Ok(false),
            other => Err(PlatformError::backend("RegGetValueW AppsUseLightTheme", format!("error {}", other.0))),
        }
    }

    async fn eject(&self, volume_id: &str) -> Result<()> {
        use std::os::windows::process::CommandExt;
        let letter = volume_id.trim().chars().next().filter(char::is_ascii_alphabetic);
        let Some(letter) = letter else {
            return Err(PlatformError::NotFound(format!("volume {volume_id}")));
        };
        let script = format!(
            "$i = (New-Object -ComObject Shell.Application).Namespace(17).ParseName('{}:');              if ($null -eq $i) {{ exit 2 }}; $i.InvokeVerb('Eject')",
            letter.to_ascii_uppercase()
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .output()?;
        match out.status.code() {
            Some(0) => Ok(()),
            Some(2) => Err(PlatformError::NotFound(format!("volume {volume_id}"))),
            _ => Err(PlatformError::backend("eject", String::from_utf8_lossy(&out.stderr).trim())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on;

    #[test]
    fn keep_awake_takes_and_releases() {
        let guard = block_on(Win.keep_awake(KeepAwake { keep_display_on: true, reason: "test".into() })).unwrap();
        guard.release();
    }

    #[test]
    fn dark_mode_reads() {
        block_on(Win.dark_mode()).unwrap();
    }

    #[test]
    fn battery_reads() {
        Win.battery().unwrap();
    }

    #[test]
    fn battery_status_mapping() {
        assert_eq!(battery_from_status(1, 128, 255, u32::MAX), None);
        assert_eq!(battery_from_status(1, 255, 255, u32::MAX), None);
        let b = battery_from_status(1, 9, 80, u32::MAX).unwrap();
        assert!(b.charging && b.on_ac && b.seconds_remaining.is_none());
        assert_eq!(b.percent, 80.0);
        let b = battery_from_status(0, 1, 50, 3600).unwrap();
        assert!(!b.charging && !b.on_ac);
        assert_eq!(b.seconds_remaining, Some(3600));
    }

    #[test]
    fn power_profiles_list_one_active() {
        let list = block_on(Win.power_profiles()).unwrap();
        assert_eq!(list.iter().filter(|p| p.active).count(), 1);
        assert!(list.iter().all(|p| !p.name.is_empty()));
        assert_eq!(GUID::try_from(list[0].id.as_str()).map(|g| guid_string(&g)).unwrap(), list[0].id);
    }

    #[test]
    fn device_readings_fill_health_and_system_power() {
        let mut r = battery_from_status(1, 8, 60, u32::MAX).unwrap();
        r.adapter_watts = Some(30.0);
        apply_device(
            &mut r,
            &BatteryDevice {
                design_mwh: Some(50000),
                full_mwh: Some(40000),
                cycles: Some(120),
                rate_mw: Some(10000),
                power_state: Some(5),
                ..Default::default()
            },
        );
        assert_eq!(r.health_percent, Some(80.0));
        assert_eq!(r.power_watts, Some(10.0));
        assert_eq!(r.state, BatteryState::Charging);
        assert_eq!(r.system_watts, Some(20.0));
    }

    #[test]
    fn power_profile_has_a_name() {
        assert!(!block_on(Win.power_profile()).unwrap().is_empty());
    }

    #[test]
    fn unimplemented_is_unsupported() {
        assert!(matches!(
            Input::hook(&Win).unwrap_err(),
            PlatformError::Unsupported { feature: Feature::InputHook, .. }
        ));
    }
}
