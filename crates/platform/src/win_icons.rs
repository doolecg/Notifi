//! A program's icon read from its file, for switcher entries that have no window to ask.

use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, PrivateExtractIconsW, DI_NORMAL, HICON};

use crate::common::Image;

/// The path as the fixed, zero-ended UTF-16 buffer `PrivateExtractIconsW` takes; `None` if it does not fit.
fn path_units(path: &str) -> Option<[u16; 260]> {
    let units: Vec<u16> = path.encode_utf16().collect();
    if units.is_empty() || units.len() >= 260 {
        return None;
    }
    let mut out = [0u16; 260];
    out[..units.len()].copy_from_slice(&units);
    Some(out)
}

/// Straightens BGRA rows into RGBA; icons without an alpha channel are made opaque.
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    bgra.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], if has_alpha { p[3] } else if p[..3] == [0, 0, 0] { 0 } else { 255 }])
        .collect()
}

/// The first icon of the executable (or any file with icons) at `path`, rendered at `size` x `size`.
pub fn file_icon(path: &str, size: u32) -> Option<Image> {
    let size = size.clamp(16, 256);
    let file = path_units(path)?;
    unsafe {
        let mut icons = [HICON::default()];
        let got = PrivateExtractIconsW(&file, 0, size as i32, size as i32, Some(&mut icons), None, 0);
        if got == 0 || got == u32::MAX || icons[0].is_invalid() {
            return None;
        }
        let icon = icons[0];
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            let _ = DestroyIcon(icon);
            return None;
        };
        let old = SelectObject(dc, bitmap.into());
        let drawn = DrawIconEx(dc, 0, 0, icon, size as i32, size as i32, 0, None, DI_NORMAL).is_ok();
        let out = (drawn && !bits.is_null()).then(|| {
            let bgra = std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize);
            Image { width: size, height: size, rgba: bgra_to_rgba(bgra) }
        });
        SelectObject(dc, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        let _ = DestroyIcon(icon);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_must_fit_the_buffer() {
        let buf = path_units(r"C:\a.exe").unwrap();
        assert_eq!(&buf[..8], &r"C:\a.exe".encode_utf16().collect::<Vec<_>>()[..]);
        assert_eq!(buf[8], 0);
        assert!(path_units("").is_none());
        assert!(path_units(&"x".repeat(260)).is_none());
        assert!(path_units(&"x".repeat(259)).is_some());
    }

    #[test]
    fn the_shell_executable_has_an_icon() {
        let img = file_icon(r"C:\Windows\explorer.exe", 48).expect("explorer.exe icon");
        assert_eq!((img.width, img.height), (48, 48));
        assert!(img.rgba.chunks_exact(4).any(|p| p[3] != 0));
    }
}
