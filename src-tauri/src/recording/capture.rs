//! Desktop capture through DXGI desktop duplication, and the pointer, click and typing effects
//! drawn into each frame before it is encoded.

use std::{ptr, time::Duration};

use windows::{
    core::Interface,
    Win32::{
        Foundation::{HMODULE, POINT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX, D3D11_CPU_ACCESS_READ,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
                DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
            },
            Gdi::{
                ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject, BITMAPINFO,
                BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
            },
        },
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            WindowsAndMessaging::{
                DrawIconEx, GetCursorInfo, GetGUIThreadInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, GUITHREADINFO, HICON,
                ICONINFO,
            },
        },
    },
};

use super::pure::*;

fn err(what: &'static str) -> impl Fn(windows::core::Error) -> String {
    move |e| format!("{what}: {e}")
}

/// Copies the chosen part of one display into a CPU buffer whenever it changes.
pub struct Capturer {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    output: IDXGIOutput1,
    dup: IDXGIOutputDuplication,
    /// Latest picture of the region, kept on the GPU between frames.
    shadow: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    src_box: D3D11_BOX,
    lost: u32,
    /// The part of the desktop being recorded (even size, inside one display).
    pub region: Region,
    /// BGRA, `region.w * region.h * 4` bytes, as of the last `read`.
    pub base: Vec<u8>,
}

impl Capturer {
    pub fn new(want: Region) -> Result<Self, String> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(err("Display list"))?;
            let (cx, cy) = want.center();
            let mut found = None;
            'search: for a in 0.. {
                let Ok(adapter) = factory.EnumAdapters1(a) else { break };
                for o in 0.. {
                    let Ok(output) = adapter.EnumOutputs(o) else { break };
                    let Ok(desc) = output.GetDesc() else { continue };
                    let r = desc.DesktopCoordinates;
                    if desc.AttachedToDesktop.as_bool() && cx >= r.left && cx < r.right && cy >= r.top && cy < r.bottom {
                        found = Some((adapter, output, r));
                        break 'search;
                    }
                }
            }
            let (adapter, output, r) = found.ok_or("The chosen area is not on a display that can be recorded")?;
            let display = Region { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
            let region = want.within(display).ok_or("The chosen area is outside the display")?.even();

            let adapter: IDXGIAdapter = adapter.cast().map_err(err("Graphics adapter"))?;
            let (mut device, mut ctx) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
            .map_err(err("Graphics device"))?;
            let device = device.ok_or("No graphics device")?;
            let ctx = ctx.ok_or("No graphics device")?;

            let output: IDXGIOutput1 = output.cast().map_err(err("Display output"))?;
            let dup = output.DuplicateOutput(&device).map_err(err("Screen duplication"))?;
            let info = dup.GetDesc();
            if info.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
                return Err("This display uses a colour format that cannot be recorded (HDR is not supported)".into());
            }
            if info.Rotation.0 > 1 {
                return Err("Rotated displays cannot be recorded yet".into());
            }

            let mut desc = D3D11_TEXTURE2D_DESC {
                Width: region.w as u32,
                Height: region.h as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: 0,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut shadow = None;
            device.CreateTexture2D(&desc, None, Some(&mut shadow)).map_err(err("Frame buffer"))?;
            desc.Usage = D3D11_USAGE_STAGING;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging)).map_err(err("Frame buffer"))?;

            let left = (region.x - r.left) as u32;
            let top = (region.y - r.top) as u32;
            Ok(Self {
                device,
                ctx,
                output,
                dup,
                shadow: shadow.ok_or("No frame buffer")?,
                staging: staging.ok_or("No frame buffer")?,
                src_box: D3D11_BOX { left, top, front: 0, right: left + region.w as u32, bottom: top + region.h as u32, back: 1 },
                lost: 0,
                region,
                base: vec![0; region.w as usize * region.h as usize * 4],
            })
        }
    }

    /// Waits up to `timeout_ms` for the screen to change and keeps the new picture of the region.
    /// A quiet screen is not an error; the previous picture simply stays.
    pub fn acquire(&mut self, timeout_ms: u32) -> Result<(), String> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            match self.dup.AcquireNextFrame(timeout_ms, &mut info, &mut resource) {
                Ok(()) => {
                    self.lost = 0;
                    // A frame with no present time only moved the pointer.
                    if info.LastPresentTime != 0 {
                        if let Some(tex) = resource.as_ref().and_then(|r| r.cast::<ID3D11Texture2D>().ok()) {
                            self.ctx.CopySubresourceRegion(&self.shadow, 0, 0, 0, 0, &tex, 0, Some(&self.src_box));
                        }
                    }
                    drop(resource);
                    let _ = self.dup.ReleaseFrame();
                    Ok(())
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => Ok(()),
                Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                    // Resolution change, lock screen or an elevated prompt: try again for a few seconds.
                    self.lost += 1;
                    if self.lost > 100 {
                        return Err("The display stopped being available".into());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                    if let Ok(dup) = self.output.DuplicateOutput(&self.device) {
                        self.dup = dup;
                    }
                    Ok(())
                }
                Err(e) => Err(format!("Screen capture failed: {e}")),
            }
        }
    }

    /// Reads the kept picture into `base`.
    pub fn read(&mut self) -> Result<(), String> {
        unsafe {
            self.ctx.CopyResource(&self.staging, &self.shadow);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).map_err(err("Reading the frame"))?;
            let row = self.region.w as usize * 4;
            for y in 0..self.region.h as usize {
                let src = std::slice::from_raw_parts((mapped.pData as *const u8).add(y * mapped.RowPitch as usize), row);
                self.base[y * row..(y + 1) * row].copy_from_slice(src);
            }
            self.ctx.Unmap(&self.staging, 0);
        }
        Ok(())
    }
}

fn pressed(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
}

/// Keys that count as typing: letters, digits, punctuation, numpad, space, backspace and enter.
fn is_typing_key(vk: usize) -> bool {
    matches!(vk, 0x08 | 0x0D | 0x20 | 0x30..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF)
}

/// The text caret on screen, for the foreground window, when it has a standard one.
fn caret() -> Option<(i32, i32)> {
    unsafe {
        let mut gui = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(0, &mut gui).ok()?;
        let r = gui.rcCaret;
        if gui.hwndCaret.is_invalid() || (r.right <= r.left && r.bottom <= r.top) {
            return None;
        }
        let mut p = POINT { x: r.left, y: (r.top + r.bottom) / 2 };
        ClientToScreen(gui.hwndCaret, &mut p).as_bool().then_some((p.x, p.y))
    }
}

/// Draws the pointer, the highlight and click rings into a frame, and zooms toward the caret while
/// the user types.
pub struct Compositor {
    w: usize,
    h: usize,
    region: Region,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    zoomed: Vec<u8>,
    effects: bool,
    typing_zoom: bool,
    ripples: Vec<Ripple>,
    buttons: [bool; 2],
    keys: [bool; 256],
    zoom: Zoom,
    last: f64,
}

impl Compositor {
    pub fn new(region: Region, effects: bool, typing_zoom: bool) -> Result<Self, String> {
        let (w, h) = (region.w as usize, region.h as usize);
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: region.w,
                    biHeight: -region.h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = ptr::null_mut();
            let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).map_err(err("Frame bitmap"))?;
            let dc = CreateCompatibleDC(None);
            let previous = SelectObject(dc, bitmap.into());
            Ok(Self {
                w,
                h,
                region,
                dc,
                bitmap,
                previous,
                bits: bits as *mut u8,
                zoomed: vec![0; w * h * 4],
                effects,
                typing_zoom,
                ripples: Vec::new(),
                buttons: [pressed(0x01), pressed(0x02)],
                keys: std::array::from_fn(|vk| is_typing_key(vk) && pressed(vk as i32)),
                zoom: Zoom::new(w as f64, h as f64),
                last: 0.0,
            })
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn frame(&self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.w * self.h * 4) }
    }

    /// The finished frame for `base` at `now_ms` of recorded time.
    pub fn compose(&mut self, base: &[u8], now_ms: u64) -> &[u8] {
        let (w, h) = (self.w as i32, self.h as i32);
        self.frame().copy_from_slice(base);
        let now = now_ms as f64 / 1000.0;
        let dt = (now - self.last).max(0.0);
        self.last = now;

        let mut cursor = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        let showing = unsafe { GetCursorInfo(&mut cursor).is_ok() } && cursor.flags.0 & CURSOR_SHOWING.0 != 0;
        let (px, py) = (cursor.ptScreenPos.x - self.region.x, cursor.ptScreenPos.y - self.region.y);

        let buttons = [pressed(0x01), pressed(0x02)];
        if self.effects {
            for i in 0..2 {
                if buttons[i] && !self.buttons[i] {
                    self.ripples.push(Ripple { x: px, y: py, born_ms: now_ms, right: i == 1 });
                }
            }
        }
        self.buttons = buttons;

        if self.typing_zoom {
            let modifier = pressed(0x11) || pressed(0x12);
            let mut typed = false;
            for vk in 0..256usize {
                if !is_typing_key(vk) {
                    continue;
                }
                let down = pressed(vk as i32);
                typed |= down && !self.keys[vk] && !modifier;
                self.keys[vk] = down;
            }
            if typed {
                self.zoom.typed(now);
            }
            let focus = caret().map_or((px, py), |c| (c.0 - self.region.x, c.1 - self.region.y));
            self.zoom.step(now, dt, (f64::from(focus.0.clamp(0, w)), f64::from(focus.1.clamp(0, h))));
        }

        if self.effects && showing {
            fill_disc(self.frame(), w, h, px, py, HIGHLIGHT_RADIUS, HIGHLIGHT, 0.28);
        }
        self.ripples.retain(|r| now_ms.saturating_sub(r.born_ms) < 550);
        if self.effects {
            for r in &self.ripples {
                if let Some((radius, alpha)) = ripple_look(now_ms.saturating_sub(r.born_ms)) {
                    let color = if r.right { RIGHT_RING } else { LEFT_RING };
                    stroke_ring(self.frame(), w, h, r.x, r.y, radius, 3.0, color, alpha);
                }
            }
        }
        if showing {
            self.draw_cursor(&cursor, px, py);
        }

        if self.zoom.level > 1.01 {
            let rect = self.zoom.rect(self.w as f64, self.h as f64);
            let src = unsafe { std::slice::from_raw_parts(self.bits, self.w * self.h * 4) };
            zoom_blit(src, self.w, self.h, rect, &mut self.zoomed);
            &self.zoomed
        } else {
            unsafe { std::slice::from_raw_parts(self.bits, self.w * self.h * 4) }
        }
    }

    fn draw_cursor(&self, cursor: &CURSORINFO, x: i32, y: i32) {
        unsafe {
            let icon = HICON(cursor.hCursor.0);
            let mut info = ICONINFO::default();
            if GetIconInfo(icon, &mut info).is_ok() {
                let _ = DrawIconEx(self.dc, x - info.xHotspot as i32, y - info.yHotspot as i32, icon, 0, 0, 0, None, DI_NORMAL);
                if !info.hbmMask.is_invalid() {
                    let _ = DeleteObject(info.hbmMask.into());
                }
                if !info.hbmColor.is_invalid() {
                    let _ = DeleteObject(info.hbmColor.into());
                }
            }
            let _ = GdiFlush();
        }
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
