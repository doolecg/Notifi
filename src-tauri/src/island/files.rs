//! The newest files in the Downloads and Screenshots folders, for the Island's Downloads and Captures modules.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use serde::Serialize;

const IMAGES: &[&str] = &["png", "jpg", "jpeg", "bmp", "webp", "gif"];
const SHOWN: usize = 14;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub name: String,
    pub path: String,
    pub modified_ms: u64,
    pub size: u64,
    pub image: bool,
}

fn home() -> Option<PathBuf> {
    super::agents::home()
}

/// The folder behind a module: "downloads" or "captures".
fn folder(kind: &str) -> Option<PathBuf> {
    let home = home()?;
    match kind {
        "downloads" => Some(home.join("Downloads")),
        "captures" => {
            let shots = home.join("Pictures").join("Screenshots");
            Some(if shots.is_dir() { shots } else { home.join("Pictures") })
        }
        _ => None,
    }
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGES.contains(&e.to_ascii_lowercase().as_str()))
}

/// Newest files in `dir`, files only, hidden and partial downloads left out.
pub fn newest(dir: &Path, images_only: bool, limit: usize) -> Vec<RecentFile> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut rows: Vec<RecentFile> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let partial = ["crdownload", "part", "tmp", "download"].iter().any(|x| name.to_ascii_lowercase().ends_with(x));
            if !meta.is_file() || name.starts_with('.') || partial || (images_only && !is_image(&path)) {
                return None;
            }
            let modified_ms = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(RecentFile { image: is_image(&path), name, path: path.to_string_lossy().into_owned(), modified_ms, size: meta.len() })
        })
        .collect();
    rows.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    rows.truncate(limit);
    rows
}

#[tauri::command]
pub fn island_recent_files(kind: String) -> Vec<RecentFile> {
    folder(&kind).map(|d| newest(&d, kind == "captures", SHOWN)).unwrap_or_default()
}

/// Only files directly inside one of the two folders may be opened or read.
fn allowed(path: &str) -> Option<PathBuf> {
    let p = fs::canonicalize(path).ok()?;
    ["downloads", "captures"].iter().filter_map(|k| folder(k)).filter_map(|d| fs::canonicalize(d).ok()).any(|d| p.parent() == Some(d.as_path())).then_some(p)
}

#[tauri::command]
pub fn island_file_thumb(path: String) -> Option<String> {
    let p = allowed(&path)?;
    if !is_image(&p) {
        return None;
    }
    let thumb = image::open(&p).ok()?.thumbnail(220, 140).to_rgba8();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(thumb).write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", super::base64(png.get_ref())))
}

#[tauri::command]
pub fn island_open_path(path: String, reveal: bool) -> Result<(), String> {
    let p = allowed(&path).ok_or("That file is outside the Downloads and Screenshots folders.")?;
    let mut cmd;
    if cfg!(windows) {
        cmd = Command::new("explorer");
        if reveal {
            cmd.arg(format!("/select,{}", p.display()));
        } else {
            cmd.arg(p);
        }
    } else {
        cmd = Command::new("xdg-open");
        cmd.arg(if reveal { p.parent().map(Path::to_path_buf).unwrap_or(p) } else { p });
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_newest_first_and_skips_partial_and_hidden() {
        let dir = std::env::temp_dir().join(format!("notifi-files-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.png"), b"x").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        fs::write(dir.join("b.txt"), b"x").unwrap();
        fs::write(dir.join("c.crdownload"), b"x").unwrap();
        fs::write(dir.join(".hidden"), b"x").unwrap();
        let all: Vec<_> = newest(&dir, false, 10).into_iter().map(|f| f.name).collect();
        assert_eq!(all, ["b.txt", "a.png"]);
        let images: Vec<_> = newest(&dir, true, 10).into_iter().map(|f| f.name).collect();
        assert_eq!(images, ["a.png"]);
        let _ = fs::remove_dir_all(dir);
    }
}
