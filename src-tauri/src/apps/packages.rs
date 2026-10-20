//! The package manager view (spec/apps/package-manager.md): which managers exist, what they have
//! installed, and search. Installing, updating and removing run as jobs (see `jobs`).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::{exec, flatpak, pkcon, scoop, winget, AppsState};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pkg {
    pub manager: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub available: Option<String>,
    pub origin: String,
}

#[derive(Serialize, Clone)]
pub struct ManagerInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub found: bool,
}

#[derive(Serialize, Clone)]
pub struct SourceError {
    pub source: String,
    pub message: String,
}

#[derive(Serialize, Default)]
pub struct Listing {
    pub items: Vec<Pkg>,
    pub errors: Vec<SourceError>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Install,
    Update,
    Remove,
}

const KNOWN: &[(&str, &str, &str)] = if cfg!(windows) {
    &[("winget", "winget", "--version"), ("scoop", "Scoop", "--version")]
} else {
    &[("pkcon", "PackageKit", "--version"), ("flatpak", "Flatpak", "--version")]
};

fn detected() -> &'static Vec<ManagerInfo> {
    static CACHE: OnceLock<Vec<ManagerInfo>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let handles: Vec<_> = KNOWN.iter().map(|&(id, label, probe)| std::thread::spawn(move || ManagerInfo { id, label, found: exec::exists(id, probe) })).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

/// Managers found on this machine and allowed by the "Managers to use" setting (empty means all).
pub fn enabled(app: &AppHandle) -> Vec<&'static str> {
    let wanted: Vec<String> = super::strings(app, "packageManager", "managers").iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
    detected()
        .iter()
        .filter(|m| m.found && (wanted.is_empty() || wanted.iter().any(|w| w == m.id || w == &m.label.to_lowercase())))
        .map(|m| m.id)
        .collect()
}

pub fn list_one(manager: &str) -> Result<Vec<Pkg>, String> {
    match manager {
        "winget" => winget::installed(),
        "scoop" => scoop::installed(),
        "pkcon" => pkcon::installed(),
        "flatpak" => flatpak::installed(),
        _ => Err(format!("Unknown package manager {manager}")),
    }
}

pub fn search_one(manager: &str, query: &str) -> Result<Vec<Pkg>, String> {
    match manager {
        "winget" => winget::search(query),
        "scoop" => scoop::search(query),
        "pkcon" => pkcon::search(query),
        "flatpak" => flatpak::search(query),
        _ => Err(format!("Unknown package manager {manager}")),
    }
}

/// `(program, args)` for an action, or `None` for a manager this machine does not use.
pub fn command_for(manager: &str, action: Action, id: &str, origin: &str) -> Option<(&'static str, Vec<String>)> {
    Some(match (manager, action) {
        ("winget", Action::Install) => ("winget", winget::install_args(id)),
        ("winget", Action::Update) => ("winget", winget::update_args(id)),
        ("winget", Action::Remove) => ("winget", winget::remove_args(id)),
        ("scoop", Action::Install) => ("scoop", scoop::install_args(id)),
        ("scoop", Action::Update) => ("scoop", scoop::update_args(id)),
        ("scoop", Action::Remove) => ("scoop", scoop::remove_args(id)),
        ("pkcon", Action::Install) => ("pkcon", pkcon::install_args(id)),
        ("pkcon", Action::Update) => ("pkcon", pkcon::update_args(id)),
        ("pkcon", Action::Remove) => ("pkcon", pkcon::remove_args(id)),
        ("flatpak", Action::Install) => ("flatpak", flatpak::install_args(id, origin)),
        ("flatpak", Action::Update) => ("flatpak", flatpak::update_args(id)),
        ("flatpak", Action::Remove) => ("flatpak", flatpak::remove_args(id)),
        _ => return None,
    })
}

/// Runs `job` for every manager at once and gathers items and failures.
pub fn gather(managers: &[&'static str], job: impl Fn(&str) -> Result<Vec<Pkg>, String> + Sync) -> Listing {
    let mut out = Listing::default();
    let results: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = managers.iter().map(|&m| (m, s.spawn(|| job(m)))).collect();
        handles.into_iter().map(|(m, h)| (m, h.join().unwrap_or_else(|_| Err("crashed".into())))).collect()
    });
    for (m, result) in results {
        match result {
            Ok(items) => out.items.extend(items),
            Err(message) => out.errors.push(SourceError { source: m.into(), message }),
        }
    }
    out
}

fn third_party_hidden(p: &Pkg) -> bool {
    !matches!(p.origin.to_lowercase().as_str(), "" | "winget" | "msstore" | "main" | "flathub" | "installed")
        && !p.origin.contains("security")
        && !p.origin.contains("updates")
}

#[tauri::command]
pub fn apps_managers() -> Vec<ManagerInfo> {
    detected().clone()
}

#[tauri::command]
pub async fn apps_pkg_installed(app: AppHandle) -> Result<Listing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let managers = enabled(&app);
        let mut listing = gather(&managers, list_one);
        if !super::flag(&app, "packageManager", "thirdPartySources", true) {
            listing.items.retain(|p| !third_party_hidden(p));
        }
        if let Some(found) = app.state::<AppsState>().updates.lock().unwrap().as_ref() {
            for p in &mut listing.items {
                if let Some(u) = found.items.iter().find(|u| u.manager == p.manager && u.id == p.id) {
                    p.available = Some(u.available.clone());
                }
            }
        }
        listing.items.sort_by_key(|p| p.name.to_lowercase());
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn apps_pkg_search(app: AppHandle, query: String) -> Result<Listing, String> {
    let query = query.trim().to_string();
    if query.len() < 2 {
        return Ok(Listing::default());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut listing = gather(&enabled(&app), |m| search_one(m, &query));
        if !super::flag(&app, "packageManager", "thirdPartySources", true) {
            listing.items.retain(|p| !third_party_hidden(p));
        }
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}
