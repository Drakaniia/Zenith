//! Playing a torrent's file while it downloads: a small HTTP server on the loopback address that
//! serves one file of a torrent with byte ranges, so a tab's video player can seek. The engine
//! fetches the pieces a player asks for first.
//!
//! Each address carries a random token, so pages (which can reach 127.0.0.1) can't read torrents.

use std::io::SeekFrom;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OnceCell;

struct Server {
    port: u16,
    token: String,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();

async fn server() -> Result<&'static Server, String> {
    SERVER
        .get_or_try_init(|| async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| e.to_string())?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let mut bytes = [0u8; 16];
            getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
            let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            tokio::spawn(async move {
                while let Ok((conn, _)) = listener.accept().await {
                    tokio::spawn(serve(conn));
                }
            });
            Ok::<_, String>(Server { port, token })
        })
        .await
}

/// The address that plays file `file` of download `id`.
pub async fn url(id: i64, file: usize, name: &str) -> Result<String, String> {
    let s = server().await?;
    let name: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    Ok(format!("http://127.0.0.1:{}/{}/{id}/{file}/{name}", s.port, s.token))
}

/// A request line and the headers this server cares about.
#[derive(Debug, PartialEq)]
pub struct Request {
    pub head_only: bool,
    pub path: String,
    pub range: Option<String>,
}

pub fn parse_request(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?;
    let path = first.next()?.to_string();
    if method != "GET" && method != "HEAD" {
        return None;
    }
    let range = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("range"))
        .map(|(_, v)| v.trim().to_string());
    Some(Request { head_only: method == "HEAD", path, range })
}

/// The `/token/id/file/...` parts of a path.
pub fn parse_path(path: &str) -> Option<(&str, i64, usize)> {
    let mut parts = path.trim_start_matches('/').split('/');
    let token = parts.next()?;
    let id = parts.next()?.parse().ok()?;
    let file = parts.next()?.parse().ok()?;
    Some((token, id, file))
}

/// The inclusive byte range a `Range` header asks for in a file of `len` bytes. Ok(None) for the
/// whole file; Err when it can't be satisfied.
pub fn parse_range(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(h) = header else { return Ok(None) };
    let spec = h.trim().strip_prefix("bytes=").ok_or(())?;
    // Only the first range of a multi-range request (players ask for one).
    let spec = spec.split(',').next().unwrap_or("").trim();
    let (a, b) = spec.split_once('-').ok_or(())?;
    if len == 0 {
        return Err(());
    }
    let (start, end) = if a.is_empty() {
        let suffix: u64 = b.parse().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        (len.saturating_sub(suffix), len - 1)
    } else {
        let start: u64 = a.parse().map_err(|_| ())?;
        let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().map_err(|_| ())?.min(len - 1) };
        (start, end)
    };
    if start > end || start >= len {
        return Err(());
    }
    Ok(Some((start, end)))
}

pub fn content_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "nfo" | "srt" | "vtt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn reply(conn: &mut TcpStream, status: &str) {
    let _ = conn.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await;
}

async fn serve(mut conn: TcpStream) {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 1024];
    let head = loop {
        match conn.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break String::from_utf8_lossy(&buf[..end]).into_owned();
        }
        if buf.len() > 16 * 1024 {
            return reply(&mut conn, "431 Request Header Fields Too Large").await;
        }
    };
    let Some(req) = parse_request(&head) else { return reply(&mut conn, "405 Method Not Allowed").await };
    let Some((token, id, file)) = parse_path(&req.path) else { return reply(&mut conn, "404 Not Found").await };
    let Some(s) = SERVER.get() else { return };
    if !same_token(token, &s.token) {
        return reply(&mut conn, "404 Not Found").await;
    }
    let Some((handle, name)) = super::stream_target(id, file) else { return reply(&mut conn, "404 Not Found").await };
    let mut stream = match handle.stream(file).await {
        Ok(s) => s,
        Err(_) => return reply(&mut conn, "503 Service Unavailable").await,
    };
    let len = stream.len();
    let (status, start, end) = match parse_range(req.range.as_deref(), len) {
        Ok(Some((a, b))) => ("206 Partial Content", a, b),
        Ok(None) => ("200 OK", 0, len.saturating_sub(1)),
        Err(()) => {
            let msg = format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = conn.write_all(msg.as_bytes()).await;
            return;
        }
    };
    let body = if len == 0 { 0 } else { end - start + 1 };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {body}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n",
        content_type(&name)
    );
    if status.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    head.push_str("\r\n");
    if conn.write_all(head.as_bytes()).await.is_err() || req.head_only || body == 0 {
        return;
    }
    if stream.seek(SeekFrom::Start(start)).await.is_err() {
        return;
    }
    let mut limited = stream.take(body);
    let _ = tokio::io::copy(&mut limited, &mut conn).await;
    let _ = conn.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests() {
        let r = parse_request("GET /t/1/0/a.mp4 HTTP/1.1\r\nHost: x\r\nrange: bytes=10-\r\n").unwrap();
        assert_eq!(r, Request { head_only: false, path: "/t/1/0/a.mp4".into(), range: Some("bytes=10-".into()) });
        assert!(parse_request("HEAD /x HTTP/1.1").unwrap().head_only);
        assert_eq!(parse_request("POST /x HTTP/1.1"), None);
        assert_eq!(parse_path("/tok/42/3/name.mkv"), Some(("tok", 42, 3)));
        assert_eq!(parse_path("/tok/x/3"), None);
        assert_eq!(parse_path("/tok/42"), None);
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range(None, 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-"), 100), Ok(Some((0, 99))));
        assert_eq!(parse_range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(parse_range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=-500"), 100), Ok(Some((0, 99))));
        assert_eq!(parse_range(Some("bytes=5-9, 20-30"), 100), Ok(Some((5, 9))));
        assert_eq!(parse_range(Some("bytes=100-"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=20-10"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=-0"), 100), Err(()));
        assert_eq!(parse_range(Some("items=0-1"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=0-"), 0), Err(()));
    }

    #[test]
    fn types_and_tokens() {
        assert_eq!(content_type("A.MP4"), "video/mp4");
        assert_eq!(content_type("song.flac"), "audio/flac");
        assert_eq!(content_type("noext"), "application/octet-stream");
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abc", "abd"));
        assert!(!same_token("abc", "abcd"));
    }
}
