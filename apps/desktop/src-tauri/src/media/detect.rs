//! Deciding which network responses and page elements are downloadable media, and the scripts
//! that find media in pages. Pure functions; the WebView2 wiring is in `sniff`.

use super::hls::path_ext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sniffed {
    Hls,
    Dash,
    File,
}

const HLS_MIMES: [&str; 6] = [
    "application/vnd.apple.mpegurl",
    "application/x-mpegurl",
    "application/mpegurl",
    "audio/mpegurl",
    "audio/x-mpegurl",
    "video/x-mpegurl",
];

/// Pieces of adaptive streams; never offered on their own.
const SEGMENT_MIMES: [&str; 4] = ["video/mp2t", "video/iso.segment", "audio/iso.segment", "video/vnd.dlna.mpeg-tts"];
const SEGMENT_EXTS: [&str; 7] = ["ts", "m4s", "m4f", "cmfv", "cmfa", "cmft", "m4i"];

/// Extensions of complete media files.
pub const FILE_EXTS: [&str; 21] = [
    "mp4", "m4v", "webm", "mov", "mkv", "ogv", "ogg", "oga", "mp3", "m4a", "aac", "flac", "wav", "opus", "3gp",
    "avi", "wmv", "flv", "mpg", "mpeg", "weba",
];
const AUDIO_EXTS: [&str; 9] = ["mp3", "m4a", "aac", "flac", "wav", "opus", "oga", "ogg", "weba"];

/// Files below this size are interface sounds and similar, not something worth saving.
pub const MIN_FILE_BYTES: u64 = 64 * 1024;

/// The MIME type without parameters, lower-cased.
pub fn mime(content_type: &str) -> String {
    content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Total size from a `Content-Range` header (`bytes 0-1023/52428800`).
pub fn content_range_total(v: &str) -> Option<u64> {
    v.rsplit('/').next()?.trim().parse().ok()
}

/// Classifies a response. `media_context` is true when a `<video>`/`<audio>` element made the
/// request; `status` is the HTTP status; `total` the full resource size when known.
pub fn classify(url: &str, content_type: &str, media_context: bool, status: u16, total: Option<u64>) -> Option<Sniffed> {
    if !(200..300).contains(&status) {
        return None;
    }
    let ct = mime(content_type);
    let ext = path_ext(url);
    let textual = ct.starts_with("text/html") || ct.starts_with("image/") || ct == "application/json" || ct.starts_with("text/css");
    if HLS_MIMES.contains(&ct.as_str()) || (ext == "m3u8" && !textual) {
        return Some(Sniffed::Hls);
    }
    if ct == "application/dash+xml" || (ext == "mpd" && !textual) {
        return Some(Sniffed::Dash);
    }
    if SEGMENT_MIMES.contains(&ct.as_str()) || SEGMENT_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let av = ct.starts_with("video/") || ct.starts_with("audio/");
    let generic = ct.is_empty() || ct == "application/octet-stream" || ct == "binary/octet-stream";
    if !(av || (generic && FILE_EXTS.contains(&ext.as_str()))) {
        return None;
    }
    if total.is_some_and(|t| t < MIN_FILE_BYTES) {
        return None;
    }
    if media_context {
        return Some(Sniffed::File);
    }
    // Requested by a script (Media Source players): only whole files read in ranges are files;
    // anything else is a stream segment.
    (status == 206 && total.is_some_and(|t| t >= 1 << 20)).then_some(Sniffed::File)
}

/// Extension and audio-only flag for a progressive file.
pub fn file_ext(content_type: &str, url: &str) -> (String, bool) {
    let ct = mime(content_type);
    let by_mime = match ct.as_str() {
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/ogg" => "ogv",
        "video/quicktime" => "mov",
        "video/x-matroska" | "video/matroska" => "mkv",
        "video/x-flv" => "flv",
        "video/3gpp" => "3gp",
        "video/x-msvideo" => "avi",
        "video/x-ms-wmv" => "wmv",
        "video/mpeg" => "mpg",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" | "audio/aacp" => "aac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/webm" => "weba",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        _ => "",
    };
    let url_ext = path_ext(url);
    let ext = if !by_mime.is_empty() {
        by_mime.to_string()
    } else if FILE_EXTS.contains(&url_ext.as_str()) {
        url_ext
    } else if ct.starts_with("audio/") {
        "mp3".to_string()
    } else {
        "mp4".to_string()
    };
    let audio = ct.starts_with("audio/") || (!ct.starts_with("video/") && AUDIO_EXTS.contains(&ext.as_str()));
    (ext, audio)
}

/// Loopback, private, link-local, carrier-grade NAT or unspecified addresses, and local names.
/// A public page must not make Zenith contact these on its own (Chromium's Private Network
/// Access rules would stop the page itself).
pub fn is_private_host(host: &str) -> bool {
    use std::net::IpAddr;
    let h = host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.').to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") || h.ends_with(".internal") || !h.contains('.') && !h.contains(':') {
        return true;
    }
    let v4 = |ip: std::net::Ipv4Addr| {
        let o = ip.octets();
        ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || (o[0] == 100 && (o[1] & 0xC0) == 64)
    };
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => v4(ip),
        Ok(IpAddr::V6(ip)) => {
            let s = ip.segments();
            ip.is_loopback() || ip.is_unspecified() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || ip.to_ipv4_mapped().is_some_and(v4)
        }
        Err(_) => false,
    }
}

/// What a URL found in the page (not on the network) most likely is, from its extension.
pub fn guess_from_url(url: &str, meta: bool) -> Option<Sniffed> {
    let ext = path_ext(url);
    match ext.as_str() {
        "m3u8" => Some(Sniffed::Hls),
        "mpd" => Some(Sniffed::Dash),
        e if FILE_EXTS.contains(&e) => Some(Sniffed::File),
        // A <video>/<audio> source plays as media whatever its address looks like; an
        // `og:video` tag often points at a player page instead.
        _ if !meta => Some(Sniffed::File),
        _ => None,
    }
}

/// Finds media in the current document (run with ExecuteScript in the page's main frame). The
/// result is treated as untrusted input.
pub const SCAN_SCRIPT: &str = r#"(() => {
  try {
    const abs = (u) => { try { return new URL(u, document.baseURI).href; } catch { return ''; } };
    const items = []; const seen = new Set(); let blobs = 0, drm = 0;
    const push = (o) => { if (!o.url || seen.has(o.url) || items.length >= 60) return; seen.add(o.url); items.push(o); };
    const roots = [document];
    const walker = document.createTreeWalker(document.documentElement || document, NodeFilter.SHOW_ELEMENT);
    for (let n = walker.nextNode(), k = 0; n && k < 20000 && roots.length < 200; n = walker.nextNode(), k++) {
      if (n.shadowRoot) roots.push(n.shadowRoot);
    }
    for (const root of roots) {
      for (const el of root.querySelectorAll('video, audio')) {
        const meta = {
          audio: el.tagName === 'AUDIO',
          duration: Number.isFinite(el.duration) ? el.duration : 0,
          width: el.videoWidth || 0,
          height: el.videoHeight || 0,
          title: String(el.getAttribute('title') || el.getAttribute('aria-label') || '').slice(0, 200),
          playing: !el.paused,
          meta: false,
          type: '',
        };
        if (el.mediaKeys) drm++;
        const src = el.currentSrc || el.src || '';
        if (/^(blob|mediasource):/i.test(src)) { blobs++; continue; }
        if (/^https?:/i.test(src)) push({ ...meta, url: abs(src) });
        for (const s of el.querySelectorAll('source[src]')) {
          const u = abs(s.getAttribute('src'));
          if (/^https?:/i.test(u)) push({ ...meta, url: u, type: String(s.getAttribute('type') || '').slice(0, 100) });
        }
      }
    }
    const sel = 'meta[property="og:video"],meta[property="og:video:url"],meta[property="og:video:secure_url"],meta[name="twitter:player:stream"],meta[property="og:audio"],meta[property="og:audio:secure_url"]';
    for (const m of document.querySelectorAll(sel)) {
      const u = abs(m.getAttribute('content') || '');
      const p = m.getAttribute('property') || m.getAttribute('name') || '';
      if (/^https?:/i.test(u)) push({ url: u, audio: /audio/.test(p), duration: 0, width: 0, height: 0, title: '', playing: false, meta: true, type: '' });
    }
    return { title: String(document.title || '').slice(0, 300), url: location.href, blobs, drm, items };
  } catch (e) {
    return { title: '', url: '', blobs: 0, drm: 0, items: [] };
  }
})()"#;

/// Injected into every frame when the "download button on videos" setting is on. It draws a small
/// button over a hovered video and, on a real click, calls `window.open` with a reserved name
/// that the host recognises (`_zenith_media`) and cancels; pages get no way to reach the app.
/// Styles are set through CSSOM so page CSP rules for inline styles do not apply.
pub const BUTTON_SCRIPT: &str = r#"(() => {
  try {
    const open = window.open.bind(window);
    const MIN_W = 220, MIN_H = 124;
    let host = null, button = null, current = null, hideTimer = 0, last = 0;
    const set = (el, props) => { for (const k in props) el.style.setProperty(k, props[k], 'important'); };
    const hide = () => { if (host) set(host, { display: 'none' }); current = null; };
    const build = () => {
      host = document.createElement('zenith-media-button');
      set(host, { all: 'initial', position: 'fixed', 'z-index': '2147483647', display: 'none', left: '0', top: '0' });
      const root = host.attachShadow({ mode: 'closed' });
      button = document.createElement('button');
      button.type = 'button';
      button.title = 'Download this video with Zenith';
      button.setAttribute('aria-label', 'Download video');
      const rest = { display: 'flex', 'align-items': 'center', gap: '6px', height: '30px', margin: '0', padding: '0 12px 0 10px', border: '0', 'border-radius': '15px', background: 'rgba(18,18,22,.78)', color: '#fff', font: '600 12.5px/1 "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif', cursor: 'pointer', 'box-shadow': '0 2px 12px rgba(0,0,0,.35)', 'backdrop-filter': 'blur(8px)', opacity: '.9', transition: 'opacity .12s, background .12s' };
      set(button, rest);
      const ns = 'http://www.w3.org/2000/svg';
      const svg = document.createElementNS(ns, 'svg');
      for (const [k, v] of Object.entries({ viewBox: '0 0 24 24', width: '15', height: '15', fill: 'none', stroke: 'currentColor', 'stroke-width': '2.2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' })) svg.setAttribute(k, v);
      for (const d of ['M12 3v12', 'M7 10.5l5 5 5-5', 'M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2']) {
        const p = document.createElementNS(ns, 'path');
        p.setAttribute('d', d);
        svg.appendChild(p);
      }
      button.append(svg, document.createTextNode('Download'));
      button.addEventListener('pointerenter', () => set(button, { opacity: '1', background: 'rgba(18,18,22,.94)' }));
      button.addEventListener('pointerleave', () => set(button, { opacity: '.9', background: 'rgba(18,18,22,.78)' }));
      for (const t of ['pointerdown', 'mousedown', 'mouseup', 'pointerup', 'dblclick']) button.addEventListener(t, (e) => e.stopPropagation());
      button.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (!e.isTrusted) return;
        const src = current ? (current.currentSrc || current.src || '') : '';
        open(/^https?:/i.test(src) ? src : location.href, '_zenith_media');
        hide();
      });
      root.appendChild(button);
    };
    const show = (v) => {
      if (!host) build();
      if (!host.isConnected) (document.body || document.documentElement).appendChild(host);
      const r = v.getBoundingClientRect();
      set(host, { display: 'block' });
      const w = button.offsetWidth || 104;
      set(host, { left: Math.max(4, r.right - w - 12) + 'px', top: Math.max(4, r.top + 12) + 'px' });
      current = v;
    };
    const hit = (x, y) => {
      if (document.fullscreenElement) return null;
      for (const v of document.getElementsByTagName('video')) {
        const r = v.getBoundingClientRect();
        if (r.width >= MIN_W && r.height >= MIN_H && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return v;
      }
      return null;
    };
    addEventListener('pointermove', (e) => {
      if (e.pointerType === 'touch' || e.timeStamp - last < 90) return;
      last = e.timeStamp;
      const v = hit(e.clientX, e.clientY);
      if (v) {
        clearTimeout(hideTimer);
        hideTimer = 0;
        show(v);
      } else if (current && !hideTimer && !(host && e.composedPath().includes(host))) {
        hideTimer = setTimeout(() => { hideTimer = 0; hide(); }, 700);
      }
    }, { capture: true, passive: true });
    addEventListener('scroll', () => { if (current) hide(); }, { capture: true, passive: true });
    document.addEventListener('fullscreenchange', hide);
  } catch (e) {}
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_are_found_by_type_or_extension() {
        let hls = Some(Sniffed::Hls);
        assert_eq!(classify("https://a/b/master.m3u8", "application/vnd.apple.mpegurl", false, 200, None), hls);
        assert_eq!(classify("https://a/b/playlist?id=1", "application/x-mpegURL; charset=UTF-8", false, 200, None), hls);
        assert_eq!(classify("https://a/b/index.m3u8?t=1", "text/plain", false, 200, None), hls);
        assert_eq!(classify("https://a/b/index.m3u8", "text/html", false, 200, None), None);
        assert_eq!(classify("https://a/b/manifest.mpd", "application/dash+xml", false, 200, None), Some(Sniffed::Dash));
        assert_eq!(classify("https://a/b/x", "application/dash+xml", false, 200, None), Some(Sniffed::Dash));
        assert_eq!(classify("https://a/b/manifest.mpd", "application/dash+xml", false, 404, None), None);
    }

    #[test]
    fn element_media_is_a_file_but_segments_are_not() {
        assert_eq!(classify("https://a/v.mp4", "video/mp4", true, 206, Some(50_000_000)), Some(Sniffed::File));
        assert_eq!(classify("https://a/song", "audio/mpeg", true, 200, None), Some(Sniffed::File));
        assert_eq!(classify("https://a/click.mp3", "audio/mpeg", true, 200, Some(9_000)), None, "interface sounds");
        assert_eq!(classify("https://a/seg-1.ts", "video/mp2t", true, 200, None), None);
        assert_eq!(classify("https://a/seg-1.m4s", "video/mp4", false, 200, Some(3_000_000)), None);
        assert_eq!(classify("https://a/chunk?n=3", "video/mp4", false, 200, Some(3_000_000)), None, "MSE segment");
        assert_eq!(classify("https://a/full.mp4", "video/mp4", false, 206, Some(80_000_000)), Some(Sniffed::File));
        assert_eq!(classify("https://a/x.mp4", "application/octet-stream", true, 200, None), Some(Sniffed::File));
        assert_eq!(classify("https://a/x.bin", "application/octet-stream", true, 200, None), None);
        assert_eq!(classify("https://a/page", "text/html", true, 200, None), None);
    }

    #[test]
    fn ranges_and_extensions() {
        assert_eq!(content_range_total("bytes 0-1023/52428800"), Some(52_428_800));
        assert_eq!(content_range_total("bytes */1234"), Some(1234));
        assert_eq!(content_range_total("bytes 0-1/*"), None);
        assert_eq!(file_ext("video/webm", "https://a/x.mp4"), ("webm".into(), false));
        assert_eq!(file_ext("application/octet-stream", "https://a/clip.MOV?x=1"), ("mov".into(), false));
        assert_eq!(file_ext("audio/mp4", "https://a/a"), ("m4a".into(), true));
        assert_eq!(file_ext("", "https://a/track.flac"), ("flac".into(), true));
        assert_eq!(file_ext("video/x-unknown", "https://a/v"), ("mp4".into(), false));
    }

    #[test]
    fn private_hosts() {
        for h in ["localhost", "127.0.0.1", "10.1.2.3", "192.168.1.1", "172.20.0.5", "169.254.1.1", "100.64.0.1", "0.0.0.0", "::1", "[fd00::1]", "fe80::1", "router", "nas.local", "printer.internal", "::ffff:192.168.0.1"] {
            assert!(is_private_host(h), "{h}");
        }
        for h in ["example.com", "cdn.example.co.uk", "8.8.8.8", "100.128.0.1", "2001:4860::8888", "[2001:db8::1]"] {
            assert!(!is_private_host(h), "{h}");
        }
    }

    #[test]
    fn page_urls_are_guessed_by_extension() {
        assert_eq!(guess_from_url("https://a/v/master.m3u8", true), Some(Sniffed::Hls));
        assert_eq!(guess_from_url("https://a/v.mp4", true), Some(Sniffed::File));
        assert_eq!(guess_from_url("https://a/embed/player?id=4", true), None);
        assert_eq!(guess_from_url("https://a/stream?id=4", false), Some(Sniffed::File));
    }
}
