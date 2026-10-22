//! The command permissions must agree with the code: every command main.rs registers is listed in build.rs and
//! in exactly one set of permissions/sets.toml, and only the settings and apps windows get the destructive set.

use std::{collections::BTreeSet, fs, path::PathBuf};

fn read(path: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The text between `open` and the next `close` after it.
fn between<'a>(text: &'a str, open: &str, close: &str) -> &'a str {
    let from = text.find(open).unwrap_or_else(|| panic!("no {open}")) + open.len();
    &text[from..from + text[from..].find(close).unwrap_or_else(|| panic!("no {close}"))]
}

fn handler_commands() -> BTreeSet<String> {
    let main = read("src/main.rs");
    between(&main, "generate_handler![", "]")
        .lines()
        .map(|l| l.split("//").next().unwrap_or("").trim().to_string())
        .flat_map(|l| l.split(',').map(|c| c.trim().to_string()).collect::<Vec<_>>())
        .filter(|c| !c.is_empty())
        .map(|c| c.rsplit("::").next().unwrap().to_string())
        .collect()
}

fn quoted(text: &str) -> Vec<String> {
    text.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

fn set(sets: &str, name: &str) -> BTreeSet<String> {
    let at = sets.find(&format!("identifier = \"{name}\"")).unwrap_or_else(|| panic!("no set {name}"));
    quoted(between(&sets[at..], "permissions = [", "]")).into_iter().collect()
}

#[test]
fn every_command_is_listed_and_in_one_set() {
    let registered = handler_commands();
    let build = read("build.rs");
    let listed: BTreeSet<String> = quoted(between(&build, "COMMANDS: &[&str] = &[", "];")).into_iter().collect();
    assert_eq!(registered, listed, "main.rs's invoke_handler and build.rs's COMMANDS differ");

    let sets = read("permissions/sets.toml");
    let (everyday, destructive) = (set(&sets, "everyday"), set(&sets, "destructive"));
    assert!(everyday.is_disjoint(&destructive), "a command is in both sets");
    let permitted: BTreeSet<String> = everyday.union(&destructive).cloned().collect();
    let wanted: BTreeSet<String> = registered.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
    assert_eq!(wanted, permitted, "permissions/sets.toml is missing a command, or has one that no longer exists");
}

#[test]
fn only_settings_and_apps_get_the_destructive_set() {
    let text = read("capabilities/destructive.json");
    assert_eq!(quoted(between(&text, "\"windows\": [", "]")), ["settings", "apps"]);
    assert_eq!(quoted(between(&text, "\"permissions\": [", "]")), ["destructive"]);
    for other in ["default.json", "commandbar.json", "everyday.json"] {
        assert!(!read(&format!("capabilities/{other}")).contains("destructive"), "{other} must not grant the destructive set");
    }
}
