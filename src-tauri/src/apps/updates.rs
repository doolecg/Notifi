//! App updates (spec/apps/app-updates.md): package-manager sources compared tolerantly, minus exceptions and rules.

use std::{
    cmp::Ordering,
    fs,
    sync::atomic::{AtomicBool, Ordering as Atomic},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{
    flatpak, packages::{self, Pkg, SourceError}, pkcon, rules::{self, Rules}, scoop, version, winget, AppsState,
};

pub const UPDATES_EVENT: &str = "apps://updates";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub manager: String,
    pub id: String,
    pub name: String,
    pub installed: String,
    pub available: String,
    pub origin: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesResult {
    pub items: Vec<UpdateItem>,
    /// Apps whose versions could not be compared.
    pub not_compared: Vec<UpdateItem>,
    pub errors: Vec<SourceError>,
    /// Enabled sources that are not built yet.
    pub pending: Vec<&'static str>,
    pub checked_at: u64,
}

fn item(p: &Pkg) -> UpdateItem {
    UpdateItem {
        manager: p.manager.clone(),
        id: p.id.clone(),
        name: p.name.clone(),
        installed: p.version.clone(),
        available: p.available.clone().unwrap_or_default(),
        origin: p.origin.clone(),
    }
}

/// Splits raw reports into real updates and ones that cannot be compared.
pub fn classify(raw: &[Pkg], exceptions: &[String], rules: &Rules, prerelease: bool) -> (Vec<UpdateItem>, Vec<UpdateItem>) {
    let (mut items, mut unsure) = (Vec::new(), Vec::new());
    for p in raw {
        let available = p.available.clone().unwrap_or_default();
        if available.is_empty() || p.name.eq_ignore_ascii_case("notifi") {
            continue;
        }
        if rules::is_excluded(exceptions, rules, &p.manager, &p.id, &p.name, &available) {
            continue;
        }
        match version::compare(&p.version, &available) {
            Some(Ordering::Less) => {
                if !prerelease && version::is_prerelease(&available) && !version::is_prerelease(&p.version) {
                    continue;
                }
                items.push(item(p));
            }
            Some(_) => {}
            None => {
                if !p.version.eq_ignore_ascii_case(&available) {
                    unsure.push(item(p));
                }
            }
        }
    }
    (items, unsure)
}

fn raw_updates(manager: &str) -> Result<Vec<Pkg>, String> {
    let mut found = match manager {
        "winget" => winget::updates()?,
        "scoop" => scoop::updates()?,
        "pkcon" => pkcon::updates()?,
        "flatpak" => flatpak::updates()?,
        _ => return Ok(Vec::new()),
    };
    if found.iter().any(|p| p.version.is_empty()) {
        if let Ok(installed) = packages::list_one(manager) {
            for p in found.iter_mut().filter(|p| p.version.is_empty()) {
                if let Some(i) = installed.iter().find(|i| i.id == p.id) {
                    p.version = i.version.clone();
                }
            }
        }
    }
    Ok(found)
}

pub fn check(app: &AppHandle) -> UpdatesResult {
    let mut result = UpdatesResult::default();
    if super::flag(app, "appUpdates", "sourcePackageManagers", true) {
        let listing = packages::gather(&packages::enabled(app), raw_updates);
        let (items, unsure) = classify(
            &listing.items,
            &super::strings(app, "appUpdates", "exceptions"),
            &rules::load(app),
            super::flag(app, "appUpdates", "prerelease", false),
        );
        result.items = items;
        result.not_compared = unsure;
        result.errors = listing.errors;
    }
    if super::flag(app, "appUpdates", "sourcePublisherFeeds", true) {
        result.pending.push("Publisher feeds");
    }
    if super::flag(app, "appUpdates", "sourceCatalogue", true) {
        result.pending.push("Public version catalogue");
    }
    result.items.sort_by_key(|i| i.name.to_lowercase());
    result.checked_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    result
}

fn run_check(app: &AppHandle) -> UpdatesResult {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Atomic::SeqCst) {
        return app.state::<AppsState>().updates.lock().unwrap().clone().unwrap_or_default();
    }
    let result = check(app);
    BUSY.store(false, Atomic::SeqCst);
    *app.state::<AppsState>().updates.lock().unwrap() = Some(result.clone());
    let _ = app.emit(UPDATES_EVENT, &result);
    result
}

#[tauri::command]
pub async fn apps_updates_check(app: AppHandle) -> Result<UpdatesResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_check(&app)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apps_updates_last(app: AppHandle) -> Option<UpdatesResult> {
    app.state::<AppsState>().updates.lock().unwrap().clone()
}

/// How many updates in `now` were not in the previous check.
fn new_count(seen: &[String], now: &[String]) -> usize {
    now.iter().filter(|n| !seen.contains(n)).count()
}

/// The least time between two "updates found" notices, so the check can run often and the alert still comes once a day.
const ALERT_EVERY_SECS: u64 = 24 * 3600;

/// Whether a notice may be shown now, given when the last one was (unix seconds). A time in the future (the clock was set back) doesn't hold it up.
fn alert_due(last: Option<u64>, now: u64) -> bool {
    last.map_or(true, |t| t > now || now - t >= ALERT_EVERY_SECS)
}

/// When the last notice was shown, kept in a file so a restart doesn't tell you again.
fn alert_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("app-updates-told"))
}

fn last_alert(app: &AppHandle) -> Option<u64> {
    fs::read_to_string(alert_file(app)?).ok()?.trim().parse().ok()
}

fn stamp_alert(app: &AppHandle, now: u64) {
    if let Some(file) = alert_file(app) {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(file, now.to_string());
    }
}

/// Checks shortly after startup and then every `checkHours`. Updates found are told about once a day at most: ones that turn up
/// in between are kept for the next notice, which then counts all of them.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(25));
        let mut last: Option<SystemTime> = None;
        let mut seen: Vec<String> = Vec::new();
        loop {
            let on_start = super::flag(&app, "appUpdates", "checkAtStartup", true);
            let hours = super::option(&app, "appUpdates", "checkHours").and_then(|v| v.as_f64()).unwrap_or(6.0).clamp(1.0, 48.0);
            let due = match last {
                None => on_start,
                Some(t) => t.elapsed().map(|e| e >= Duration::from_secs_f64(hours * 3600.0)).unwrap_or(true),
            };
            if due && super::flag(&app, "appUpdates", "enabled", true) {
                let found = run_check(&app);
                let now: Vec<String> = found.items.iter().map(|i| format!("{}@{}", i.id, i.available)).collect();
                let fresh = new_count(&seen, &now);
                let wanted = fresh > 0 && super::flag(&app, "appUpdates", "alertOnFound", true);
                let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let told = wanted && alert_due(last_alert(&app), stamp);
                if told {
                    let text = if fresh == 1 { "1 app update found".to_string() } else { format!("{fresh} app updates found") };
                    crate::island::notice_to(&app, text, Some("apps:updates"));
                    stamp_alert(&app, stamp);
                }
                // Updates not told about yet (the day isn't up) stay new, so the next notice has them.
                if !wanted || told {
                    seen = now;
                }
                last = Some(SystemTime::now());
            } else if last.is_none() && !on_start {
                last = Some(SystemTime::now());
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_are_told_about_once_a_day() {
        let day = ALERT_EVERY_SECS;
        assert!(alert_due(None, 1_000));
        assert!(!alert_due(Some(1_000), 1_000 + day - 1));
        assert!(alert_due(Some(1_000), 1_000 + day));
        // The clock was set back since the last notice: not held up for the difference.
        assert!(alert_due(Some(5_000_000), 1_000));
    }

    #[test]
    fn counts_only_new_updates() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(new_count(&s(&[]), &s(&["a@1", "b@2"])), 2);
        assert_eq!(new_count(&s(&["a@1"]), &s(&["a@1", "b@2"])), 1);
        assert_eq!(new_count(&s(&["a@1"]), &s(&["a@2"])), 1);
        assert_eq!(new_count(&s(&["a@1"]), &s(&[])), 0);
    }

    fn pkg(id: &str, name: &str, version: &str, available: &str) -> Pkg {
        Pkg { manager: "winget".into(), id: id.into(), name: name.into(), version: version.into(), available: Some(available.into()), origin: "winget".into() }
    }

    #[test]
    fn real_updates_only() {
        let raw = [pkg("A", "A", "1.0", "1.1"), pkg("B", "B", "v2.0", "2.0"), pkg("C", "C", "3.0", "2.9"), pkg("D", "D", "Unknown", "4.0"), pkg("E", "E", "1.0-beta9", "1.0-beta10")];
        let (items, unsure) = classify(&raw, &[], &Rules::default(), false);
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["A", "E"]);
        assert_eq!(unsure.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["D"]);
    }

    #[test]
    fn exceptions_and_rules_hide_apps() {
        let raw = [pkg("A.A", "App A", "1.0", "2.0"), pkg("B.B", "App B", "1.0", "2.0"), pkg("C.C", "App C", "1.0", "2.0")];
        let rules = Rules {
            skipped: vec![rules::Skipped { manager: "winget".into(), id: "B.B".into(), name: "App B".into(), version: "2.0".into() }],
            ignored: vec![],
        };
        let (items, _) = classify(&raw, &["app a".to_string()], &rules, false);
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["C.C"]);
    }

    #[test]
    fn prerelease_channel_and_own_update() {
        let raw = [pkg("A", "A", "1.0", "2.0-rc1"), pkg("N", "Notifi", "0.1", "0.2")];
        assert!(classify(&raw, &[], &Rules::default(), false).0.is_empty());
        assert_eq!(classify(&raw, &[], &Rules::default(), true).0.len(), 1);
    }
}
