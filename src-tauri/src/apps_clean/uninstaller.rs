//! The uninstaller (spec/apps/uninstaller.md): list apps, scan leftovers, run the app's own
//! removal and delete what the user ticked. Platform specifics are in the `_win` / `_linux` files.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    fsx,
    leftovers::{self, Identity, Kind, Leftover, Others},
};

/// How an app is taken off the machine. Never sent to the page.
#[derive(Clone, Debug, Default)]
pub struct Remove {
    pub kind: String,
    pub target: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// Where it came from: registry, store, scoop, apt, dnf, pacman, flatpak, snap, appimage.
    pub source: String,
    pub size: u64,
    pub installed: String,
    pub location: Option<String>,
    pub stamp: String,
    #[serde(skip)]
    pub ids: Vec<String>,
    #[serde(skip)]
    pub remove: Remove,
}

impl App {
    pub fn seal(mut self) -> Self {
        let mut h = Sha256::new();
        h.update(format!("{}|{}|{}|{}", self.name, self.version, self.remove.kind, self.remove.target));
        self.stamp = h.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect();
        self
    }

    pub fn identity(&self) -> Identity {
        Identity { name: self.name.clone(), publisher: self.publisher.clone(), ids: self.ids.clone(), location: self.location.as_ref().map(PathBuf::from) }
    }
}

pub fn list() -> Vec<App> {
    #[cfg(windows)]
    let mut apps = super::uninstaller_win::list();
    #[cfg(not(windows))]
    let mut apps = super::uninstaller_linux::list();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    apps
}

pub fn others_of(apps: &[App], skip: &str) -> Others {
    let mut others = Others::default();
    for a in apps.iter().filter(|a| a.id != skip) {
        others.names.push(leftovers::norm(&a.name));
        others.names.extend(a.ids.iter().map(|i| leftovers::norm(i)));
        if let Some(l) = &a.location {
            others.locations.push(PathBuf::from(l));
        }
    }
    others.names.retain(|n| n.len() >= 4);
    others
}

pub fn scan(app: &App, all: &[App], deep: bool, cancel: &AtomicBool) -> Vec<Leftover> {
    let others = others_of(all, &app.id);
    scan_identity(&app.identity(), &others, deep, cancel)
}

pub fn scan_identity(id: &Identity, others: &Others, deep: bool, cancel: &AtomicBool) -> Vec<Leftover> {
    #[cfg(windows)]
    let mut found = super::uninstaller_win::scan(id, others, deep, cancel);
    #[cfg(not(windows))]
    let mut found = super::uninstaller_linux::scan(id, others, deep, cancel);
    for item in found.iter_mut().filter(|i| i.kind == Kind::File) {
        item.size = fsx::size_of(Path::new(&item.path), cancel);
    }
    found.sort_by(|a, b| (b.confidence == leftovers::Confidence::Verified).cmp(&(a.confidence == leftovers::Confidence::Verified)).then(b.size.cmp(&a.size)));
    found
}

/// Stops running processes that live inside the app's own folder.
pub fn stop_background(location: &Path) -> usize {
    use sysinfo::{ProcessesToUpdate, System};
    if location.as_os_str().is_empty() || location.parent().is_none() {
        return 0;
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let me = std::process::id();
    let mut stopped = 0;
    for p in sys.processes().values() {
        if p.pid().as_u32() != me && p.exe().is_some_and(|e| e.starts_with(location)) && p.kill() {
            stopped += 1;
        }
    }
    stopped
}

/// Runs the app's own removal and waits for it. `Ok` carries a plain-words note.
pub fn run_remove(app: &App) -> Result<String, String> {
    #[cfg(windows)]
    return super::uninstaller_win::remove(app);
    #[cfg(not(windows))]
    super::uninstaller_linux::remove(app)
}

/// Whether the app is still on the machine (id based).
pub fn still_listed(id: &str) -> bool {
    list().iter().any(|a| a.id == id)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub path: String,
    pub kind: Kind,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub removed: Vec<String>,
    pub failed: Vec<Failed>,
    pub freed: u64,
    pub needs_elevation: bool,
    pub app_removed: bool,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct Failed {
    pub item: String,
    pub reason: String,
}

/// Deletes ticked leftovers, to the bin when `trash` is set. Sizes count once.
pub fn delete_items(items: &[Item], trash: bool, cancel: &AtomicBool, report: &mut Report) {
    for item in items {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let path = Path::new(&item.path);
        let outcome = match item.kind {
            Kind::File => {
                let size = fsx::size_of(path, &AtomicBool::new(false));
                let done = if trash { fsx::trash(path) } else { fsx::remove_all(path).map_err(|e| fsx::why(&e)) };
                done.map(|_| size)
            }
            #[cfg(windows)]
            Kind::Registry => super::uninstaller_win::delete_key(&item.path).map(|_| 0),
            #[cfg(not(windows))]
            Kind::Registry => Err("registry items only exist on Windows".into()),
        };
        match outcome {
            Ok(size) => {
                report.freed += size;
                report.removed.push(item.path.clone());
            }
            Err(reason) => {
                if reason.contains("rights") || reason.contains("denied") || reason.contains("protected") {
                    report.needs_elevation = true;
                }
                report.failed.push(Failed { item: item.path.clone(), reason });
            }
        }
    }
}

/// Deletes with administrator rights; the user sees the system prompt.
pub fn delete_elevated(paths: &[String]) -> Result<(), String> {
    for p in paths {
        let path = Path::new(p);
        if !path.exists() {
            continue;
        }
        #[cfg(windows)]
        {
            let (cmd, quoted) = if path.is_dir() { ("rmdir /s /q", format!("\"{p}\"")) } else { ("del /f /q", format!("\"{p}\"")) };
            let script = format!("Start-Process cmd -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '/c','{cmd} {}'", quoted.replace('\'', "''"));
            fsx::output("powershell.exe", &["-NoProfile", "-Command", &script])?;
        }
        #[cfg(not(windows))]
        fsx::output("pkexec", &["rm", "-rf", "--", p])?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Stored {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub ids: Vec<String>,
    pub location: Option<String>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(value) {
        let _ = fs::write(path, text);
    }
}

/// Remembers which apps are installed. Apps that vanish since last time join the "removed" list
/// so the cleaner can look for their leftovers later.
pub fn remember(dir: &Path, current: &[App]) {
    let snap_path = dir.join("installed.json");
    let removed_path = dir.join("removed.json");
    let before: Vec<Stored> = read_json(&snap_path);
    let mut removed: Vec<Stored> = read_json(&removed_path);
    for old in before {
        if !current.iter().any(|c| c.id == old.id) && !removed.iter().any(|r| r.id == old.id) {
            removed.push(old);
        }
    }
    removed.retain(|r| !current.iter().any(|c| c.id == r.id));
    removed.truncate(200);
    write_json(&removed_path, &removed);
    let now: Vec<Stored> =
        current.iter().map(|a| Stored { id: a.id.clone(), name: a.name.clone(), publisher: a.publisher.clone(), ids: a.ids.clone(), location: a.location.clone() }).collect();
    write_json(&snap_path, &now);
}

pub fn removed(dir: &Path) -> Vec<Stored> {
    read_json(&dir.join("removed.json"))
}

pub fn forget_removed(dir: &Path, ids: &[String]) {
    let mut removed: Vec<Stored> = removed(dir);
    removed.retain(|r| !ids.contains(&r.id));
    write_json(&dir.join("removed.json"), &removed);
}

pub fn pretty_package_name(raw: &str) -> String {
    let last = raw.rsplit('.').next().unwrap_or(raw);
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in last.chars() {
        if ch.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        out.push(if ch == '_' || ch == '-' { ' ' } else { ch });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str) -> App {
        App { id: id.into(), name: name.into(), ids: vec![id.into()], remove: Remove { kind: "x".into(), target: id.into() }, ..Default::default() }.seal()
    }

    #[test]
    fn stamp_changes_when_the_app_changes() {
        let a = app("a", "Alpha");
        let mut b = a.clone();
        b.version = "2".into();
        assert_eq!(a.stamp, app("a", "Alpha").stamp);
        assert_ne!(a.stamp, b.seal().stamp);
    }

    #[test]
    fn removed_apps_are_remembered_between_lists() {
        let dir = std::env::temp_dir().join(format!("notifi-remember-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        remember(&dir, &[app("a", "Alpha"), app("b", "Beta")]);
        assert!(removed(&dir).is_empty());
        remember(&dir, &[app("b", "Beta")]);
        assert_eq!(removed(&dir).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Alpha"]);
        remember(&dir, &[app("b", "Beta"), app("a", "Alpha")]);
        assert!(removed(&dir).is_empty());
        remember(&dir, &[app("b", "Beta")]);
        forget_removed(&dir, &["a".into()]);
        assert!(removed(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_leftovers_reports_each_item_once() {
        let root = std::env::temp_dir().join(format!("notifi-del-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("one")).unwrap();
        fs::write(root.join("one/f"), [0u8; 7]).unwrap();
        let items = [Item { path: root.join("one").to_string_lossy().into_owned(), kind: Kind::File }];
        let mut report = Report::default();
        delete_items(&items, false, &AtomicBool::new(false), &mut report);
        assert_eq!(report.freed, 7);
        assert_eq!(report.removed.len(), 1);
        assert!(!root.join("one").exists());
        assert!(report.failed.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn package_names_read_well() {
        assert_eq!(pretty_package_name("Microsoft.WindowsCalculator"), "Windows Calculator");
        assert_eq!(pretty_package_name("Notepad_plus"), "Notepad plus");
    }
}
