//! Dragging a file out of the chrome as a real shell drag: web pages (an upload area in any tab)
//! and other apps receive it exactly as a file dragged from Explorer.

use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize, DROPEFFECT_COPY, DROPEFFECT_NONE};
use windows::Win32::UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName, SHDoDragDrop};

/// Drags `path` from the pointer until it is dropped or cancelled; true when it was dropped.
/// Runs a modal loop, so call it on the UI thread while the mouse button is still down. The file
/// is only ever copied: the drop target can't move it out of the downloads folder.
pub fn drag_file(hwnd: Option<HWND>, path: &Path) -> Result<bool, String> {
    unsafe {
        // The UI thread already has OLE; this only pairs with the OleUninitialize below.
        let ole = OleInitialize(None).is_ok();
        let result = (|| {
            let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None).map_err(|e| e.to_string())?;
            // The shell's data object carries the file (CF_HDROP and friends) and its drag image.
            let data: IDataObject = item.BindToHandler(None, &BHID_DataObject).map_err(|e| e.to_string())?;
            let effect = SHDoDragDrop(hwnd, &data, None, DROPEFFECT_COPY).map_err(|e| e.to_string())?;
            Ok(effect != DROPEFFECT_NONE)
        })();
        if ole {
            OleUninitialize();
        }
        result
    }
}
