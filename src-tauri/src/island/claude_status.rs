//! Claude's 5 hour and weekly limits, from Claude Code itself. Its status line hands the command it runs the
//! session data, which includes `rate_limits` for Pro and Max plans (code.claude.com/docs/en/statusline).
//! Switching this on sets that command, in `~/.claude/settings.json`, to Notifi (`notifi --claude-statusline`).
//! Each run saves the limits to a file Notifi reads, then runs the status line the user had before, if any.
//! Nothing is sent anywhere and no login is read.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::agents::Limit;
use crate::ask::hooks;

pub const FLAG: &str = "--claude-statusline";
const LIMITS_FILE: &str = "claude-limits.json";
const ORIGINAL_FILE: &str = "claude-statusline-original.json";
/// Claude's two windows.
const FIVE_HOURS_MS: u64 = 5 * 60 * 60 * 1000;
const WEEK_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// More than this on stdin is not a status line's data.
const MAX_INPUT: u64 = 1024 * 1024;

/// Notifi's data folder, found without the app running (the status line command is a separate process).
/// The same place as Tauri's app data folder for io.notifi.app.
fn data_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?
    };
    Some(base.join("io.notifi.app"))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Window {
    used_percent: f64,
    /// Unix ms.
    resets_at: u64,
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq)]
struct Saved {
    /// When Claude Code last reported, Unix ms.
    at: u64,
    session: Option<Window>,
    week: Option<Window>,
}

fn window(rate_limits: &Value, key: &str) -> Option<Window> {
    let w = rate_limits.get(key)?;
    Some(Window { used_percent: w.get("used_percentage")?.as_f64()?.clamp(0.0, 100.0), resets_at: w.get("resets_at")?.as_u64()?.checked_mul(1000)? })
}

/// The limits in one status line report; none while the report has no `rate_limits` (before the first reply, or on other plans).
fn parse(report: &Value, now: u64) -> Option<Saved> {
    let rate_limits = report.get("rate_limits")?;
    let saved = Saved { at: now, session: window(rate_limits, "five_hour"), week: window(rate_limits, "seven_day") };
    (saved.session.is_some() || saved.week.is_some()).then_some(saved)
}

fn save(saved: &Saved) {
    let Some(dir) = data_dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let tmp = dir.join(format!("{LIMITS_FILE}.tmp"));
    if serde_json::to_vec(saved).ok().is_some_and(|bytes| fs::write(&tmp, bytes).is_ok()) {
        let _ = fs::rename(&tmp, dir.join(LIMITS_FILE));
    }
}

fn load() -> Option<Saved> {
    serde_json::from_slice(&fs::read(data_dir()?.join(LIMITS_FILE)).ok()?).ok()
}

/// A window that has passed reads as empty until Claude Code reports again.
fn limit(w: Option<Window>, window_ms: u64, now: u64) -> Option<Limit> {
    let w = w?;
    let used_percent = if w.resets_at <= now { 0.0 } else { w.used_percent };
    Some(Limit { used_percent, resets_at: (w.resets_at > now).then_some(w.resets_at), window_ms: Some(window_ms) })
}

/// (5 hour, weekly) as the Island shows them; `None` while Claude Code has never reported.
pub fn read(now: u64) -> Option<(Option<Limit>, Option<Limit>)> {
    let saved = load()?;
    Some((limit(saved.session, FIVE_HOURS_MS, now), limit(saved.week, WEEK_MS, now)))
}

// ---- The status line command ----

/// `notifi --claude-statusline`, run by Claude Code on every status line update. Returns true when this process was
/// started for that and should exit instead of opening the app.
pub fn statusline_main() -> bool {
    if !std::env::args().any(|a| a == FLAG) {
        return false;
    }
    let mut input = String::new();
    let _ = std::io::stdin().take(MAX_INPUT).read_to_string(&mut input);
    let report = serde_json::from_str::<Value>(input.trim_start_matches('\u{feff}')).ok();
    let saved = report.as_ref().and_then(|r| parse(r, now_ms()));
    if let Some(saved) = &saved {
        save(saved);
    }
    match original() {
        Some(command) => {
            let _ = std::io::stdout().write_all(&run_original(&command, &input));
        }
        None => {
            if let Some((s, w)) = saved.map(|s| (s.session, s.week)) {
                let p = |w: Option<Window>| w.map_or("-".to_string(), |w| format!("{}%", w.used_percent.round()));
                println!("5h {} | 7d {}", p(s), p(w));
            }
        }
    }
    true
}

/// The status line command the user had before Notifi's replaced it.
fn original() -> Option<String> {
    let text = fs::read_to_string(data_dir()?.join(ORIGINAL_FILE)).ok()?;
    serde_json::from_str::<Value>(&text).ok()?.get("command")?.as_str().map(str::to_string)
}

/// Runs the user's own status line command with the same data on stdin and returns what it printed.
fn run_original(command: &str, input: &str) -> Vec<u8> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-Command", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let Ok(mut child) = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return Vec::new() };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    child.wait_with_output().map(|o| o.stdout).unwrap_or_default()
}

/// What goes in `statusLine.command`. On Windows it is wrapped in PowerShell, which every shell Claude Code may use can start with this quoting.
fn command_for(exe: &Path) -> String {
    let exe = exe.display();
    if cfg!(windows) {
        format!("powershell -NoProfile -Command \"& '{exe}' {FLAG}\"")
    } else {
        format!("\"{exe}\" {FLAG}")
    }
}

fn is_ours(status_line: Option<&Value>) -> bool {
    status_line.and_then(|s| s.get("command")).and_then(Value::as_str).is_some_and(|c| c.contains(FLAG))
}

/// Sets the status line to Notifi's, keeping the user's own to run from it. A backup of settings.json is made by the first write.
pub fn install(settings: &Path, exe: &Path) -> Result<(), String> {
    let mut root = hooks::read_root(settings)?;
    let object = root.as_object_mut().ok_or("Claude Code's settings are not a JSON object. Nothing was changed.")?;
    let dir = data_dir().ok_or("Could not find Notifi's data folder.")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !is_ours(object.get("statusLine")) {
        match object.get("statusLine") {
            Some(own) => fs::write(dir.join(ORIGINAL_FILE), serde_json::to_vec_pretty(own).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
            None => {
                let _ = fs::remove_file(dir.join(ORIGINAL_FILE));
            }
        }
    }
    object.insert("statusLine".into(), json!({ "type": "command", "command": command_for(exe) }));
    hooks::write(settings, &root)
}

/// Puts the user's own status line back, or removes the setting when there was none.
pub fn remove(settings: &Path) -> Result<(), String> {
    let mut root = hooks::read_root(settings)?;
    let Some(object) = root.as_object_mut() else { return Ok(()) };
    if !is_ours(object.get("statusLine")) {
        return Ok(());
    }
    let sidecar = data_dir().map(|d| d.join(ORIGINAL_FILE));
    match sidecar.as_ref().and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(own) => {
            object.insert("statusLine".into(), own);
        }
        None => {
            object.remove("statusLine");
        }
    }
    hooks::write(settings, &root)?;
    if let Some(p) = sidecar {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

pub fn installed(settings: &Path) -> bool {
    hooks::read_root(settings).is_ok_and(|root| is_ours(root.get("statusLine")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    installed: bool,
    /// When Claude Code last reported, Unix ms.
    reported_at: Option<u64>,
    session: Option<f64>,
    week: Option<f64>,
}

fn status() -> Status {
    let now = now_ms();
    let (s, w) = read(now).unwrap_or((None, None));
    Status {
        installed: hooks::settings_path().is_some_and(|p| installed(&p)),
        reported_at: load().map(|l| l.at),
        session: s.map(|l| l.used_percent),
        week: w.map(|l| l.used_percent),
    }
}

#[tauri::command]
pub fn claude_limits_status() -> Status {
    status()
}

#[tauri::command]
pub fn claude_limits_install() -> Result<Status, String> {
    let settings = hooks::settings_path().ok_or("Could not find your home folder.")?;
    install(&settings, &std::env::current_exe().map_err(|e| e.to_string())?)?;
    Ok(status())
}

#[tauri::command]
pub fn claude_limits_remove() -> Result<Status, String> {
    remove(&hooks::settings_path().ok_or("Could not find your home folder.")?)?;
    Ok(status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_limits_is_read() {
        let report = json!({ "model": {}, "rate_limits": { "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 }, "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 } } });
        let saved = parse(&report, 5).unwrap();
        assert_eq!(saved.session, Some(Window { used_percent: 23.5, resets_at: 1_738_425_600_000 }));
        assert_eq!(saved.week.unwrap().used_percent, 41.2);
        assert_eq!(saved.at, 5);
    }

    #[test]
    fn a_report_without_limits_or_with_one_window_is_handled() {
        assert_eq!(parse(&json!({ "model": {} }), 0), None);
        assert_eq!(parse(&json!({ "rate_limits": {} }), 0), None);
        let one = parse(&json!({ "rate_limits": { "seven_day": { "used_percentage": 150.0, "resets_at": 10 } } }), 0).unwrap();
        assert_eq!((one.session, one.week.unwrap().used_percent), (None, 100.0));
    }

    #[test]
    fn a_window_that_has_passed_reads_as_empty() {
        let w = Some(Window { used_percent: 80.0, resets_at: 1000 });
        assert_eq!(limit(w, FIVE_HOURS_MS, 500).unwrap().used_percent, 80.0);
        let past = limit(w, FIVE_HOURS_MS, 2000).unwrap();
        assert_eq!((past.used_percent, past.resets_at), (0.0, None));
    }

    #[test]
    fn installing_keeps_the_users_own_status_line_and_removing_puts_it_back() {
        let dir = std::env::temp_dir().join(format!("notifi-statusline-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // The only test that touches Notifi's data folder, so pointing it at a temporary one is safe.
        let (var, value) = if cfg!(windows) { ("APPDATA", dir.join("data")) } else { ("XDG_DATA_HOME", dir.join("data")) };
        std::env::set_var(var, &value);
        let settings = dir.join("settings.json");
        let own = json!({ "type": "command", "command": "~/.claude/statusline.sh", "padding": 1 });
        fs::write(&settings, serde_json::to_vec(&json!({ "model": "opus", "statusLine": own })).unwrap()).unwrap();

        install(&settings, Path::new("/apps/notifi")).unwrap();
        assert!(installed(&settings));
        assert_eq!(original().as_deref(), Some("~/.claude/statusline.sh"));
        // A second install keeps what the first saved.
        install(&settings, Path::new("/apps/notifi")).unwrap();
        assert_eq!(original().as_deref(), Some("~/.claude/statusline.sh"));

        remove(&settings).unwrap();
        let back: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!((back["statusLine"].clone(), back["model"].clone()), (own, json!("opus")));
        assert!(!installed(&settings));

        // With none before, removing takes the setting out.
        fs::write(&settings, "{}").unwrap();
        install(&settings, Path::new("/apps/notifi")).unwrap();
        assert!(original().is_none());
        remove(&settings).unwrap();
        assert!(hooks::read_root(&settings).unwrap().get("statusLine").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_notifis_own_command_counts_as_ours() {
        assert!(is_ours(Some(&json!({ "command": command_for(Path::new("C:/Apps/Notifi/notifi.exe")) }))));
        assert!(!is_ours(Some(&json!({ "command": "~/.claude/statusline.sh" }))));
        assert!(!is_ours(None));
    }
}
