//! Which installed Scoop apps need which others. `scoop export` does not say, but every installed
//! app keeps its manifest with a `depends` field, so the answer is read from disk. winget, PackageKit
//! and Flatpak list no dependencies for installed packages, so only Scoop apps can be grouped.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Needs {
    pub manager: &'static str,
    pub id: String,
    pub needs: Vec<String>,
}

/// The app names in a manifest's `depends` (a name or a list, each maybe `bucket/name`).
pub fn parse_depends(manifest: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<Value>(manifest.trim_start_matches('\u{feff}')) else { return Vec::new() };
    let names: Vec<&str> = match json.get("depends") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.rsplit('/').next().unwrap_or(name).trim();
        if !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// Scoop's install folder: `SCOOP`, else `root_path` from its config, else `~/scoop`.
fn root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SCOOP").map(PathBuf::from).filter(|p| p.is_dir()) {
        return Some(dir);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("scoop").join("config.json");
    let configured = fs::read_to_string(config)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|j| j.get("root_path").and_then(Value::as_str).map(PathBuf::from))
        .filter(|p| p.is_dir());
    configured.or_else(|| Some(home.join("scoop")).filter(|p| p.is_dir()))
}

fn read(root: &Path) -> Vec<Needs> {
    let Ok(apps) = fs::read_dir(root.join("apps")) else { return Vec::new() };
    let mut out: Vec<Needs> = apps
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            let manifest = fs::read_to_string(entry.path().join("current").join("manifest.json")).ok()?;
            let needs = parse_depends(&manifest);
            (!needs.is_empty()).then_some(Needs { manager: "scoop", id, needs })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// What each installed app needs, for the "Group dependencies under their package" option.
#[tauri::command]
pub async fn apps_pkg_dependencies() -> Vec<Needs> {
    tauri::async_runtime::spawn_blocking(|| root().map(|r| read(&r)).unwrap_or_default()).await.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depends_as_a_name_or_a_list() {
        assert_eq!(parse_depends(r#"{"depends": "git"}"#), ["git"]);
        assert_eq!(parse_depends(r#"{"depends": ["main/7zip", "extras/vcredist2022", "7zip"]}"#), ["7zip", "vcredist2022"]);
    }

    #[test]
    fn no_depends_or_bad_json_is_empty() {
        assert!(parse_depends(r#"{"version": "1"}"#).is_empty());
        assert!(parse_depends(r#"{"depends": 5}"#).is_empty());
        assert!(parse_depends("not json").is_empty());
        assert_eq!(parse_depends("\u{feff}{\"depends\": [\"curl\"]}"), ["curl"]);
    }

    #[test]
    fn reads_the_installed_manifests() {
        let dir = std::env::temp_dir().join(format!("notifi-scoop-deps-{}", std::process::id()));
        let app = dir.join("apps").join("mytool").join("current");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("manifest.json"), r#"{"depends": ["runtime"]}"#).unwrap();
        let plain = dir.join("apps").join("runtime").join("current");
        fs::create_dir_all(&plain).unwrap();
        fs::write(plain.join("manifest.json"), "{}").unwrap();
        let found = read(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found, [Needs { manager: "scoop", id: "mytool".into(), needs: vec!["runtime".into()] }]);
    }
}
