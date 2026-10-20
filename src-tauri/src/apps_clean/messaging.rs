//! Messaging downloads (spec/apps/messaging-downloads.md): retention rules on the media chat apps
//! leave in Downloads. Only file names and times are looked at; chats and contents are never read.

use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::fsx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Trash,
    Organise,
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub retention_days: f64,
    pub keep_touched: bool,
    /// Megabytes per app; 0 means no cap.
    pub size_cap_mb: f64,
    pub apps: Vec<String>,
    pub action: Action,
}

impl Rules {
    pub fn read(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        Self {
            retention_days: get("retentionDays").and_then(|v| v.as_f64()).unwrap_or(30.0).max(1.0),
            keep_touched: get("keepTouched").and_then(|v| v.as_bool()).unwrap_or(false),
            size_cap_mb: get("sizeCap").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0),
            apps: get("apps")
                .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.trim().to_lowercase())).filter(|s| !s.is_empty()).collect()))
                .unwrap_or_default(),
            action: if get("action").and_then(|v| v.as_str().map(|s| s == "organise")).unwrap_or(false) { Action::Organise } else { Action::Trash },
        }
    }
}

enum Where {
    /// Every file below this folder.
    Folder(PathBuf),
    /// Files directly in this folder whose names start with one of the prefixes.
    Prefixed(PathBuf, &'static [&'static str]),
}

struct AppDef {
    name: &'static str,
    places: Vec<Where>,
}

fn definitions(downloads: &Path, home: &Path) -> Vec<AppDef> {
    let docs = home.join("Documents");
    vec![
        AppDef { name: "WhatsApp", places: vec![Where::Prefixed(downloads.into(), &["whatsapp "]), Where::Folder(docs.join("WhatsApp")), Where::Folder(downloads.join("WhatsApp Unknown"))] },
        AppDef { name: "Telegram", places: vec![Where::Folder(downloads.join("Telegram Desktop")), Where::Folder(docs.join("Telegram Desktop"))] },
        AppDef { name: "Signal", places: vec![Where::Prefixed(downloads.into(), &["signal-"])] },
        AppDef { name: "Discord", places: vec![Where::Folder(downloads.join("Discord"))] },
    ]
}

pub fn kind_of(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let ext = n.rsplit('.').next().unwrap_or("");
    if n.contains("ptt") || n.contains("voice") || matches!(ext, "opus" | "oga" | "ogg") {
        "voice"
    } else if n.contains("sticker") || matches!(ext, "gif" | "webp") {
        "stickers"
    } else if matches!(ext, "jpg" | "jpeg" | "png" | "heic" | "bmp" | "tif" | "tiff") {
        "images"
    } else if matches!(ext, "mp4" | "mov" | "mkv" | "avi" | "webm" | "3gp") {
        "video"
    } else if matches!(ext, "mp3" | "m4a" | "wav" | "aac" | "flac") {
        "audio"
    } else {
        "documents"
    }
}

fn partial(name: &str) -> bool {
    let n = name.to_lowercase();
    [".crdownload", ".part", ".tmp", ".download", ".opdownload"].iter().any(|e| n.ends_with(e))
}

/// Full-content SHA-256: two files are duplicates only when every byte matches.
pub fn fingerprint(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub kind: &'static str,
    pub size: u64,
    pub age_days: f64,
    pub eligible: bool,
    pub reason: String,
    pub duplicate_of: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub app: String,
    pub size: u64,
    pub eligible_size: u64,
    pub files: Vec<FileInfo>,
}

struct Seen {
    path: PathBuf,
    mtime: SystemTime,
    atime: SystemTime,
    size: u64,
}

fn collect(place: &Where, out: &mut Vec<Seen>, cancel: &AtomicBool) {
    fn walk(dir: &Path, depth: u32, filter: &dyn Fn(&str) -> bool, out: &mut Vec<Seen>, cancel: &AtomicBool) {
        let Ok(read) = fs::read_dir(dir) else { return };
        for e in read.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let path = e.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                if depth > 0 {
                    walk(&path, depth - 1, filter, out, cancel);
                }
            } else if filter(&e.file_name().to_string_lossy()) {
                out.push(Seen {
                    path,
                    mtime: meta.modified().unwrap_or(UNIX_EPOCH),
                    atime: meta.accessed().unwrap_or(UNIX_EPOCH),
                    size: meta.len(),
                });
            }
        }
    }
    match place {
        Where::Folder(dir) => walk(dir, 4, &|_| true, out, cancel),
        Where::Prefixed(dir, prefixes) => walk(dir, 0, &|name| prefixes.iter().any(|p| name.to_lowercase().starts_with(p)), out, cancel),
    }
}

/// Applies the rules to the files of one app. Pure on its input, so it can be tested on fixtures.
fn judge(app: &str, mut seen: Vec<Seen>, rules: &Rules, now: SystemTime) -> Group {
    seen.sort_by_key(|s| s.mtime);
    let mut files: Vec<FileInfo> = seen
        .iter()
        .map(|s| {
            let name = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let age = now.duration_since(s.mtime).unwrap_or_default();
            let (eligible, reason) = if partial(&name) || age < Duration::from_secs(120) {
                (false, "Still downloading or just changed".to_string())
            } else if rules.keep_touched && s.atime > s.mtime + Duration::from_secs(60) {
                (false, "You opened it".to_string())
            } else if age.as_secs_f64() / 86_400.0 >= rules.retention_days {
                (true, format!("Older than {} days", rules.retention_days))
            } else {
                (false, "Newer than the retention age".to_string())
            };
            FileInfo { path: s.path.to_string_lossy().into_owned(), kind: kind_of(&name), name, size: s.size, age_days: age.as_secs_f64() / 86_400.0, eligible, reason, duplicate_of: None }
        })
        .collect();

    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate().filter(|(_, f)| f.size > 0) {
        by_size.entry(f.size).or_default().push(i);
    }
    for group in by_size.values().filter(|g| g.len() > 1) {
        let mut first_with: HashMap<String, usize> = HashMap::new();
        for &i in group {
            let Ok(print) = fingerprint(Path::new(&files[i].path)) else { continue };
            match first_with.get(&print) {
                Some(&original) => {
                    files[i].duplicate_of = Some(files[original].path.clone());
                    if !partial(&files[i].name) && now.duration_since(seen[i].mtime).unwrap_or_default() >= Duration::from_secs(120) && !files[i].eligible {
                        files[i].eligible = true;
                        files[i].reason = format!("Same content as {}", files[original].name);
                    }
                }
                None => {
                    first_with.insert(print, i);
                }
            }
        }
    }

    if rules.size_cap_mb > 0.0 {
        let cap = (rules.size_cap_mb * 1_048_576.0) as u64;
        let mut kept: u64 = files.iter().filter(|f| !f.eligible).map(|f| f.size).sum();
        for f in files.iter_mut().filter(|f| !f.eligible && f.reason != "Still downloading or just changed" && f.reason != "You opened it") {
            if kept <= cap {
                break;
            }
            f.eligible = true;
            f.reason = format!("Over the {} MB cap", rules.size_cap_mb);
            kept -= f.size;
        }
    }
    let size = files.iter().map(|f| f.size).sum();
    let eligible_size = files.iter().filter(|f| f.eligible).map(|f| f.size).sum();
    Group { app: app.to_string(), size, eligible_size, files }
}

pub fn scan(rules: &Rules, downloads: &Path, home: &Path, cancel: &AtomicBool) -> Vec<Group> {
    let mut groups = vec![];
    for def in definitions(downloads, home) {
        if !rules.apps.is_empty() && !rules.apps.iter().any(|a| def.name.to_lowercase().contains(a.as_str())) {
            continue;
        }
        let mut seen = vec![];
        for place in &def.places {
            collect(place, &mut seen, cancel);
        }
        seen.dedup_by(|a, b| a.path == b.path);
        if seen.is_empty() {
            continue;
        }
        groups.push(judge(def.name, seen, rules, SystemTime::now()));
    }
    groups
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub moved: usize,
    pub freed: u64,
    pub failed: Vec<(String, String)>,
}

fn dated_folder(downloads: &Path, app: &str, file: &FileInfo) -> PathBuf {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).saturating_sub((file.age_days * 86_400.0) as u64);
    let (y, m, ..) = fsx::civil(secs);
    downloads.join(app).join(format!("{y:04}-{m:02}"))
}

/// Copies to the dated folder, checks the copy byte for byte, and only then removes the original.
pub fn organise_one(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let name = src.file_name().ok_or("no file name")?;
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let original = fingerprint(src).map_err(|e| e.to_string())?;
    let mut dest = dest_dir.join(name);
    let mut n = 1;
    while dest.exists() {
        if fingerprint(&dest).map(|p| p == original).unwrap_or(false) {
            fs::remove_file(src).map_err(|e| fsx::why(&e))?;
            return Ok(dest);
        }
        n += 1;
        dest = dest_dir.join(format!("{} ({n}).{}", src.file_stem().unwrap_or_default().to_string_lossy(), src.extension().unwrap_or_default().to_string_lossy()));
    }
    fs::copy(src, &dest).map_err(|e| e.to_string())?;
    if fingerprint(&dest).map(|p| p != original).unwrap_or(true) {
        let _ = fs::remove_file(&dest);
        return Err("the copy did not match; the original was kept".into());
    }
    fs::remove_file(src).map_err(|e| fsx::why(&e))?;
    Ok(dest)
}

/// Runs the rules on `selected` paths (all eligible ones when `None`). Re-scans first so a stale
/// review can never delete a file that is no longer eligible.
pub fn apply(rules: &Rules, selected: Option<&[String]>, downloads: &Path, home: &Path, cancel: &AtomicBool) -> Outcome {
    let mut out = Outcome::default();
    for group in scan(rules, downloads, home, cancel) {
        for f in group.files.iter().filter(|f| f.eligible && selected.is_none_or(|s| s.contains(&f.path))) {
            if cancel.load(Ordering::Relaxed) {
                return out;
            }
            let src = Path::new(&f.path);
            let result = match rules.action {
                Action::Trash => fsx::trash(src),
                Action::Organise => organise_one(src, &dated_folder(downloads, &group.app, f)).map(|_| ()),
            };
            match result {
                Ok(()) => {
                    out.moved += 1;
                    out.freed += f.size;
                }
                Err(e) => out.failed.push((f.path.clone(), e)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notifi-msg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn aged(path: &Path, body: &[u8], days: u64) {
        fs::write(path, body).unwrap();
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(days * 86_400)).unwrap();
    }

    fn rules(days: f64) -> Rules {
        Rules { retention_days: days, keep_touched: false, size_cap_mb: 0.0, apps: vec![], action: Action::Trash }
    }

    #[test]
    fn fingerprints_compare_content_not_names_or_sizes() {
        let dir = temp("print");
        fs::write(dir.join("a.jpg"), b"same bytes").unwrap();
        fs::write(dir.join("b.jpg"), b"same bytes").unwrap();
        fs::write(dir.join("c.jpg"), b"diff bytes").unwrap();
        assert_eq!(fingerprint(&dir.join("a.jpg")).unwrap(), fingerprint(&dir.join("b.jpg")).unwrap());
        assert_ne!(fingerprint(&dir.join("a.jpg")).unwrap(), fingerprint(&dir.join("c.jpg")).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_recognised_files_old_enough_are_eligible() {
        let dir = temp("scan");
        let home = temp("scan-home");
        aged(&dir.join("WhatsApp Image 2024.jpg"), b"1", 60);
        aged(&dir.join("WhatsApp Video new.mp4"), b"22", 3);
        aged(&dir.join("holiday.jpg"), b"333", 90);
        fs::create_dir_all(dir.join("Telegram Desktop")).unwrap();
        aged(&dir.join("Telegram Desktop/old.pdf"), b"4444", 45);
        aged(&dir.join("Telegram Desktop/half.pdf.part"), b"5", 45);
        let groups = scan(&rules(30.0), &dir, &home, &AtomicBool::new(false));
        let find = |app: &str, name: &str| groups.iter().find(|g| g.app == app).unwrap().files.iter().find(|f| f.name == name).cloned();
        assert!(find("WhatsApp", "WhatsApp Image 2024.jpg").unwrap().eligible);
        assert!(!find("WhatsApp", "WhatsApp Video new.mp4").unwrap().eligible);
        assert!(find("WhatsApp", "holiday.jpg").is_none(), "the user's own files are left alone");
        assert!(find("Telegram", "old.pdf").unwrap().eligible);
        assert!(!find("Telegram", "half.pdf.part").unwrap().eligible);
        assert!(groups.iter().all(|g| g.app != "Signal"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn duplicates_are_found_by_content() {
        let dir = temp("dups");
        let home = temp("dups-home");
        aged(&dir.join("WhatsApp Image a.jpg"), b"same", 5);
        aged(&dir.join("WhatsApp Image b.jpg"), b"same", 4);
        aged(&dir.join("WhatsApp Image c.jpg"), b"sAme", 4);
        let g = &scan(&rules(30.0), &dir, &home, &AtomicBool::new(false))[0];
        let b = g.files.iter().find(|f| f.name.ends_with("b.jpg")).unwrap();
        assert!(b.duplicate_of.as_deref().is_some_and(|p| p.ends_with("a.jpg")));
        assert!(b.eligible);
        assert!(g.files.iter().find(|f| f.name.ends_with("a.jpg")).unwrap().duplicate_of.is_none());
        assert!(g.files.iter().find(|f| f.name.ends_with("c.jpg")).unwrap().duplicate_of.is_none());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn size_cap_removes_oldest_first() {
        let dir = temp("cap");
        let home = temp("cap-home");
        for (n, days) in [(1u8, 20), (2, 10), (3, 5)] {
            aged(&dir.join(format!("WhatsApp Video {n}.mp4")), &vec![n; 1_048_576], days);
        }
        let mut r = rules(30.0);
        r.size_cap_mb = 2.0;
        let g = &scan(&r, &dir, &home, &AtomicBool::new(false))[0];
        let eligible: Vec<&str> = g.files.iter().filter(|f| f.eligible).map(|f| f.name.as_str()).collect();
        assert_eq!(eligible, ["WhatsApp Video 1.mp4"]);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn organising_verifies_the_copy_and_never_loses_a_file() {
        let dir = temp("org");
        aged(&dir.join("x.jpg"), b"data", 1);
        let dest = dir.join("WhatsApp/2024-01");
        let moved = organise_one(&dir.join("x.jpg"), &dest).unwrap();
        assert!(moved.exists() && !dir.join("x.jpg").exists());
        aged(&dir.join("x.jpg"), b"data", 1);
        let again = organise_one(&dir.join("x.jpg"), &dest).unwrap();
        assert_eq!(again, moved, "an identical copy already there is reused, then the original goes");
        aged(&dir.join("x.jpg"), b"other", 1);
        let renamed = organise_one(&dir.join("x.jpg"), &dest).unwrap();
        assert!(renamed.to_string_lossy().contains("x (2).jpg"));
        assert_eq!(fs::read(&moved).unwrap(), b"data");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_trashes_only_selected_files() {
        let dir = temp("apply");
        let home = temp("apply-home");
        aged(&dir.join("WhatsApp Image a.jpg"), b"1", 60);
        aged(&dir.join("WhatsApp Image b.jpg"), b"2", 60);
        let mut r = rules(30.0);
        r.action = Action::Organise;
        let pick = vec![dir.join("WhatsApp Image a.jpg").to_string_lossy().into_owned()];
        let out = apply(&r, Some(&pick), &dir, &home, &AtomicBool::new(false));
        assert_eq!(out.moved, 1);
        assert!(dir.join("WhatsApp Image b.jpg").exists());
        assert!(!dir.join("WhatsApp Image a.jpg").exists());
        assert!(fs::read_dir(dir.join("WhatsApp")).unwrap().next().is_some());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("WhatsApp Ptt 2024.opus"), "voice");
        assert_eq!(kind_of("a.MP4"), "video");
        assert_eq!(kind_of("x.gif"), "stickers");
        assert_eq!(kind_of("a.pdf"), "documents");
    }
}
