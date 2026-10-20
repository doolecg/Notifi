//! Adds and removes Notifi's two HTTP hooks in Claude Code's user settings, leaving everything else alone.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

const PREFIX: &str = "http://127.0.0.1:";
const TIMEOUT_SECS: u64 = 600;
const BACKUP: &str = "settings.json.notifi-backup";

/// Event, matcher, and the last part of the hook URL.
const ENTRIES: [(&str, Option<&str>, &str); 2] = [("PreToolUse", Some("AskUserQuestion"), "ask"), ("PermissionRequest", None, "permission")];

pub fn settings_path() -> Option<PathBuf> {
    crate::island::agents::home().map(|h| h.join(".claude").join("settings.json"))
}

fn url(port: u16, token: &str, tail: &str) -> String {
    format!("{PREFIX}{port}/hook/{token}/{tail}")
}

fn is_ours(url: &str, token: &str) -> bool {
    url.starts_with(PREFIX) && url.contains(&format!("/hook/{token}/"))
}

fn handler_url(handler: &Value) -> Option<&str> {
    handler.get("url")?.as_str()
}

/// Drops our handlers from one event's groups, and any group that held only ours.
fn strip_event(groups: &mut Vec<Value>, token: &str) -> bool {
    let mut removed = false;
    groups.retain_mut(|group| {
        let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
        let before = list.len();
        list.retain(|h| !handler_url(h).is_some_and(|u| is_ours(u, token)));
        if list.len() == before {
            return true;
        }
        removed = true;
        !list.is_empty()
    });
    removed
}

/// Adds our two entries, replacing earlier ones of ours (a changed port, say).
pub fn merge(root: &mut Value, port: u16, token: &str) -> Result<(), String> {
    let object = root.as_object_mut().ok_or("settings.json is not a JSON object.")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("\"hooks\" in settings.json is not an object.")?;
    for (event, matcher, tail) in ENTRIES {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("\"hooks.{event}\" in settings.json is not a list."))?;
        strip_event(groups, token);
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), json!(m));
        }
        group.insert("hooks".into(), json!([{ "type": "http", "url": url(port, token, tail), "timeout": TIMEOUT_SECS }]));
        groups.push(Value::Object(group));
    }
    Ok(())
}

/// Takes our entries out, and the lists and `hooks` object they leave empty. True when anything changed.
pub fn strip(root: &mut Value, token: &str) -> Result<bool, String> {
    let object = root.as_object_mut().ok_or("settings.json is not a JSON object.")?;
    let Some(hooks) = object.get_mut("hooks") else { return Ok(false) };
    let hooks = hooks.as_object_mut().ok_or("\"hooks\" in settings.json is not an object.")?;
    let mut emptied = Vec::new();
    let mut changed = false;
    for (event, _, _) in ENTRIES {
        if let Some(Value::Array(groups)) = hooks.get_mut(event) {
            if strip_event(groups, token) {
                changed = true;
                if groups.is_empty() {
                    emptied.push(event);
                }
            }
        }
    }
    hooks.retain(|k, _| !emptied.contains(&k.as_str()));
    if changed && hooks.is_empty() {
        object.retain(|k, _| k != "hooks");
    }
    Ok(changed)
}

fn has_entry(root: &Value, event: &str, matcher: Option<&str>, expected: &str) -> bool {
    let Some(groups) = root.pointer(&format!("/hooks/{event}")).and_then(Value::as_array) else { return false };
    groups.iter().any(|g| {
        g.get("matcher").and_then(Value::as_str) == matcher
            && g.get("hooks").and_then(Value::as_array).is_some_and(|l| {
                l.iter().any(|h| h.get("type").and_then(Value::as_str) == Some("http") && handler_url(h) == Some(expected))
            })
    })
}

/// Whether both of our entries are present for this port and token.
pub fn is_installed(root: &Value, port: u16, token: &str) -> bool {
    ENTRIES.iter().all(|(event, matcher, tail)| has_entry(root, event, *matcher, &url(port, token, tail)))
}

/// Whether the file holds hooks that are not ours.
pub fn has_other_hooks(root: &Value, token: &str) -> bool {
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else { return false };
    hooks.values().filter_map(Value::as_array).flatten().any(|group| match group.get("hooks").and_then(Value::as_array) {
        Some(list) => list.iter().any(|h| !handler_url(h).is_some_and(|u| is_ours(u, token))),
        None => true,
    })
}

/// (installed, other hooks present). An unreadable or invalid file counts as neither.
pub fn status(path: &Path, port: u16, token: &str) -> (bool, bool) {
    match read_root(path) {
        Ok(root) => (is_installed(&root, port, token), has_other_hooks(&root, token)),
        Err(_) => (false, false),
    }
}

/// The settings document; `{}` when the file is missing or blank. Invalid JSON is an error so nothing gets overwritten.
fn read_root(path: &Path) -> Result<Value, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(text).map_err(|e| format!("{} is not valid JSON ({e}). Nothing was changed.", path.display()))
}

/// Writes the document through a temp file. The first write copies the old file to `settings.json.notifi-backup`; an existing backup is kept.
fn write(path: &Path, root: &Value) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(fail)?;
    }
    if path.exists() {
        let backup = path.with_file_name(BACKUP);
        if !backup.exists() {
            fs::copy(path, &backup).map_err(fail)?;
        }
    }
    let mut text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = path.with_extension("json.notifi-tmp");
    fs::write(&tmp, text).map_err(fail)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail(e)
    })
}

pub fn install(path: &Path, port: u16, token: &str) -> Result<(), String> {
    let mut root = read_root(path)?;
    merge(&mut root, port, token)?;
    write(path, &root)
}

pub fn remove(path: &Path, token: &str) -> Result<(), String> {
    let mut root = read_root(path)?;
    if strip(&mut root, token)? {
        write(path, &root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn user_settings() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "echo hi" }] }],
                "Stop": [{ "hooks": [{ "type": "command", "command": "echo done" }] }]
            },
            "zzz": { "keep": true },
            "aaa": 1
        })
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-ask-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn merge_adds_both_entries_and_keeps_everything_else() {
        let mut root = user_settings();
        merge(&mut root, 47821, TOKEN).unwrap();
        assert!(is_installed(&root, 47821, TOKEN));
        assert!(!is_installed(&root, 47822, TOKEN));
        assert_eq!(root["model"], "opus");
        assert_eq!(root["zzz"], json!({ "keep": true }));
        assert_eq!(root["hooks"]["Stop"], user_settings()["hooks"]["Stop"]);
        let pre = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0], user_settings()["hooks"]["PreToolUse"][0]);
        assert_eq!(
            pre[1],
            json!({ "matcher": "AskUserQuestion", "hooks": [{ "type": "http", "url": format!("http://127.0.0.1:47821/hook/{TOKEN}/ask"), "timeout": 600 }] })
        );
        assert_eq!(
            root["hooks"]["PermissionRequest"],
            json!([{ "hooks": [{ "type": "http", "url": format!("http://127.0.0.1:47821/hook/{TOKEN}/permission"), "timeout": 600 }] }])
        );
        assert!(has_other_hooks(&root, TOKEN));
    }

    #[test]
    fn merge_twice_changes_nothing_and_a_new_port_replaces_ours() {
        let mut once = user_settings();
        merge(&mut once, 47821, TOKEN).unwrap();
        let mut twice = once.clone();
        merge(&mut twice, 47821, TOKEN).unwrap();
        assert_eq!(once, twice);
        merge(&mut twice, 47830, TOKEN).unwrap();
        assert!(is_installed(&twice, 47830, TOKEN));
        assert!(!is_installed(&twice, 47821, TOKEN));
        assert_eq!(twice["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(twice["hooks"]["PermissionRequest"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn strip_restores_the_original_and_clears_what_it_created() {
        let mut root = user_settings();
        merge(&mut root, 47821, TOKEN).unwrap();
        assert!(strip(&mut root, TOKEN).unwrap());
        assert_eq!(root, user_settings());
        assert!(!strip(&mut root, TOKEN).unwrap());

        let mut bare = json!({ "model": "opus" });
        merge(&mut bare, 47821, TOKEN).unwrap();
        assert!(strip(&mut bare, TOKEN).unwrap());
        assert_eq!(bare, json!({ "model": "opus" }));
    }

    #[test]
    fn strip_leaves_another_hook_in_the_same_group() {
        let mut root = json!({ "hooks": { "PermissionRequest": [{ "hooks": [
            { "type": "command", "command": "mine" },
            { "type": "http", "url": format!("http://127.0.0.1:47821/hook/{TOKEN}/permission") }
        ] }] } });
        assert!(strip(&mut root, TOKEN).unwrap());
        assert_eq!(root["hooks"]["PermissionRequest"], json!([{ "hooks": [{ "type": "command", "command": "mine" }] }]));
    }

    #[test]
    fn other_tokens_and_hosts_are_not_ours() {
        let theirs = json!({ "hooks": { "PreToolUse": [{ "matcher": "AskUserQuestion", "hooks": [
            { "type": "http", "url": "http://127.0.0.1:9000/hook/other/ask" },
            { "type": "http", "url": format!("http://example.com/hook/{TOKEN}/ask") }
        ] }] } });
        let mut root = theirs.clone();
        assert!(!strip(&mut root, TOKEN).unwrap());
        assert_eq!(root, theirs);
    }

    #[test]
    fn key_order_survives_a_round_trip() {
        let text = r#"{"zzz":1,"aaa":{"y":1,"b":2},"mmm":[]}"#;
        let mut root: Value = serde_json::from_str(text).unwrap();
        merge(&mut root, 47821, TOKEN).unwrap();
        let keys: Vec<&String> = root.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["zzz", "aaa", "mmm", "hooks"]);
        let inner: Vec<&String> = root["aaa"].as_object().unwrap().keys().collect();
        assert_eq!(inner, ["y", "b"]);
    }

    #[test]
    fn wrong_shapes_are_refused_without_changes() {
        assert!(merge(&mut json!({ "hooks": { "PreToolUse": {} } }), 1, TOKEN).is_err());
        let mut list = json!([]);
        assert!(merge(&mut list, 1, TOKEN).is_err());
        assert!(strip(&mut list, TOKEN).is_err());
        assert!(merge(&mut json!({ "hooks": [] }), 1, TOKEN).is_err());
        assert!(merge(&mut json!({ "hooks": { "PermissionRequest": "x" } }), 1, TOKEN).is_err());
    }

    #[test]
    fn install_creates_the_file_and_remove_deletes_only_ours() {
        let path = scratch("create");
        install(&path, 47821, TOKEN).unwrap();
        let (installed, other) = status(&path, 47821, TOKEN);
        assert!(installed && !other);
        assert!(!path.with_file_name(BACKUP).exists());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\n  \"hooks\": {"));
        remove(&path, TOKEN).unwrap();
        assert_eq!(read_root(&path).unwrap(), json!({}));
    }

    #[test]
    fn install_backs_up_once_and_never_overwrites_the_backup() {
        let path = scratch("backup");
        let original = serde_json::to_string_pretty(&user_settings()).unwrap();
        fs::write(&path, &original).unwrap();
        install(&path, 47821, TOKEN).unwrap();
        let backup = path.with_file_name(BACKUP);
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        install(&path, 47830, TOKEN).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        assert!(status(&path, 47830, TOKEN).0);
        remove(&path, TOKEN).unwrap();
        assert_eq!(read_root(&path).unwrap(), user_settings());
    }

    #[test]
    fn invalid_json_is_refused_and_left_untouched() {
        let path = scratch("invalid");
        fs::write(&path, "{ \"hooks\": ").unwrap();
        assert!(install(&path, 47821, TOKEN).unwrap_err().contains("not valid JSON"));
        assert!(remove(&path, TOKEN).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"hooks\": ");
        assert!(!path.with_file_name(BACKUP).exists());
        assert_eq!(status(&path, 47821, TOKEN), (false, false));
    }

    #[test]
    fn a_blank_or_bom_file_is_treated_as_empty() {
        let path = scratch("blank");
        fs::write(&path, "\u{feff}  \n").unwrap();
        install(&path, 47821, TOKEN).unwrap();
        assert!(status(&path, 47821, TOKEN).0);
    }
}
