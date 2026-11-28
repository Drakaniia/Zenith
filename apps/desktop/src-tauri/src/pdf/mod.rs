//! The built-in PDF viewer and editor (`zenith://pdf/`).
//!
//! When a tab's main frame receives a PDF (`host/tab/requests.rs`), the load is redirected to
//! the viewer's waiting page and the same address is downloaded by CEF itself, with the page's
//! cookies (`host/downloads.rs`). A local file is used where it is; a web PDF goes to a
//! temporary file. Either way the file gets a random token and the tab loads the viewer, an
//! untrusted page with no IPC bridge, which reads and saves the file by that token only
//! (`scheme.rs`).

pub mod scheme;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

pub const HOST: &str = "pdf";
pub const VIEWER_URL: &str = "zenith://pdf/";

#[derive(Debug, Clone)]
pub struct Doc {
    pub path: PathBuf,
    /// The file is the user's own (opened from disk), so Save may overwrite it.
    pub local: bool,
}

/// A web PDF being saved to a temporary file, by CEF download id.
#[derive(Debug, Clone)]
pub struct Capture {
    pub src: String,
    pub name: String,
    pub path: PathBuf,
}

static DOCS: LazyLock<Mutex<HashMap<String, Doc>>> = LazyLock::new(Default::default);
static CAPTURES: LazyLock<Mutex<HashMap<u32, Capture>>> = LazyLock::new(Default::default);
/// Addresses whose download the viewer asked for (other PDF downloads stay downloads), with
/// when. That download's own response passes the PDF check again and must not start another.
static REQUESTED: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
const REQUEST_TTL: Duration = Duration::from_secs(30);

fn token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("the OS random generator failed");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Whether a response is a PDF.
pub fn is_pdf(mime: &str) -> bool {
    mime.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/pdf")
}

/// Asks for `src` to be fetched for the viewer; returns the waiting page to show meanwhile, or
/// None when that fetch is already under way.
pub fn request(src: &str) -> Option<String> {
    let mut r = REQUESTED.lock().ok()?;
    r.retain(|_, at| at.elapsed() < REQUEST_TTL);
    if r.contains_key(src) {
        return None;
    }
    r.insert(src.to_string(), Instant::now());
    Some(format!("{VIEWER_URL}?src={}", enc(src)))
}

/// Whether a download is one the viewer asked for (and forgets the request).
pub fn take_request(src: &str) -> bool {
    REQUESTED.lock().is_ok_and(|mut r| r.remove(src).is_some_and(|at| at.elapsed() < REQUEST_TTL))
}

/// Downloads started by the viewer itself (Save As / Download of the edited file).
pub fn is_viewer_blob(uri: &str) -> bool {
    uri.starts_with("blob:zenith://pdf/")
}

/// The local path of a `file:` URL.
pub fn file_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok().filter(|u| u.scheme() == "file")?.to_file_path().ok()
}

/// Registers a file and returns the viewer address for it.
pub fn open(path: PathBuf, local: bool, src: &str, name: &str) -> String {
    let t = token();
    if let Ok(mut docs) = DOCS.lock() {
        docs.insert(t.clone(), Doc { path, local });
    }
    format!("{VIEWER_URL}?t={t}&src={}&name={}", enc(src), enc(name))
}

pub fn doc(token: &str) -> Option<Doc> {
    DOCS.lock().ok()?.get(token).cloned()
}

fn capture_dir() -> PathBuf {
    static CLEAN: Once = Once::new();
    let dir = std::env::temp_dir().join("zenith-pdf");
    // Files from earlier runs are no longer referenced by any token.
    CLEAN.call_once(|| {
        let _ = std::fs::remove_dir_all(&dir);
    });
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Starts capturing a web PDF; returns the temporary path CEF should save it to.
pub fn start_capture(cef_id: u32, src: String, name: String) -> PathBuf {
    let path = capture_dir().join(format!("{}.pdf", token()));
    if let Ok(mut c) = CAPTURES.lock() {
        c.insert(cef_id, Capture { src, name, path: path.clone() });
    }
    path
}

pub fn is_capture(cef_id: u32) -> bool {
    CAPTURES.lock().is_ok_and(|c| c.contains_key(&cef_id))
}

/// Ends a capture and returns the viewer address: the document, or the failure.
pub fn finish_capture(cef_id: u32, complete: bool) -> Option<String> {
    let cap = CAPTURES.lock().ok()?.remove(&cef_id)?;
    if !complete {
        let _ = std::fs::remove_file(&cap.path);
        return Some(format!("{VIEWER_URL}?failed=1&src={}", enc(&cap.src)));
    }
    Some(open(cap.path, false, &cap.src, &cap.name))
}

/// Writes edited bytes over a local document. Refuses anything that is not a PDF.
pub fn save(token: &str, bytes: &[u8]) -> Result<(), String> {
    let d = doc(token).ok_or("Unknown document")?;
    if !d.local {
        return Err("This PDF is not a file on this computer".into());
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("Not a PDF".into());
    }
    crate::storage::write_atomic(&d.path, bytes).map_err(|e| e.to_string())
}

/// The token of a `zenith://pdf/<kind>/<token>` path.
pub fn path_token<'a>(path: &'a str, kind: &str) -> Option<&'a str> {
    let t = path.strip_prefix(kind)?.strip_prefix('/')?;
    (t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit())).then_some(t)
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document.pdf".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pdfs() {
        assert!(is_pdf("application/pdf"));
        assert!(is_pdf("Application/PDF; charset=binary"));
        assert!(!is_pdf("text/html"));
    }

    #[test]
    fn only_requested_downloads_are_taken_over() {
        let src = "https://example.com/requested.pdf";
        assert_eq!(request(src).as_deref(), Some("zenith://pdf/?src=https%3A%2F%2Fexample.com%2Frequested.pdf"));
        assert_eq!(request(src), None, "the download's own response must not start another");
        assert!(take_request(src));
        assert!(!take_request(src));
        assert!(!take_request("https://example.com/other.pdf"));
    }

    #[test]
    fn parses_tokens() {
        let t = "0123456789abcdef0123456789abcdef";
        assert_eq!(path_token(&format!("doc/{t}"), "doc"), Some(t));
        assert_eq!(path_token("doc/../secret", "doc"), None);
        assert_eq!(path_token(&format!("save/{t}"), "doc"), None);
    }

    #[test]
    fn viewer_url_encodes_source() {
        let url = open(PathBuf::from("C:\\a b.pdf"), true, "file:///C:/a%20b.pdf", "a b.pdf");
        assert!(url.starts_with("zenith://pdf/?t="));
        assert!(url.ends_with("&src=file%3A%2F%2F%2FC%3A%2Fa%2520b.pdf&name=a+b.pdf"));
    }

    #[test]
    fn save_refuses_web_docs_and_non_pdfs() {
        let dir = std::env::temp_dir().join(format!("zenith-pdf-test-{}", token()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.pdf");
        std::fs::write(&path, b"%PDF-1.7 old").unwrap();
        let url = open(path.clone(), true, "", "x.pdf");
        let t = &url["zenith://pdf/?t=".len()..][..32];
        assert!(save(t, b"<html>").is_err());
        save(t, b"%PDF-1.7 new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"%PDF-1.7 new");
        let web = open(path.clone(), false, "https://x", "x.pdf");
        assert!(save(&web["zenith://pdf/?t=".len()..][..32], b"%PDF-1.7").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn local_paths_from_file_urls() {
        assert_eq!(file_path("file:///C:/docs/a%20b.pdf"), Some(PathBuf::from("C:\\docs\\a b.pdf")));
        assert_eq!(file_path("https://x/a.pdf"), None);
    }
}
