//! The `zenith://pdf/` handler: the viewer page and its assets from the built UI, the document
//! bytes by token (`doc/<token>`), and in-place saves (`POST save/<token>`). Document requests
//! are answered only to the viewer itself, so a page that learns a token still can't use it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cef::wrapper::byte_read_handler::{ByteReadHandler, ByteStream};
use cef::wrapper::stream_resource_handler::StreamResourceHandler;
use cef::*;

use super::VIEWER_URL;
use crate::app::bridge::mime_for_path;
use crate::app::security::resolve_asset_path;
use crate::host::cef_app::cef_str;

fn respond(status: i32, mime: &str, bytes: Vec<u8>) -> Option<ResourceHandler> {
    let mut reader = ByteReadHandler::new(Arc::new(Mutex::new(ByteStream::new(bytes))));
    let stream = stream_reader_create_for_handler(Some(&mut reader))?;
    let text = if status == 200 { "OK" } else { "Error" };
    Some(StreamResourceHandler::new(status, text.into(), mime.into(), None, Some(stream)))
}

fn post_body(request: &Request) -> Vec<u8> {
    let Some(data) = request.post_data() else { return Vec::new() };
    let mut elements = vec![None; data.element_count()];
    data.elements(Some(&mut elements));
    let mut out = Vec::new();
    for el in elements.into_iter().flatten() {
        let n = el.bytes_count();
        let start = out.len();
        out.resize(start + n, 0);
        let got = el.bytes(n, out[start..].as_mut_ptr());
        out.truncate(start + got);
    }
    out
}

wrap_scheme_handler_factory! {
    pub struct PdfSchemeFactory {
        ui_dir: PathBuf,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request = request?;
            let url = cef_str(request.url());
            let path = url.strip_prefix(VIEWER_URL)?.split(['?', '#']).next().unwrap_or("");
            if path.is_empty() {
                let page = std::fs::read(self.ui_dir.join("pdf.html")).ok()?;
                return respond(200, "text/html", page);
            }
            if path.starts_with("assets/") || path.starts_with("pdfjs/") {
                let file = resolve_asset_path(&self.ui_dir, &format!("zenith://app/{path}"))?;
                let bytes = std::fs::read(&file).ok()?;
                return respond(200, mime_for_path(path), bytes);
            }
            let from_viewer = frame.is_some_and(|f| cef_str(f.url()).starts_with(VIEWER_URL));
            if !from_viewer {
                return None;
            }
            if let Some(t) = super::path_token(path, "doc") {
                let bytes = std::fs::read(super::doc(t)?.path).ok()?;
                return respond(200, "application/pdf", bytes);
            }
            if let Some(t) = super::path_token(path, "save") {
                if !cef_str(request.method()).eq_ignore_ascii_case("POST") {
                    return None;
                }
                return match super::save(t, &post_body(request)) {
                    Ok(()) => respond(200, "text/plain", b"saved".to_vec()),
                    Err(e) => respond(500, "text/plain", e.into_bytes()),
                };
            }
            None
        }
    }
}

pub fn register(ui_dir: PathBuf) {
    let mut factory = PdfSchemeFactory::new(ui_dir);
    register_scheme_handler_factory(
        Some(&CefString::from(crate::app::security::TRUSTED_SCHEME)),
        Some(&CefString::from(super::HOST)),
        Some(&mut factory),
    );
}
