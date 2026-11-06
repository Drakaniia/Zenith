//! Streaming HTTP(S) GET over WinHTTP for media downloads: custom headers (Referer, Origin,
//! cookies), byte ranges, HTTP/2, and stopping between reads. One `Client` (WinHTTP session) per
//! download job, so connections are pooled across a stream's segments.
//!
//! Redirects are followed here rather than by WinHTTP, so each hop gets its own headers (a
//! site's cookies never travel to the host it redirects to), passes the client's address policy,
//! and never downgrades from HTTPS to HTTP.

use std::ffi::c_void;
use std::io::Write;

use tauri::Url;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The server answered with an error status.
    Status(u32),
    /// Connection, TLS, DNS, timeout or a truncated body.
    Network(String),
    /// Writing the data failed (disk full, access denied).
    Io(String),
    /// The caller asked to stop.
    Stopped,
    /// The address is not allowed (a public page pointing into the local network).
    Blocked,
}

impl Error {
    /// Worth another attempt after a short wait.
    pub fn retryable(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Status(s) => *s == 408 || *s == 425 || *s == 429 || *s >= 500,
            _ => false,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Status(s @ (401 | 403)) => write!(f, "The server refused access (HTTP {s})"),
            Error::Status(404) | Error::Status(410) => write!(f, "The file is no longer available on the server"),
            Error::Status(s) => write!(f, "Server error (HTTP {s})"),
            Error::Network(e) => write!(f, "Network error: {e}"),
            Error::Io(e) => write!(f, "Could not write the file: {e}"),
            Error::Stopped => write!(f, "Stopped"),
            Error::Blocked => write!(f, "Blocked: this page's media points into your local network"),
        }
    }
}

struct Handle(*mut c_void);

// WinHTTP handles may be used from any thread; each request handle is only used by one at a time.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn check(h: *mut c_void, what: &str) -> Result<Handle, Error> {
    if h.is_null() {
        Err(Error::Network(format!("{what} failed: {}", windows::core::Error::from_thread().message())))
    } else {
        Ok(Handle(h))
    }
}

fn net(e: windows::core::Error) -> Error {
    Error::Network(e.message().trim().to_string())
}

/// Removes characters that would let a value break out of its header line.
fn clean(v: &str) -> String {
    v.chars().filter(|c| *c != '\r' && *c != '\n' && *c != '\0').collect()
}

/// Decides which addresses a client may contact (checked on every redirect hop).
pub type Policy = Box<dyn Fn(&str) -> bool + Send + Sync>;

/// Request headers for an address (called per redirect hop, so cookies match each host).
pub type Headers<'a> = &'a dyn Fn(&str) -> Vec<(String, String)>;

const MAX_REDIRECTS: usize = 10;

pub struct Client {
    session: Handle,
    allow: Policy,
}

enum Hop {
    Done(Response),
    Redirect(String),
}

pub struct Response {
    request: Handle,
    _connect: Handle,
    pub status: u32,
    pub content_length: Option<u64>,
    /// Size of the whole resource (from Content-Range for partial responses).
    pub total: Option<u64>,
    /// Address after redirects.
    pub final_url: String,
}

unsafe fn query_string(request: *mut c_void, info: u32) -> Option<String> {
    unsafe {
        let mut len = 0u32;
        let _ = WinHttpQueryHeaders(request, info, PCWSTR::null(), None, &mut len, std::ptr::null_mut());
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize / 2 + 1];
        let mut len2 = (buf.len() * 2) as u32;
        WinHttpQueryHeaders(
            request,
            info,
            PCWSTR::null(),
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut len2,
            std::ptr::null_mut(),
        )
        .ok()?;
        Some(String::from_utf16_lossy(&buf[..(len2 as usize / 2).min(buf.len())]))
    }
}

unsafe fn query_status(request: *mut c_void) -> Option<u32> {
    unsafe {
        let mut v: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut v as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;
        Some(v)
    }
}

impl Client {
    /// A client that may contact any http(s) address (tests; the app always passes a policy).
    #[cfg(test)]
    pub fn new(user_agent: &str) -> Result<Client, Error> {
        Self::with_policy(user_agent, Box::new(|_| true))
    }

    pub fn with_policy(user_agent: &str, allow: Policy) -> Result<Client, Error> {
        let agent = if user_agent.trim().is_empty() { "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" } else { user_agent };
        unsafe {
            let session = check(
                WinHttpOpen(
                    &HSTRING::from(clean(agent)),
                    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                    PCWSTR::null(),
                    PCWSTR::null(),
                    0,
                ),
                "WinHttpOpen",
            )?;
            let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 30_000);
            // HTTP/2 where the server offers it (many CDNs behave better with it).
            let _ = WinHttpSetOption(
                Some(session.0),
                WINHTTP_OPTION_ENABLE_HTTP_PROTOCOL,
                Some(&WINHTTP_PROTOCOL_FLAG_HTTP2.to_ne_bytes()),
            );
            Ok(Client { session, allow })
        }
    }

    /// Starts a GET request, following redirects. `range` is (first byte, last byte inclusive).
    /// Error statuses are returned as `Error::Status`.
    pub fn get(&self, url: &str, headers: Headers, range: Option<(u64, Option<u64>)>, decompress: bool) -> Result<Response, Error> {
        let mut current = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            if !(self.allow)(&current) {
                return Err(Error::Blocked);
            }
            match self.request(&current, &headers(&current), range, decompress)? {
                Hop::Done(mut r) => {
                    r.final_url = current;
                    return Ok(r);
                }
                Hop::Redirect(location) => {
                    let next = Url::parse(&current)
                        .and_then(|base| base.join(location.trim()))
                        .map_err(|e| Error::Network(format!("bad redirect: {e}")))?;
                    if current.starts_with("https:") && next.scheme() != "https" {
                        return Err(Error::Network("the server redirected to an insecure address".into()));
                    }
                    current = next.to_string();
                }
            }
        }
        Err(Error::Network("too many redirects".into()))
    }

    /// One request without following redirects.
    fn request(
        &self,
        url: &str,
        headers: &[(String, String)],
        range: Option<(u64, Option<u64>)>,
        decompress: bool,
    ) -> Result<Hop, Error> {
        let u = Url::parse(url).map_err(|e| Error::Network(format!("bad address: {e}")))?;
        let secure = match u.scheme() {
            "https" => true,
            "http" => false,
            other => return Err(Error::Network(format!("unsupported scheme {other}"))),
        };
        let host = u.host_str().ok_or_else(|| Error::Network("missing host".into()))?.to_string();
        let port = u.port_or_known_default().unwrap_or(if secure { 443 } else { 80 });
        let mut object = u.path().to_string();
        if let Some(q) = u.query() {
            object.push('?');
            object.push_str(q);
        }
        unsafe {
            let connect = check(WinHttpConnect(self.session.0, &HSTRING::from(host.as_str()), port, 0), "WinHttpConnect")?;
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
            // Cookies come from the browser profile (passed in `headers`), never from WinHTTP, and
            // redirects are followed by `get` so every hop gets headers of its own.
            let disabled: u32 = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_REDIRECTS;
            let _ = WinHttpSetOption(Some(request.0), WINHTTP_OPTION_DISABLE_FEATURE, Some(&disabled.to_ne_bytes()));
            if decompress {
                let d: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
                let _ = WinHttpSetOption(Some(request.0), WINHTTP_OPTION_DECOMPRESSION, Some(&d.to_ne_bytes()));
            }
            let mut lines = String::from("Accept: */*\r\n");
            for (k, v) in headers {
                let (k, v) = (clean(k), clean(v));
                if !k.is_empty() && !v.is_empty() {
                    lines.push_str(&format!("{k}: {v}\r\n"));
                }
            }
            match range {
                Some((start, Some(end))) => lines.push_str(&format!("Range: bytes={start}-{end}\r\n")),
                Some((start, None)) => lines.push_str(&format!("Range: bytes={start}-\r\n")),
                None => {}
            }
            let wide: Vec<u16> = lines.encode_utf16().collect();
            WinHttpAddRequestHeaders(request.0, &wide, WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE).map_err(net)?;
            WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(net)?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(net)?;

            let status = query_status(request.0).unwrap_or(0);
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                return match query_string(request.0, WINHTTP_QUERY_LOCATION).filter(|l| !l.trim().is_empty()) {
                    Some(location) => Ok(Hop::Redirect(location)),
                    None => Err(Error::Status(status)),
                };
            }
            if !(200..300).contains(&status) {
                return Err(Error::Status(status));
            }
            // Parsed from text: lengths above 4 GB do not fit the 32-bit numeric query.
            let content_length =
                query_string(request.0, WINHTTP_QUERY_CONTENT_LENGTH).and_then(|v| v.trim().parse::<u64>().ok());
            let total = if status == 206 {
                query_string(request.0, WINHTTP_QUERY_CONTENT_RANGE)
                    .and_then(|r| super::detect::content_range_total(&r))
            } else {
                content_length
            };
            Ok(Hop::Done(Response {
                final_url: String::new(),
                request,
                _connect: connect,
                status,
                content_length,
                total,
            }))
        }
    }

    /// Fetches a small text document (playlist, manifest).
    pub fn text(&self, url: &str, headers: Headers, max: usize) -> Result<(String, String), Error> {
        let mut r = self.get(url, headers, None, true)?;
        let body = r.bytes(max)?;
        Ok((String::from_utf8_lossy(&body).into_owned(), r.final_url.clone()))
    }
}

impl Response {
    /// Reads the next chunk; 0 at the end of the body.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut read = 0u32;
        unsafe { WinHttpReadData(self.request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) }
            .map_err(net)?;
        Ok(read as usize)
    }

    /// The whole body, refusing anything larger than `max`.
    pub fn bytes(&mut self, max: usize) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = self.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
            if out.len() > max {
                return Err(Error::Network("the response is too large".into()));
            }
        }
        Ok(out)
    }

    /// Streams the body into `out`, skipping the first `skip` bytes and stopping after `take`
    /// bytes (for servers that ignore a range request). Checks `stop` between reads and reports
    /// every written chunk to `progress`. Returns the number of bytes written.
    pub fn copy_to(
        &mut self,
        out: &mut dyn Write,
        mut skip: u64,
        take: Option<u64>,
        stop: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, Error> {
        let mut buf = vec![0u8; 256 * 1024];
        let mut written = 0u64;
        let mut seen = 0u64;
        loop {
            if stop() {
                return Err(Error::Stopped);
            }
            if take.is_some_and(|t| written >= t) {
                break;
            }
            let n = self.read(&mut buf)?;
            if n == 0 {
                break;
            }
            seen += n as u64;
            let mut chunk = &buf[..n];
            if skip > 0 {
                let s = skip.min(chunk.len() as u64) as usize;
                chunk = &chunk[s..];
                skip -= s as u64;
            }
            if let Some(t) = take {
                let left = (t - written).min(chunk.len() as u64) as usize;
                chunk = &chunk[..left];
            }
            if !chunk.is_empty() {
                out.write_all(chunk).map_err(|e| Error::Io(e.to_string()))?;
                written += chunk.len() as u64;
                progress(chunk.len() as u64);
            }
        }
        if take.is_none() {
            if let Some(len) = self.content_length {
                if seen < len {
                    return Err(Error::Network("the connection closed before the download finished".into()));
                }
            }
        } else if take.is_some_and(|t| written < t) {
            return Err(Error::Network("the server sent less data than expected".into()));
        }
        Ok(written)
    }
}
