//! Turning a listed item and the user's choices into a download request.

use super::job::{self, Plan, Request, Source};
use super::naming::stem_from_url;
use super::{dash, mime_for, sanitize_stem, Manifest, TabMedia};

impl TabMedia {
    /// Turns a listed item and the user's choices into a download request.
    pub fn request(&self, tab: &str, item_id: u32, variant: Option<&str>, audio: Option<&str>, user_agent: &str) -> Result<Request, String> {
        let e = self.entries.iter().find(|e| e.item.id == item_id).ok_or("This item is no longer on the page; reload the list")?;
        if e.item.protected {
            return Err(job::PROTECTED.into());
        }
        let title = Some(sanitize_stem(&e.item.title)).filter(|t| t != "video" || e.item.title.trim() == "video");
        let stem = title.unwrap_or_else(|| sanitize_stem(&stem_from_url(&e.item.url)));
        let index = |id: Option<&str>, prefix: char| id.and_then(|s| s.strip_prefix(prefix)).and_then(|n| n.parse::<usize>().ok());
        let (plan, suffix, ext, audio_only) = match &e.manifest {
            Manifest::None => (
                Plan { main: Source::File { url: e.item.url.clone(), ext: e.item.ext.clone() }, audio: None, live: false },
                String::new(),
                e.item.ext.clone(),
                e.item.audio_only,
            ),
            Manifest::HlsMedia(p) => (
                Plan { main: Source::Hls(e.item.url.clone()), audio: None, live: p.live() },
                String::new(),
                p.container().to_string(),
                e.item.audio_only,
            ),
            Manifest::HlsMaster(m) => {
                let v = index(variant, 'v')
                    .and_then(|i| m.variants.get(i))
                    .or_else(|| m.variants.iter().filter(|v| !v.audio_only()).max_by_key(|v| v.bandwidth))
                    .or_else(|| m.variants.iter().max_by_key(|v| v.bandwidth))
                    .ok_or("The stream lists no qualities")?;
                let group = m.audio_renditions(v.audio.as_deref());
                let wanted = index(audio, 'a').and_then(|i| m.renditions.get(i));
                let rendition = wanted
                    .and_then(|w| group.iter().find(|r| r.language == w.language && r.name == w.name).copied())
                    .or_else(|| m.default_audio(v));
                let audio_src = if v.audio_only() { None } else { rendition.and_then(|r| r.uri.clone()).map(Source::Hls) };
                let suffix = if v.audio_only() {
                    "audio".to_string()
                } else {
                    v.resolution.map(|r| format!("{}p", r.1)).unwrap_or_default()
                };
                (
                    Plan { main: Source::Hls(v.uri.clone()), audio: audio_src, live: e.item.live },
                    suffix,
                    if v.audio_only() { "m4a".to_string() } else { "mp4".to_string() },
                    v.audio_only(),
                )
            }
            Manifest::Dash(mpd) => {
                if mpd.dynamic {
                    return Err("Live DASH streams can't be recorded".into());
                }
                let reps = &mpd.representations;
                let usable = |i: &usize| matches!(reps[*i].kind, dash::Kind::Video | dash::Kind::Audio) && !reps[*i].protected;
                let main = index(variant, 'r')
                    .filter(|i| *i < reps.len() && usable(i))
                    .or_else(|| {
                        (0..reps.len())
                            .filter(|i| usable(i) && reps[*i].kind == dash::Kind::Video)
                            .max_by_key(|&i| (reps[i].height, reps[i].bandwidth))
                    })
                    .or_else(|| (0..reps.len()).filter(usable).max_by_key(|&i| reps[i].bandwidth))
                    .ok_or(job::PROTECTED)?;
                let rep = &reps[main];
                let audio_rep = if rep.kind == dash::Kind::Video {
                    index(audio, 'r')
                        .filter(|i| *i < reps.len() && usable(i) && reps[*i].kind == dash::Kind::Audio)
                        .or_else(|| {
                            let first_lang = reps.iter().find(|r| r.kind == dash::Kind::Audio && !r.protected).map(|r| r.lang.clone());
                            (0..reps.len())
                                .filter(|i| usable(i) && reps[*i].kind == dash::Kind::Audio && Some(&reps[*i].lang) == first_lang.as_ref())
                                .max_by_key(|&i| reps[i].bandwidth)
                        })
                } else {
                    None
                };
                let source = |r: &dash::Representation| match &r.segments {
                    dash::Segments::Single(url) => Source::File { url: url.clone(), ext: r.container().to_string() },
                    dash::Segments::List { init, media } => {
                        Source::Segments { init: init.clone(), media: media.clone(), ext: r.container().to_string() }
                    }
                };
                let is_audio = rep.kind == dash::Kind::Audio;
                (
                    Plan { main: source(rep), audio: audio_rep.map(|i| source(&reps[i])), live: false },
                    if is_audio { "audio".into() } else { rep.height.map(|h| format!("{h}p")).unwrap_or_default() },
                    rep.container().to_string(),
                    is_audio,
                )
            }
        };
        let stem = if suffix.is_empty() { stem } else { sanitize_stem(&format!("{stem} ({suffix})")) };
        let mut headers: Vec<(String, String)> = e.headers.iter().filter(|(k, _)| k != "User-Agent").cloned().collect();
        if !headers.iter().any(|(k, _)| k == "Referer") && !e.page_url.is_empty() {
            headers.push(("Referer".into(), e.page_url.clone()));
        }
        let ua = e.headers.iter().find(|(k, _)| k == "User-Agent").map(|(_, v)| v.clone()).unwrap_or_else(|| user_agent.to_string());
        Ok(Request {
            tab: tab.to_string(),
            stem,
            mime: mime_for(&ext, audio_only),
            ext,
            source_url: e.item.url.clone(),
            page_url: e.page_url.clone(),
            headers,
            user_agent: ua,
            plan,
        })
    }
}
