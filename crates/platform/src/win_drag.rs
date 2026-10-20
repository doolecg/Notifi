//! Dragging files out of a Notifi window into other apps, using the shell's own data object so
//! Explorer, mail clients and browsers get the same formats a drag from Explorer gives them.

use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::core::{implement, BOOL, HRESULT, PCWSTR};
use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK};
use windows::Win32::System::Com::{IBindCtx, IDataObject};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, OleUninitialize, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    DROPEFFECT_MOVE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{BHID_DataObject, ILFree, SHCreateShellItemArrayFromIDLists, SHParseDisplayName};

use crate::error::{PlatformError, Result};

#[implement(IDropSource)]
struct Source;

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

fn werr(e: windows::core::Error) -> PlatformError {
    PlatformError::backend("drag out", e)
}

/// Drags `paths` from where the pointer is; call while the left button is still down. Returns
/// once the drag ends, true when it was dropped somewhere.
pub fn drag_files(paths: &[PathBuf]) -> Result<bool> {
    let paths = paths.to_vec();
    std::thread::Builder::new()
        .name("notifi-drag-out".into())
        .spawn(move || {
            unsafe { OleInitialize(None) }.map_err(werr)?;
            let mut pidls: Vec<*const ITEMIDLIST> = Vec::new();
            for path in &paths {
                let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                let mut pidl = std::ptr::null_mut();
                if unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None) }.is_ok() {
                    pidls.push(pidl);
                }
            }
            let result = if pidls.is_empty() { Err(PlatformError::NotFound("none of the files exist".into())) } else { start(&pidls) };
            for pidl in pidls {
                unsafe { ILFree(Some(pidl)) };
            }
            unsafe { OleUninitialize() };
            result
        })?
        .join()
        .map_err(|_| PlatformError::backend("drag out", "the drag thread stopped"))?
}

fn start(pidls: &[*const ITEMIDLIST]) -> Result<bool> {
    let items = unsafe { SHCreateShellItemArrayFromIDLists(pidls) }.map_err(werr)?;
    let data: IDataObject = unsafe { items.BindToHandler(None::<&IBindCtx>, &BHID_DataObject) }.map_err(werr)?;
    let source: IDropSource = Source.into();
    let mut effect = DROPEFFECT(0);
    let allowed = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0 | DROPEFFECT_LINK.0);
    let hr = unsafe { DoDragDrop(&data, &source, allowed, &mut effect) };
    Ok(hr == DRAGDROP_S_DROP)
}
