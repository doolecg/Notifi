//! Update exceptions and per-app rules (skip one version, ignore an app).

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Rules {
    #[serde(default)]
    pub skipped: Vec<Skipped>,
    #[serde(default)]
    pub ignored: Vec<Ignored>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Skipped {
    pub manager: String,
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ignored {
    pub manager: String,
    pub id: String,
    pub name: String,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("update-rules.json"))
}

pub fn load(app: &AppHandle) -> Rules {
    path(app).and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, rules: &Rules) -> Result<(), String> {
    let path = path(app).ok_or("No settings folder")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Whole-text match, ignoring case; `*` stands for any run of characters.
pub fn matches(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.trim().to_lowercase(), text.trim().to_lowercase());
    if p.is_empty() {
        return false;
    }
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == t;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !t.starts_with(first) || !t[first.len()..].ends_with(last) || t.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &t[first.len()..t.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

pub fn is_excluded(exceptions: &[String], rules: &Rules, manager: &str, id: &str, name: &str, available: &str) -> bool {
    exceptions.iter().any(|e| matches(e, name) || matches(e, id))
        || rules.ignored.iter().any(|r| r.manager == manager && r.id == id)
        || rules.skipped.iter().any(|r| r.manager == manager && r.id == id && r.version.eq_ignore_ascii_case(available))
}

#[tauri::command]
pub fn apps_rules(app: AppHandle) -> Rules {
    load(&app)
}

/// `kind` is `skip` (needs `version`) or `ignore`.
#[tauri::command]
pub fn apps_rule_add(app: AppHandle, kind: String, manager: String, id: String, name: String, version: Option<String>) -> Result<Rules, String> {
    let mut rules = load(&app);
    match kind.as_str() {
        "skip" => {
            let version = version.filter(|v| !v.is_empty()).ok_or("A version to skip is needed")?;
            if !rules.skipped.iter().any(|r| r.manager == manager && r.id == id && r.version == version) {
                rules.skipped.push(Skipped { manager, id, name, version });
            }
        }
        "ignore" => {
            if !rules.ignored.iter().any(|r| r.manager == manager && r.id == id) {
                rules.ignored.push(Ignored { manager, id, name });
            }
        }
        _ => return Err("Unknown rule".into()),
    }
    save(&app, &rules)?;
    let _ = app.emit(super::jobs::CHANGED_EVENT, ());
    Ok(rules)
}

#[tauri::command]
pub fn apps_rule_remove(app: AppHandle, kind: String, manager: String, id: String, version: Option<String>) -> Result<Rules, String> {
    let mut rules = load(&app);
    match kind.as_str() {
        "skip" => rules.skipped.retain(|r| !(r.manager == manager && r.id == id && version.as_ref().is_none_or(|v| &r.version == v))),
        _ => rules.ignored.retain(|r| !(r.manager == manager && r.id == id)),
    }
    save(&app, &rules)?;
    let _ = app.emit(super::jobs::CHANGED_EVENT, ());
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_matching_is_whole_name() {
        assert!(matches("Git", "git"));
        assert!(!matches("Git", "GitHub Desktop"));
        assert!(matches("Microsoft.*", "Microsoft.VisualStudioCode"));
        assert!(matches("*studio*", "Visual Studio Code"));
        assert!(!matches("a*b", "ab c"));
        assert!(matches("a*b", "ab"));
        assert!(!matches("", "anything"));
    }

    #[test]
    fn rules_exclude_by_id_and_version() {
        let rules = Rules {
            skipped: vec![Skipped { manager: "winget".into(), id: "A.A".into(), name: "A".into(), version: "2.0".into() }],
            ignored: vec![Ignored { manager: "scoop".into(), id: "git".into(), name: "git".into() }],
        };
        assert!(is_excluded(&[], &rules, "winget", "A.A", "A", "2.0"));
        assert!(!is_excluded(&[], &rules, "winget", "A.A", "A", "2.1"));
        assert!(is_excluded(&[], &rules, "scoop", "git", "git", "9"));
        assert!(!is_excluded(&[], &rules, "winget", "git", "git", "9"));
        assert!(is_excluded(&["a".to_string()], &Rules::default(), "winget", "X.X", "A", "1"));
    }
}
