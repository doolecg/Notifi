//! App icons for the mixer rows as PNG data URLs.

use std::path::Path;

fn data_url(png: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(22 + png.len().div_ceil(3) * 4);
    out.push_str("data:image/png;base64,");
    for c in png.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn load(source: &str) -> Option<String> {
    let path = Path::new(source);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    if ext == "png" {
        return std::fs::read(path).ok().map(|b| data_url(&b));
    }
    #[cfg(windows)]
    if path.is_file() {
        return windows_icon(path).map(|b| data_url(&b));
    }
    #[cfg(not(windows))]
    if !source.contains('/') {
        return theme_icon(source).and_then(|p| std::fs::read(p).ok()).map(|b| data_url(&b));
    }
    None
}

#[cfg(not(windows))]
fn theme_icon(name: &str) -> Option<std::path::PathBuf> {
    let sizes = ["48x48", "64x64", "128x128", "256x256", "32x32", "scalable"];
    let mut roots = vec![std::path::PathBuf::from("/usr/share/icons/hicolor"), std::path::PathBuf::from("/usr/share/pixmaps")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(Path::new(&home).join(".local/share/icons/hicolor"));
    }
    for root in &roots {
        for size in sizes {
            let p = root.join(size).join("apps").join(format!("{name}.png"));
            if p.is_file() {
                return Some(p);
            }
        }
        let p = root.join(format!("{name}.png"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(windows)]
fn windows_icon(path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS},
            Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
            UI::{
                Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON},
                WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO},
            },
        },
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut info = SHFILEINFOW::default();
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if ok == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let result = unsafe {
        let mut icon = ICONINFO::default();
        let mut out = None;
        if GetIconInfo(info.hIcon, &mut icon).is_ok() {
            let mut bitmap = BITMAP::default();
            if GetObjectW(icon.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bitmap as *mut _ as *mut _)) != 0 {
                let (w, h) = (bitmap.bmWidth, bitmap.bmHeight);
                let mut header = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                };
                let mut pixels = vec![0u8; (w * h * 4) as usize];
                let dc = GetDC(None);
                let rows = GetDIBits(dc, icon.hbmColor, 0, h as u32, Some(pixels.as_mut_ptr() as *mut _), &mut header as *mut _ as *mut BITMAPINFO, DIB_RGB_COLORS);
                ReleaseDC(None, dc);
                if rows != 0 {
                    for px in pixels.chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                    if pixels.chunks_exact(4).all(|p| p[3] == 0) {
                        pixels.chunks_exact_mut(4).for_each(|p| p[3] = 255);
                    }
                    out = encode(&pixels, w as u32, h as u32);
                }
            }
            let _ = DeleteObject(icon.hbmColor.into());
            let _ = DeleteObject(icon.hbmMask.into());
        }
        out
    };
    unsafe {
        let _ = DestroyIcon(info.hIcon);
    }
    result
}

#[cfg(windows)]
fn encode(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(w, h, rgba.to_vec())?;
    let mut png = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(png)
}
