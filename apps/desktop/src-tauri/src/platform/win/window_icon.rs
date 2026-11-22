//! Converts PNG bytes (e.g. profile avatars or app icon) into a Win32 `HICON`.

use std::io::Cursor;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, SendMessageW, HICON, ICON_BIG, ICON_SMALL, LR_DEFAULTCOLOR, WM_SETICON,
};

/// Decodes a PNG image and builds a Win32 icon handle from it.
pub fn hicon_from_png(png_bytes: &[u8]) -> Option<HICON> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let w = info.width;
    let h = info.height;
    if w == 0 || h == 0 || w > 512 || h > 512 {
        return None;
    }
    // Windows Vista+ CreateIconFromResourceEx directly accepts raw PNG data with version 0x00030000.
    unsafe { CreateIconFromResourceEx(png_bytes, true, 0x0003_0000, w as i32, h as i32, LR_DEFAULTCOLOR).ok() }
}

/// Applies an `HICON` to a top-level Win32 window (both taskbar and titlebar/Alt+Tab slots).
pub fn apply_to_window(hwnd: HWND, icon: HICON) {
    unsafe {
        let _ = SendMessageW(hwnd, WM_SETICON, Some(windows::Win32::Foundation::WPARAM(ICON_SMALL as usize)), Some(windows::Win32::Foundation::LPARAM(icon.0 as isize)));
        let _ = SendMessageW(hwnd, WM_SETICON, Some(windows::Win32::Foundation::WPARAM(ICON_BIG as usize)), Some(windows::Win32::Foundation::LPARAM(icon.0 as isize)));
    }
}
