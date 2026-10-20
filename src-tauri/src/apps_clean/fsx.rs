//! Shared file-system helpers: sizes, ages, the bin, local time and running commands.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// Runs a command and returns its standard output, or an error naming why it failed.
pub fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = command(program).args(args).output().map_err(|e| format!("{program} could not be started: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{program} failed: {}", err.trim().lines().next().unwrap_or("no details")))
    }
}

pub fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home() -> PathBuf {
    env_path(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_else(|| PathBuf::from("."))
}

pub fn downloads() -> PathBuf {
    home().join("Downloads")
}

pub fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

/// Total bytes under `path`, not following links. Stops early when `cancel` is set.
pub fn size_of(path: &Path, cancel: &AtomicBool) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if meta.file_type().is_symlink() {
        return 0;
    }
    if meta.is_file() {
        return meta.len();
    }
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    let mut total = 0;
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        total += size_of(&entry.path(), cancel);
    }
    total
}

/// Most recent modification time in a file or folder tree (bounded so huge trees stay quick).
pub fn newest_mtime(path: &Path) -> SystemTime {
    fn walk(path: &Path, budget: &mut u32) -> SystemTime {
        let Ok(meta) = fs::symlink_metadata(path) else { return UNIX_EPOCH };
        let mut newest = meta.modified().unwrap_or(UNIX_EPOCH);
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    if *budget == 0 {
                        break;
                    }
                    *budget -= 1;
                    newest = newest.max(walk(&entry.path(), budget));
                }
            }
        }
        newest
    }
    walk(path, &mut 5000)
}

pub fn age_of(time: SystemTime) -> Duration {
    SystemTime::now().duration_since(time).unwrap_or_default()
}

pub fn age_days(time: SystemTime) -> f64 {
    age_of(time).as_secs_f64() / 86_400.0
}

/// Moves a file or folder to the Recycle Bin / trash.
pub fn trash(path: &Path) -> Result<(), String> {
    trash_impl(path)
}

#[cfg(windows)]
fn trash_impl(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW},
    };
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(if path.exists() { format!("could not be moved to the Recycle Bin (code {code:#x})") } else { return Ok(()) });
    }
    Ok(())
}

#[cfg(not(windows))]
fn trash_impl(path: &Path) -> Result<(), String> {
    let base = env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("Trash");
    trash_into(path, &base)
}

/// The freedesktop trash layout: `files/<name>` plus `info/<name>.trashinfo`.
#[cfg_attr(windows, allow(dead_code))]
pub fn trash_into(path: &Path, base: &Path) -> Result<(), String> {
    let (files, info) = (base.join("files"), base.join("info"));
    fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    fs::create_dir_all(&info).map_err(|e| e.to_string())?;
    let absolute = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("no file name")?;
    let mut name = stem.clone();
    let mut n = 1;
    while files.join(&name).exists() || info.join(format!("{name}.trashinfo")).exists() {
        n += 1;
        name = format!("{stem}.{n}");
    }
    let (y, m, d, hh, mm, ss) = civil(now_secs());
    let record = format!("[Trash Info]\nPath={}\nDeletionDate={y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}\n", percent_encode(&absolute.to_string_lossy()));
    let info_file = info.join(format!("{name}.trashinfo"));
    fs::write(&info_file, record).map_err(|e| e.to_string())?;
    let target = files.join(&name);
    let moved = fs::rename(path, &target).or_else(|_| copy_all(path, &target).and_then(|_| remove_all(path)));
    moved.map_err(|e| {
        let _ = fs::remove_file(&info_file);
        e.to_string()
    })
}

fn percent_encode(s: &str) -> String {
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
        out
    })
}

pub fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

pub fn remove_all(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Plain words for a failed delete.
pub fn why(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "in use or protected; it needs more rights".into(),
        std::io::ErrorKind::NotFound => "already gone".into(),
        _ => e.to_string(),
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// (year, month, day, hour, minute, second) in UTC for a Unix time.
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
}

/// Local wall-clock time: `weekday` 0 is Monday.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

#[cfg(windows)]
pub fn local_now() -> Option<Local> {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    Some(Local {
        year: i64::from(t.wYear),
        month: u32::from(t.wMonth),
        day: u32::from(t.wDay),
        weekday: (u32::from(t.wDayOfWeek) + 6) % 7,
        hour: u32::from(t.wHour),
        minute: u32::from(t.wMinute),
    })
}

#[cfg(not(windows))]
pub fn local_now() -> Option<Local> {
    let text = output("date", &["+%Y %m %d %u %H %M"]).ok()?;
    let n: Vec<i64> = text.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let [year, month, day, weekday, hour, minute] = n[..] else { return None };
    Some(Local { year, month: month as u32, day: day as u32, weekday: (weekday as u32 + 6) % 7, hour: hour as u32, minute: minute as u32 })
}

pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        let _ = command("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        let _ = command("xdg-open").arg(dir).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_700_000_000), (2023, 11, 14, 22, 13, 20));
    }

    #[test]
    fn trash_layout_keeps_a_restorable_record() {
        let root = std::env::temp_dir().join(format!("notifi-trash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("a b.txt");
        fs::write(&file, "x").unwrap();
        let bin = root.join("Trash");
        trash_into(&file, &bin).unwrap();
        assert!(!file.exists());
        assert!(bin.join("files/a b.txt").exists());
        let record = fs::read_to_string(bin.join("info/a b.txt.trashinfo")).unwrap();
        assert!(record.contains("a%20b.txt"));
        fs::write(&file, "y").unwrap();
        trash_into(&file, &bin).unwrap();
        assert!(bin.join("files/a b.txt.2").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sizes_add_up_and_ignore_nothing_missing() {
        let root = std::env::temp_dir().join(format!("notifi-size-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a"), [0u8; 10]).unwrap();
        fs::write(root.join("sub/b"), [0u8; 5]).unwrap();
        assert_eq!(size_of(&root, &AtomicBool::new(false)), 15);
        assert_eq!(size_of(&root.join("missing"), &AtomicBool::new(false)), 0);
        let _ = fs::remove_dir_all(&root);
    }
}
