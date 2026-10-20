//! Installed apps: Start Menu shortcuts and AppX packages on Windows, `.desktop` files on Linux.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Launch {
    /// A Start Menu shortcut or any file the shell can open.
    Shell(PathBuf),
    /// A packaged app, opened through `shell:AppsFolder\<id>`.
    Appx(String),
    /// A `.desktop` Exec line with the field codes already removed.
    #[cfg_attr(windows, allow(dead_code))]
    Exec(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub name: String,
    pub launch: Launch,
    pub detail: String,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(windows, allow(dead_code))]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub comment: String,
    pub hidden: bool,
}

#[cfg_attr(windows, allow(dead_code))]
/// Reads the `[Desktop Entry]` group of a `.desktop` file; `None` when it is not a launchable app.
pub fn parse_desktop(text: &str) -> Option<DesktopEntry> {
    let mut in_entry = false;
    let (mut name, mut exec, mut comment) = (None, None, String::new());
    let (mut kind, mut hidden) = (String::new(), false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(strip_field_codes(value.trim())),
            "Comment" => comment = value.trim().to_string(),
            "Type" => kind = value.trim().to_string(),
            "NoDisplay" | "Hidden" if value.trim() == "true" => hidden = true,
            _ => {}
        }
    }
    if kind != "Application" {
        return None;
    }
    let (name, exec) = (name.filter(|n| !n.is_empty())?, exec.filter(|e| !e.is_empty())?);
    Some(DesktopEntry { name, exec, comment, hidden })
}

#[cfg_attr(windows, allow(dead_code))]
/// Drops `%f`, `%U` and the like from an Exec line; `%%` becomes a literal percent sign.
pub fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(_) | None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The display name of a Start Menu shortcut, or `None` for entries that are not launchers.
pub fn shortcut_name(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if ext != "lnk" && ext != "url" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.trim();
    let lower = stem.to_lowercase();
    if stem.is_empty() || lower.starts_with("uninstall") || lower.ends_with("uninstall") || lower.contains("readme") {
        return None;
    }
    Some(stem.to_string())
}

/// Parses the JSON that `Get-StartApps | ConvertTo-Json` prints; keeps only packaged apps.
pub fn parse_start_apps(json: &str) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let items = match value {
        serde_json::Value::Array(a) => a,
        one @ serde_json::Value::Object(_) => vec![one],
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|v| {
            let name = v.get("Name")?.as_str()?.trim();
            let id = v.get("AppID")?.as_str()?.trim();
            (!name.is_empty() && id.contains('!')).then(|| (name.to_string(), id.to_string()))
        })
        .collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 6 {
                walk(&path, depth + 1, out);
            }
        } else {
            out.push(path);
        }
    }
}

#[cfg(windows)]
pub fn scan() -> Vec<App> {
    let mut roots = Vec::new();
    for var in ["ProgramData", "APPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            roots.push(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"));
        }
    }
    let mut files = Vec::new();
    for root in &roots {
        walk(root, 0, &mut files);
    }
    let mut apps: Vec<App> = files
        .into_iter()
        .filter_map(|path| {
            let name = shortcut_name(&path)?;
            let detail = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).filter(|n| *n != "Programs").unwrap_or("").to_string();
            Some(App { name, launch: Launch::Shell(path), detail })
        })
        .collect();
    for (name, id) in appx() {
        apps.push(App { name, launch: Launch::Appx(id), detail: "Store app".into() });
    }
    dedupe(apps)
}

#[cfg(windows)]
fn appx() -> Vec<(String, String)> {
    use std::{os::windows::process::CommandExt, process::Command};
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-StartApps | ConvertTo-Json -Compress"])
        .creation_flags(0x0800_0000)
        .output();
    match out {
        Ok(o) if o.status.success() => parse_start_apps(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

#[cfg(not(windows))]
pub fn scan() -> Vec<App> {
    let mut roots = vec![PathBuf::from("/usr/share/applications"), PathBuf::from("/usr/local/share/applications")];
    roots.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".local/share/applications"));
        roots.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    let mut files = Vec::new();
    for root in &roots {
        walk(root, 0, &mut files);
    }
    let apps = files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
        .filter_map(|path| {
            let entry = parse_desktop(&fs::read_to_string(&path).ok()?)?;
            (!entry.hidden).then(|| App { name: entry.name, launch: Launch::Exec(entry.exec), detail: entry.comment })
        })
        .collect();
    dedupe(apps)
}

fn dedupe(mut apps: Vec<App>) -> Vec<App> {
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "# comment\n[Desktop Entry]\nVersion=1.0\nName=Firefox\nName[de]=Feuerfuchs\nComment=Browse the web\nExec=firefox %u\nIcon=firefox\nType=Application\n\n[Desktop Action new-window]\nName=New Window\nExec=firefox --new-window\n";

    #[test]
    fn parses_desktop_entry_and_ignores_actions() {
        let e = parse_desktop(FIREFOX).unwrap();
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.exec, "firefox");
        assert_eq!(e.comment, "Browse the web");
        assert!(!e.hidden);
    }

    #[test]
    fn hidden_and_non_apps_are_flagged() {
        let hidden = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n";
        assert!(parse_desktop(hidden).unwrap().hidden);
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=X\nURL=http://x\n").is_none());
        assert!(parse_desktop("[Desktop Entry]\nType=Application\nName=X\n").is_none());
    }

    #[test]
    fn field_codes_are_stripped() {
        assert_eq!(strip_field_codes("gimp-2.10 %U"), "gimp-2.10");
        assert_eq!(strip_field_codes("env A=1 app --file=%f --x"), "env A=1 app --file= --x");
        assert_eq!(strip_field_codes("echo 100%%"), "echo 100%");
    }

    #[test]
    fn shortcut_names() {
        assert_eq!(shortcut_name(Path::new(r"C:\Menu\Visual Studio Code.lnk")).as_deref(), Some("Visual Studio Code"));
        assert_eq!(shortcut_name(Path::new("Steam.URL")).as_deref(), Some("Steam"));
        assert_eq!(shortcut_name(Path::new("Uninstall Foo.lnk")), None);
        assert_eq!(shortcut_name(Path::new("notes.txt")), None);
    }

    #[test]
    fn start_apps_json_keeps_packaged_apps() {
        let json = r#"[{"Name":"Calculator","AppID":"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"},{"Name":"Notepad","AppID":"C:\\Windows\\notepad.exe"}]"#;
        assert_eq!(parse_start_apps(json), vec![("Calculator".to_string(), "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_string())]);
        let single = r#"{"Name":"Photos","AppID":"Microsoft.Photos_8wekyb3d8bbwe!App"}"#;
        assert_eq!(parse_start_apps(single).len(), 1);
        assert!(parse_start_apps("not json").is_empty());
    }
}
