//! Wallpaper gallery (spec/apps/wallpaper-gallery.md): thumbnails of a folder, click to apply.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::fsx;

const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "webp"];
const LIMIT: usize = 2000;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Wall {
    pub path: String,
    pub name: String,
}

/// The folders to show: the chosen ones, or Pictures plus the system wallpaper folders.
pub fn folders(chosen: &[String]) -> Vec<PathBuf> {
    let chosen: Vec<PathBuf> = chosen.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(PathBuf::from).collect();
    if !chosen.is_empty() {
        return chosen;
    }
    let mut out = vec![fsx::home().join("Pictures")];
    if cfg!(windows) {
        out.push(fsx::env_path("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Web\\Wallpaper"));
    } else {
        out.extend([PathBuf::from("/usr/share/backgrounds"), PathBuf::from("/usr/share/wallpapers")]);
    }
    out
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<Wall>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for e in read.flatten() {
        if out.len() >= LIMIT {
            return;
        }
        let p = e.path();
        if fsx::is_link(&p) {
            continue;
        }
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else if is_image(&p) {
            out.push(Wall { path: p.to_string_lossy().into_owned(), name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default() });
        }
    }
}

pub fn list(dirs: &[PathBuf]) -> Vec<Wall> {
    let mut out = vec![];
    for d in dirs {
        walk(d, 2, &mut out);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Only images inside the shown folders can be read or applied.
pub fn allowed(path: &Path, dirs: &[PathBuf]) -> bool {
    let Ok(real) = fs::canonicalize(path) else { return false };
    is_image(&real) && dirs.iter().filter_map(|d| fs::canonicalize(d).ok()).any(|d| real.starts_with(d))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// A small PNG of the image as a data URL.
pub fn thumbnail(path: &Path) -> Result<String, String> {
    let img = image::open(path).map_err(|e| e.to_string())?.thumbnail(256, 160).to_rgba8();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64(&png)))
}

/// The commands that set the wallpaper on a Linux desktop, in order.
#[cfg_attr(windows, allow(dead_code))]
pub fn linux_commands(desktop: &str, path: &str) -> Vec<Vec<String>> {
    let d = desktop.to_lowercase();
    let uri = format!("file://{path}");
    let s = |parts: &[&str]| parts.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    if d.contains("kde") || d.contains("plasma") {
        vec![s(&["plasma-apply-wallpaperimage", path])]
    } else if d.contains("cinnamon") {
        vec![s(&["gsettings", "set", "org.cinnamon.desktop.background", "picture-uri", &uri])]
    } else if d.contains("gnome") || d.contains("unity") || d.contains("budgie") || d.contains("pantheon") {
        vec![
            s(&["gsettings", "set", "org.gnome.desktop.background", "picture-uri", &uri]),
            s(&["gsettings", "set", "org.gnome.desktop.background", "picture-uri-dark", &uri]),
        ]
    } else if d.contains("xfce") {
        vec![s(&["xfconf-query", "-c", "xfce4-desktop", "-p", "/backdrop/screen0/monitor0/workspace0/last-image", "-s", path])]
    } else {
        vec![s(&["feh", "--bg-fill", path])]
    }
}

#[cfg(not(windows))]
pub fn apply(path: &Path, _all: bool) -> Result<(), String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let mut last = Err("no wallpaper command".to_string());
    for cmd in linux_commands(&desktop, &path.to_string_lossy()) {
        let args: Vec<&str> = cmd[1..].iter().map(String::as_str).collect();
        last = fsx::output(&cmd[0], &args).map(|_| ());
        if last.is_err() {
            break;
        }
    }
    last.map_err(|e| format!("The wallpaper could not be set on this desktop. {e}"))
}

#[cfg(windows)]
pub fn apply(path: &Path, all: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::POINT,
            System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
            UI::{
                Shell::{DesktopWallpaper, IDesktopWallpaper},
                WindowsAndMessaging::GetCursorPos,
            },
        },
    };
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let apply_to = |all: bool| -> windows::core::Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let wallpaper: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?;
            if all {
                return wallpaper.SetWallpaper(PCWSTR::null(), PCWSTR(file.as_ptr()));
            }
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor)?;
            for i in 0..wallpaper.GetMonitorDevicePathCount()? {
                let id = wallpaper.GetMonitorDevicePathAt(i)?;
                let rect = wallpaper.GetMonitorRECT(PCWSTR(id.0));
                let hit = rect.is_ok_and(|r| cursor.x >= r.left && cursor.x < r.right && cursor.y >= r.top && cursor.y < r.bottom);
                let result = if hit { Some(wallpaper.SetWallpaper(PCWSTR(id.0), PCWSTR(file.as_ptr()))) } else { None };
                CoTaskMemFree(Some(id.0 as *const _));
                if let Some(r) = result {
                    return r;
                }
            }
            wallpaper.SetWallpaper(PCWSTR::null(), PCWSTR(file.as_ptr()))
        }
    };
    apply_to(all).map_err(|e| format!("The wallpaper could not be set. {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_images_and_sorts_by_name() {
        let root = std::env::temp_dir().join(format!("notifi-wall-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        for f in ["b.PNG", "a.jpg", "notes.txt", "sub/c.webp"] {
            fs::write(root.join(f), "x").unwrap();
        }
        let names: Vec<String> = list(&[root.clone()]).into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert!(allowed(&root.join("a.jpg"), &[root.clone()]));
        assert!(!allowed(&root.join("notes.txt"), &[root.clone()]));
        assert!(!allowed(&root.join("missing.jpg"), &[root.clone()]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn only_commands_are_planned_never_run() {
        assert_eq!(linux_commands("KDE", "/p/a.jpg"), [["plasma-apply-wallpaperimage", "/p/a.jpg"]]);
        let gnome = linux_commands("ubuntu:GNOME", "/p/a.jpg");
        assert_eq!(gnome.len(), 2);
        assert_eq!(gnome[0][4], "file:///p/a.jpg");
        assert_eq!(linux_commands("sway", "/p/a.jpg")[0][0], "feh");
    }

    #[test]
    fn thumbnails_are_small_png_data_urls() {
        let root = std::env::temp_dir().join(format!("notifi-thumb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("t.png");
        image::RgbaImage::from_pixel(800, 600, image::Rgba([10, 20, 30, 255])).save(&file).unwrap();
        let url = thumbnail(&file).unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBOR"));
        assert!(url.len() < 4000);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
