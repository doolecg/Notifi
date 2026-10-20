//! Recent captures (spec/capture/recent-captures.md): a private list of the latest screenshots and
//! recordings Notifi saved, with open, copy, reveal, edit, pin, trim and delete. The list is a small
//! file in the app data folder; the images and videos themselves are never touched by it, and a
//! delete only ever reaches files inside the capture folders.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tauri::{ipc::Response, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{flag, number, png_fast, sys, text, Last, Opts};
use crate::settings::Store;

pub const RECENT: &str = "capturerecent";
const CHANGED_EVENT: &str = "capture://recent";
const NS: &str = "recentCaptures";
const FILE: &str = "recent-captures.json";
const IMAGE_TYPES: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

static FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    /// `shot` or `video`.
    pub kind: String,
    /// Seconds since 1970 when it was saved.
    pub time: u64,
}

// ---- Pure rules ----

/// Puts `entry` first, drops an older entry for the same file and keeps at most `keep` entries.
pub fn push_entry(list: &mut Vec<Entry>, entry: Entry, keep: usize) {
    list.retain(|e| e.path != entry.path);
    list.insert(0, entry);
    list.truncate(keep.max(1));
}

/// True when `path` is a file somewhere below `root` (not `root` itself) and never climbs with `..`.
pub fn within(root: &Path, path: &Path) -> bool {
    path != root && path.starts_with(root) && !path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Whether Notifi may delete `path`: it must be in the list and below one of the capture folders.
/// Paths are compared as given, so callers pass canonical ones.
pub fn deletable(list: &[Entry], roots: &[PathBuf], path: &Path) -> bool {
    list.iter().any(|e| Path::new(&e.path) == path) && roots.iter().any(|r| within(r, path))
}

// ---- Storage ----

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

/// The saved list. A file that cannot be read is set aside as `.bad` instead of being overwritten.
fn load(app: &AppHandle) -> Vec<Entry> {
    let Some(path) = file(app) else { return Vec::new() };
    let Ok(bytes) = fs::read(&path) else { return Vec::new() };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        let _ = fs::rename(&path, path.with_extension("json.bad"));
        Vec::new()
    })
}

fn store(app: &AppHandle, list: &[Entry]) {
    let Some(path) = file(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(list) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

fn keep_count(app: &AppHandle) -> usize {
    let s = app.state::<Store>().get();
    number(&s, NS, "keep", 50.0).clamp(5.0, 500.0) as usize
}

/// Records a saved screenshot (`shot`) or recording (`video`) when the feature is switched on.
pub fn remember(app: &AppHandle, path: &Path, kind: &str) {
    if !flag(&app.state::<Store>().get(), NS, "enabled", false) {
        return;
    }
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    {
        let _guard = FILE_LOCK.lock().unwrap();
        let mut list = load(app);
        push_entry(&mut list, Entry { path: path.display().to_string(), kind: kind.into(), time }, keep_count(app));
        store(app, &list);
    }
    let _ = app.emit_to(RECENT, CHANGED_EVENT, ());
}

/// The folders captures are saved to: the screenshot folder and the recording folder.
fn roots(app: &AppHandle) -> Vec<PathBuf> {
    let s = app.state::<Store>().get();
    let shots = match Opts::read(app).folder.trim() {
        "" => app.path().picture_dir().ok(),
        folder => Some(PathBuf::from(folder)),
    };
    let videos = crate::recording::save_dir(&text(&s, "screenRecording", "folder", ""));
    shots.into_iter().chain([videos]).filter_map(|d| fs::canonicalize(d).ok()).collect()
}

// ---- Commands ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    path: String,
    name: String,
    kind: String,
    time: u64,
}

/// The list for the Recent captures window and for the Island, newest first, leaving out files that
/// no longer exist.
pub fn list(app: &AppHandle) -> Vec<Item> {
    let _guard = FILE_LOCK.lock().unwrap();
    load(app)
        .into_iter()
        .filter(|e| Path::new(&e.path).is_file())
        .map(|e| Item {
            name: Path::new(&e.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path: e.path,
            kind: e.kind,
            time: e.time,
        })
        .collect()
}

#[tauri::command]
pub fn capture_recent_list(app: AppHandle) -> Vec<Item> {
    list(&app)
}

fn listed(app: &AppHandle, path: &str) -> Result<Entry, String> {
    let _guard = FILE_LOCK.lock().unwrap();
    load(app).into_iter().find(|e| e.path == path).ok_or_else(|| "That capture is not in the list.".to_string())
}

fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|e| IMAGE_TYPES.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn read_image(path: &Path) -> Result<RgbaImage, String> {
    Ok(image::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?.to_rgba8())
}

/// A small PNG of a listed image for the grid.
#[tauri::command]
pub async fn capture_recent_thumb(app: AppHandle, path: String) -> Result<Response, String> {
    listed(&app, &path)?;
    let img = read_image(Path::new(&path))?;
    let (w, h) = img.dimensions();
    let fit = (240.0 / f64::from(w)).min(160.0 / f64::from(h)).min(1.0);
    let small = image::imageops::resize(
        &img,
        ((f64::from(w) * fit).round() as u32).max(1),
        ((f64::from(h) * fit).round() as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    Ok(Response::new(png_fast(&small)?))
}

/// One button of the list: open, reveal, copy, edit, pin, trim or delete.
#[tauri::command]
pub fn capture_recent_action(app: AppHandle, path: String, action: String) -> Result<String, String> {
    let entry = listed(&app, &path)?;
    let file = PathBuf::from(&entry.path);
    if !file.is_file() {
        return Err("That file is gone.".into());
    }
    match action.as_str() {
        "open" => {
            sys::open_url(&entry.path);
            Ok(String::new())
        }
        "reveal" => {
            crate::apps_clean::apps_clean_reveal(entry.path);
            Ok(String::new())
        }
        "copy" if is_image(&file) => {
            crate::clipboard::set_png(&png_fast(&read_image(&file)?)?)?;
            Ok("Copied".into())
        }
        "copy" => {
            crate::clipboard::set_text(&entry.path)?;
            Ok("Path copied".into())
        }
        "edit" | "pin" if !is_image(&file) => Err("Only pictures can be edited or pinned.".into()),
        "edit" => {
            let img = read_image(&file)?;
            let last = std::sync::Arc::new(Last { img, scale: 1.0, saved: Mutex::new(Some(file)) });
            *app.state::<super::CaptureState>().last.lock().unwrap() = Some(last);
            super::open_editor(&app);
            Ok(String::new())
        }
        "pin" => {
            let img = read_image(&file)?;
            super::pin::pin_image(&app, &img, None);
            Ok(String::new())
        }
        "trim" if !is_image(&file) => {
            crate::recording::open_trimmer(&app, &file);
            Ok(String::new())
        }
        "trim" => Err("Only recordings can be trimmed.".into()),
        "delete" => delete(&app, &file),
        _ => Err("Unknown action.".into()),
    }
}

fn delete(app: &AppHandle, file: &Path) -> Result<String, String> {
    let real = fs::canonicalize(file).map_err(|e| e.to_string())?;
    let roots = roots(app);
    {
        let _guard = FILE_LOCK.lock().unwrap();
        // The list holds the path as saved; compare it with the same file resolved the same way.
        let canonical: Vec<Entry> = load(app)
            .into_iter()
            .filter_map(|e| Some(Entry { path: fs::canonicalize(&e.path).ok()?.display().to_string(), ..e }))
            .collect();
        if !deletable(&canonical, &roots, &real) {
            return Err("Notifi only deletes files inside your screenshot and recording folders.".into());
        }
    }
    sys::trash(&real)?;
    let _guard = FILE_LOCK.lock().unwrap();
    let mut list = load(app);
    list.retain(|e| fs::canonicalize(&e.path).map_or(e.path != file.display().to_string(), |p| p != real));
    store(app, &list);
    drop(_guard);
    let _ = app.emit_to(RECENT, CHANGED_EVENT, ());
    Ok("Moved to the Recycle Bin".into())
}

/// Empties the list. The files stay where they are.
#[tauri::command]
pub fn capture_recent_clear(app: AppHandle) {
    {
        let _guard = FILE_LOCK.lock().unwrap();
        store(&app, &[]);
    }
    let _ = app.emit_to(RECENT, CHANGED_EVENT, ());
}

#[tauri::command]
pub fn capture_recent_close(app: AppHandle) {
    if let Some(window) = app.get_webview_window(RECENT) {
        let _ = window.close();
    }
}

/// Opens the Recent captures window, or brings it forward.
pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(RECENT) {
        let _ = app.emit_to(RECENT, CHANGED_EVENT, ());
        let _ = window.unminimize();
        let _ = window.show();
        if !crate::window::no_focus() {
            let _ = window.set_focus();
        }
        return;
    }
    // Building a webview from a command can deadlock on Windows, so build off-thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, RECENT, WebviewUrl::App("capture.html#recent".into()))
            .title("Notifi Recent Captures")
            .inner_size(560.0, 600.0)
            .min_inner_size(360.0, 320.0)
            .center()
            .focused(!crate::window::no_focus())
            .build();
    });
}

#[tauri::command]
pub fn capture_recent_open(app: AppHandle) {
    open(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, time: u64) -> Entry {
        Entry { path: path.into(), kind: "shot".into(), time }
    }

    #[test]
    fn newest_first_without_duplicates_and_capped() {
        let mut list = vec![entry("a", 1), entry("b", 2), entry("c", 3)];
        push_entry(&mut list, entry("b", 4), 3);
        assert_eq!(list.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), ["b", "a", "c"]);
        push_entry(&mut list, entry("d", 5), 3);
        assert_eq!(list.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), ["d", "b", "a"]);
        assert_eq!(list[0].time, 5);
    }

    #[test]
    fn keep_is_at_least_one() {
        let mut list = Vec::new();
        push_entry(&mut list, entry("a", 1), 0);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn only_files_below_a_capture_folder_count_as_inside() {
        let root = Path::new("/pics/shots");
        assert!(within(root, Path::new("/pics/shots/a.png")));
        assert!(within(root, Path::new("/pics/shots/sub/a.png")));
        assert!(!within(root, root));
        assert!(!within(root, Path::new("/pics/shots2/a.png")));
        assert!(!within(root, Path::new("/pics/other/a.png")));
        assert!(!within(root, Path::new("/pics/shots/../secret.txt")));
    }

    #[test]
    fn delete_needs_the_file_in_the_list_and_inside_a_folder() {
        let list = vec![entry("/pics/shots/a.png", 1), entry("/elsewhere/b.png", 2)];
        let roots = vec![PathBuf::from("/pics/shots"), PathBuf::from("/videos")];
        assert!(deletable(&list, &roots, Path::new("/pics/shots/a.png")));
        assert!(!deletable(&list, &roots, Path::new("/elsewhere/b.png")));
        assert!(!deletable(&list, &roots, Path::new("/pics/shots/not-listed.png")));
        assert!(!deletable(&list, &[], Path::new("/pics/shots/a.png")));
    }
}
