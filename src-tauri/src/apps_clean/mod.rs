//! Phase 7, second half (spec/apps/uninstaller.md, cleaner.md, messaging-downloads.md,
//! wallpaper-gallery.md, clipboard/installer-helper.md, clipboard/appimage-installer.md).
//! Everything is driven from the settings pages; there is no window of its own.

mod cleaner;
mod fsx;
mod installer;
mod leftovers;
mod messaging;
mod uninstaller;
#[cfg(not(windows))]
mod uninstaller_linux;
#[cfg(windows)]
mod uninstaller_win;
mod wallpaper;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::settings::Store;

use self::{
    cleaner::{CategoryResult, CleanReport, Item as CleanItem, Request},
    installer::{AppImageInstalled, Inspect, Installed, Offer},
    leftovers::Leftover,
    messaging::{Group, Outcome},
    uninstaller::{App, Item, Report},
    wallpaper::Wall,
};

const OFFER_EVENT: &str = "apps-clean://offers";
const ELEVATED_FLAG: &str = "--apps-clean-scan";

#[derive(Default)]
pub struct AppsCleanState {
    cancel: AtomicBool,
    running_schedule: AtomicBool,
    installed: Mutex<Vec<App>>,
    scans: Mutex<HashMap<String, Vec<Leftover>>>,
    system: Mutex<HashMap<String, Vec<CleanItem>>>,
    offers: Mutex<Vec<Offer>>,
    seen: Mutex<installer::Seen>,
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir()).join("apps-clean")
}

fn getter(app: &AppHandle, namespace: &'static str) -> impl Fn(&str) -> Option<Value> {
    let settings = app.state::<Store>().get();
    move |key| settings.option(namespace, key).cloned()
}

fn flag(app: &AppHandle, namespace: &'static str, key: &str, default: bool) -> bool {
    getter(app, namespace)(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn cleaner_opts(app: &AppHandle) -> cleaner::Opts {
    cleaner::Opts::read(getter(app, "cleaner"))
}

fn messaging_rules(app: &AppHandle) -> messaging::Rules {
    messaging::Rules::read(getter(app, "messagingDownloads"))
}

async fn blocking<T: Send + 'static>(app: &AppHandle, work: impl FnOnce(AppHandle) -> T + Send + 'static) -> Result<T, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || work(handle)).await.map_err(|e| e.to_string())
}

fn refresh_installed(app: &AppHandle) -> Vec<App> {
    let list = uninstaller::list();
    uninstaller::remember(&data_dir(app), &list);
    *app.state::<AppsCleanState>().installed.lock().unwrap() = list.clone();
    list
}

fn installed_or_list(app: &AppHandle) -> Vec<App> {
    let cached = app.state::<AppsCleanState>().installed.lock().unwrap().clone();
    if cached.is_empty() { refresh_installed(app) } else { cached }
}

#[tauri::command]
pub async fn uninstaller_list(app: AppHandle) -> Result<Vec<App>, String> {
    app.state::<AppsCleanState>().cancel.store(false, Ordering::Relaxed);
    blocking(&app, |app| refresh_installed(&app)).await
}

#[tauri::command]
pub async fn uninstaller_scan(app: AppHandle, id: String, deep: bool) -> Result<Vec<Leftover>, String> {
    blocking(&app, move |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        let all = state.installed.lock().unwrap().clone();
        let target = all.iter().find(|a| a.id == id).ok_or("That app is no longer in the list. Reopen the list.")?;
        let found = uninstaller::scan(target, &all, deep, &state.cancel);
        let verified_only = !flag(&app, "uninstaller", "showNameMatched", true);
        let found: Vec<Leftover> = found.into_iter().filter(|l| !verified_only || l.confidence == leftovers::Confidence::Verified).collect();
        state.scans.lock().unwrap().insert(id, found.clone());
        Ok(found)
    })
    .await?
}

#[tauri::command]
pub fn apps_clean_cancel(state: State<AppsCleanState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn uninstaller_remove(app: AppHandle, id: String, stamp: String, items: Vec<Item>) -> Result<Report, String> {
    blocking(&app, move |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        let target = state.installed.lock().unwrap().iter().find(|a| a.id == id).cloned().ok_or("That app is no longer in the list.")?;
        if target.stamp != stamp {
            return Err("This app changed since it was scanned. Scan it again before removing.".to_string());
        }
        let known = state.scans.lock().unwrap().get(&id).cloned().unwrap_or_default();
        let items: Vec<Item> = items.into_iter().filter(|i| known.iter().any(|k| k.path == i.path && k.kind == i.kind)).collect();
        let mut report = Report::default();
        if flag(&app, "uninstaller", "stopBackground", true) {
            if let Some(location) = &target.location {
                uninstaller::stop_background(Path::new(location));
            }
        }
        match uninstaller::run_remove(&target) {
            Ok(note) => {
                report.note = note;
                report.app_removed = true;
            }
            Err(e) => {
                report.note = e;
                return Ok(report);
            }
        }
        let trash = flag(&app, "uninstaller", "trashLeftovers", true);
        uninstaller::delete_items(&items, trash, &state.cancel, &mut report);
        refresh_installed(&app);
        Ok(report)
    })
    .await?
}

#[tauri::command]
pub async fn uninstaller_retry_elevated(app: AppHandle, id: String, paths: Vec<String>) -> Result<(), String> {
    blocking(&app, move |app| {
        let known = app.state::<AppsCleanState>().scans.lock().unwrap().get(&id).cloned().unwrap_or_default();
        let paths: Vec<String> = paths.into_iter().filter(|p| known.iter().any(|k| &k.path == p && k.kind == leftovers::Kind::File)).collect();
        uninstaller::delete_elevated(&paths)
    })
    .await?
}

#[tauri::command]
pub fn apps_clean_reveal(path: String) {
    fsx::reveal(Path::new(&path));
}

#[tauri::command]
pub async fn cleaner_scan(app: AppHandle) -> Result<Vec<CategoryResult>, String> {
    blocking(&app, |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        let installed = if cleaner_opts(&app).leftovers { installed_or_list(&app) } else { vec![] };
        let system = state.system.lock().unwrap().clone();
        cleaner::scan(&cleaner_opts(&app), &installed, &data_dir(&app), &system, &state.cancel)
    })
    .await
}

/// The deeper scan: relaunches this program with administrator rights to size system folders.
#[tauri::command]
pub async fn cleaner_scan_elevated(app: AppHandle) -> Result<(), String> {
    blocking(&app, |app| {
        let dir = data_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let out = dir.join("system-scan.json");
        let _ = std::fs::remove_file(&out);
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if cfg!(windows) {
            let q = |s: &str| s.replace('\'', "''");
            let script = format!("Start-Process -FilePath '{}' -ArgumentList '{ELEVATED_FLAG}','\"{}\"' -Verb RunAs -Wait", q(&exe.to_string_lossy()), q(&out.to_string_lossy()));
            fsx::output("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script])?;
        } else {
            fsx::output("pkexec", &[&exe.to_string_lossy(), ELEVATED_FLAG, &out.to_string_lossy()])?;
        }
        let text = std::fs::read_to_string(&out).map_err(|_| "The deeper scan did not finish. It needs administrator approval.".to_string())?;
        let map: HashMap<String, Vec<CleanItem>> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        *app.state::<AppsCleanState>().system.lock().unwrap() = map;
        Ok(())
    })
    .await?
}

#[tauri::command]
pub async fn cleaner_clean(app: AppHandle, request: Request) -> Result<CleanReport, String> {
    blocking(&app, move |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        let installed = if request.categories.iter().any(|c| c == "leftovers") { installed_or_list(&app) } else { vec![] };
        let system = state.system.lock().unwrap().clone();
        cleaner::clean(&request, &cleaner_opts(&app), &installed, &data_dir(&app), &system, &state.cancel)
    })
    .await
}

#[tauri::command]
pub async fn cleaner_clean_system(app: AppHandle, roots: Vec<String>) -> Result<(), String> {
    blocking(&app, move |_| cleaner::clean_system_roots(&roots)).await?
}

#[tauri::command]
pub async fn messaging_scan(app: AppHandle) -> Result<Vec<Group>, String> {
    blocking(&app, |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        messaging::scan(&messaging_rules(&app), &fsx::downloads(), &fsx::home(), &state.cancel)
    })
    .await
}

#[tauri::command]
pub async fn messaging_apply(app: AppHandle, paths: Vec<String>) -> Result<Outcome, String> {
    blocking(&app, move |app| {
        let state = app.state::<AppsCleanState>();
        state.cancel.store(false, Ordering::Relaxed);
        messaging::apply(&messaging_rules(&app), Some(&paths), &fsx::downloads(), &fsx::home(), &state.cancel)
    })
    .await
}

fn offers_enabled(app: &AppHandle) -> bool {
    if cfg!(windows) {
        flag(app, "installerHelper", "enabled", false)
    } else {
        flag(app, "appImageInstaller", "enabled", false) && flag(app, "appImageInstaller", "offerOnOpen", true)
    }
}

fn current_offers() -> Vec<Offer> {
    installer::candidates(&fsx::downloads(), cfg!(windows), &installer::applications_dir())
}

#[tauri::command]
pub fn installer_offers(app: AppHandle) -> Vec<Offer> {
    let found = current_offers();
    *app.state::<AppsCleanState>().offers.lock().unwrap() = found.clone();
    found
}

fn offered(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let known = app.state::<AppsCleanState>().offers.lock().unwrap().iter().any(|o| o.path == path) || current_offers().iter().any(|o| o.path == path);
    if known { Ok(PathBuf::from(path)) } else { Err("That file is no longer offered for installing.".into()) }
}

#[tauri::command]
pub async fn installer_inspect(app: AppHandle, path: String) -> Result<Inspect, String> {
    let file = offered(&app, &path)?;
    blocking(&app, move |_| installer::inspect(&file)).await?
}

#[tauri::command]
pub async fn installer_install(app: AppHandle, path: String) -> Result<Installed, String> {
    let file = offered(&app, &path)?;
    blocking(&app, move |app| {
        let silent = flag(&app, "installerHelper", "silentInstall", true);
        let folder = getter(&app, "installerHelper")("portableFolder").and_then(|v| v.as_str().map(|s| s.trim().to_string())).unwrap_or_default();
        if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            let dir = if folder.is_empty() { fsx::env_path("LOCALAPPDATA").unwrap_or_else(fsx::home).join("Notifi").join("Apps") } else { PathBuf::from(folder) };
            installer::extract_portable(&file, &dir)
        } else {
            installer::install_windows(&file, silent)
        }
    })
    .await
}

#[tauri::command]
pub async fn installer_shortcut(app: AppHandle, program: String, desktop: bool) -> Result<(), String> {
    blocking(&app, move |_| installer::make_shortcuts(Path::new(&program), desktop)).await?
}

/// Moves the download to the Recycle Bin once it has been installed.
#[tauri::command]
pub fn installer_trash_download(app: AppHandle, path: String) -> Result<(), String> {
    let file = offered(&app, &path)?;
    fsx::trash(&file)
}

#[tauri::command]
pub fn installer_launch(program: String) -> Result<(), String> {
    let path = Path::new(&program);
    if !path.exists() {
        return Err("The program is no longer there.".into());
    }
    fsx::command(&program).current_dir(path.parent().unwrap_or(path)).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn appimage_install(app: AppHandle, path: String) -> Result<AppImageInstalled, String> {
    let file = offered(&app, &path)?;
    blocking(&app, move |_| {
        let home = fsx::home();
        let data = fsx::env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        installer::install_appimage(&file, &installer::applications_dir(), &data.join("applications"), &data.join("icons"))
    })
    .await?
}

fn wallpaper_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let chosen: Vec<String> = getter(app, "wallpaper")("folders")
        .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    wallpaper::folders(&chosen)
}

#[tauri::command]
pub async fn wallpaper_list(app: AppHandle) -> Result<Vec<Wall>, String> {
    blocking(&app, |app| wallpaper::list(&wallpaper_dirs(&app))).await
}

#[tauri::command]
pub async fn wallpaper_thumb(app: AppHandle, path: String) -> Result<String, String> {
    blocking(&app, move |app| {
        if !wallpaper::allowed(Path::new(&path), &wallpaper_dirs(&app)) {
            return Err("That image is outside the gallery folders.".to_string());
        }
        wallpaper::thumbnail(Path::new(&path))
    })
    .await?
}

#[tauri::command]
pub async fn wallpaper_apply(app: AppHandle, path: String) -> Result<(), String> {
    blocking(&app, move |app| {
        if !wallpaper::allowed(Path::new(&path), &wallpaper_dirs(&app)) {
            return Err("That image is outside the gallery folders.".to_string());
        }
        let all = getter(&app, "wallpaper")("applyTo").and_then(|v| v.as_str().map(|s| s == "all")).unwrap_or(false);
        wallpaper::apply(Path::new(&path), all)
    })
    .await?
}

fn size_words(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let (mut value, mut unit) = (bytes as f64, 0);
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

#[derive(Serialize, serde::Deserialize, Default)]
struct ScheduleFile {
    last: Option<i64>,
}

fn tell(app: &AppHandle, title: &str, body: &str) {
    if flag(app, "cleaner", "notifyAfter", true) {
        crate::monitor::notify::send(title.into(), body.into(), false);
    }
}

fn run_scheduled(app: &AppHandle) {
    let state = app.state::<AppsCleanState>();
    let opts = cleaner_opts(app);
    let dir = data_dir(app);
    let installed = if opts.leftovers { installed_or_list(app) } else { vec![] };
    let cancel = AtomicBool::new(false);
    let found = cleaner::scan(&opts, &installed, &dir, &state.system.lock().unwrap().clone(), &cancel);
    let mut request = Request::default();
    for cat in found.iter().filter(|c| c.checked && c.id != "installers") {
        request.categories.push(cat.id.clone());
        if cat.itemised {
            request.items.insert(cat.id.clone(), cat.items.iter().filter(|i| !i.heuristic).map(|i| i.path.clone()).collect());
        }
    }
    let report = cleaner::clean(&request, &opts, &installed, &dir, &state.system.lock().unwrap().clone(), &cancel);
    let mut freed = report.report.freed;
    let mut failed = report.report.failed.len();
    if flag(app, "messagingDownloads", "enabled", false) && flag(app, "messagingDownloads", "onSchedule", false) {
        let out = messaging::apply(&messaging_rules(app), None, &fsx::downloads(), &fsx::home(), &cancel);
        freed += out.freed;
        failed += out.failed.len();
    }
    if failed == 0 {
        tell(app, "Cleanup finished", &format!("Freed {}.", size_words(freed)));
    } else {
        tell(app, "Cleanup could not finish", &format!("Freed {}; {failed} item(s) could not be removed.", size_words(freed)));
    }
}

fn check_schedule(app: &AppHandle) {
    if !flag(app, "cleaner", "enabled", false) {
        return;
    }
    let schedule = cleaner::Schedule::read(getter(app, "cleaner"));
    if !schedule.auto {
        return;
    }
    let Some(now) = fsx::local_now() else { return };
    let path = data_dir(app).join("schedule.json");
    let mut file: ScheduleFile = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let save = |file: &ScheduleFile| {
        let _ = std::fs::create_dir_all(data_dir(app));
        let _ = std::fs::write(&path, serde_json::to_string(file).unwrap_or_default());
    };
    if file.last.is_none() {
        file.last = Some(cleaner::minutes(now));
        save(&file);
        return;
    }
    if cleaner::is_due(&schedule, now, file.last) && !app.state::<AppsCleanState>().running_schedule.swap(true, Ordering::SeqCst) {
        file.last = Some(cleaner::minutes(now));
        save(&file);
        run_scheduled(app);
        app.state::<AppsCleanState>().running_schedule.store(false, Ordering::SeqCst);
    }
}

fn check_offers(app: &AppHandle) {
    if !offers_enabled(app) {
        return;
    }
    let state = app.state::<AppsCleanState>();
    let found = current_offers();
    *state.offers.lock().unwrap() = found.clone();
    let fresh = state.seen.lock().unwrap().fresh(found);
    if !fresh.is_empty() {
        let _ = app.emit(OFFER_EVENT, &fresh);
        let names: Vec<&str> = fresh.iter().map(|o| o.name.as_str()).collect();
        let place = if cfg!(windows) { "Installer helper" } else { "AppImage installer" };
        crate::monitor::notify::send(format!("Install {}?", names[0]), format!("Open Notifi settings, {place}, to install it."), false);
    }
}

/// Starts the schedule and the Downloads watcher. Both read the settings on every pass.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        {
            let state = handle.state::<AppsCleanState>();
            let mut seen = state.seen.lock().unwrap();
            for offer in current_offers() {
                seen.0.insert(offer.path);
            }
        }
        for tick in 0u64.. {
            std::thread::sleep(Duration::from_secs(5));
            check_offers(&handle);
            if tick % 12 == 0 {
                check_schedule(&handle);
            }
        }
    });
}

/// `--apps-clean-scan <file>`: the elevated helper for the cleaner's deeper scan. Returns true when
/// this process was started for that and should exit instead of opening the app.
pub fn elevated_main() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let Some(at) = args.iter().position(|a| a == ELEVATED_FLAG) else { return false };
    if let Some(out) = args.get(at + 1) {
        let map = cleaner::scan_system(&AtomicBool::new(false));
        if let Ok(text) = serde_json::to_string(&map) {
            let _ = std::fs::write(out.trim_matches('"'), text);
        }
    }
    true
}
