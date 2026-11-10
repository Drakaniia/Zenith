//! Adding a torrent: reading its file list (from peers for a magnet link), waiting for the user's
//! file choice when it has several files, and handing it to the engine. Also the file list and
//! streaming of a torrent that was added.

use std::collections::HashSet;
use std::path::PathBuf;

use librqbit::api::TorrentIdOrHash;
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse};

use super::list::{display_name, publish};
use super::status::Stage;
use super::store::{self, Saved};
use super::{engine, is_magnet, lock, save_list, source, stream, torrent_args, update, Entry, Handle, Input, TorrentFile};
use crate::host::downloads::Origin;
use crate::storage::now_ms;

/// Starts a torrent and returns its download id.
pub fn add(input: Input, tab: &str, private: bool, origin: Option<Origin>) -> Result<i64, String> {
    let (hash, name, source) = match input {
        Input::Magnet(uri) => {
            let hash = source::magnet_hash(&uri).ok_or("This magnet link isn't valid.")?;
            (hash, source::magnet_name(&uri).unwrap_or_default(), uri)
        }
        Input::File(bytes) => {
            let (hash, name) = source::torrent_file_info(&bytes)?;
            let kept = store::keep_torrent_file(&engine::folder(), &hash, &bytes).map_err(|e| e.to_string())?;
            (hash, name, kept.display().to_string())
        }
    };
    let id = {
        let mut entries = lock();
        if let Some(e) = entries.values().find(|e| e.saved.hash == hash) {
            return Err(format!("\u{201c}{}\u{201d} is already in your downloads.", display_name(&e.saved)));
        }
        let id = crate::host::downloads::next_id();
        let dir = crate::media::job::download_dir().display().to_string();
        let saved = Saved { id, hash, source, name, path: dir.clone(), dir, started: now_ms(), private, ..Default::default() };
        entries.insert(id, Entry::new(saved, tab, origin));
        id
    };
    save_list();
    publish(id);
    begin(id);
    Ok(id)
}

/// Adds a .torrent file from disk (a finished page download, or one opened from Windows).
pub fn add_file(path: &str, tab: &str, private: bool) -> Result<i64, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Could not read {path}: {e}"))?;
    add(Input::File(bytes), tab, private, None)
}

/// Torrents named on a command line (magnet links and .torrent files Windows opened Zenith with).
pub fn open_args(argv: &[String]) {
    if !super::enabled() {
        return;
    }
    for arg in torrent_args(argv) {
        let result = if is_magnet(&arg) { add(Input::Magnet(arg), "", false, None) } else { add_file(&arg, "", false) };
        if let Err(e) = result {
            crate::host::emit("torrent-error", e);
        }
    }
}

fn fail(id: i64, reason: String) {
    update(id, |e| {
        e.early = Some(Stage::Error);
        e.reason = reason;
    });
    publish(id);
}

/// Gets the torrent going (again): file list, file choice, engine.
pub(super) fn begin(id: i64) {
    let task = engine::spawn(async move {
        if let Err(reason) = prepare(id).await {
            fail(id, reason);
        }
    });
    update(id, |e| e.task = Some(task));
}

fn files_of(h: &Handle) -> Vec<(String, u64)> {
    h.with_metadata(|m| m.file_infos.iter().map(|f| (f.relative_filename.display().to_string(), f.len)).collect::<Vec<_>>())
        .unwrap_or_default()
}

/// Gets the torrent's file list and starts it, or waits for the user to choose files.
async fn prepare(id: i64) -> Result<(), String> {
    let session = engine::session().await?;
    let Some(saved) = lock().get(&id).map(|e| e.saved.clone()) else { return Ok(()) };
    // The engine keeps its own list and already restored it.
    if let Some(h) = TorrentIdOrHash::try_from(saved.hash.as_str()).ok().and_then(|h| session.get(h)) {
        let files = files_of(&h);
        update(id, |e| e.files = files);
        attach(id, h);
        return Ok(());
    }
    update(id, |e| {
        e.early = Some(Stage::Metadata);
        e.reason.clear();
    });
    publish(id);
    let add = if is_magnet(&saved.source) {
        AddTorrent::from_url(saved.source.clone())
    } else {
        AddTorrent::from_bytes(std::fs::read(&saved.source).map_err(|_| "The .torrent file is gone.".to_string())?)
    };
    let opts = AddTorrentOptions { list_only: true, ..Default::default() };
    let listed = match session.add_torrent(add, Some(opts)).await.map_err(|e| format!("{e:#}"))? {
        AddTorrentResponse::ListOnly(l) => l,
        other => {
            let h = other.into_handle().ok_or("The torrent could not be started.")?;
            let files = files_of(&h);
            update(id, |e| e.files = files);
            attach(id, h);
            return Ok(());
        }
    };
    let files: Vec<(String, u64)> = listed.info.iter_file_details().map(|f| (f.filename.to_pathbuf().display().to_string(), f.len)).collect();
    let name = listed.info.name().map(|n| n.into_owned()).filter(|n| !n.is_empty()).unwrap_or(saved.name);
    let kept = store::keep_torrent_file(&engine::folder(), &saved.hash, &listed.torrent_bytes).map_err(|e| e.to_string())?;
    let choose = files.len() > 1 && saved.only_files.is_none() && crate::host::config().torrent_choose_files;
    update(id, |e| {
        let base = PathBuf::from(&e.saved.dir);
        e.saved.path = match files.as_slice() {
            [one] => base.join(&one.0),
            _ => base.join(source::folder_name(&name)),
        }
        .display()
        .to_string();
        e.saved.name = name;
        e.saved.source = kept.display().to_string();
        e.files = files;
        e.peers = listed.seen_peers;
        e.early = choose.then_some(Stage::Choosing);
    });
    save_list();
    publish(id);
    if choose {
        return Ok(());
    }
    start(id).await
}

/// Hands the torrent (with the chosen files) to the engine.
async fn start(id: i64) -> Result<(), String> {
    let session = engine::session().await?;
    let Some((saved, peers, multi)) = lock().get(&id).map(|e| (e.saved.clone(), e.peers.clone(), e.files.len() > 1)) else {
        return Ok(());
    };
    let bytes = std::fs::read(&saved.source).map_err(|_| "The .torrent file is gone.".to_string())?;
    let opts = AddTorrentOptions {
        output_folder: Some(if multi { saved.path.clone() } else { saved.dir.clone() }),
        only_files: saved.only_files.clone(),
        // Continue from data already on disk (it is checked piece by piece).
        overwrite: true,
        initial_peers: (!peers.is_empty()).then_some(peers),
        ..Default::default()
    };
    let response = session.add_torrent(AddTorrent::from_bytes(bytes), Some(opts)).await.map_err(|e| format!("{e:#}"))?;
    attach(id, response.into_handle().ok_or("The torrent could not be started.")?);
    Ok(())
}

fn attach(id: i64, h: Handle) {
    let attached = {
        let mut entries = lock();
        match entries.get_mut(&id) {
            Some(e) => {
                e.handle = Some(h.clone());
                e.early = None;
                e.reason.clear();
                e.task = None;
                true
            }
            None => false,
        }
    };
    if attached {
        publish(id);
    } else {
        // Canceled while starting.
        engine::spawn(async move {
            if let Some(s) = engine::existing() {
                let _ = s.delete(TorrentIdOrHash::Id(h.id()), true).await;
            }
        });
    }
}

/// The torrent's files, with what was downloaded and which are selected.
pub fn files(id: i64) -> Result<Vec<TorrentFile>, String> {
    let entries = lock();
    let e = entries.get(&id).ok_or("This torrent is no longer in the list.")?;
    if e.files.is_empty() {
        return Err("The torrent's file list isn't known yet.".into());
    }
    let (done, only) = match &e.handle {
        Some(h) => (h.stats().file_progress, h.only_files()),
        None => (Vec::new(), e.saved.only_files.clone()),
    };
    Ok(e.files
        .iter()
        .enumerate()
        .map(|(index, (name, size))| TorrentFile {
            index,
            name: name.clone(),
            size: *size,
            done: done.get(index).copied().unwrap_or(0),
            selected: only.as_ref().is_none_or(|o| o.contains(&index)),
        })
        .collect())
}

/// Downloads only `chosen` of the torrent's files (and starts a torrent waiting for the choice).
pub async fn choose_files(id: i64, chosen: Vec<usize>) -> Result<(), String> {
    let mut chosen: Vec<usize> = chosen.into_iter().collect::<HashSet<_>>().into_iter().collect();
    chosen.sort_unstable();
    if chosen.is_empty() {
        return Err("Choose at least one file.".into());
    }
    let (handle, waiting) = {
        let mut entries = lock();
        let e = entries.get_mut(&id).ok_or("This torrent is no longer in the list.")?;
        if chosen.iter().any(|&i| i >= e.files.len()) {
            return Err("That file isn't in this torrent.".into());
        }
        e.saved.only_files = Some(chosen.clone());
        let waiting = e.early == Some(Stage::Choosing);
        if waiting {
            e.early = None;
        }
        (e.handle.clone(), waiting)
    };
    save_list();
    publish(id);
    match handle {
        Some(h) => {
            engine::run(async move {
                let s = engine::session().await?;
                s.update_only_files(&h, &chosen.into_iter().collect()).await.map_err(|e| format!("{e:#}"))
            })
            .await?;
        }
        None if waiting => {
            let task = engine::spawn(async move {
                if let Err(reason) = start(id).await {
                    fail(id, reason);
                }
            });
            update(id, |e| e.task = Some(task));
        }
        None => {}
    }
    Ok(())
}

/// A local address that plays one of the torrent's files while it downloads.
pub async fn stream_url(id: i64, file: usize) -> Result<String, String> {
    let (_, name) = stream_target(id, file).ok_or("This file can't be played yet.")?;
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
    engine::run(async move { stream::url(id, file, &leaf).await }).await
}

/// The torrent and file name the streaming server serves for `id`/`file`.
pub(super) fn stream_target(id: i64, file: usize) -> Option<(Handle, String)> {
    let entries = lock();
    let e = entries.get(&id)?;
    let name = e.files.get(file)?.0.clone();
    Some((e.handle.clone()?, name))
}
