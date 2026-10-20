//! Windows side of the uninstaller: the Uninstall registry keys, Store (AppX) packages and Scoop.

use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde_json::Value;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            RegCloseKey, RegDeleteTreeW, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
            KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS, REG_VALUE_TYPE,
        },
    },
};

use super::{
    fsx,
    leftovers::{self, Confidence, Identity, Kind, Leftover, Others},
    uninstaller::{pretty_package_name, App, Remove},
};

const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<Key> {
    let w = wide(path);
    let mut handle = HKEY::default();
    let r = unsafe { RegOpenKeyExW(root, PCWSTR(w.as_ptr()), None, KEY_READ | view, &mut handle) };
    (r == ERROR_SUCCESS).then_some(Key(handle))
}

fn subkeys(key: &Key) -> Vec<String> {
    let mut out = vec![];
    for index in 0.. {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let r = unsafe { RegEnumKeyExW(key.0, index, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    out
}

enum Val {
    Text(String),
    Number(u32),
}

fn value(key: &Key, name: &str) -> Option<Val> {
    let n = wide(name);
    let (mut ty, mut len) = (REG_VALUE_TYPE(0), 0u32);
    let probe = unsafe { RegQueryValueExW(key.0, PCWSTR(n.as_ptr()), None, Some(&mut ty), None, Some(&mut len)) };
    if probe != ERROR_SUCCESS || len == 0 {
        return None;
    }
    let mut data = vec![0u8; len as usize + 2];
    let mut size = len;
    let r = unsafe { RegQueryValueExW(key.0, PCWSTR(n.as_ptr()), None, Some(&mut ty), Some(data.as_mut_ptr()), Some(&mut size)) };
    if r != ERROR_SUCCESS {
        return None;
    }
    match ty.0 {
        1 | 2 => {
            let units: Vec<u16> = data[..size as usize].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let text = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
            Some(Val::Text(if ty.0 == 2 { expand(&text) } else { text }))
        }
        4 if size >= 4 => Some(Val::Number(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))),
        _ => None,
    }
}

fn text(key: &Key, name: &str) -> String {
    match value(key, name) {
        Some(Val::Text(t)) => t.trim().to_string(),
        _ => String::new(),
    }
}

fn number(key: &Key, name: &str) -> u32 {
    match value(key, name) {
        Some(Val::Number(n)) => n,
        _ => 0,
    }
}

fn expand(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                out.push_str(&std::env::var(&after[..end]).unwrap_or_else(|_| format!("%{}%", &after[..end])));
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `MsiExec /I{id}` is how Windows lists "Modify"; removing needs `/X`.
fn msi_remove_command(cmd: &str) -> String {
    let lower = cmd.to_lowercase();
    if lower.contains("msiexec") {
        if let Some(i) = lower.find("/i{").or_else(|| lower.find("/i {")) {
            let mut fixed = cmd.to_string();
            fixed.replace_range(i..i + 2, "/X");
            return fixed;
        }
    }
    cmd.to_string()
}

fn date(raw: &str) -> String {
    if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..])
    } else {
        String::new()
    }
}

fn registry_apps() -> Vec<App> {
    let mut apps = vec![];
    let cancel = AtomicBool::new(false);
    for (hive_name, hive) in [("HKLM", HKEY_LOCAL_MACHINE), ("HKCU", HKEY_CURRENT_USER)] {
        for (view_name, view) in [("64", KEY_WOW64_64KEY), ("32", KEY_WOW64_32KEY)] {
            let Some(root) = open(hive, UNINSTALL, view) else { continue };
            for sub in subkeys(&root) {
                let Some(key) = open(hive, &format!("{UNINSTALL}\\{sub}"), view) else { continue };
                let name = text(&key, "DisplayName");
                let uninstall = text(&key, "UninstallString");
                let release = text(&key, "ReleaseType").to_lowercase();
                if name.is_empty()
                    || uninstall.is_empty()
                    || number(&key, "SystemComponent") == 1
                    || !text(&key, "ParentKeyName").is_empty()
                    || matches!(release.as_str(), "update" | "hotfix" | "security update")
                {
                    continue;
                }
                let location = Some(text(&key, "InstallLocation").trim_matches('"').to_string()).filter(|l| !l.is_empty() && Path::new(l).is_dir());
                let mut size = u64::from(number(&key, "EstimatedSize")) * 1024;
                if size == 0 {
                    if let Some(l) = &location {
                        size = fsx::size_of(Path::new(l), &cancel);
                    }
                }
                apps.push(
                    App {
                        id: format!("reg:{hive_name}:{view_name}:{sub}"),
                        name,
                        version: text(&key, "DisplayVersion"),
                        publisher: text(&key, "Publisher"),
                        source: "registry".into(),
                        size,
                        installed: date(&text(&key, "InstallDate")),
                        location,
                        ids: vec![sub.clone()],
                        remove: Remove { kind: "cmd".into(), target: msi_remove_command(&uninstall) },
                        ..Default::default()
                    }
                    .seal(),
                );
            }
        }
    }
    apps
}

fn store_apps() -> Vec<App> {
    let script = "Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.NonRemovable -and $_.SignatureKind -ne 'System' } | Select-Object Name,PackageFullName,PackageFamilyName,Publisher,Version,InstallLocation | ConvertTo-Json -Compress";
    let Ok(out) = fsx::output("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", script]) else { return vec![] };
    let Ok(json) = serde_json::from_str::<Value>(out.trim()) else { return vec![] };
    let list = match json {
        Value::Array(a) => a,
        v @ Value::Object(_) => vec![v],
        _ => vec![],
    };
    let field = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    list.iter()
        .filter(|p| !field(p, "Name").is_empty() && !field(p, "PackageFullName").is_empty())
        .map(|p| {
            let publisher = field(p, "Publisher").split(',').find_map(|part| part.trim().strip_prefix("CN=").map(str::to_string)).unwrap_or_default();
            App {
                id: format!("store:{}", field(p, "PackageFullName")),
                name: pretty_package_name(&field(p, "Name")),
                version: field(p, "Version"),
                publisher,
                source: "store".into(),
                location: Some(field(p, "InstallLocation")).filter(|l| !l.is_empty()),
                ids: vec![field(p, "Name"), field(p, "PackageFamilyName")],
                remove: Remove { kind: "appx".into(), target: field(p, "PackageFullName") },
                ..Default::default()
            }
            .seal()
        })
        .collect()
}

fn scoop_root() -> PathBuf {
    fsx::env_path("SCOOP").unwrap_or_else(|| fsx::home().join("scoop"))
}

fn scoop_apps() -> Vec<App> {
    let apps = scoop_root().join("apps");
    let Ok(entries) = std::fs::read_dir(&apps) else { return vec![] };
    let cancel = AtomicBool::new(false);
    entries
        .flatten()
        .filter(|e| e.path().join("current").exists() && e.file_name() != "scoop")
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let manifest = std::fs::read_to_string(e.path().join("current").join("manifest.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
            let version = manifest.as_ref().and_then(|m| m.get("version")).and_then(Value::as_str).unwrap_or_default().to_string();
            App {
                id: format!("scoop:{name}"),
                name: name.clone(),
                version,
                source: "scoop".into(),
                size: fsx::size_of(&e.path(), &cancel),
                location: Some(e.path().to_string_lossy().into_owned()),
                ids: vec![name.clone()],
                remove: Remove { kind: "scoop".into(), target: name },
                ..Default::default()
            }
            .seal()
        })
        .collect()
}

pub fn list() -> Vec<App> {
    let store = std::thread::spawn(store_apps);
    let mut apps = registry_apps();
    apps.extend(scoop_apps());
    apps.extend(store.join().unwrap_or_default());
    apps
}

pub fn remove(app: &App) -> Result<String, String> {
    match app.remove.kind.as_str() {
        "appx" => {
            let script = format!("Remove-AppxPackage -Package '{}'", app.remove.target.replace('\'', "''"));
            fsx::output("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script])?;
            Ok("Removed through Windows".into())
        }
        "scoop" => {
            fsx::output("cmd.exe", &["/C", "scoop", "uninstall", &app.remove.target])?;
            Ok("Removed through Scoop".into())
        }
        _ => {
            let status = fsx::command("cmd.exe")
                .raw_arg("/C")
                .raw_arg(format!("\"{}\"", app.remove.target))
                .status()
                .map_err(|e| format!("The app's uninstaller could not be started: {e}"))?;
            for _ in 0..20 {
                if !super::uninstaller::still_listed(&app.id) {
                    return Ok("Removed with the app's own uninstaller".into());
                }
                std::thread::sleep(Duration::from_millis(750));
            }
            Err(format!(
                "The app's uninstaller finished (exit code {}) but the app is still installed. It may have been cancelled.",
                status.code().map_or("none".into(), |c| c.to_string())
            ))
        }
    }
}

fn roots(deep: bool) -> Vec<PathBuf> {
    let local = fsx::env_path("LOCALAPPDATA");
    let roaming = fsx::env_path("APPDATA");
    let mut r: Vec<PathBuf> = vec![];
    if let Some(l) = &local {
        r.extend([l.clone(), l.join("Programs"), l.join("Packages")]);
    }
    if let Some(a) = &roaming {
        r.extend([a.clone(), a.join("Microsoft\\Windows\\Start Menu\\Programs"), a.join("Microsoft\\Windows\\Start Menu\\Programs\\Startup")]);
    }
    r.extend(fsx::env_path("PROGRAMDATA"));
    if deep {
        r.extend(fsx::env_path("ProgramFiles"));
        r.extend(fsx::env_path("ProgramFiles(x86)"));
    }
    r
}

pub fn scan(id: &Identity, others: &Others, deep: bool, cancel: &AtomicBool) -> Vec<Leftover> {
    let mut found = leftovers::scan_roots(id, others, &roots(deep), cancel);
    found.extend(registry_leftovers(id, others, deep, cancel));
    found
}

fn registry_leftovers(id: &Identity, others: &Others, deep: bool, cancel: &AtomicBool) -> Vec<Leftover> {
    let mut found = vec![];
    let mut spots: Vec<(&str, HKEY, &str, REG_SAM_FLAGS)> = vec![("HKCU", HKEY_CURRENT_USER, "SOFTWARE", REG_SAM_FLAGS(0))];
    if deep {
        spots.push(("HKLM", HKEY_LOCAL_MACHINE, "SOFTWARE", KEY_WOW64_64KEY));
        spots.push(("HKLM", HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node", KEY_WOW64_64KEY));
    }
    let blocked = |raw: &str| others.names.contains(&leftovers::norm(raw));
    for (hive, root, path, view) in spots {
        let Some(key) = open(root, path, view) else { continue };
        for name in subkeys(&key) {
            if cancel.load(Ordering::Relaxed) {
                return found;
            }
            if blocked(&name) {
                continue;
            }
            let shown = format!("{hive}\\{path}\\{name}");
            if let Some(c) = leftovers::classify(id, &name) {
                let why = if c == Confidence::Verified { "Registry key named after the app's identifier" } else { "Registry key with a similar name; check before ticking" };
                found.push(Leftover { path: shown.clone(), kind: Kind::Registry, confidence: c, size: 0, why: why.into() });
            }
            let vendor_like = !leftovers::ambiguous(&name) || leftovers::norm(&name).starts_with(&leftovers::norm(id.publisher.split_whitespace().next().unwrap_or("-")));
            if vendor_like {
                if let Some(vendor) = open(root, &format!("{path}\\{name}"), view) {
                    for child in subkeys(&vendor).into_iter().filter(|c| !blocked(c)) {
                        if leftovers::classify_pair(id, &name, &child).is_some() {
                            found.push(Leftover {
                                path: format!("{shown}\\{child}"),
                                kind: Kind::Registry,
                                confidence: Confidence::Verified,
                                size: 0,
                                why: "Registry key of the app's vendor and product".into(),
                            });
                        }
                    }
                }
            }
        }
    }
    found
}

pub fn delete_key(shown: &str) -> Result<(), String> {
    let (hive, rest) = shown.split_once('\\').ok_or("not a registry path")?;
    let root = match hive {
        "HKCU" => HKEY_CURRENT_USER,
        "HKLM" => HKEY_LOCAL_MACHINE,
        _ => return Err("not a registry path".into()),
    };
    if rest.matches('\\').count() < 1 {
        return Err("refused: too broad a registry key".into());
    }
    let w = wide(rest);
    let r = unsafe { RegDeleteTreeW(root, PCWSTR(w.as_ptr())) };
    match r {
        ERROR_SUCCESS => Ok(()),
        e if e.0 == 5 => Err("no rights to change this registry key".into()),
        e if e.0 == 2 => Ok(()),
        e => Err(format!("registry error {}", e.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msi_modify_becomes_remove() {
        assert_eq!(msi_remove_command("MsiExec.exe /I{ABC-1}"), "MsiExec.exe /X{ABC-1}");
        assert_eq!(msi_remove_command("\"C:\\App\\unins000.exe\" /SILENT"), "\"C:\\App\\unins000.exe\" /SILENT");
        assert_eq!(msi_remove_command("MsiExec.exe /X{ABC-1}"), "MsiExec.exe /X{ABC-1}");
    }

    #[test]
    fn env_variables_expand() {
        std::env::set_var("NOTIFI_TEST_VAR", "yes");
        assert_eq!(expand("%NOTIFI_TEST_VAR%\\x"), "yes\\x");
        assert_eq!(expand("100%"), "100%");
    }

    #[test]
    fn install_dates_read() {
        assert_eq!(date("20240131"), "2024-01-31");
        assert_eq!(date("bad"), "");
    }
}
