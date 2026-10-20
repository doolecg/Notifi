//! Taskbar buttons on Windows, read through UI Automation.
//!
//! The taskbar exposes one UIA button per app ("Taskbar.TaskListButtonAutomationPeer" on
//! Windows 11, children of "MSTaskListWClass" on Windows 10). [`buttons`] lists them with their
//! screen rectangles so callers can hit-test the pointer themselves (fast enough for a mouse
//! hook) and refresh the list only now and then. Limits: the class names, the "Appid: <AUMID>"
//! automation id and the English " - N running windows" name suffix are not a documented
//! contract and may change with taskbar redesigns; buttons of apps grouped under one AUMID can't
//! be told apart.

use std::cell::RefCell;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Children, TreeScope_Descendants,
    UIA_ButtonControlTypeId, UIA_ClassNamePropertyId, UIA_ControlTypePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetAncestor, GetClassNameW, GetWindowRect, WindowFromPoint, GA_ROOT,
};

use crate::common::Rect;
use crate::error::{PlatformError, Result};
use crate::Feature;

const TASKBARS: &[&str] = &["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskbarButton {
    /// UIA name, e.g. "Notepad - 2 running windows".
    pub name: String,
    /// UIA automation id, e.g. "Appid: Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".
    pub automation_id: String,
    pub rect: Rect,
    /// The rectangle of the taskbar holding the button.
    pub taskbar: Rect,
}

impl TaskbarButton {
    /// The name without the running-window count.
    pub fn app_name(&self) -> &str {
        let name = self.name.strip_suffix(" pinned").unwrap_or(&self.name);
        match name.rfind(" - ") {
            Some(i) if name[i + 3..].contains("running window") => &name[..i],
            _ => name,
        }
    }

    /// The AppUserModelID from the automation id, when it has one.
    pub fn app_id(&self) -> Option<&str> {
        self.automation_id.strip_prefix("Appid: ").map(str::trim).filter(|s| !s.is_empty())
    }

    /// Whether a window of process `exe` (with AppUserModelID `aumid`, if known) belongs to this button.
    pub fn matches(&self, exe: Option<&str>, aumid: Option<&str>) -> bool {
        let id = self.app_id().map(str::to_lowercase);
        if let (Some(id), Some(aumid)) = (&id, aumid) {
            if id.eq_ignore_ascii_case(aumid) {
                return true;
            }
        }
        let Some(exe) = exe.map(str::to_lowercase) else { return false };
        if let Some(id) = &id {
            if id.ends_with(&format!("\\{exe}")) || id.ends_with(&format!("/{exe}")) || *id == exe {
                return true;
            }
        }
        let stem = exe.strip_suffix(".exe").unwrap_or(&exe);
        let name = self.app_name().to_lowercase();
        stem.len() >= 3 && (name == stem || name.split(|c: char| !c.is_alphanumeric()).any(|w| w == stem))
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        contains(self.rect, x, y)
    }
}

fn contains(r: Rect, x: i32, y: i32) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.width as i32 && y < r.y + r.height as i32
}

fn rect(r: RECT) -> Rect {
    Rect { x: r.left, y: r.top, width: (r.right - r.left).max(0) as u32, height: (r.bottom - r.top).max(0) as u32 }
}

unsafe fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = GetClassNameW(h, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Whether the point (physical pixels) is over a taskbar.
pub fn is_over_taskbar(x: i32, y: i32) -> bool {
    unsafe {
        let h = WindowFromPoint(POINT { x, y });
        if h.is_invalid() {
            return false;
        }
        let root = GetAncestor(h, GA_ROOT);
        TASKBARS.contains(&class_of(if root.is_invalid() { h } else { root }).as_str())
    }
}

unsafe extern "system" fn collect(h: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    if TASKBARS.contains(&class_of(h).as_str()) {
        out.push(h);
    }
    BOOL(1)
}

fn taskbar_windows() -> Vec<HWND> {
    let mut out = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut Vec<HWND> as isize));
        if out.is_empty() {
            if let Ok(h) = FindWindowW(windows::core::w!("Shell_TrayWnd"), None) {
                out.push(h);
            }
        }
    }
    out
}

/// Rectangles of every taskbar (one per display when the taskbar is shown on all displays).
pub fn taskbars() -> Vec<Rect> {
    taskbar_windows()
        .into_iter()
        .filter_map(|h| unsafe {
            let mut r = RECT::default();
            GetWindowRect(h, &mut r).ok().map(|_| rect(r))
        })
        .collect()
}

thread_local! {
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn automation() -> Result<IUIAutomation> {
    UIA.with(|cell| {
        if let Some(a) = cell.borrow().as_ref() {
            return Ok(a.clone());
        }
        let a: IUIAutomation = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|e| PlatformError::unsupported(Feature::WindowList, format!("UI Automation: {e}")))?;
        *cell.borrow_mut() = Some(a.clone());
        Ok(a)
    })
}

fn variant_i32(v: i32) -> VARIANT {
    let mut var = VARIANT::default();
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = v;
    }
    var
}

unsafe fn button(el: &IUIAutomationElement, taskbar: Rect) -> Option<TaskbarButton> {
    let r = rect(el.CurrentBoundingRectangle().ok()?);
    (r.width > 0 && r.height > 0).then(|| TaskbarButton {
        name: el.CurrentName().map(|s| s.to_string()).unwrap_or_default(),
        automation_id: el.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default(),
        rect: r,
        taskbar,
    })
}

/// Every app button on every taskbar. Takes tens of milliseconds; cache the result.
pub fn buttons() -> Result<Vec<TaskbarButton>> {
    let uia = automation()?;
    let mut out = Vec::new();
    unsafe {
        let is_button = uia
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &variant_i32(UIA_ButtonControlTypeId.0))
            .map_err(|e| PlatformError::unsupported(Feature::WindowList, format!("UIA condition: {e}")))?;
        let is_list = uia
            .CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from("MSTaskListWClass"))
            .map_err(|e| PlatformError::unsupported(Feature::WindowList, format!("UIA condition: {e}")))?;
        for h in taskbar_windows() {
            let mut r = RECT::default();
            let _ = GetWindowRect(h, &mut r);
            let bar = rect(r);
            let Ok(root) = uia.ElementFromHandle(h) else { continue };
            if let Ok(all) = root.FindAll(TreeScope_Descendants, &is_button) {
                for i in 0..all.Length().unwrap_or(0) {
                    let Ok(el) = all.GetElement(i) else { continue };
                    let class = el.CurrentClassName().map(|s| s.to_string()).unwrap_or_default();
                    if class.contains("TaskListButton") {
                        out.extend(button(&el, bar));
                    }
                }
            }
            if let Ok(lists) = root.FindAll(TreeScope_Descendants, &is_list) {
                for i in 0..lists.Length().unwrap_or(0) {
                    let Ok(list) = lists.GetElement(i) else { continue };
                    let Ok(kids) = list.FindAll(TreeScope_Children, &is_button) else { continue };
                    for j in 0..kids.Length().unwrap_or(0) {
                        if let Ok(el) = kids.GetElement(j) {
                            out.extend(button(&el, bar));
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The button under the point, from a cached [`buttons`] list.
pub fn hit<'a>(buttons: &'a [TaskbarButton], x: i32, y: i32) -> Option<&'a TaskbarButton> {
    buttons.iter().find(|b| b.contains(x, y))
}

/// The AppUserModelID of a packaged process, if it has one.
pub fn process_app_id(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let r = GetApplicationUserModelId(h, &mut len, Some(PWSTR(buf.as_mut_ptr())));
        let _ = CloseHandle(h);
        if r.0 != 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(len as usize);
        Some(String::from_utf16_lossy(&buf[..end])).filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(name: &str, id: &str) -> TaskbarButton {
        TaskbarButton { name: name.into(), automation_id: id.into(), rect: Rect { x: 10, y: 1000, width: 40, height: 40 }, taskbar: Rect::default() }
    }

    #[test]
    fn names_and_ids() {
        let n = b("Notepad - 2 running windows", "Appid: Microsoft.WindowsNotepad_8wekyb3d8bbwe!App");
        assert_eq!(n.app_name(), "Notepad");
        assert_eq!(n.app_id(), Some("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"));
        assert_eq!(b("Docs - Draft", "").app_name(), "Docs - Draft");
        assert_eq!(b("Parsec - 1 running window pinned", "").app_name(), "Parsec");
        assert_eq!(b("Steam pinned", "").app_name(), "Steam");
        assert_eq!(b("noctty - 2 running windows and 1 group", "").app_name(), "noctty");
        assert_eq!(b("x", "").app_id(), None);
    }

    #[test]
    fn matching() {
        let n = b("Notepad - 1 running window", "Appid: Microsoft.WindowsNotepad_8wekyb3d8bbwe!App");
        assert!(n.matches(Some("Notepad.exe"), None));
        assert!(n.matches(None, Some("microsoft.windowsnotepad_8wekyb3d8bbwe!app")));
        assert!(!n.matches(Some("chrome.exe"), None));
        let path = b("Paint", "Appid: {1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\mspaint.exe");
        assert!(path.matches(Some("mspaint.exe"), None));
        let chrome = b("Google Chrome - 3 running windows", "Appid: Chrome");
        assert!(chrome.matches(Some("chrome.exe"), None));
        assert!(!b("Antivirus", "").matches(Some("a.exe"), None));
    }

    #[test]
    fn hit_testing() {
        let list = vec![b("A", ""), TaskbarButton { rect: Rect { x: 50, y: 1000, width: 40, height: 40 }, ..b("B", "") }];
        assert_eq!(hit(&list, 60, 1010).map(|b| b.name.as_str()), Some("B"));
        assert!(hit(&list, 95, 1010).is_none());
    }

    #[test]
    #[ignore = "reads the live taskbar"]
    fn lists_live_buttons() {
        let list = buttons().unwrap();
        for b in &list {
            println!("{:?} {:?} {:?}", b.name, b.automation_id, b.rect);
        }
        assert!(!list.is_empty());
    }
}
