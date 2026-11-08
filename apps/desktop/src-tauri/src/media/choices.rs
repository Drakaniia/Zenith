//! Quality and audio-track choices offered for HLS master playlists and DASH manifests.

use std::collections::HashSet;

use super::{dash, format_bitrate, hls, quality_label, Choice, MediaItem};

pub(super) fn hls_master_item(m: &hls::Master) -> (Vec<Choice>, Vec<Choice>, Option<(u32, u32)>) {
    let mut seen = HashSet::new();
    let mut variants: Vec<(usize, &hls::Variant)> = m
        .variants
        .iter()
        .enumerate()
        .filter(|(_, v)| seen.insert((v.resolution, v.bandwidth, v.audio_only())))
        .collect();
    variants.sort_by(|(_, a), (_, b)| {
        a.audio_only()
            .cmp(&b.audio_only())
            .then(b.resolution.map(|r| r.1).cmp(&a.resolution.map(|r| r.1)))
            .then(b.bandwidth.cmp(&a.bandwidth))
    });
    let best = variants.iter().find(|(_, v)| !v.audio_only()).and_then(|(_, v)| v.resolution);
    let choices = variants
        .iter()
        .map(|(i, v)| Choice {
            id: format!("v{i}"),
            label: quality_label(v.resolution.map(|r| r.1), v.bandwidth, v.frame_rate, v.audio_only()),
            height: v.resolution.map(|r| r.1),
            bandwidth: Some(v.bandwidth).filter(|b| *b > 0),
            audio_only: v.audio_only(),
        })
        .collect();
    let mut langs = HashSet::new();
    let audio: Vec<Choice> = m
        .renditions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == "AUDIO" && r.uri.is_some())
        .filter(|(_, r)| langs.insert((r.language.clone(), r.name.clone())))
        .map(|(i, r)| Choice {
            id: format!("a{i}"),
            label: match (r.name.is_empty(), r.language.is_empty()) {
                (false, _) => r.name.clone(),
                (true, false) => r.language.clone(),
                _ => format!("Track {}", i + 1),
            },
            height: None,
            bandwidth: None,
            audio_only: true,
        })
        .collect();
    (choices, if audio.len() > 1 { audio } else { Vec::new() }, best)
}

pub(super) fn dash_choices(mpd: &dash::Mpd) -> (Vec<Choice>, Vec<Choice>, Option<(u32, u32)>) {
    let reps = &mpd.representations;
    let mut videos: Vec<usize> = (0..reps.len()).filter(|&i| reps[i].kind == dash::Kind::Video && !reps[i].protected).collect();
    videos.sort_by(|&a, &b| reps[b].height.cmp(&reps[a].height).then(reps[b].bandwidth.cmp(&reps[a].bandwidth)));
    // Best audio representation per language.
    let mut audios: Vec<usize> = Vec::new();
    for i in (0..reps.len()).filter(|&i| reps[i].kind == dash::Kind::Audio && !reps[i].protected) {
        match audios.iter().position(|&j| reps[j].lang == reps[i].lang) {
            Some(k) if reps[audios[k]].bandwidth < reps[i].bandwidth => audios[k] = i,
            Some(_) => {}
            None => audios.push(i),
        }
    }
    let best = videos.first().and_then(|&i| Some((reps[i].width?, reps[i].height?)));
    let mut variants: Vec<Choice> = videos
        .iter()
        .map(|&i| Choice {
            id: format!("r{i}"),
            label: quality_label(reps[i].height, reps[i].bandwidth, reps[i].frame_rate, false),
            height: reps[i].height,
            bandwidth: Some(reps[i].bandwidth).filter(|b| *b > 0),
            audio_only: false,
        })
        .collect();
    variants.extend(audios.iter().map(|&i| {
        let lang = if reps[i].lang.is_empty() { String::new() } else { format!(" · {}", reps[i].lang) };
        Choice {
            id: format!("r{i}"),
            label: format!("Audio only{lang} · {}", format_bitrate(reps[i].bandwidth)),
            height: None,
            bandwidth: Some(reps[i].bandwidth).filter(|b| *b > 0),
            audio_only: true,
        }
    }));
    let tracks = if audios.len() > 1 {
        audios
            .iter()
            .map(|&i| Choice {
                id: format!("r{i}"),
                label: [reps[i].label.as_str(), reps[i].lang.as_str()]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or("Audio")
                    .to_string(),
                height: None,
                bandwidth: Some(reps[i].bandwidth),
                audio_only: true,
            })
            .collect()
    } else {
        Vec::new()
    };
    (variants, tracks, best)
}

pub(super) fn fold_media_info(item: &mut MediaItem, p: &hls::MediaPlaylist) -> bool {
    let before = (item.duration, item.live, item.protected);
    item.live = p.live();
    item.duration = if item.live { None } else { Some(p.duration()).filter(|d| *d > 0.0) };
    item.protected |= p.encrypted;
    before != (item.duration, item.live, item.protected)
}
