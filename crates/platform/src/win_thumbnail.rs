//! Live window thumbnails and window icons on Windows.
//!
//! Thumbnails are DWM thumbnails: DWM composites the source window's live surface into one of our
//! top-level windows at a rectangle we choose. They are drawn above the host's own content
//! (including a WebView2 child), so a WebView can lay out cards, leave the preview slots empty and
//! report their rectangles; the thumbnails then sit exactly in those slots with no copying.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetClassLongPtrW, SendMessageTimeoutW, DI_NORMAL, GCLP_HICON, GCLP_HICONSM, HICON, ICON_BIG,
    ICON_SMALL2, SMTO_ABORTIFHUNG, WM_GETICON,
};

use crate::common::{Image, Rect};
use crate::error::{PlatformError, Result};
use crate::traits::WindowId;
use crate::Feature;

fn hwnd(id: u64) -> HWND {
    HWND(id as usize as *mut c_void)
}

/// One registered DWM thumbnail; unregistered on drop.
#[derive(Debug)]
pub struct Thumbnail {
    handle: isize,
}

impl Thumbnail {
    /// Registers `source` to be drawn into `host`, which must be one of our top-level windows.
    pub fn register(host: u64, source: WindowId) -> Result<Self> {
        let handle = unsafe { DwmRegisterThumbnail(hwnd(host), hwnd(source.0)) }
            .map_err(|e| PlatformError::unsupported(Feature::WindowThumbnails, format!("DwmRegisterThumbnail: {e}")))?;
        Ok(Self { handle })
    }

    /// The source window's size in physical pixels.
    pub fn source_size(&self) -> Option<(u32, u32)> {
        let size: SIZE = unsafe { DwmQueryThumbnailSourceSize(self.handle) }.ok()?;
        (size.cx > 0 && size.cy > 0).then_some((size.cx as u32, size.cy as u32))
    }

    /// Shows the thumbnail at `dest` (host client coordinates, physical pixels) with `opacity` 0..=255.
    pub fn place(&self, dest: Rect, opacity: u8) -> Result<()> {
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: RECT {
                left: dest.x,
                top: dest.y,
                right: dest.x + dest.width as i32,
                bottom: dest.y + dest.height as i32,
            },
            opacity,
            fVisible: true.into(),
            fSourceClientAreaOnly: false.into(),
            ..Default::default()
        };
        unsafe { DwmUpdateThumbnailProperties(self.handle, &props) }
            .map_err(|e| PlatformError::unsupported(Feature::WindowThumbnails, format!("DwmUpdateThumbnailProperties: {e}")))
    }

    pub fn hide(&self) {
        let props = DWM_THUMBNAIL_PROPERTIES { dwFlags: DWM_TNP_VISIBLE, fVisible: false.into(), ..Default::default() };
        let _ = unsafe { DwmUpdateThumbnailProperties(self.handle, &props) };
    }
}

impl Drop for Thumbnail {
    fn drop(&mut self) {
        let _ = unsafe { DwmUnregisterThumbnail(self.handle) };
    }
}

unsafe fn window_icon(h: HWND) -> Option<HICON> {
    for kind in [ICON_BIG, ICON_SMALL2] {
        let mut out = 0usize;
        let ok = SendMessageTimeoutW(h, WM_GETICON, WPARAM(kind as usize), LPARAM(0), SMTO_ABORTIFHUNG, 50, Some(&mut out));
        if ok.0 != 0 && out != 0 {
            return Some(HICON(out as *mut c_void));
        }
    }
    for index in [GCLP_HICON, GCLP_HICONSM] {
        let v = GetClassLongPtrW(h, index);
        if v != 0 {
            return Some(HICON(v as *mut c_void));
        }
    }
    None
}

/// Straightens BGRA rows into RGBA; icons without an alpha channel are made opaque.
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    bgra.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], if has_alpha { p[3] } else if p[..3] == [0, 0, 0] { 0 } else { 255 }])
        .collect()
}

/// The window's icon rendered at `size` x `size`.
pub fn icon(source: WindowId, size: u32) -> Option<Image> {
    let size = size.clamp(16, 256);
    unsafe {
        let icon = window_icon(hwnd(source.0))?;
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
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_rules() {
        assert_eq!(bgra_to_rgba(&[1, 2, 3, 128]), vec![3, 2, 1, 128]);
        assert_eq!(bgra_to_rgba(&[1, 2, 3, 0, 0, 0, 0, 0]), vec![3, 2, 1, 255, 0, 0, 0, 0]);
    }
}
