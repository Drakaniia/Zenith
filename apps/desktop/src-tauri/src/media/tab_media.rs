//! `TabMedia`: a tab's list of found media: adding files and manifests, folding HLS media
//! playlists into their master, merging page scans and ordering the panel's items.

use super::choices::{dash_choices, fold_media_info, hls_master_item};
use super::detect::{self, Sniffed};
use super::{dash, hls, Entry, Found, Manifest, MediaItem, Scan, TabMedia};

/// Items kept per tab (pages that load dozens of clips keep the most recent).
const MAX_ENTRIES: usize = 60;
/// Manifests read per page, and at the same time: a hostile page could otherwise make Zenith
/// read and parse an endless stream of playlists.
const MAX_MANIFEST_READS: u32 = 200;
pub(super) const MAX_PENDING_MANIFESTS: usize = 8;

fn now() -> i64 {
    crate::storage::now_ms()
}

impl TabMedia {
    /// A new page: forget everything.
    pub fn reset(&mut self) {
        self.entries.clear();
        self.media_requests.clear();
        self.pending.clear();
        self.manifest_reads = 0;
        self.blobs = 0;
        self.drm = 0;
        self.epoch += 1;
    }

    /// Claims a manifest read, unless too many are running or this page already had its share.
    pub fn begin_manifest(&mut self, url: &str) -> bool {
        if self.pending.len() >= MAX_PENDING_MANIFESTS || self.manifest_reads >= MAX_MANIFEST_READS || self.pending.contains(url) {
            return false;
        }
        self.manifest_reads += 1;
        self.pending.insert(url.to_string());
        true
    }

    pub fn note_media_request(&mut self, url: &str) {
        if url.len() > 4096 || self.media_requests.iter().any(|u| u == url) {
            return;
        }
        self.media_requests.push_back(url.to_string());
        if self.media_requests.len() > 64 {
            self.media_requests.pop_front();
        }
    }

    pub fn is_media_request(&self, url: &str) -> bool {
        self.media_requests.iter().any(|u| u == url)
    }

    fn find_mut(&mut self, url: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.item.url == url)
    }

    /// Already listed, part of a listed master, or being read.
    pub fn knows(&self, url: &str) -> bool {
        self.pending.contains(url) || self.entries.iter().any(|e| e.item.url == url || e.children.iter().any(|c| c == url))
    }

    /// Whether a manifest response is worth reading: not being read, not listed, and not a
    /// variant playlist already folded into its master. A variant seen for the first time is
    /// read even though its master is listed: it carries the duration and live/encrypted state.
    pub fn wants_manifest(&self, url: &str) -> bool {
        !self.pending.contains(url) && !self.entries.iter().any(|e| e.item.url == url || e.folded.iter().any(|c| c == url))
    }

    /// Items shown in the panel.
    pub fn count(&self) -> u32 {
        self.entries.len() as u32
    }

    fn push(&mut self, mut item: MediaItem, manifest: Manifest, found: Found, children: Vec<String>) {
        if self.entries.len() >= MAX_ENTRIES {
            let drop = self.entries.iter().position(|e| matches!(e.manifest, Manifest::None)).unwrap_or(0);
            self.entries.remove(drop);
        }
        self.next_id += 1;
        item.id = self.next_id;
        item.found_by = found.by;
        item.detected = now();
        if item.title.is_empty() {
            item.title = found.title.to_string();
        }
        self.entries.push(Entry {
            item,
            manifest,
            headers: found.headers,
            page_url: found.page_url.to_string(),
            children,
            folded: Vec::new(),
        });
    }

    fn blank(kind: &'static str, url: &str) -> MediaItem {
        MediaItem {
            id: 0,
            kind,
            url: url.to_string(),
            title: String::new(),
            ext: String::new(),
            size: None,
            duration: None,
            width: None,
            height: None,
            audio_only: false,
            live: false,
            protected: false,
            found_by: "network",
            detected: 0,
            variants: Vec::new(),
            audio_tracks: Vec::new(),
            note: String::new(),
        }
    }

    /// Adds a progressive file. Returns true when the list changed.
    pub fn add_file(&mut self, url: &str, content_type: &str, size: Option<u64>, found: Found) -> bool {
        if let Some(e) = self.find_mut(url) {
            if size.is_some() && e.item.size != size {
                e.item.size = size;
                return true;
            }
            return false;
        }
        if self.knows(url) {
            return false;
        }
        let (ext, audio) = detect::file_ext(content_type, url);
        let mut item = Self::blank("file", url);
        item.ext = ext;
        item.audio_only = audio;
        item.size = size;
        self.push(item, Manifest::None, found, Vec::new());
        true
    }

    /// Adds a parsed HLS playlist; media playlists of a known master enrich it instead.
    pub fn add_hls(&mut self, url: &str, playlist: hls::Playlist, found: Found) -> bool {
        match playlist {
            hls::Playlist::Master(m) => {
                if self.find_mut(url).is_some() {
                    return false;
                }
                let children: Vec<String> = m.playlist_uris().map(str::to_string).collect();
                let mut absorbed = Vec::new();
                self.entries.retain(|e| match &e.manifest {
                    Manifest::HlsMedia(p) if children.contains(&e.item.url) => {
                        absorbed.push((e.item.url.clone(), p.clone()));
                        false
                    }
                    _ => true,
                });
                let (variants, audio_tracks, best) = hls_master_item(&m);
                let mut item = Self::blank("hls", url);
                item.ext = "mp4".into();
                item.width = best.map(|b| b.0);
                item.height = best.map(|b| b.1);
                item.audio_only = !m.variants.is_empty() && m.variants.iter().all(|v| v.audio_only());
                item.protected = m.encrypted;
                item.variants = variants;
                item.audio_tracks = audio_tracks;
                for (_, p) in &absorbed {
                    fold_media_info(&mut item, p);
                }
                self.push(item, Manifest::HlsMaster(m), found, children);
                if let Some(e) = self.entries.last_mut() {
                    e.folded = absorbed.into_iter().map(|(u, _)| u).collect();
                }
                true
            }
            hls::Playlist::Media(p) => {
                if let Some(master) = self.entries.iter_mut().find(|e| e.children.iter().any(|c| c == url)) {
                    if !master.folded.iter().any(|c| c == url) {
                        master.folded.push(url.to_string());
                    }
                    return fold_media_info(&mut master.item, &p);
                }
                if let Some(e) = self.find_mut(url) {
                    let changed = fold_media_info(&mut e.item, &p);
                    e.manifest = Manifest::HlsMedia(p);
                    return changed;
                }
                let mut item = Self::blank("hls", url);
                item.ext = p.container().to_string();
                item.audio_only = item.ext == "aac" || item.ext == "mp3";
                fold_media_info(&mut item, &p);
                self.push(item, Manifest::HlsMedia(p), found, Vec::new());
                true
            }
        }
    }

    /// Adds a parsed DASH manifest.
    pub fn add_dash(&mut self, url: &str, mpd: dash::Mpd, found: Found) -> bool {
        if self.find_mut(url).is_some() {
            return false;
        }
        let (variants, audio_tracks, best) = dash_choices(&mpd);
        let mut item = Self::blank("dash", url);
        item.ext = "mp4".into();
        item.width = best.map(|b| b.0);
        item.height = best.map(|b| b.1);
        item.duration = mpd.duration.filter(|d| *d > 0.0);
        item.live = mpd.dynamic;
        item.audio_only = !variants.is_empty() && variants.iter().all(|v| v.audio_only);
        item.protected = variants.is_empty() && mpd.protected();
        if mpd.dynamic {
            item.note = "Live DASH streams can't be recorded".into();
        }
        item.variants = variants;
        item.audio_tracks = audio_tracks;
        self.push(item, Manifest::Dash(mpd), found, Vec::new());
        true
    }

    /// Merges a page scan. Returns whether the list changed and the manifests to read.
    pub fn apply_scan(&mut self, scan: &Scan, page_url: &str, title: &str) -> (bool, Vec<(String, Sniffed)>) {
        let mut changed = (self.blobs, self.drm) != (scan.blobs, scan.drm);
        self.blobs = scan.blobs.min(1000);
        self.drm = scan.drm.min(1000);
        let mut fetch = Vec::new();
        for it in scan.items.iter().take(60) {
            let url = it.url.trim();
            if !(url.starts_with("http://") || url.starts_with("https://")) || url.len() > 4096 {
                continue;
            }
            if let Some(e) = self.find_mut(url) {
                if e.item.duration.is_none() && it.duration.is_finite() && it.duration > 0.0 {
                    e.item.duration = Some(it.duration);
                    changed = true;
                }
                if e.item.height.is_none() && it.height > 0 {
                    e.item.width = Some(it.width);
                    e.item.height = Some(it.height);
                    changed = true;
                }
                continue;
            }
            if self.knows(url) {
                continue;
            }
            let headers = vec![("Referer".to_string(), page_url.to_string())];
            match detect::guess_from_url(url, it.meta) {
                Some(Sniffed::File) => {
                    let ct = if !it.mime.is_empty() {
                        it.mime.clone()
                    } else if it.audio {
                        "audio/x-unknown".into()
                    } else {
                        String::new()
                    };
                    if self.add_file(url, &ct, None, Found { by: "page", headers, page_url, title }) {
                        if let Some(e) = self.find_mut(url) {
                            e.item.duration = Some(it.duration).filter(|d| d.is_finite() && *d > 0.0);
                            e.item.width = Some(it.width).filter(|w| *w > 0);
                            e.item.height = Some(it.height).filter(|h| *h > 0);
                        }
                        changed = true;
                    }
                }
                Some(kind) => fetch.push((url.to_string(), kind)),
                None => {}
            }
        }
        (changed, fetch)
    }

    /// Items for the panel: downloadable first, then the longest and largest.
    pub fn items(&self) -> Vec<MediaItem> {
        let mut items: Vec<MediaItem> = self.entries.iter().map(|e| e.item.clone()).collect();
        items.sort_by(|a, b| {
            a.protected
                .cmp(&b.protected)
                .then(b.duration.unwrap_or(0.0).total_cmp(&a.duration.unwrap_or(0.0)))
                .then(b.size.cmp(&a.size))
                .then(b.detected.cmp(&a.detected))
        });
        items
    }
}
