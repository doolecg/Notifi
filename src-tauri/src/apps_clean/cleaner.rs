//! The cleaner (spec/apps/cleaner.md): size each category first, delete only what is ticked.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::{
    fsx,
    leftovers::{Confidence, Kind},
    uninstaller::{self, App, Report},
};

/// Nothing newer than this is ever touched.
const MIN_AGE: Duration = Duration::from_secs(600);
/// System folders are cleared by the system tool, which only takes things a day old.
const SYSTEM_MIN_DAYS: u32 = 1;
const INSTALLER_DAYS: f64 = 60.0;
const PRESERVE: &[&str] = &["themes", "icons", "fonts", "locale", "locales", "plugins", "extensions", "languages", "dictionaries", "tracker3", "tracker", "baloo"];

#[derive(Clone, Debug, PartialEq)]
pub struct Opts {
    pub caches: bool,
    pub logs: bool,
    pub temp: bool,
    pub recycle: bool,
    pub screenshots: bool,
    pub screenshot_days: f64,
    pub leftovers: bool,
    pub installers: bool,
}

impl Opts {
    pub fn read(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let flag = |k: &str, d: bool| get(k).and_then(|v| v.as_bool()).unwrap_or(d);
        Self {
            caches: flag("caches", true),
            logs: flag("logs", true),
            temp: flag("temp", true),
            recycle: flag("recycleBin", false),
            screenshots: flag("screenshots", false),
            screenshot_days: get("screenshotAge").and_then(|v| v.as_f64()).unwrap_or(30.0).clamp(1.0, 365.0),
            leftovers: flag("leftovers", false),
            installers: flag("oldInstallers", false),
        }
    }

    pub fn ticked(&self, group: &str) -> bool {
        match group {
            "caches" => self.caches,
            "logs" => self.logs,
            "temp" => self.temp,
            "recycleBin" => self.recycle,
            "screenshots" => self.screenshots,
            "leftovers" => self.leftovers,
            "oldInstallers" => self.installers,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Filter {
    All,
    /// Entries whose name starts with one of these (case-insensitive).
    Starts(&'static [&'static str]),
    Screenshots,
    Installers,
}

#[derive(Clone, Debug)]
struct Root {
    path: PathBuf,
    filter: Filter,
    system: bool,
}

fn user(path: PathBuf) -> Root {
    Root { path, filter: Filter::All, system: false }
}

fn user_starting(path: PathBuf, starts: &'static [&'static str]) -> Root {
    Root { path, filter: Filter::Starts(starts), system: false }
}

fn system(path: &str) -> Root {
    Root { path: PathBuf::from(path), filter: Filter::All, system: true }
}

struct Def {
    id: &'static str,
    label: &'static str,
    why: &'static str,
    /// The settings switch that decides whether it starts ticked.
    group: &'static str,
    itemised: bool,
}

const DEFS: &[Def] = &[
    Def { id: "temp", label: "Temporary files", why: "Apps leave these behind and recreate what they need.", group: "temp", itemised: false },
    Def { id: "caches", label: "Application caches", why: "Apps rebuild caches; they may start a little slower once.", group: "caches", itemised: false },
    Def { id: "browser", label: "Browser caches", why: "Only cached pages and images. History and logins stay.", group: "caches", itemised: false },
    Def { id: "thumbnails", label: "Thumbnail caches", why: "Thumbnails are redrawn the next time a folder opens.", group: "caches", itemised: false },
    Def { id: "packages", label: "Package manager caches", why: "Downloaded installers are fetched again if you reinstall.", group: "caches", itemised: false },
    Def { id: "logs", label: "Logs", why: "Old logs only help when diagnosing a past problem.", group: "logs", itemised: false },
    Def { id: "crashdumps", label: "Crash dumps", why: "Memory snapshots of crashes that already happened.", group: "logs", itemised: false },
    Def { id: "recycleBin", label: "Recycle Bin or trash", why: "Emptying it makes deleted files unrecoverable.", group: "recycleBin", itemised: false },
    Def { id: "screenshots", label: "Forgotten screenshots", why: "Untouched screenshots older than your limit. Renamed ones are kept.", group: "screenshots", itemised: true },
    Def { id: "installers", label: "Old installers", why: "Found by analysis: check each path before ticking.", group: "oldInstallers", itemised: true },
    Def { id: "leftovers", label: "Leftovers of removed apps", why: "Verified against the removed app's own identifiers.", group: "leftovers", itemised: true },
];

fn local() -> PathBuf {
    fsx::env_path("LOCALAPPDATA").unwrap_or_else(|| fsx::home().join("AppData\\Local"))
}

fn chromium_roots(base: PathBuf, user_data: &str) -> Vec<Root> {
    let data = base.join(user_data);
    let Ok(profiles) = fs::read_dir(&data) else { return vec![] };
    let mut out = vec![];
    for p in profiles.flatten() {
        let name = p.file_name().to_string_lossy().into_owned();
        if name == "Default" || name.starts_with("Profile ") {
            for sub in ["Cache", "Code Cache", "GPUCache"] {
                out.push(user(p.path().join(sub)));
            }
        }
    }
    out
}

fn firefox_roots(profiles: PathBuf) -> Vec<Root> {
    let Ok(entries) = fs::read_dir(profiles) else { return vec![] };
    entries.flatten().map(|e| user(e.path().join("cache2"))).collect()
}

fn roots_for(id: &str, days: f64) -> Vec<Root> {
    let _ = days;
    let home = fsx::home();
    if cfg!(windows) {
        let l = local();
        let roam = fsx::env_path("APPDATA").unwrap_or_else(|| home.join("AppData\\Roaming"));
        let win = fsx::env_path("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        let wins = |p: &str| Root { path: win.join(p), filter: Filter::All, system: true };
        match id {
            "temp" => vec![user(std::env::temp_dir()), wins("Temp")],
            "caches" => vec![user(l.join("D3DSCache")), user(l.join("Microsoft\\Windows\\INetCache")), wins("SoftwareDistribution\\Download"), wins("Prefetch")],
            "browser" => {
                let mut r = chromium_roots(l.join("Google\\Chrome"), "User Data");
                r.extend(chromium_roots(l.join("Microsoft\\Edge"), "User Data"));
                r.extend(chromium_roots(l.join("BraveSoftware\\Brave-Browser"), "User Data"));
                r.extend(firefox_roots(l.join("Mozilla\\Firefox\\Profiles")));
                let _ = roam;
                r
            }
            "thumbnails" => vec![user_starting(l.join("Microsoft\\Windows\\Explorer"), &["thumbcache_", "iconcache_"])],
            "packages" => vec![
                user(l.join("npm-cache")),
                user(l.join("pip\\Cache")),
                user(fsx::env_path("SCOOP").unwrap_or_else(|| home.join("scoop")).join("cache")),
            ],
            "logs" => vec![user(l.join("Microsoft\\Windows\\WER\\ReportArchive")), user(l.join("Microsoft\\Windows\\WER\\ReportQueue")), wins("Logs\\CBS")],
            "crashdumps" => vec![user(l.join("CrashDumps")), wins("Minidump")],
            "screenshots" => vec![
                Root { path: home.join("Pictures\\Screenshots"), filter: Filter::Screenshots, system: false },
                Root { path: home.join("Pictures"), filter: Filter::Screenshots, system: false },
            ],
            "installers" => vec![Root { path: fsx::downloads(), filter: Filter::Installers, system: false }],
            _ => vec![],
        }
    } else {
        let cache = fsx::env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let share = fsx::env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        match id {
            "temp" => vec![user(std::env::temp_dir())],
            "caches" => vec![user(cache.clone())],
            "browser" => {
                let mut r: Vec<Root> = ["google-chrome", "chromium", "BraveSoftware", "microsoft-edge", "vivaldi"].iter().map(|n| user(cache.join(n))).collect();
                r.extend(firefox_roots(cache.join("mozilla/firefox")));
                r
            }
            "thumbnails" => vec![user(cache.join("thumbnails"))],
            "packages" => vec![user(home.join(".npm/_cacache")), system("/var/cache/apt/archives"), system("/var/cache/pacman/pkg"), system("/var/cache/dnf")],
            "logs" => vec![user_starting(home.clone(), &[".xsession-errors"]), user(share.join("xorg"))],
            "crashdumps" => vec![system("/var/crash"), system("/var/lib/systemd/coredump")],
            "recycleBin" => vec![user(share.join("Trash/files")), user(share.join("Trash/info"))],
            "screenshots" => vec![
                Root { path: home.join("Pictures/Screenshots"), filter: Filter::Screenshots, system: false },
                Root { path: home.join("Pictures"), filter: Filter::Screenshots, system: false },
            ],
            "installers" => vec![Root { path: fsx::downloads(), filter: Filter::Installers, system: false }],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    pub size: u64,
    pub age_days: f64,
    /// Found by analysis; never ticked on its own.
    pub heuristic: bool,
    #[serde(default)]
    pub system: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResult {
    pub id: String,
    pub label: String,
    pub why: String,
    pub size: u64,
    pub count: usize,
    pub checked: bool,
    pub itemised: bool,
    pub items: Vec<Item>,
    /// Part of this category sits in system locations that need more rights.
    pub needs_elevation: bool,
}

fn is_screenshot_name(name: &str) -> bool {
    let n = name.to_lowercase();
    let image = [".png", ".jpg", ".jpeg", ".bmp", ".webp"].iter().any(|e| n.ends_with(e));
    image && ["screenshot", "screen shot", "spectacle", "snip"].iter().any(|p| n.starts_with(p))
}

fn is_installer_name(name: &str) -> bool {
    let n = name.to_lowercase();
    [".msi", ".exe", ".msix", ".deb", ".rpm", ".pkg.tar.zst"].iter().any(|e| n.ends_with(e))
}

fn preserved(name: &str) -> bool {
    PRESERVE.contains(&name.to_lowercase().as_str())
}

/// The removable entries of one root: old enough, not links, not customisation folders.
fn entries(root: &Root, min_days: f64, cancel: &AtomicBool) -> Vec<Item> {
    let Ok(read) = fs::read_dir(&root.path) else { return vec![] };
    let mut out = vec![];
    for e in read.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let ok = match root.filter {
            Filter::All => !preserved(&name) || !meta.is_dir(),
            Filter::Starts(prefixes) => meta.is_file() && prefixes.iter().any(|p| name.to_lowercase().starts_with(p)),
            Filter::Screenshots => meta.is_file() && is_screenshot_name(&name),
            Filter::Installers => meta.is_file() && is_installer_name(&name),
        };
        if !ok {
            continue;
        }
        let newest = if meta.is_dir() { fsx::newest_mtime(&path) } else { meta.modified().unwrap_or(std::time::UNIX_EPOCH) };
        if fsx::age_of(newest) < MIN_AGE || fsx::age_days(newest) < min_days {
            continue;
        }
        let heuristic = root.filter == Filter::Installers;
        out.push(Item { path: path.to_string_lossy().into_owned(), size: fsx::size_of(&path, cancel), age_days: fsx::age_days(newest), heuristic, system: root.system });
    }
    out
}

fn min_days(id: &str, opts: &Opts) -> f64 {
    match id {
        "screenshots" => opts.screenshot_days,
        "installers" => INSTALLER_DAYS,
        _ => 0.0,
    }
}

fn recycle_bin_size() -> (u64, usize) {
    #[cfg(windows)]
    {
        use windows::{core::PCWSTR, Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO}};
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
        if unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_ok() {
            return (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as usize);
        }
    }
    (0, 0)
}

/// Scans every category. `installed` guards leftovers; `removed_dir` is where removed apps are remembered.
pub fn scan(opts: &Opts, installed: &[App], data_dir: &Path, system_items: &HashMap<String, Vec<Item>>, cancel: &AtomicBool) -> Vec<CategoryResult> {
    scan_with(opts, installed, data_dir, system_items, cancel, &|_| {})
}

/// Measures the categories side by side (the slow ones are disk walks) and reports each as soon as it is done.
pub fn scan_with(opts: &Opts, installed: &[App], data_dir: &Path, system_items: &HashMap<String, Vec<Item>>, cancel: &AtomicBool, on_done: &(dyn Fn(&CategoryResult) + Sync)) -> Vec<CategoryResult> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<CategoryResult>>> = DEFS.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= DEFS.len() || cancel.load(Ordering::Relaxed) {
                    break;
                }
                let done = scan_one(&DEFS[i], opts, installed, data_dir, system_items, cancel);
                on_done(&done);
                *slots[i].lock().unwrap() = Some(done);
            });
        }
    });
    slots.into_iter().filter_map(|s| s.into_inner().unwrap()).collect()
}

fn scan_one(def: &Def, opts: &Opts, installed: &[App], data_dir: &Path, system_items: &HashMap<String, Vec<Item>>, cancel: &AtomicBool) -> CategoryResult {
    {
        let mut items: Vec<Item> = vec![];
        let mut needs_elevation = false;
        let (mut size, mut count) = (0u64, 0usize);
        if def.id == "recycleBin" && cfg!(windows) {
            (size, count) = recycle_bin_size();
        } else if def.id == "leftovers" {
            items = leftover_items(installed, data_dir, cancel);
        } else {
            for root in roots_for(def.id, 0.0) {
                if root.system {
                    needs_elevation = true;
                    if let Some(known) = system_items.get(def.id) {
                        items.extend(known.iter().filter(|i| Path::new(&i.path).starts_with(&root.path)).cloned());
                    }
                    continue;
                }
                items.extend(entries(&root, min_days(def.id, opts), cancel));
            }
        }
        if !items.is_empty() {
            size = items.iter().map(|i| i.size).sum();
            count = items.len();
        }
        let mut seen = std::collections::HashSet::new();
        items.retain(|i| seen.insert(i.path.clone()));
        if !def.itemised {
            items.clear();
        } else {
            items.sort_by(|a, b| b.size.cmp(&a.size));
        }
        CategoryResult {
            id: def.id.into(),
            label: def.label.into(),
            why: def.why.into(),
            size,
            count,
            checked: opts.ticked(def.group),
            itemised: def.itemised,
            items,
            needs_elevation,
        }
    }
}

fn leftover_items(installed: &[App], data_dir: &Path, cancel: &AtomicBool) -> Vec<Item> {
    let mut out = vec![];
    for gone in uninstaller::removed(data_dir) {
        let id = super::leftovers::Identity { name: gone.name.clone(), publisher: gone.publisher.clone(), ids: gone.ids.clone(), location: gone.location.as_ref().map(PathBuf::from) };
        let others = uninstaller::others_of(installed, &gone.id);
        for l in uninstaller::scan_identity(&id, &others, false, cancel).into_iter().filter(|l| l.kind == Kind::File) {
            out.push(Item {
                age_days: fsx::age_days(fsx::newest_mtime(Path::new(&l.path))),
                path: l.path,
                size: l.size,
                heuristic: l.confidence != Confidence::Verified,
                system: false,
            });
        }
    }
    out
}

/// What the elevated helper scans: only system locations.
pub fn scan_system(cancel: &AtomicBool) -> HashMap<String, Vec<Item>> {
    let mut map = HashMap::new();
    for def in DEFS {
        let items: Vec<Item> = roots_for(def.id, 0.0).into_iter().filter(|r| r.system).flat_map(|r| entries(&r, f64::from(SYSTEM_MIN_DAYS), cancel)).collect();
        if !items.is_empty() {
            map.insert(def.id.to_string(), items);
        }
    }
    map
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Ticked category ids.
    pub categories: Vec<String>,
    /// For itemised categories: the ticked item paths.
    #[serde(default)]
    pub items: HashMap<String, Vec<String>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    #[serde(flatten)]
    pub report: Report,
    pub cancelled: bool,
    pub elevated_roots: Vec<String>,
}

fn delete_entry(item: &Item, to_bin: bool, report: &mut Report) {
    let one = [uninstaller::Item { path: item.path.clone(), kind: Kind::File }];
    uninstaller::delete_items(&one, to_bin, &AtomicBool::new(false), report);
}

/// Deletes the ticked categories. Everything is re-scanned first, so only paths that are still
/// inside the known roots (and still old enough) can be removed.
pub fn clean(req: &Request, opts: &Opts, installed: &[App], data_dir: &Path, system_items: &HashMap<String, Vec<Item>>, cancel: &AtomicBool) -> CleanReport {
    let mut out = CleanReport::default();
    let fresh = scan(opts, installed, data_dir, system_items, cancel);
    for cat in fresh.iter().filter(|c| req.categories.contains(&c.id)) {
        if cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        if cat.id == "recycleBin" && cfg!(windows) {
            empty_recycle_bin(&mut out.report);
            continue;
        }
        let def = DEFS.iter().find(|d| d.id == cat.id).expect("known category");
        let chosen: Option<&Vec<String>> = req.items.get(&cat.id);
        let list: Vec<Item> = if def.itemised {
            cat.items.iter().filter(|i| chosen.is_some_and(|c| c.contains(&i.path))).cloned().collect()
        } else {
            roots_for(&cat.id, 0.0).iter().filter(|r| !r.system).flat_map(|r| entries(r, 0.0, &AtomicBool::new(false))).collect()
        };
        let to_bin = def.itemised;
        for item in list.iter().filter(|i| !i.system) {
            if cancel.load(Ordering::Relaxed) {
                out.cancelled = true;
                break;
            }
            delete_entry(item, to_bin, &mut out.report);
        }
        if cat.needs_elevation {
            out.elevated_roots.extend(roots_for(&cat.id, 0.0).into_iter().filter(|r| r.system).map(|r| r.path.to_string_lossy().into_owned()));
        }
    }
    if req.categories.iter().any(|c| c == "leftovers") && !out.cancelled {
        let ids: Vec<String> = uninstaller::removed(data_dir).into_iter().map(|r| r.id).collect();
        if out.report.failed.is_empty() {
            uninstaller::forget_removed(data_dir, &ids);
        }
    }
    out
}

fn empty_recycle_bin(report: &mut Report) {
    #[cfg(windows)]
    {
        use windows::{core::PCWSTR, Win32::UI::Shell::SHEmptyRecycleBinW};
        let (before, _) = recycle_bin_size();
        match unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), 1 | 2 | 4) } {
            Ok(()) => {
                report.freed += before;
                report.removed.push("Recycle Bin".into());
            }
            Err(e) => report.failed.push(uninstaller::Failed { item: "Recycle Bin".into(), reason: e.message() }),
        }
    }
    #[cfg(not(windows))]
    let _ = report;
}

/// Clears the contents of fixed system folders with administrator rights, one prompt per folder.
pub fn clean_system_roots(roots: &[String]) -> Result<(), String> {
    for root in roots {
        let known = DEFS.iter().flat_map(|d| roots_for(d.id, 0.0)).any(|r| r.system && r.path.to_string_lossy() == root.as_str());
        if !known {
            return Err("refused: not a known system cleaning folder".into());
        }
        #[cfg(windows)]
        {
            let inner = format!("forfiles /p \"{root}\" /d -{SYSTEM_MIN_DAYS} /c \"cmd /c if @isdir==TRUE (rmdir /s /q @path) else (del /f /q @path)\"");
            let script = format!("Start-Process cmd -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '/c','{}'", inner.replace('\'', "''"));
            fsx::output("powershell.exe", &["-NoProfile", "-Command", &script])?;
        }
        #[cfg(not(windows))]
        fsx::output("pkexec", &["find", root, "-mindepth", "1", "-maxdepth", "1", "-mtime", &format!("+{SYSTEM_MIN_DAYS}"), "-exec", "rm", "-rf", "--", "{}", "+"])?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    pub auto: bool,
    pub weekly: bool,
    /// 0 is Monday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub notify: bool,
}

impl Schedule {
    pub fn read(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let text = get("time").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let (hour, minute) = parse_time(&text).unwrap_or((3, 0));
        let weekday = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"]
            .iter()
            .position(|d| get("weekday").and_then(|v| v.as_str().map(|s| s == *d)).unwrap_or(false))
            .unwrap_or(6) as u32;
        Self {
            auto: get("autoClean").and_then(|v| v.as_bool()).unwrap_or(false),
            weekly: get("frequency").and_then(|v| v.as_str().map(|s| s != "daily")).unwrap_or(true),
            weekday,
            hour,
            minute,
            notify: get("notifyAfter").and_then(|v| v.as_bool()).unwrap_or(true),
        }
    }
}

pub fn parse_time(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Local minutes since the epoch, for comparing wall-clock moments.
pub fn minutes(t: fsx::Local) -> i64 {
    days_from_civil(t.year, t.month, t.day) * 1440 + i64::from(t.hour) * 60 + i64::from(t.minute)
}

/// The most recent moment the schedule should have fired, at or before `now`.
pub fn last_due(s: &Schedule, now: fsx::Local) -> i64 {
    let today = minutes(fsx::Local { hour: 0, minute: 0, ..now }) + i64::from(s.hour) * 60 + i64::from(s.minute);
    let mut due = today;
    if s.weekly {
        due -= i64::from((now.weekday + 7 - s.weekday) % 7) * 1440;
    }
    let step = if s.weekly { 7 * 1440 } else { 1440 };
    if due > minutes(now) {
        due -= step;
    }
    due
}

/// Whether a run is owed: it has not run since the last scheduled moment.
pub fn is_due(s: &Schedule, now: fsx::Local, last_run: Option<i64>) -> bool {
    s.auto && last_run.is_some_and(|last| last < last_due(s, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-clean-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn old(path: &Path, days: u64) {
        let when = std::time::SystemTime::now() - Duration::from_secs(days * 86_400);
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_modified(when).unwrap();
    }

    #[test]
    fn sizes_each_root_and_skips_fresh_links_and_preserved_folders() {
        let dir = temp("size");
        fs::write(dir.join("old.bin"), [0u8; 100]).unwrap();
        old(&dir.join("old.bin"), 2);
        fs::write(dir.join("fresh.bin"), [0u8; 50]).unwrap();
        fs::create_dir_all(dir.join("themes")).unwrap();
        fs::create_dir_all(dir.join("cachedir")).unwrap();
        fs::write(dir.join("cachedir/x"), [0u8; 25]).unwrap();
        old(&dir.join("cachedir/x"), 2);
        let found = entries(&user(dir.clone()), 0.0, &AtomicBool::new(false));
        let mut names: Vec<String> = found.iter().map(|i| Path::new(&i.path).file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["old.bin"], "cachedir's own mtime is fresh; fresh files and themes are kept");
        assert_eq!(found[0].size, 100);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn screenshot_age_and_renamed_files() {
        let dir = temp("shots");
        for (name, days) in [("Screenshot 2024-01-01.png", 40), ("Screenshot new.png", 3), ("Holiday.png", 90), ("Screenshot old.txt", 90)] {
            fs::write(dir.join(name), "x").unwrap();
            old(&dir.join(name), days);
        }
        let root = Root { path: dir.clone(), filter: Filter::Screenshots, system: false };
        let found = entries(&root, 30.0, &AtomicBool::new(false));
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("Screenshot 2024-01-01.png"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn installers_are_always_heuristic() {
        let dir = temp("inst");
        fs::write(dir.join("setup.exe"), "x").unwrap();
        old(&dir.join("setup.exe"), 90);
        fs::write(dir.join("notes.txt"), "x").unwrap();
        old(&dir.join("notes.txt"), 90);
        let root = Root { path: dir.clone(), filter: Filter::Installers, system: false };
        let found = entries(&root, INSTALLER_DAYS, &AtomicBool::new(false));
        assert_eq!(found.len(), 1);
        assert!(found[0].heuristic);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleaning_deletes_only_the_ticked_items() {
        let dir = temp("clean");
        for n in ["a.png", "b.png"] {
            fs::write(dir.join(format!("Screenshot {n}")), "x").unwrap();
            old(&dir.join(format!("Screenshot {n}")), 60);
        }
        let root = Root { path: dir.clone(), filter: Filter::Screenshots, system: false };
        let items = entries(&root, 30.0, &AtomicBool::new(false));
        assert_eq!(items.len(), 2);
        let keep = items.iter().find(|i| i.path.ends_with("b.png")).unwrap().clone();
        let mut report = Report::default();
        delete_entry(items.iter().find(|i| i.path.ends_with("a.png")).unwrap(), false, &mut report);
        assert_eq!(report.removed.len(), 1);
        assert!(Path::new(&keep.path).exists());
        assert!(!dir.join("Screenshot a.png").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_read_with_defaults() {
        let o = Opts::read(|_| None);
        assert!(o.caches && o.logs && o.temp && !o.recycle && !o.screenshots && !o.leftovers && !o.installers);
        assert_eq!(o.screenshot_days, 30.0);
        assert!(o.ticked("temp") && !o.ticked("screenshots"));
    }

    fn at(day: u32, weekday: u32, hour: u32, minute: u32) -> fsx::Local {
        fsx::Local { year: 2026, month: 10, day, weekday, hour, minute }
    }

    #[test]
    fn schedule_fires_once_after_the_set_moment() {
        let weekly = Schedule { auto: true, weekly: true, weekday: 6, hour: 3, minute: 0, notify: true };
        let sunday_noon = at(18, 6, 12, 0);
        let before = minutes(at(18, 6, 2, 0));
        assert!(is_due(&weekly, sunday_noon, Some(before)));
        let after = minutes(at(18, 6, 3, 5));
        assert!(!is_due(&weekly, sunday_noon, Some(after)));
        assert!(!is_due(&weekly, at(18, 6, 2, 59), Some(minutes(at(11, 6, 3, 1)))), "not yet this Sunday");
        assert!(is_due(&weekly, at(21, 2, 9, 0), Some(minutes(at(11, 6, 3, 1)))), "missed Sunday 18th while the machine was off");
        assert!(!is_due(&weekly, at(21, 2, 9, 0), None), "a first sighting never fires");
        let daily = Schedule { weekly: false, ..weekly };
        assert!(is_due(&daily, at(19, 0, 3, 1), Some(minutes(at(18, 6, 3, 30)))));
        assert!(!is_due(&daily, at(19, 0, 3, 1), Some(minutes(at(19, 0, 3, 0)))));
        assert!(!is_due(&Schedule { auto: false, ..daily }, at(19, 0, 9, 0), Some(0)));
    }

    #[test]
    fn schedule_reads_settings() {
        let s = Schedule::read(|k| match k {
            "time" => Some(serde_json::json!("07:45")),
            "weekday" => Some(serde_json::json!("friday")),
            "frequency" => Some(serde_json::json!("weekly")),
            "autoClean" => Some(serde_json::json!(true)),
            _ => None,
        });
        assert_eq!((s.weekday, s.hour, s.minute, s.weekly, s.auto, s.notify), (4, 7, 45, true, true, true));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_time("9:05"), Some((9, 5)));
    }
}
