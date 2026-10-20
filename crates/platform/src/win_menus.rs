//! Menu commands of a window on Windows, read and run through UI Automation.
//!
//! The window's menu bar items are each expanded for a moment to read what is under them, then
//! collapsed. Classic Win32, MFC and WPF menu bars work this way. Apps that draw their own menus
//! (ribbons, most Electron apps) expose no menu bar, so they list nothing. The first scan of an app
//! visibly opens each of its menus in turn.

use std::cell::RefCell;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    TreeScope_Children, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_MenuBarControlTypeId, UIA_MenuControlTypeId, UIA_MenuItemControlTypeId,
};

use crate::error::{PlatformError, Result};
use crate::menus::{clean, Node};
use crate::Feature;

/// Menus nested deeper than this (File > Export > As > ...) are not opened.
const MAX_DEPTH: usize = 3;
/// Time for a menu to fill in after it opens.
const OPEN_WAIT: Duration = Duration::from_millis(45);

thread_local! {
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn unsupported(reason: impl Into<String>) -> PlatformError {
    PlatformError::unsupported(Feature::WindowList, reason)
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
        .map_err(|e| unsupported(format!("UI Automation: {e}")))?;
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

fn name(el: &IUIAutomationElement) -> String {
    unsafe { el.CurrentName() }.map(|s| s.to_string()).unwrap_or_default()
}

/// Elements of one control type under `el`, in tree order.
unsafe fn find(uia: &IUIAutomation, el: &IUIAutomationElement, deep: bool, control_type: i32) -> Vec<IUIAutomationElement> {
    let Ok(cond) = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &variant_i32(control_type)) else { return Vec::new() };
    let scope = if deep { TreeScope_Descendants } else { TreeScope_Children };
    let Ok(all) = el.FindAll(scope, &cond) else { return Vec::new() };
    (0..all.Length().unwrap_or(0)).filter_map(|i| all.GetElement(i).ok()).collect()
}

/// The items of every menu bar in the window except the system menu (the one on the icon).
unsafe fn bar_items(uia: &IUIAutomation, root: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
    find(uia, root, true, UIA_MenuBarControlTypeId.0)
        .iter()
        .filter(|bar| bar.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default() != "SystemMenuBar")
        .flat_map(|bar| find(uia, bar, false, UIA_MenuItemControlTypeId.0))
        .collect()
}

unsafe fn expander(item: &IUIAutomationElement) -> Option<IUIAutomationExpandCollapsePattern> {
    item.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId).ok()
}

/// The items of the menu an expanded item opened: its own children, or else a popup menu of the
/// same process shown as a separate top-level element.
unsafe fn submenu_items(uia: &IUIAutomation, item: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
    let own = find(uia, item, true, UIA_MenuItemControlTypeId.0);
    if !own.is_empty() {
        return own;
    }
    let (Ok(pid), Ok(desktop)) = (item.CurrentProcessId(), uia.GetRootElement()) else { return Vec::new() };
    find(uia, &desktop, false, UIA_MenuControlTypeId.0)
        .into_iter()
        .filter(|popup| popup.CurrentProcessId().is_ok_and(|p| p == pid))
        .flat_map(|popup| find(uia, &popup, false, UIA_MenuItemControlTypeId.0))
        .collect()
}

unsafe fn read_item(uia: &IUIAutomation, item: &IUIAutomationElement, depth: usize) -> Option<Node> {
    let label = name(item);
    if clean(&label).is_empty() || !item.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(true) {
        return None;
    }
    let Some(pattern) = expander(item) else { return Some(Node { name: label, children: Vec::new() }) };
    if depth >= MAX_DEPTH {
        return Some(Node { name: label, children: Vec::new() });
    }
    if pattern.Expand().is_err() {
        return (depth > 0).then_some(Node { name: label, children: Vec::new() });
    }
    std::thread::sleep(OPEN_WAIT);
    let children: Vec<Node> = submenu_items(uia, item).iter().filter_map(|kid| read_item(uia, kid, depth + 1)).collect();
    let _ = pattern.Collapse();
    // A menu-bar entry that opens nothing is not a command.
    if depth == 0 && children.is_empty() {
        return None;
    }
    Some(Node { name: label, children })
}

fn root(uia: &IUIAutomation, window: u64) -> Result<IUIAutomationElement> {
    unsafe { uia.ElementFromHandle(HWND(window as usize as *mut _)) }.map_err(|e| unsupported(format!("window {window}: {e}")))
}

/// The menu tree of the window, one node per menu-bar entry.
pub fn read(window: u64) -> Result<Vec<Node>> {
    let uia = automation()?;
    let root = root(&uia, window)?;
    unsafe { Ok(bar_items(&uia, &root).iter().filter_map(|item| read_item(&uia, item, 0)).collect()) }
}

/// Opens the menus along `path` and invokes the last item.
pub fn invoke(window: u64, path: &[String]) -> Result<()> {
    let uia = automation()?;
    let root = root(&uia, window)?;
    let (first, rest) = path.split_first().ok_or_else(|| PlatformError::NotFound("empty menu path".into()))?;
    let missing = || PlatformError::NotFound(format!("menu item {}", path.join(" > ")));
    unsafe {
        let mut item = bar_items(&uia, &root).into_iter().find(|i| clean(&name(i)) == *first).ok_or_else(missing)?;
        for step in rest {
            let pattern = expander(&item).ok_or_else(missing)?;
            pattern.Expand().map_err(|e| unsupported(format!("open menu: {e}")))?;
            std::thread::sleep(OPEN_WAIT);
            match submenu_items(&uia, &item).into_iter().find(|i| clean(&name(i)) == *step) {
                Some(next) => item = next,
                None => {
                    let _ = pattern.Collapse();
                    return Err(missing());
                }
            }
        }
        let invoke = item
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .map_err(|_| unsupported("this menu item cannot be run"))?;
        invoke.Invoke().map_err(|e| unsupported(format!("run menu item: {e}")))
    }
}
