//! Minimal HTTPS GET over WinHTTP (ships with Windows, so it adds no TLS stack to the binary).
//! Used only for opt-in search suggestions and filter-list updates.

use std::ffi::c_void;

use tauri::Url;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn check(h: *mut c_void, what: &str) -> Result<Handle, String> {
    if h.is_null() {
        Err(format!("{what} failed: {}", windows::core::Error::from_thread()))
    } else {
        Ok(Handle(h))
    }
}

/// Performs a blocking GET request. Call from a background thread.
pub fn get(url: &str, max_bytes: usize, timeout_ms: i32) -> Result<Vec<u8>, String> {
    fetch(url, max_bytes, timeout_ms, true)
}

/// Like `get`, but a redirect is an error: for fetches whose address was only checked as given
/// (a redirect could lead to a private network address).
pub fn get_no_redirects(url: &str, max_bytes: usize, timeout_ms: i32) -> Result<Vec<u8>, String> {
    fetch(url, max_bytes, timeout_ms, false)
}

fn fetch(url: &str, max_bytes: usize, timeout_ms: i32, follow_redirects: bool) -> Result<Vec<u8>, String> {
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    let secure = match url.scheme() {
        "https" => true,
        "http" => false,
        other => return Err(format!("unsupported scheme {other}")),
    };
    let host = url.host_str().ok_or("missing host")?.to_string();
    let port = url.port_or_known_default().unwrap_or(if secure { 443 } else { 80 });
    let mut object = url.path().to_string();
    if let Some(q) = url.query() {
        object.push('?');
        object.push_str(q);
    }

    unsafe {
        let session = check(
            WinHttpOpen(
                &HSTRING::from("Zenith/0.1 (+https://zenith.invalid)"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ),
            "WinHttpOpen",
        )?;
        let _ = WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
        let connect = check(
            WinHttpConnect(session.0, &HSTRING::from(host.as_str()), port, 0),
            "WinHttpConnect",
        )?;
        let flags = if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let request = check(
            WinHttpOpenRequest(
                connect.0,
                &HSTRING::from("GET"),
                &HSTRING::from(object.as_str()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                flags,
            ),
            "WinHttpOpenRequest",
        )?;
        let decompression: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
        let _ = WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DECOMPRESSION,
            Some(&decompression.to_ne_bytes()),
        );
        if !follow_redirects {
            let never: u32 = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
            WinHttpSetOption(Some(request.0), WINHTTP_OPTION_REDIRECT_POLICY, Some(&never.to_ne_bytes())).map_err(|e| e.to_string())?;
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| e.to_string())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| e.to_string())?;

        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|e| e.to_string())?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available).map_err(|e| e.to_string())?;
            if available == 0 {
                break;
            }
            let start = body.len();
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                body[start..].as_mut_ptr() as *mut c_void,
                available,
                &mut read,
            )
            .map_err(|e| e.to_string())?;
            body.truncate(start + read as usize);
            if body.len() > max_bytes {
                return Err("response too large".into());
            }
        }
        Ok(body)
    }
}
