//! Space map (spec/apps/space-map.md): scans a folder into memory and serves a pruned, filtered
//! tree for the treemap. The scan runs on its own thread and the tree can be read while it runs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::AppsState;

const TAGS: [&str; 4] = ["red", "yellow", "green", "blue"];

struct Node {
    name: String,
    parent: u32,
    dir: bool,
    size: u64,
    files: u64,
    mtime: i64,
    kids: Vec<u32>,
    small_size: u64,
    small_count: u64,
}

impl Node {
    fn new(name: String, parent: u32, dir: bool, size: u64, mtime: i64) -> Self {
        Node { name, parent, dir, size, files: u64::from(!dir), mtime, kids: Vec::new(), small_size: 0, small_count: 0 }
    }
}

struct Tree {
    nodes: Vec<Node>,
    root: PathBuf,
    scanning: bool,
    dirs: u64,
    files: u64,
    unreadable: u64,
    current: String,
}

impl Tree {
    fn new(root: PathBuf) -> Self {
        let name = root.display().to_string();
        Tree { nodes: vec![Node::new(name, 0, true, 0, 0)], root, scanning: false, dirs: 1, files: 0, unreadable: 0, current: String::new() }
    }

    /// Takes an item out of the tree after it was deleted, and its size out of every folder above it.
    fn remove(&mut self, idx: u32) {
        let parent = self.nodes[idx as usize].parent;
        let (size, files, mut dirs, mut stack) = (self.nodes[idx as usize].size, self.nodes[idx as usize].files, 0u64, vec![idx]);
        while let Some(i) = stack.pop() {
            if self.nodes[i as usize].dir {
                dirs += 1;
                stack.extend(self.nodes[i as usize].kids.iter().copied());
            }
        }
        self.nodes[parent as usize].kids.retain(|&k| k != idx);
        let mut up = parent;
        loop {
            let n = &mut self.nodes[up as usize];
            n.size = n.size.saturating_sub(size);
            n.files = n.files.saturating_sub(files);
            if up == 0 {
                break;
            }
            up = n.parent;
        }
        self.files = self.files.saturating_sub(files);
        self.dirs = self.dirs.saturating_sub(dirs);
    }

    /// Adds `size` bytes and `files` files to a directory and every parent above it.
    fn bump(&mut self, mut idx: u32, size: u64, files: u64) {
        loop {
            let n = &mut self.nodes[idx as usize];
            n.size += size;
            n.files += files;
            if idx == 0 {
                break;
            }
            idx = n.parent;
        }
    }

    fn find(&self, path: &Path) -> Option<u32> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut idx = 0u32;
        for part in rel.components() {
            let want = part.as_os_str().to_string_lossy();
            idx = *self.nodes[idx as usize].kids.iter().find(|&&k| same_name(&self.nodes[k as usize].name, &want))?;
        }
        Some(idx)
    }
}

fn same_name(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

#[derive(Default)]
pub struct SpaceState {
    tree: Mutex<Option<Arc<Mutex<Tree>>>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    tags: Mutex<HashMap<String, String>>,
}

impl SpaceState {
    fn tree(&self) -> Result<Arc<Mutex<Tree>>, String> {
        self.tree.lock().unwrap().clone().ok_or_else(|| "Nothing has been scanned yet.".to_string())
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn tag_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

#[cfg(windows)]
fn is_reparse(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn is_hidden(_: &str, md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes() & 0x6 != 0
}
#[cfg(not(windows))]
fn is_hidden(name: &str, _: &fs::Metadata) -> bool {
    name.starts_with('.')
}

/// What one folder holds, read without touching the shared tree.
struct Listing {
    files: Vec<(String, u64, i64)>,
    dirs: Vec<String>,
    small_size: u64,
    small_count: u64,
}

fn list_dir(dir: &Path, min: u64, hidden: bool) -> Option<Listing> {
    let read = fs::read_dir(dir).ok()?;
    let mut out = Listing { files: Vec::new(), dirs: Vec::new(), small_size: 0, small_count: 0 };
    for entry in read.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let Ok(md) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !hidden && is_hidden(&name, &md) {
            continue;
        }
        if kind.is_dir() {
            if !is_reparse(&md) {
                out.dirs.push(name);
            }
            continue;
        }
        let size = md.len();
        if size < min {
            out.small_size += size;
            out.small_count += 1;
        } else {
            let mtime = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
            out.files.push((name, size, mtime));
        }
    }
    Some(out)
}

/// Folders waiting to be read, and how many are queued or being read, shared by the scan's workers.
struct Work {
    queue: Mutex<(Vec<(u32, PathBuf)>, usize)>,
    wake: Condvar,
}

/// Reading a folder waits on the disk, not the processor, so several folders are read at once.
fn scan_threads() -> usize {
    std::env::var("NOTIFI_SCAN_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get() * 2).clamp(4, 16))
}

fn scan(shared: Arc<Mutex<Tree>>, cancel: Arc<AtomicBool>, root: PathBuf, min: u64, hidden: bool, threads: usize) {
    let work = Arc::new(Work { queue: Mutex::new((vec![(0u32, root)], 1)), wake: Condvar::new() });
    let workers: Vec<_> = (0..threads.max(1))
        .map(|_| {
            let (shared, cancel, work) = (shared.clone(), cancel.clone(), work.clone());
            std::thread::spawn(move || worker(&shared, &cancel, &work, min, hidden))
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
    let mut t = shared.lock().unwrap();
    t.scanning = false;
    t.current.clear();
}

fn worker(shared: &Mutex<Tree>, cancel: &AtomicBool, work: &Work, min: u64, hidden: bool) {
    loop {
        let (idx, dir) = {
            let mut q = work.queue.lock().unwrap();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    q.0.clear();
                    q.1 = 0;
                    work.wake.notify_all();
                    return;
                }
                if let Some(item) = q.0.pop() {
                    break item;
                }
                if q.1 == 0 {
                    work.wake.notify_all();
                    return;
                }
                q = work.wake.wait(q).unwrap();
            }
        };
        let listing = list_dir(&dir, min, hidden);
        let mut found = Vec::new();
        {
            let mut t = shared.lock().unwrap();
            t.current = dir.display().to_string();
            match listing {
                None => t.unreadable += 1,
                Some(l) => {
                    let mut total = l.small_size;
                    let mut count = l.small_count;
                    {
                        let n = &mut t.nodes[idx as usize];
                        n.small_size += l.small_size;
                        n.small_count += l.small_count;
                    }
                    for (name, size, mtime) in l.files {
                        total += size;
                        count += 1;
                        let id = t.nodes.len() as u32;
                        t.nodes.push(Node::new(name, idx, false, size, mtime));
                        t.nodes[idx as usize].kids.push(id);
                    }
                    for name in l.dirs {
                        let id = t.nodes.len() as u32;
                        let path = dir.join(&name);
                        t.nodes.push(Node::new(name, idx, true, 0, 0));
                        t.nodes[idx as usize].kids.push(id);
                        found.push((id, path));
                        t.dirs += 1;
                    }
                    t.files += count;
                    t.bump(idx, total, count);
                }
            }
        }
        let mut q = work.queue.lock().unwrap();
        q.1 = (q.1 + found.len()).saturating_sub(1);
        if !cancel.load(Ordering::Relaxed) {
            q.0.extend(found);
        }
        work.wake.notify_all();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    name: String,
    mount: String,
    total: u64,
    free: u64,
}

#[tauri::command]
pub fn space_drives() -> Vec<Drive> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<Drive> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| Drive {
            name: d.name().to_string_lossy().into_owned(),
            mount: d.mount_point().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

#[tauri::command]
pub fn space_start(app: AppHandle, state: State<AppsState>, path: String) -> Result<(), String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err("That folder does not exist.".into());
    }
    let min = super::option(&app, "spaceMap", "smallKb").and_then(|v| v.as_f64()).unwrap_or(16.0).max(0.0) as u64 * 1024;
    let hidden = super::flag(&app, "spaceMap", "showHidden", true);
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = state.space.cancel.lock().unwrap().replace(cancel.clone()) {
        old.store(true, Ordering::Relaxed);
    }
    let mut tree = Tree::new(root.clone());
    tree.scanning = true;
    let shared = Arc::new(Mutex::new(tree));
    *state.space.tree.lock().unwrap() = Some(shared.clone());
    std::thread::Builder::new()
        .name("space-scan".into())
        .spawn(move || scan(shared, cancel, root, min, hidden, scan_threads()))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn space_cancel(state: State<AppsState>) {
    if let Some(c) = state.space.cancel.lock().unwrap().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    scanning: bool,
    root: String,
    files: u64,
    dirs: u64,
    bytes: u64,
    unreadable: u64,
    current: String,
}

#[tauri::command]
pub fn space_status(state: State<AppsState>) -> Option<Status> {
    let tree = state.space.tree().ok()?;
    let t = tree.lock().unwrap();
    Some(Status {
        scanning: t.scanning,
        root: t.root.display().to_string(),
        files: t.files,
        dirs: t.dirs,
        bytes: t.nodes[0].size,
        unreadable: t.unreadable,
        current: t.current.clone(),
    })
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewNode {
    name: String,
    path: String,
    dir: bool,
    other: bool,
    size: u64,
    files: u64,
    kind: &'static str,
    tag: Option<String>,
    mtime: i64,
    children: Vec<ViewNode>,
}

fn kind_of(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "zip" | "rar" | "7z" | "gz" | "tar" | "xz" | "bz2" | "zst" | "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "cab" => "archive",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "flv" | "m4v" | "mpg" | "mpeg" => "video",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "raw" | "psd" | "svg" | "ico" => "image",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => "audio",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" | "odt" | "rtf" => "document",
        "rs" | "js" | "ts" | "py" | "java" | "c" | "h" | "cpp" | "cs" | "go" | "json" | "html" | "css" | "xml" | "yml" | "yaml" | "toml" | "svelte" | "lua" => "code",
        "exe" | "dll" | "msi" | "sys" | "bin" | "so" | "dmg" | "apk" | "jar" | "pak" => "app",
        _ => "other",
    }
}

enum Term {
    Name(Vec<char>),
    Size { larger: bool, bytes: u64 },
    Age { older: bool, secs: i64 },
    Tag(String),
    Not(Box<Term>),
}

#[derive(Default)]
pub struct Filter(Vec<Term>);

fn glob(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si, mut star, mut mark) = (0, 0, None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

fn parse_amount(s: &str) -> Option<(f64, String)> {
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let n: f64 = s[..split].parse().ok()?;
    Some((n, s[split..].trim().to_ascii_lowercase()))
}

impl Filter {
    pub fn parse(text: &str) -> Filter {
        let mut terms = Vec::new();
        for raw in text.split(';') {
            let mut e = raw.trim();
            if e.is_empty() {
                continue;
            }
            let not = e.starts_with('!');
            if not {
                e = e[1..].trim();
            }
            let term = if let Some(tag) = e.strip_prefix(':') {
                Term::Tag(tag.trim().to_ascii_lowercase())
            } else if let Some(rest) = e.strip_prefix('>').or_else(|| e.strip_prefix('<')) {
                let larger = e.starts_with('>');
                match parse_amount(rest.trim()) {
                    Some((n, unit)) => match unit.as_str() {
                        "b" => Term::Size { larger, bytes: n as u64 },
                        "kb" => Term::Size { larger, bytes: (n * 1024.0) as u64 },
                        "mb" => Term::Size { larger, bytes: (n * 1048576.0) as u64 },
                        "gb" => Term::Size { larger, bytes: (n * 1073741824.0) as u64 },
                        "tb" => Term::Size { larger, bytes: (n * 1099511627776.0) as u64 },
                        "d" => Term::Age { older: larger, secs: (n * 86400.0) as i64 },
                        "w" => Term::Age { older: larger, secs: (n * 604800.0) as i64 },
                        "m" => Term::Age { older: larger, secs: (n * 2592000.0) as i64 },
                        "y" => Term::Age { older: larger, secs: (n * 31536000.0) as i64 },
                        _ => continue,
                    },
                    None => continue,
                }
            } else {
                Term::Name(e.to_lowercase().chars().collect())
            };
            terms.push(if not { Term::Not(Box::new(term)) } else { term });
        }
        Filter(terms)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn term_matches(t: &Term, name: &[char], size: u64, mtime: i64, tag: Option<&str>, now: i64) -> bool {
        match t {
            Term::Name(p) => glob(p, name),
            Term::Size { larger, bytes } => if *larger { size > *bytes } else { size < *bytes },
            Term::Age { older, secs } => if *older { now - mtime > *secs } else { now - mtime < *secs },
            Term::Tag(c) => tag == Some(c.as_str()),
            Term::Not(inner) => !Self::term_matches(inner, name, size, mtime, tag, now),
        }
    }

    fn matches(&self, name: &str, size: u64, mtime: i64, tag: Option<&str>, now: i64) -> bool {
        let lower: Vec<char> = name.to_lowercase().chars().collect();
        self.0.iter().all(|t| Self::term_matches(t, &lower, size, mtime, tag, now))
    }
}

struct Ctx<'a> {
    tree: &'a Tree,
    filter: &'a Filter,
    tags: &'a HashMap<String, String>,
    limit: usize,
    now: i64,
}

fn build(c: &Ctx, idx: u32, path: &Path, inherited: Option<&str>, depth: u32) -> Option<ViewNode> {
    let n = &c.tree.nodes[idx as usize];
    let own = if c.tags.is_empty() { None } else { c.tags.get(&tag_key(path)).map(String::as_str) };
    let tag = own.or(inherited);
    let mut out = ViewNode {
        name: n.name.clone(),
        path: path.display().to_string(),
        dir: n.dir,
        kind: if n.dir { "folder" } else { kind_of(&n.name) },
        tag: own.map(str::to_string),
        mtime: n.mtime,
        ..Default::default()
    };
    if !n.dir {
        if !c.filter.is_empty() && !c.filter.matches(&n.name, n.size, n.mtime, tag, c.now) {
            return None;
        }
        out.size = n.size;
        out.files = 1;
        return Some(out);
    }
    let filtered = !c.filter.is_empty();
    if !filtered && depth == 0 {
        out.size = n.size;
        out.files = n.files;
        return Some(out);
    }
    let mut kids = Vec::new();
    let (mut size, mut files) = (0u64, 0u64);
    for &k in &n.kids {
        let child = &c.tree.nodes[k as usize];
        let p = path.join(&child.name);
        if let Some(v) = build(c, k, &p, tag, depth.saturating_sub(1)) {
            size += v.size;
            files += v.files;
            kids.push(v);
        }
    }
    if !filtered {
        size += n.small_size;
        files += n.small_count;
    }
    if filtered && kids.is_empty() {
        return None;
    }
    out.size = size;
    out.files = files;
    if depth == 0 {
        return Some(out);
    }
    kids.sort_by(|a, b| b.size.cmp(&a.size));
    let (mut rest_size, mut rest_count) = if filtered { (0, 0) } else { (n.small_size, n.small_count) };
    if kids.len() > c.limit {
        for v in kids.split_off(c.limit) {
            rest_size += v.size;
            rest_count += v.files.max(1);
        }
    }
    if rest_count > 0 {
        kids.push(ViewNode {
            name: format!("{rest_count} small items"),
            path: out.path.clone(),
            other: true,
            size: rest_size,
            files: rest_count,
            kind: "other",
            ..Default::default()
        });
    }
    out.children = kids;
    Some(out)
}

#[tauri::command]
pub fn space_view(state: State<AppsState>, path: String, filter: String, depth: u32, limit: usize) -> Result<Option<ViewNode>, String> {
    let shared = state.space.tree()?;
    let tags = state.space.tags.lock().unwrap().clone();
    let t = shared.lock().unwrap();
    let p = PathBuf::from(&path);
    let idx = t.find(&p).ok_or_else(|| "That folder is not in the scan.".to_string())?;
    let f = Filter::parse(&filter);
    let ctx = Ctx { tree: &t, filter: &f, tags: &tags, limit: limit.clamp(5, 400), now: now() };
    Ok(build(&ctx, idx, &p, None, depth.clamp(1, 6)))
}

#[tauri::command]
pub fn space_tag(state: State<AppsState>, path: String, color: Option<String>) -> Result<(), String> {
    let mut tags = state.space.tags.lock().unwrap();
    let key = tag_key(Path::new(&path));
    match color {
        Some(c) if TAGS.contains(&c.as_str()) => {
            tags.insert(key, c);
        }
        Some(_) => return Err("Unknown tag colour.".into()),
        None => {
            tags.remove(&key);
        }
    }
    Ok(())
}

/// Whether a path is somewhere Notifi will not delete from: the Windows folder and what is inside it, and the
/// system folders at the top of a drive. Those go through the uninstaller or Windows itself.
fn protected(path: &Path) -> bool {
    // Elsewhere the root and every folder directly under it (/usr, /home, /etc) are kept.
    if cfg!(not(windows)) {
        return path.parent().is_none_or(|p| p.parent().is_none());
    }
    let key = tag_key(path);
    let key = key.trim_end_matches('\\');
    let windows = std::env::var("SystemRoot").map(|r| tag_key(Path::new(&r)).trim_end_matches('\\').to_string()).ok();
    if windows.is_some_and(|w| key == w || key.starts_with(&format!("{w}\\"))) {
        return true;
    }
    let top = path.parent().is_some_and(|p| p.parent().is_none());
    top && matches!(key.rsplit('\\').next(), Some("program files" | "program files (x86)" | "programdata" | "users" | "system volume information" | "$recycle.bin"))
}

/// Deletes a file or folder that is in the scan: to the Recycle Bin, or for good. Returns the bytes freed.
#[tauri::command]
pub fn space_delete(state: State<AppsState>, path: String, permanent: bool) -> Result<u64, String> {
    let shared = state.space.tree()?;
    let p = PathBuf::from(&path);
    let (idx, size) = {
        let t = shared.lock().unwrap();
        if t.scanning {
            return Err("Wait for the scan to finish, or cancel it, before deleting.".into());
        }
        let idx = t.find(&p).filter(|&i| i != 0).ok_or_else(|| "That item is not in the scan.".to_string())?;
        (idx, t.nodes[idx as usize].size)
    };
    if protected(&p) {
        return Err("Notifi does not delete a drive's top-level folders or anything in the Windows folder.".into());
    }
    let md = fs::symlink_metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    if permanent {
        if md.is_dir() && !md.file_type().is_symlink() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) }.map_err(|e| format!("{}: {e}", p.display()))?;
    } else {
        crate::apps_clean::fsx::trash(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    shared.lock().unwrap().remove(idx);
    Ok(size)
}

#[tauri::command]
pub fn space_open(state: State<AppsState>, path: String) -> Result<(), String> {
    // Only something inside the scanned folder, whatever `..` the address holds.
    let root = state.space.tree()?.lock().unwrap().root.clone();
    let target = fs::canonicalize(&path).map_err(|e| format!("{path}: {e}"))?;
    if !target.starts_with(fs::canonicalize(&root).map_err(|e| e.to_string())?) {
        return Err("That is outside the scanned folder.".into());
    }
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    super::exec::command(program).arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn size_text(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", U[i]) }
}

fn report_lines(node: &ViewNode, level: u32, max: u32, out: &mut String) {
    let pad = "  ".repeat(level as usize);
    let what = if node.dir { "/" } else { "" };
    out.push_str(&format!("{pad}{} {}{what}\n", size_text(node.size), node.name));
    if level < max {
        for c in &node.children {
            report_lines(c, level + 1, max, out);
        }
    }
}

/// Writes a text report of the current view into the Documents folder and returns its path.
#[tauri::command]
pub fn space_export(app: AppHandle, state: State<AppsState>, path: String, filter: String, depth: u32) -> Result<String, String> {
    let max = depth.clamp(1, 6);
    let view = space_view(state.clone(), path.clone(), filter.clone(), max + 1, 200)?.ok_or("Nothing matches the filter.")?;
    let mut text = format!("Space map report\nFolder: {path}\nFilter: {}\nTotal: {} in {} files\n\n", if filter.trim().is_empty() { "none" } else { filter.trim() }, size_text(view.size), view.files);
    report_lines(&view, 0, max, &mut text);
    let dir = app.path().document_dir().map_err(|e| e.to_string())?;
    let stamp = now();
    let file = dir.join(format!("Space map report {stamp}.txt"));
    fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_dir(root: &Path, threads: usize) -> Arc<Mutex<Tree>> {
        let shared = Arc::new(Mutex::new(Tree::new(root.to_path_buf())));
        shared.lock().unwrap().scanning = true;
        scan(shared.clone(), Arc::new(AtomicBool::new(false)), root.to_path_buf(), 0, true, threads);
        shared
    }

    #[test]
    fn scanning_on_many_threads_counts_the_same_as_one() {
        let root = std::env::temp_dir().join(format!("notifi-space-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in 0..6 {
            for e in 0..4 {
                let dir = root.join(format!("d{d}")).join(format!("e{e}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("a.bin"), vec![0u8; 100 + d * 10 + e]).unwrap();
                fs::write(root.join(format!("d{d}")).join("b.bin"), vec![0u8; 7]).unwrap();
            }
        }
        let want: u64 = (0..6u64).map(|d| (0..4u64).map(|e| 100 + d * 10 + e).sum::<u64>() + 7).sum();
        for threads in [1, 8] {
            let t = scan_dir(&root, threads);
            let t = t.lock().unwrap();
            assert!(!t.scanning);
            assert_eq!((t.nodes[0].size, t.nodes[0].files, t.dirs), (want, 30, 1 + 6 + 24), "{threads} threads");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// `NOTIFI_SCAN_DIR=C:\Windows cargo test scan_speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_speed() {
        let root = PathBuf::from(std::env::var("NOTIFI_SCAN_DIR").expect("set NOTIFI_SCAN_DIR"));
        for threads in [1, 4, 8, 16] {
            let start = std::time::Instant::now();
            let t = scan_dir(&root, threads);
            let t = t.lock().unwrap();
            println!("{threads:>2} threads: {:?}, {} files, {} folders, {} bytes", start.elapsed(), t.files, t.dirs, t.nodes[0].size);
        }
    }

    #[test]
    fn deleting_takes_the_size_out_of_every_folder_above() {
        let root = std::env::temp_dir().join(format!("notifi-space-rm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/x.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("a/y.bin"), vec![0u8; 20]).unwrap();
        let shared = scan_dir(&root, 2);
        let mut t = shared.lock().unwrap();
        let b = t.find(&root.join("a/b")).unwrap();
        t.remove(b);
        let a = t.find(&root.join("a")).unwrap();
        assert_eq!((t.nodes[0].size, t.nodes[0].files, t.nodes[a as usize].size, t.files, t.dirs), (20, 1, 20, 1, 2));
        assert!(t.find(&root.join("a/b")).is_none());
        drop(t);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn system_folders_are_protected() {
        let sys = std::env::var("SystemRoot").unwrap();
        assert!(protected(Path::new(&sys)));
        assert!(protected(&Path::new(&sys).join("System32").join("x.dll")));
        assert!(protected(Path::new("C:/Program Files")));
        assert!(!protected(Path::new("C:/Program Files/Some App")));
        assert!(!protected(&std::env::temp_dir().join("anything")));
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn glob_matches_stars_and_marks() {
        assert!(glob(&chars("*.zip"), &chars("a.zip")));
        assert!(!glob(&chars("*.zip"), &chars("a.zip.txt")));
        assert!(glob(&chars("a?c*"), &chars("abcdef")));
        assert!(glob(&chars("*"), &chars("")));
    }

    #[test]
    fn filter_parses_sizes_ages_tags_and_not() {
        let f = Filter::parse("*.zip; >500mb ;:Yellow;!old*;<2w;>bogus");
        assert_eq!(f.0.len(), 5);
        assert!(f.matches("a.zip", 600 << 20, now() - 100, Some("yellow"), now()));
        assert!(!f.matches("old.zip", 600 << 20, now() - 100, Some("yellow"), now()));
        assert!(!f.matches("a.zip", 1 << 20, now() - 100, Some("yellow"), now()));
    }

    fn sample() -> (PathBuf, Arc<Mutex<Tree>>) {
        // Tests run in parallel in one process, so each call gets its own folder.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!("notifi-space-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/deep")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("a/big.zip"), vec![0u8; 40_000]).unwrap();
        fs::write(root.join("a/deep/mid.bin"), vec![0u8; 20_000]).unwrap();
        fs::write(root.join("a/tiny.txt"), b"hi").unwrap();
        fs::write(root.join("b/other.mp4"), vec![0u8; 30_000]).unwrap();
        let shared = Arc::new(Mutex::new(Tree::new(root.clone())));
        scan(shared.clone(), Arc::new(AtomicBool::new(false)), root.clone(), 16 * 1024, true, 1);
        (root, shared)
    }

    #[test]
    fn scan_totals_and_small_files() {
        let (root, shared) = sample();
        let t = shared.lock().unwrap();
        assert_eq!(t.nodes[0].size, 90_002);
        assert_eq!(t.nodes[0].files, 4);
        let a = t.find(&root.join("a")).unwrap();
        assert_eq!(t.nodes[a as usize].small_count, 1);
        let ctx = Ctx { tree: &t, filter: &Filter::default(), tags: &HashMap::new(), limit: 50, now: now() };
        let v = build(&ctx, 0, &root, None, 3).unwrap();
        assert_eq!(v.size, 90_002);
        assert_eq!(v.children[0].name, "a");
        assert!(v.children[0].children.iter().any(|c| c.other && c.size == 2));
        drop(t);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn filter_prunes_empty_folders() {
        let (root, shared) = sample();
        let t = shared.lock().unwrap();
        let f = Filter::parse("*.mp4");
        let ctx = Ctx { tree: &t, filter: &f, tags: &HashMap::new(), limit: 50, now: now() };
        let v = build(&ctx, 0, &root, None, 3).unwrap();
        assert_eq!(v.size, 30_000);
        assert_eq!(v.children.len(), 1);
        assert_eq!(v.children[0].name, "b");
        drop(t);
        let _ = fs::remove_dir_all(root);
    }
}
