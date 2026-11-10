use super::*;

fn record(id: i64, url: &str, state: &str) -> DownloadRecord {
    DownloadRecord {
        id,
        url: url.into(),
        path: format!("C:\\d\\{id}.bin"),
        mime: String::new(),
        total: 1,
        received: 1,
        state: state.into(),
        started: id,
        ended: None,
    }
}

#[test]
fn finds_only_completed_downloads_of_the_same_url() {
    let mut conn = Connection::open_in_memory().unwrap();
    crate::storage::migrate(&conn).unwrap();
    download_upsert(&mut conn, &record(1, "https://a.test/f", "completed")).unwrap();
    download_upsert(&mut conn, &record(2, "https://a.test/f", "canceled")).unwrap();
    download_upsert(&mut conn, &record(3, "https://a.test/g", "completed")).unwrap();
    assert_eq!(completed_paths_for_url(&mut conn, "https://a.test/f").unwrap(), vec!["C:\\d\\1.bin"]);
    assert!(completed_paths_for_url(&mut conn, "https://a.test/none").unwrap().is_empty());
}

#[test]
fn downloads_left_running_by_an_earlier_session_are_marked_failed() {
    let mut conn = Connection::open_in_memory().unwrap();
    crate::storage::migrate(&conn).unwrap();
    let now = 100_000;
    download_upsert(&mut conn, &record(1_000, "https://a.test/stale", "progress")).unwrap();
    download_upsert(&mut conn, &record(2_000, "https://a.test/live", "progress")).unwrap();
    download_upsert(&mut conn, &record(now - 100, "https://a.test/just-started", "progress")).unwrap();
    download_upsert(&mut conn, &record(3_000, "https://a.test/done", "completed")).unwrap();
    assert_eq!(settle_stale(&mut conn, &[2_000], now).unwrap(), 1);
    let mut state = |id: i64| downloads_list(&mut conn, 10).unwrap().into_iter().find(|d| d.id == id).map(|d| (d.state, d.ended));
    assert_eq!(state(1_000), Some(("interrupted".into(), Some(now))));
    assert_eq!(state(2_000), Some(("progress".into(), None)), "still downloading");
    assert_eq!(state(now - 100), Some(("progress".into(), None)), "may not be listed as active yet");
    assert_eq!(state(3_000), Some(("completed".into(), None)));
}

#[test]
fn a_download_is_read_back_from_the_history() {
    let mut conn = Connection::open_in_memory().unwrap();
    crate::storage::migrate(&conn).unwrap();
    download_upsert(&mut conn, &record(7, "https://a.test/setup.exe", "interrupted")).unwrap();
    let got = download_get(&mut conn, 7).unwrap().unwrap();
    assert_eq!((got.url.as_str(), got.state.as_str()), ("https://a.test/setup.exe", "interrupted"));
    assert!(download_get(&mut conn, 8).unwrap().is_none());
}

#[test]
fn names_split_into_stem_and_extension() {
    use super::again::split_name;
    let s = |n: &str| split_name(n);
    assert_eq!(s("setup.exe"), ("setup".into(), "exe".into()));
    assert_eq!(s("archive.tar.gz"), ("archive.tar".into(), "gz".into()));
    assert_eq!(s("README"), ("README".into(), String::new()));
    assert_eq!(s(".env"), (".env".into(), String::new()));
    assert_eq!(s("report."), ("report.".into(), String::new()));
    assert_eq!(s("v1.2 final"), ("v1.2 final".into(), String::new()));
}

#[test]
fn only_web_addresses_are_checked() {
    assert_eq!(already_downloaded("blob:https://a.test/x"), None);
    assert_eq!(already_message("a.zip"), "Already downloaded: a.zip");
}
