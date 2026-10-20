//! Audio on Windows: Core Audio (MMDevice, endpoint volume, session manager) plus two undocumented
//! interfaces Windows' own sound settings use, for the default device and per-app outputs.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use windows::core::{implement, interface, IUnknown, IUnknown_Vtbl, Interface, BOOL, GUID, HRESULT, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, PROPERTYKEY, S_OK};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl, IAudioMeterInformation,
};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateExpired, EDataFlow, ERole,
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, ISimpleAudioVolume, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor,
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Variant::{VT_LPWSTR, VT_UI4};
use windows::Win32::System::WinRT::RoGetActivationFactory;

use crate::common::EventStream;
use crate::error::{PlatformError, Result};
use crate::traits::*;
use crate::win::Win;

/// Keeps COM initialised (MTA) on the calling thread for the guard's lifetime.
pub(crate) struct Com(bool);

impl Com {
    pub(crate) fn init() -> Self {
        // RPC_E_CHANGED_MODE (an STA thread, e.g. the UI thread) still lets these free-threaded objects work.
        Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) fn werr(context: &'static str) -> impl Fn(windows::core::Error) -> PlatformError {
    move |e| PlatformError::backend(context, e)
}

// Undocumented: IPolicyConfig (Vista+), stable for years and used by every default-device switcher.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn get_mix_format(&self) -> HRESULT;
    fn get_device_format(&self) -> HRESULT;
    fn reset_device_format(&self) -> HRESULT;
    fn set_device_format(&self) -> HRESULT;
    fn get_processing_period(&self) -> HRESULT;
    fn set_processing_period(&self) -> HRESULT;
    fn get_share_mode(&self) -> HRESULT;
    fn set_share_mode(&self) -> HRESULT;
    fn get_property_value(&self) -> HRESULT;
    fn set_property_value(&self) -> HRESULT;
    fn set_default_endpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
}

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

// RISK: per-app output uses Windows.Media.Internal.AudioPolicyConfig, the undocumented factory behind
// Settings > Sound > Volume mixer. Its IID changed in Windows 10 21H2 and the vtable could change again in a
// future build; every call checks its HRESULT and the feature reports an error (the UI hides the menu) if it fails.
macro_rules! audio_policy_factory {
    ($name:ident, $iid:literal) => {
        #[interface($iid)]
        unsafe trait $name: IUnknown {
            fn get_iids(&self) -> HRESULT;
            fn get_runtime_class_name(&self) -> HRESULT;
            fn get_trust_level(&self) -> HRESULT;
            fn add_ctx_volume_change(&self) -> HRESULT;
            fn remove_ctx_volume_changed(&self) -> HRESULT;
            fn add_ringer_vibrate_state_changed(&self) -> HRESULT;
            fn remove_ringer_vibrate_state_change(&self) -> HRESULT;
            fn set_volume_group_gain_for_id(&self) -> HRESULT;
            fn get_volume_group_gain_for_id(&self) -> HRESULT;
            fn get_active_volume_group_for_endpoint_id(&self) -> HRESULT;
            fn get_volume_groups_for_endpoint(&self) -> HRESULT;
            fn get_current_volume_context(&self) -> HRESULT;
            fn set_volume_group_mute_for_id(&self) -> HRESULT;
            fn get_volume_group_mute_for_id(&self) -> HRESULT;
            fn set_ringer_vibrate_state(&self) -> HRESULT;
            fn get_ringer_vibrate_state(&self) -> HRESULT;
            fn set_preferred_chat_application(&self) -> HRESULT;
            fn reset_preferred_chat_application(&self) -> HRESULT;
            fn get_preferred_chat_application(&self) -> HRESULT;
            fn get_current_chat_applications(&self) -> HRESULT;
            fn add_chat_context_changed(&self) -> HRESULT;
            fn remove_chat_context_changed(&self) -> HRESULT;
            fn set_persisted_default_audio_endpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device: *mut c_void) -> HRESULT;
            fn get_persisted_default_audio_endpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device: *mut *mut c_void) -> HRESULT;
        }
    };
}

audio_policy_factory!(IAudioPolicyConfigFactory, "ab3d4648-e242-459f-b02f-541c70306324");
audio_policy_factory!(IAudioPolicyConfigFactoryLegacy, "2a59116d-6c4f-45e0-a74f-707e3fef9258");

enum PolicyFactory {
    Current(IAudioPolicyConfigFactory),
    Legacy(IAudioPolicyConfigFactoryLegacy),
}

impl PolicyFactory {
    fn open() -> Result<Self> {
        let class = HSTRING::from("Windows.Media.Internal.AudioPolicyConfig");
        unsafe {
            if let Ok(f) = RoGetActivationFactory::<IAudioPolicyConfigFactory>(&class) {
                return Ok(Self::Current(f));
            }
            RoGetActivationFactory::<IAudioPolicyConfigFactoryLegacy>(&class)
                .map(Self::Legacy)
                .map_err(|e| PlatformError::unsupported(crate::caps::Feature::AudioPerAppOutput, e.to_string()))
        }
    }

    fn set(&self, pid: u32, role: ERole, device: &HSTRING) -> HRESULT {
        let raw = if device.is_empty() { std::ptr::null_mut() } else { unsafe { std::mem::transmute_copy(device) } };
        unsafe {
            match self {
                Self::Current(f) => f.set_persisted_default_audio_endpoint(pid, eRender, role, raw),
                Self::Legacy(f) => f.set_persisted_default_audio_endpoint(pid, eRender, role, raw),
            }
        }
    }

    fn route(&self, pid: u32) -> Option<String> {
        let mut raw: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            match self {
                Self::Current(f) => f.get_persisted_default_audio_endpoint(pid, eRender, eMultimedia, &mut raw),
                Self::Legacy(f) => f.get_persisted_default_audio_endpoint(pid, eRender, eMultimedia, &mut raw),
            }
        };
        if hr.is_err() || raw.is_null() {
            return None;
        }
        let h: HSTRING = unsafe { std::mem::transmute(raw) };
        from_policy_id(&h.to_string())
    }
}

const RENDER_INTERFACE: &str = "{e6327cad-dcec-4949-ae8a-991e976a79d2}";

/// MMDevice id (`{0.0.0.00000000}.{guid}`) to the device-interface path the policy factory expects.
fn to_policy_id(device_id: &str) -> String {
    format!(r"\\?\SWD#MMDEVAPI#{device_id}#{RENDER_INTERFACE}")
}

fn from_policy_id(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\SWD#MMDEVAPI#")?;
    let id = rest.split('#').next()?;
    (!id.is_empty()).then(|| id.to_string())
}

fn flow(direction: DeviceDirection) -> EDataFlow {
    match direction {
        DeviceDirection::Output => eRender,
        DeviceDirection::Input => eCapture,
    }
}

/// EndpointFormFactor values from the Core Audio headers.
fn kind_from_form_factor(form_factor: Option<u32>, direction: DeviceDirection) -> DeviceKind {
    match (form_factor, direction) {
        (Some(5 | 6), _) => DeviceKind::Headset,
        (_, DeviceDirection::Input) => DeviceKind::Microphone,
        (Some(1), _) => DeviceKind::Speakers,
        (Some(3), _) => DeviceKind::Headphones,
        (Some(9), _) => DeviceKind::Display,
        _ => DeviceKind::Other,
    }
}

fn pick_app_name(display: &str, description: Option<&str>, exe: Option<&str>, system: bool) -> String {
    if system {
        return "System sounds".into();
    }
    let display = display.trim();
    if !display.is_empty() && !display.starts_with('@') {
        return display.to_string();
    }
    if let Some(d) = description.map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    exe.and_then(|p| std::path::Path::new(p).file_stem()?.to_str().map(str::to_string))
        .unwrap_or_else(|| "Unknown app".into())
}

fn finite(v: f32) -> Option<f32> {
    v.is_finite().then_some(v.clamp(0.0, 1.0))
}

unsafe fn take_pwstr(p: PWSTR) -> String {
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const c_void));
    s
}

unsafe fn prop(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore, key: &PROPERTYKEY) -> Option<PROPVARIANT> {
    store.GetValue(key).ok()
}

unsafe fn prop_string(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let mut v = prop(store, key)?;
    let out = (v.Anonymous.Anonymous.vt == VT_LPWSTR).then(|| v.Anonymous.Anonymous.Anonymous.pwszVal.to_string().ok()).flatten();
    let _ = PropVariantClear(&mut v);
    out
}

unsafe fn prop_u32(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore, key: &PROPERTYKEY) -> Option<u32> {
    let mut v = prop(store, key)?;
    let out = (v.Anonymous.Anonymous.vt == VT_UI4).then(|| v.Anonymous.Anonymous.Anonymous.ulVal);
    let _ = PropVariantClear(&mut v);
    out
}

fn enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(werr("MMDeviceEnumerator"))
}

unsafe fn device_id(dev: &IMMDevice) -> Result<String> {
    Ok(take_pwstr(dev.GetId().map_err(werr("IMMDevice::GetId"))?))
}

unsafe fn default_id(en: &IMMDeviceEnumerator, direction: DeviceDirection) -> Option<String> {
    let dev = en.GetDefaultAudioEndpoint(flow(direction), eConsole).ok()?;
    device_id(&dev).ok()
}

unsafe fn endpoint_volume(dev: &IMMDevice) -> Result<IAudioEndpointVolume> {
    dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).map_err(werr("IAudioEndpointVolume"))
}

unsafe fn describe(dev: &IMMDevice, direction: DeviceDirection, default: Option<&str>) -> Result<AudioDevice> {
    let id = device_id(dev)?;
    let store = dev.OpenPropertyStore(STGM_READ).map_err(werr("OpenPropertyStore"))?;
    let name = prop_string(&store, &PKEY_Device_FriendlyName).unwrap_or_else(|| id.clone());
    let kind = kind_from_form_factor(prop_u32(&store, &PKEY_AudioEndpoint_FormFactor), direction);
    let (volume, muted) = match endpoint_volume(dev) {
        Ok(ep) => (ep.GetMasterVolumeLevelScalar().ok().and_then(finite), ep.GetMute().map(BOOL::as_bool).unwrap_or(false)),
        Err(_) => (None, false),
    };
    Ok(AudioDevice {
        is_default: default == Some(id.as_str()),
        id,
        name,
        direction,
        kind,
        volume: volume.unwrap_or(0.0),
        muted,
        has_volume: volume.is_some(),
    })
}

unsafe fn active_devices(en: &IMMDeviceEnumerator, direction: DeviceDirection) -> Result<Vec<IMMDevice>> {
    let coll = en.EnumAudioEndpoints(flow(direction), DEVICE_STATE_ACTIVE).map_err(werr("EnumAudioEndpoints"))?;
    let n = coll.GetCount().map_err(werr("EnumAudioEndpoints"))?;
    Ok((0..n).filter_map(|i| coll.Item(i).ok()).collect())
}

unsafe fn device_by_id(en: &IMMDeviceEnumerator, id: &str) -> Result<IMMDevice> {
    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    en.GetDevice(PCWSTR(wide.as_ptr())).map_err(|_| PlatformError::NotFound(format!("audio device {id}")))
}

static APP_INFO: Mutex<Option<HashMap<u32, AppInfo>>> = Mutex::new(None);

#[derive(Clone, Default)]
struct AppInfo {
    exe: Option<String>,
    description: Option<String>,
}

unsafe fn exe_path(pid: u32) -> Option<String> {
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(h);
    r.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// The version resource's FileDescription, which is the name Windows shows for an app.
pub(crate) unsafe fn file_description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    let translation = windows::core::w!("\\VarFileInfo\\Translation");
    let (lang, cp) = if VerQueryValueW(data.as_ptr() as *const c_void, translation, &mut ptr, &mut len).as_bool() && len >= 4 {
        let pair = ptr as *const u16;
        (*pair, *pair.add(1))
    } else {
        (0x0409, 0x04b0)
    };
    let key: Vec<u16> = format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription").encode_utf16().chain(Some(0)).collect();
    if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
        return None;
    }
    let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
    let s = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn app_info(pid: u32) -> AppInfo {
    let mut guard = APP_INFO.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(info) = cache.get(&pid) {
        return info.clone();
    }
    let exe = unsafe { exe_path(pid) };
    let description = exe.as_deref().and_then(|p| unsafe { file_description(p) });
    let info = AppInfo { exe, description };
    if cache.len() > 512 {
        cache.clear();
    }
    cache.insert(pid, info.clone());
    info
}

struct SessionHandle {
    id: String,
    pid: u32,
    control: IAudioSessionControl2,
}

unsafe fn session_handles(en: &IMMDeviceEnumerator) -> Result<Vec<SessionHandle>> {
    let mut out = Vec::new();
    for dev in active_devices(en, DeviceDirection::Output)? {
        let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
        let Ok(list) = mgr.GetSessionEnumerator() else { continue };
        for i in 0..list.GetCount().unwrap_or(0) {
            let Ok(ctl) = list.GetSession(i) else { continue };
            if ctl.GetState().map(|s| s == AudioSessionStateExpired).unwrap_or(true) {
                continue;
            }
            let Ok(control) = ctl.cast::<IAudioSessionControl2>() else { continue };
            let Ok(id) = control.GetSessionInstanceIdentifier().map(|p| take_pwstr(p)) else { continue };
            out.push(SessionHandle { id, pid: control.GetProcessId().unwrap_or(0), control });
        }
    }
    Ok(out)
}

unsafe fn find_session(id: &str) -> Result<SessionHandle> {
    session_handles(&enumerator()?)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| PlatformError::NotFound(format!("audio session {id}")))
}

unsafe fn describe_session(s: &SessionHandle, policy: Option<&PolicyFactory>) -> Option<AudioSession> {
    let volume: ISimpleAudioVolume = s.control.cast().ok()?;
    let system = s.control.IsSystemSoundsSession() == S_OK;
    let display = s.control.GetDisplayName().map(|p| take_pwstr(p)).unwrap_or_default();
    let icon = s.control.GetIconPath().map(|p| take_pwstr(p)).unwrap_or_default();
    let info = if s.pid != 0 && !system { app_info(s.pid) } else { AppInfo::default() };
    let peak = s.control.cast::<IAudioMeterInformation>().ok().and_then(|m| m.GetPeakValue().ok()).and_then(finite);
    let icon_path = if icon.is_empty() { info.exe.clone() } else { Some(icon) };
    let app_id = info
        .exe
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_name()?.to_str().map(|n| n.to_ascii_lowercase()));
    Some(AudioSession {
        id: s.id.clone(),
        pid: (s.pid != 0).then_some(s.pid),
        app_name: pick_app_name(&display, info.description.as_deref(), info.exe.as_deref(), system),
        app_id: if system { Some("system-sounds".into()) } else { app_id },
        icon_path,
        volume: finite(volume.GetMasterVolume().ok()?)?,
        muted: volume.GetMute().map(BOOL::as_bool).unwrap_or(false),
        peak: peak.unwrap_or(0.0),
        device_id: policy.filter(|_| s.pid != 0).and_then(|p| p.route(s.pid)),
    })
}

impl Audio for Win {
    fn sessions(&self) -> Result<Vec<AudioSession>> {
        let _com = Com::init();
        let policy = PolicyFactory::open().ok();
        unsafe {
            let handles = session_handles(&enumerator()?)?;
            Ok(handles.iter().filter_map(|s| describe_session(s, policy.as_ref())).collect())
        }
    }

    fn set_session_volume(&self, session_id: &str, volume: f32) -> Result<()> {
        let _com = Com::init();
        unsafe {
            let s = find_session(session_id)?;
            let v: ISimpleAudioVolume = s.control.cast().map_err(werr("ISimpleAudioVolume"))?;
            v.SetMasterVolume(finite(volume).unwrap_or(0.0), std::ptr::null()).map_err(werr("SetMasterVolume"))
        }
    }

    fn set_session_mute(&self, session_id: &str, muted: bool) -> Result<()> {
        let _com = Com::init();
        unsafe {
            let s = find_session(session_id)?;
            let v: ISimpleAudioVolume = s.control.cast().map_err(werr("ISimpleAudioVolume"))?;
            v.SetMute(muted, std::ptr::null()).map_err(werr("SetMute"))
        }
    }

    fn set_session_output(&self, session_id: &str, device_id: &str) -> Result<()> {
        let _com = Com::init();
        let s = unsafe { find_session(session_id)? };
        if s.pid == 0 {
            return Err(PlatformError::backend("per-app output", "the session has no process"));
        }
        let policy = PolicyFactory::open()?;
        let target = if device_id.is_empty() { HSTRING::new() } else { HSTRING::from(to_policy_id(device_id)) };
        for role in [eConsole, eMultimedia] {
            policy.set(s.pid, role, &target).ok().map_err(werr("SetPersistedDefaultAudioEndpoint"))?;
        }
        Ok(())
    }

    fn devices(&self, direction: DeviceDirection) -> Result<Vec<AudioDevice>> {
        let _com = Com::init();
        unsafe {
            let en = enumerator()?;
            let default = default_id(&en, direction);
            Ok(active_devices(&en, direction)?
                .iter()
                .filter_map(|d| describe(d, direction, default.as_deref()).ok())
                .collect())
        }
    }

    fn default_device(&self, direction: DeviceDirection) -> Result<Option<AudioDevice>> {
        let _com = Com::init();
        unsafe {
            let en = enumerator()?;
            match en.GetDefaultAudioEndpoint(flow(direction), eConsole) {
                Ok(dev) => {
                    let id = device_id(&dev)?;
                    describe(&dev, direction, Some(&id)).map(Some)
                }
                Err(_) => Ok(None),
            }
        }
    }

    fn set_default_device(&self, device_id: &str) -> Result<()> {
        let _com = Com::init();
        unsafe {
            device_by_id(&enumerator()?, device_id)?;
            let cfg: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).map_err(werr("PolicyConfig"))?;
            let wide: Vec<u16> = device_id.encode_utf16().chain(Some(0)).collect();
            for role in [eConsole, eMultimedia, eCommunications] {
                cfg.set_default_endpoint(PCWSTR(wide.as_ptr()), role).ok().map_err(werr("SetDefaultEndpoint"))?;
            }
        }
        Ok(())
    }

    fn set_device_volume(&self, device_id: &str, volume: f32) -> Result<()> {
        let _com = Com::init();
        unsafe {
            let ep = endpoint_volume(&device_by_id(&enumerator()?, device_id)?)?;
            ep.SetMasterVolumeLevelScalar(finite(volume).unwrap_or(0.0), std::ptr::null())
                .map_err(werr("SetMasterVolumeLevelScalar"))
        }
    }

    fn set_device_mute(&self, device_id: &str, muted: bool) -> Result<()> {
        let _com = Com::init();
        unsafe {
            let ep = endpoint_volume(&device_by_id(&enumerator()?, device_id)?)?;
            ep.SetMute(muted, std::ptr::null()).map_err(werr("IAudioEndpointVolume::SetMute"))
        }
    }

    fn mute_all_microphones(&self, muted: bool) -> Result<()> {
        let _com = Com::init();
        let mut failed = Vec::new();
        unsafe {
            for dev in active_devices(&enumerator()?, DeviceDirection::Input)? {
                let ok = endpoint_volume(&dev).and_then(|ep| ep.SetMute(muted, std::ptr::null()).map_err(werr("SetMute")));
                if ok.is_err() {
                    failed.push(describe(&dev, DeviceDirection::Input, None).map(|d| d.name).unwrap_or_else(|_| "unknown".into()));
                }
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(PlatformError::backend("mute microphones", format!("could not change {}", failed.join(", "))))
        }
    }

    fn events(&self) -> Result<EventStream<AudioEvent>> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::Builder::new().name("notifi-audio-events".into()).spawn(move || {
            let _com = Com::init();
            if let Err(e) = unsafe { watch(tx, &ready_tx) } {
                let _ = ready_tx.send(Err(e));
            }
        })?;
        ready_rx.recv().map_err(|_| PlatformError::backend("audio events", "watcher thread ended"))??;
        Ok(rx)
    }
}

enum Notice {
    Event(AudioEvent),
    DefaultMoved,
}

#[implement(IMMNotificationClient)]
struct DeviceWatcher {
    tx: mpsc::Sender<Notice>,
}

fn pcwstr(p: &PCWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { p.to_string().unwrap_or_default() }
    }
}

impl IMMNotificationClient_Impl for DeviceWatcher_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, state: DEVICE_STATE) -> windows::core::Result<()> {
        let id = pcwstr(id);
        let event = if state == DEVICE_STATE_ACTIVE { AudioEvent::DeviceAdded(id) } else { AudioEvent::DeviceRemoved(id) };
        let _ = self.tx.send(Notice::Event(event));
        Ok(())
    }
    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.tx.send(Notice::Event(AudioEvent::DeviceAdded(pcwstr(id))));
        Ok(())
    }
    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.tx.send(Notice::Event(AudioEvent::DeviceRemoved(pcwstr(id))));
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, id: &PCWSTR) -> windows::core::Result<()> {
        if role == eConsole {
            let direction = if flow == eCapture { DeviceDirection::Input } else { DeviceDirection::Output };
            let _ = self.tx.send(Notice::Event(AudioEvent::DefaultChanged { direction, device_id: pcwstr(id) }));
            let _ = self.tx.send(Notice::DefaultMoved);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeWatcher {
    device_id: String,
    tx: mpsc::Sender<Notice>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeWatcher_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if let Some(d) = unsafe { data.as_ref() } {
            let _ = self.tx.send(Notice::Event(AudioEvent::VolumeChanged {
                device_id: self.device_id.clone(),
                volume: finite(d.fMasterVolume).unwrap_or(0.0),
                muted: d.bMuted.as_bool(),
            }));
        }
        Ok(())
    }
}

type Registered = Vec<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>;

unsafe fn watch_defaults(en: &IMMDeviceEnumerator, tx: &mpsc::Sender<Notice>) -> Registered {
    let mut out = Vec::new();
    for direction in [DeviceDirection::Output, DeviceDirection::Input] {
        let Ok(dev) = en.GetDefaultAudioEndpoint(flow(direction), eConsole) else { continue };
        let (Ok(id), Ok(ep)) = (device_id(&dev), endpoint_volume(&dev)) else { continue };
        let cb: IAudioEndpointVolumeCallback = VolumeWatcher { device_id: id, tx: tx.clone() }.into();
        if ep.RegisterControlChangeNotify(&cb).is_ok() {
            out.push((ep, cb));
        }
    }
    out
}

fn unwatch(registered: Registered) {
    for (ep, cb) in registered {
        let _ = unsafe { ep.UnregisterControlChangeNotify(&cb) };
    }
}

unsafe fn session_ids(en: &IMMDeviceEnumerator) -> HashSet<String> {
    session_handles(en).map(|v| v.into_iter().map(|s| s.id).collect()).unwrap_or_default()
}

/// Device and volume changes arrive through callbacks; the session set is compared once a second.
unsafe fn watch(out: mpsc::Sender<AudioEvent>, ready: &mpsc::Sender<Result<()>>) -> Result<()> {
    let en = enumerator()?;
    let (tx, rx) = mpsc::channel();
    let client: IMMNotificationClient = DeviceWatcher { tx: tx.clone() }.into();
    en.RegisterEndpointNotificationCallback(&client).map_err(werr("RegisterEndpointNotificationCallback"))?;
    let mut volumes = watch_defaults(&en, &tx);
    let mut sessions = session_ids(&en);
    let _ = ready.send(Ok(()));
    loop {
        let sent = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Notice::Event(e)) => out.send(e).is_ok(),
            Ok(Notice::DefaultMoved) => {
                unwatch(std::mem::take(&mut volumes));
                volumes = watch_defaults(&en, &tx);
                true
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let now = session_ids(&en);
                if now != sessions {
                    sessions = now;
                    out.send(AudioEvent::SessionsChanged).is_ok()
                } else {
                    true
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => false,
        };
        if !sent {
            break;
        }
    }
    unwatch(volumes);
    let _ = en.UnregisterEndpointNotificationCallback(&client);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_id_round_trip() {
        let id = "{0.0.0.00000000}.{8f2b7c4e-1d2a-4e55-9a3b-0c1d2e3f4a5b}";
        let path = to_policy_id(id);
        assert_eq!(path, r"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{8f2b7c4e-1d2a-4e55-9a3b-0c1d2e3f4a5b}#{e6327cad-dcec-4949-ae8a-991e976a79d2}");
        assert_eq!(from_policy_id(&path).as_deref(), Some(id));
        assert_eq!(from_policy_id(""), None);
        assert_eq!(from_policy_id(r"\\?\SWD#MMDEVAPI##x"), None);
    }

    #[test]
    fn form_factors() {
        assert_eq!(kind_from_form_factor(Some(3), DeviceDirection::Output), DeviceKind::Headphones);
        assert_eq!(kind_from_form_factor(Some(5), DeviceDirection::Input), DeviceKind::Headset);
        assert_eq!(kind_from_form_factor(Some(1), DeviceDirection::Output), DeviceKind::Speakers);
        assert_eq!(kind_from_form_factor(Some(9), DeviceDirection::Output), DeviceKind::Display);
        assert_eq!(kind_from_form_factor(Some(4), DeviceDirection::Input), DeviceKind::Microphone);
        assert_eq!(kind_from_form_factor(None, DeviceDirection::Output), DeviceKind::Other);
    }

    #[test]
    fn app_names() {
        assert_eq!(pick_app_name("", None, None, true), "System sounds");
        assert_eq!(pick_app_name("Teams call", Some("Microsoft Teams"), None, false), "Teams call");
        assert_eq!(pick_app_name("@%SystemRoot%\\x.dll,-5", Some("Spotify"), None, false), "Spotify");
        assert_eq!(pick_app_name("", None, Some(r"C:\Apps\firefox.exe"), false), "firefox");
        assert_eq!(pick_app_name(" ", Some(" "), None, false), "Unknown app");
    }

    #[test]
    fn policy_interfaces_load() {
        let _com = Com::init();
        assert!(PolicyFactory::open().is_ok());
        let cfg: windows::core::Result<IPolicyConfig> = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) };
        assert!(cfg.is_ok());
    }

    #[test]
    fn events_start() {
        let rx = Audio::events(&Win).unwrap();
        drop(rx);
    }

    #[test]
    fn volume_sanitising() {
        assert_eq!(finite(f32::NAN), None);
        assert_eq!(finite(1.5), Some(1.0));
        assert_eq!(finite(-0.2), Some(0.0));
    }
}
