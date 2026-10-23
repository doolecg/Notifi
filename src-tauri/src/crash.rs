//! Crash logs. A panic, or an unhandled Windows exception, writes a text file to `<config>/crashes`. The next start
//! tells you on the Island, and Settings > About can open a GitHub issue with that log filled in. Notifi sends
//! nothing itself: the issue is only created if you submit the page that opens in your browser.
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Logs kept (new and reported together); the oldest go first.
const KEEP: usize = 10;
/// A second log within this many seconds of the last is dropped, so a panic in a loop can't flood the folder.
const QUIET_SECS: u64 = 5;
/// How much of the log goes in the issue; the whole URL has to stay under what GitHub accepts.
const BODY_CHARS: usize = 2000;
const NEW: &str = "txt";
const SENT: &str = "done";

static DIR: OnceLock<PathBuf> = OnceLock::new();
static LAST: AtomicU64 = AtomicU64::new(0);

/// Hooks panics (and, on Windows, unhandled exceptions) so they are written to the crash folder.
pub fn install(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let _ = DIR.set(dir.join("crashes"));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let title = format!("panic: {} ({place})", message.lines().next().unwrap_or_default());
        let detail = format!("thread: {}\n\n{message}\n\n{}", thread.name().unwrap_or("unnamed"), std::backtrace::Backtrace::force_capture());
        write(&title, &detail);
        previous(info);
    }));
    #[cfg(windows)]
    native::install();
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn write(title: &str, detail: &str) {
    let Some(dir) = DIR.get() else { return };
    let now = now_secs();
    if now.saturating_sub(LAST.swap(now, Ordering::Relaxed)) < QUIET_SECS {
        return;
    }
    let text = format!("title: {title}\nversion: {}\nos: {}\ntime: {now}\n{detail}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(dir.join(format!("crash-{now}-{}.{NEW}", std::process::id())), text);
    prune(dir);
}

/// Every log in the folder, oldest first.
fn logs(dir: &Path) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == NEW || x == SENT))
        .collect();
    all.sort();
    all
}

fn prune(dir: &Path) {
    let all = logs(dir);
    for old in all.iter().take(all.len().saturating_sub(KEEP)) {
        let _ = fs::remove_file(old);
    }
}

fn unreported(dir: &Path) -> Vec<PathBuf> {
    logs(dir).into_iter().filter(|p| p.extension().is_some_and(|x| x == NEW)).collect()
}

fn mark_sent(files: &[PathBuf]) {
    for file in files {
        let _ = fs::rename(file, file.with_extension(SENT));
    }
}

/// The folder Notifi reads, even if no crash was written this run.
fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("crashes"))
}

#[derive(Serialize)]
pub struct CrashInfo {
    title: String,
    /// Seconds since 1970.
    time: u64,
}

fn field<'a>(log: &'a str, name: &str) -> Option<&'a str> {
    log.lines().find_map(|l| l.strip_prefix(name)?.strip_prefix(": "))
}

fn info(file: &Path) -> Option<CrashInfo> {
    let log = fs::read_to_string(file).ok()?;
    Some(CrashInfo { title: field(&log, "title")?.to_string(), time: field(&log, "time")?.parse().ok()? })
}

/// Replaces your user name and home folder, so the log shows no more of your PC than it has to.
fn redact(text: &str, private: &[String]) -> String {
    private.iter().filter(|p| p.chars().count() >= 3).fold(text.to_string(), |t, p| t.replace(p.as_str(), "<user>"))
}

fn private_strings() -> Vec<String> {
    let mut out = Vec::new();
    for name in ["USERPROFILE", "HOME", "USERNAME", "USER"] {
        if let Ok(value) = std::env::var(name) {
            out.push(value.replace('\\', "/"));
            out.push(value);
        }
    }
    out
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn issue_url(repo: &str, log: &str, private: &[String]) -> String {
    let log = redact(log, private);
    let title: String = field(&log, "title").unwrap_or("crash").chars().take(120).collect();
    let version = field(&log, "version").unwrap_or("unknown");
    let shown: String = log.chars().take(BODY_CHARS).collect();
    let more = if shown.len() < log.len() { "\n(cut short)" } else { "" };
    let body = format!("Notifi {version} wrote this crash log. What were you doing when it happened?\n\n```\n{shown}{more}\n```\n");
    format!("https://github.com/{repo}/issues/new?labels=crash&title={}&body={}", encode(&format!("Crash: {title}")), encode(&body))
}

/// Tells you at start-up if the last run left a crash log you haven't dealt with.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        if dir(&app).is_some_and(|d| !unreported(&d).is_empty()) {
            crate::island::notice_to(&app, "Notifi hit a problem last time. Settings, About can report it.".into(), Some("settings:about"));
        }
    });
}

/// The newest crash log that hasn't been reported or dismissed.
#[tauri::command]
pub fn crash_status(app: AppHandle) -> Option<CrashInfo> {
    unreported(&dir(&app)?).last().and_then(|f| info(f))
}

/// Opens a new GitHub issue with the newest log filled in; it is only created when you submit it there.
#[tauri::command]
pub fn crash_report(app: AppHandle) -> Result<(), String> {
    let files = unreported(&dir(&app).ok_or("No crash folder.")?);
    let log = fs::read_to_string(files.last().ok_or("No crash to report.")?).map_err(|e| e.to_string())?;
    crate::capture::sys::open_url(&issue_url(crate::updater::REPO, &log, &private_strings()));
    mark_sent(&files);
    Ok(())
}

/// Forgets the crash logs without reporting them.
#[tauri::command]
pub fn crash_dismiss(app: AppHandle) {
    if let Some(d) = dir(&app) {
        mark_sent(&unreported(&d));
    }
}

#[cfg(windows)]
mod native {
    use windows::Win32::System::Diagnostics::Debug::{SetUnhandledExceptionFilter, EXCEPTION_POINTERS};

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        if let Some(record) = info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) {
            let code = record.ExceptionCode.0 as u32;
            let at = record.ExceptionAddress as usize;
            super::write(
                &format!("windows exception {code:#010X} at {at:#x}"),
                &format!("\n{}", std::backtrace::Backtrace::force_capture()),
            );
        }
        // Not handled here: Windows still shows its own report and ends the app.
        0
    }

    pub fn install() {
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "title: panic: boom (src/x.rs:3)\nversion: 0.3.2\nos: windows\ntime: 100\nthread: main\n\nboom\n";

    #[test]
    fn fields_are_read_by_name() {
        assert_eq!(field(LOG, "title"), Some("panic: boom (src/x.rs:3)"));
        assert_eq!(field(LOG, "time"), Some("100"));
        assert_eq!(field(LOG, "missing"), None);
    }

    #[test]
    fn private_strings_are_replaced_in_both_slash_styles() {
        let private = vec!["C:/Users/Jo Bloggs".to_string(), "C:\\Users\\Jo Bloggs".to_string()];
        assert_eq!(redact("at C:\\Users\\Jo Bloggs\\a and C:/Users/Jo Bloggs/b", &private), "at <user>\\a and <user>/b");
    }

    #[test]
    fn short_private_strings_are_left_alone() {
        assert_eq!(redact("a b a", &["a".to_string()]), "a b a");
    }

    #[test]
    fn the_url_is_encoded_and_has_no_raw_ampersands_from_the_log() {
        let url = issue_url("o/r", "title: a & b\nversion: 1\nx=1&y=2\n", &[]);
        assert!(url.starts_with("https://github.com/o/r/issues/new?labels=crash&title=Crash%3A%20a%20%26%20b&body="));
        assert_eq!(url.matches('&').count(), 2);
        assert!(!url.contains(' ') && !url.contains('\n'));
    }

    #[test]
    fn a_long_log_is_cut() {
        let long = format!("title: t\nversion: 1\n{}", "x".repeat(BODY_CHARS * 2));
        let url = issue_url("o/r", &long, &[]);
        assert!(url.contains("%28cut%20short%29"));
        assert!(url.len() < BODY_CHARS * 2);
    }

    #[test]
    fn a_written_log_reads_back_and_a_second_one_straight_after_is_dropped() {
        let dir = std::env::temp_dir().join(format!("notifi-crash-write-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        DIR.set(dir.clone()).unwrap();
        write("panic: boom (src/x.rs:3)", "thread: main

boom");
        write("panic: again", "x");
        let new = unreported(&dir);
        assert_eq!(new.len(), 1);
        let got = info(&new[0]).unwrap();
        assert_eq!(got.title, "panic: boom (src/x.rs:3)");
        assert!(got.time > 0);
        assert!(fs::read_to_string(&new[0]).unwrap().contains(env!("CARGO_PKG_VERSION")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_oldest_logs_are_pruned_and_reported_ones_are_not_new() {
        let dir = std::env::temp_dir().join(format!("notifi-crash-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for n in 0..KEEP + 3 {
            fs::write(dir.join(format!("crash-{n:03}-1.{NEW}")), LOG).unwrap();
        }
        prune(&dir);
        assert_eq!(logs(&dir).len(), KEEP);
        let new = unreported(&dir);
        mark_sent(&new[..2]);
        assert_eq!(unreported(&dir).len(), KEEP - 2);
        assert_eq!(logs(&dir).len(), KEEP);
        assert_eq!(info(new.last().unwrap()).unwrap().time, 100);
        let _ = fs::remove_dir_all(&dir);
    }
}
