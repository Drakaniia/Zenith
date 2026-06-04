use crate::models::HistoryEntry;
use chrono::Utc;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tauri::Manager;
use uuid::Uuid;

const HISTORY_FILE_NAME: &str = "history.json";
static HISTORY_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn history_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(HISTORY_FILE_NAME))
        .map_err(|error| format!("failed to resolve app data directory: {error}"))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn with_history_file_lock<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _guard = HISTORY_FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "history file lock is poisoned".to_string())?;

    operation()
}

fn read_history(path: &Path) -> Result<Vec<HistoryEntry>, String> {
    match path.try_exists() {
        Ok(false) => {
            let backup_path = backup_path(path);
            return match backup_path.try_exists() {
                Ok(true) => read_history_file(&backup_path),
                Ok(false) => Ok(Vec::new()),
                Err(error) => Err(format!("failed to check history backup file: {error}")),
            };
        }
        Ok(true) => {}
        Err(error) => return Err(format!("failed to check history file: {error}")),
    }

    read_history_file(path)
}

fn read_history_file(path: &Path) -> Result<Vec<HistoryEntry>, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read history file: {error}"))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("failed to parse history file: {error}"))
}

fn write_history(path: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "history path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create history directory: {error}"))?;

    let temp_path = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(entries)
        .map_err(|error| format!("failed to serialize history: {error}"))?;

    {
        let mut temp_file = fs::File::create(&temp_path)
            .map_err(|error| format!("failed to create temporary history file: {error}"))?;
        temp_file
            .write_all(&json)
            .map_err(|error| format!("failed to write temporary history file: {error}"))?;
        temp_file
            .sync_all()
            .map_err(|error| format!("failed to flush temporary history file: {error}"))?;
    }

    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    if !path
        .try_exists()
        .map_err(|error| format!("failed to check existing history file: {error}"))?
    {
        return fs::rename(temp_path, path)
            .map_err(|error| format!("failed to save history file: {error}"));
    }

    let backup_path = backup_path(path);
    if backup_path
        .try_exists()
        .map_err(|error| format!("failed to check history backup file: {error}"))?
    {
        fs::remove_file(&backup_path)
            .map_err(|error| format!("failed to remove stale history backup file: {error}"))?;
    }

    fs::rename(path, &backup_path)
        .map_err(|error| format!("failed to back up existing history file: {error}"))?;

    match fs::rename(temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(save_error) => match fs::rename(&backup_path, path) {
            Ok(()) => Err(format!("failed to save history file: {save_error}")),
            Err(restore_error) => Err(format!(
                "failed to save history file: {save_error}; failed to restore history backup: {restore_error}"
            )),
        },
    }
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path).map_err(|error| format!("failed to save history file: {error}"))
}

fn get_history_at_path(path: &Path) -> Result<Vec<HistoryEntry>, String> {
    with_history_file_lock(|| get_history_at_path_unlocked(path))
}

fn get_history_at_path_unlocked(path: &Path) -> Result<Vec<HistoryEntry>, String> {
    let mut entries = read_history(path)?;
    entries.sort_by(|left, right| right.visited_at.cmp(&left.visited_at));
    Ok(entries)
}

fn add_history_entry_at_path(
    path: &Path,
    title: String,
    url: String,
) -> Result<HistoryEntry, String> {
    with_history_file_lock(|| {
        let mut entries = read_history(path)?;
        let entry = HistoryEntry {
            id: Uuid::new_v4().to_string(),
            title,
            url,
            visited_at: Utc::now().to_rfc3339(),
        };

        entries.push(entry.clone());
        write_history(path, &entries)?;

        Ok(entry)
    })
}

fn search_history_at_path(path: &Path, query: &str) -> Result<Vec<HistoryEntry>, String> {
    with_history_file_lock(|| {
        let query = query.to_lowercase();
        Ok(get_history_at_path_unlocked(path)?
            .into_iter()
            .filter(|entry| {
                entry.title.to_lowercase().contains(&query)
                    || entry.url.to_lowercase().contains(&query)
            })
            .collect())
    })
}

fn clear_history_at_path(path: &Path) -> Result<(), String> {
    with_history_file_lock(|| write_history(path, &[]))
}

fn delete_history_entry_at_path(path: &Path, id: &str) -> Result<(), String> {
    with_history_file_lock(|| {
        let mut entries = read_history(path)?;
        entries.retain(|entry| entry.id != id);
        write_history(path, &entries)
    })
}

#[tauri::command]
pub fn get_history(app: tauri::AppHandle) -> Result<Vec<HistoryEntry>, String> {
    get_history_at_path(&history_path(&app)?)
}

#[tauri::command]
pub fn add_history_entry(app: tauri::AppHandle, title: String, url: String) -> Result<(), String> {
    add_history_entry_at_path(&history_path(&app)?, title, url).map(|_| ())
}

#[tauri::command]
pub fn search_history(app: tauri::AppHandle, query: String) -> Result<Vec<HistoryEntry>, String> {
    search_history_at_path(&history_path(&app)?, &query)
}

#[tauri::command]
pub fn clear_history(app: tauri::AppHandle) -> Result<(), String> {
    clear_history_at_path(&history_path(&app)?)
}

#[tauri::command]
pub fn delete_history_entry(app: tauri::AppHandle, id: String) -> Result<(), String> {
    delete_history_entry_at_path(&history_path(&app)?, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn history_entry(id: &str, title: &str, url: &str, visited_at: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            visited_at: visited_at.to_string(),
        }
    }

    #[test]
    fn missing_history_json_returns_empty_list() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);

        let entries = read_history(&path).expect("read missing history");

        assert!(entries.is_empty());
    }

    #[test]
    fn missing_history_json_recovers_from_backup_file() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let backup_path = path.with_extension("json.bak");
        let entries = vec![history_entry(
            "existing",
            "Existing",
            "https://example.com",
            "2026-06-04T09:00:00Z",
        )];
        let json = serde_json::to_vec_pretty(&entries).expect("serialize history");
        fs::write(&backup_path, json).expect("write backup history");

        let recovered = read_history(&path).expect("read backup history");

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, "existing");
    }

    #[test]
    fn concurrent_history_adds_do_not_overwrite_each_other() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = std::sync::Arc::new(directory.path().join(HISTORY_FILE_NAME));
        let handles = (0..16)
            .map(|index| {
                let path = std::sync::Arc::clone(&path);
                std::thread::spawn(move || {
                    add_history_entry_at_path(
                        path.as_path(),
                        format!("Visit {index}"),
                        format!("https://example.com/{index}"),
                    )
                    .expect("add history entry");
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().expect("history thread joined");
        }

        let stored = read_history(path.as_path()).expect("read concurrent history");
        assert_eq!(stored.len(), 16);
    }

    #[test]
    fn get_history_sorts_most_recent_first() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let entries = vec![
            history_entry(
                "older",
                "Older",
                "https://example.com/older",
                "2026-06-04T08:00:00Z",
            ),
            history_entry(
                "newer",
                "Newer",
                "https://example.com/newer",
                "2026-06-04T09:00:00Z",
            ),
            history_entry(
                "oldest",
                "Oldest",
                "https://example.com/oldest",
                "2026-06-03T09:00:00Z",
            ),
        ];
        write_history(&path, &entries).expect("write unsorted history");

        let sorted = get_history_at_path(&path).expect("get sorted history");

        assert_eq!(
            sorted
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["newer", "older", "oldest"]
        );
    }

    #[test]
    fn add_history_entry_records_title_url_uuid_and_timestamp() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("nested").join(HISTORY_FILE_NAME);

        let entry = add_history_entry_at_path(
            &path,
            "Example".to_string(),
            "https://example.com".to_string(),
        )
        .expect("add history entry");

        Uuid::parse_str(&entry.id).expect("history id is UUID");
        DateTime::parse_from_rfc3339(&entry.visited_at).expect("visited_at is RFC3339 timestamp");
        assert_eq!(entry.title, "Example");
        assert_eq!(entry.url, "https://example.com");

        let stored = read_history(&path).expect("read saved history");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, entry.id);
        assert_eq!(stored[0].title, "Example");
        assert_eq!(stored[0].url, "https://example.com");
    }

    #[test]
    fn adding_same_url_twice_creates_separate_visit_entries() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);

        let first = add_history_entry_at_path(
            &path,
            "First Visit".to_string(),
            "https://example.com".to_string(),
        )
        .expect("add first history entry");
        let second = add_history_entry_at_path(
            &path,
            "Second Visit".to_string(),
            "https://example.com".to_string(),
        )
        .expect("add second history entry");

        let stored = read_history(&path).expect("read saved history");
        assert_eq!(stored.len(), 2);
        assert_ne!(first.id, second.id);
        assert_eq!(
            stored
                .iter()
                .filter(|entry| entry.url == "https://example.com")
                .count(),
            2
        );
    }

    #[test]
    fn search_history_matches_case_insensitive_title_and_url() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let entries = vec![
            history_entry(
                "title-match",
                "Rust Language",
                "https://www.rust-lang.org",
                "2026-06-04T09:00:00Z",
            ),
            history_entry(
                "url-match",
                "Example",
                "https://docs.rs/tauri",
                "2026-06-04T08:00:00Z",
            ),
            history_entry(
                "miss",
                "No Match",
                "https://example.com",
                "2026-06-04T07:00:00Z",
            ),
        ];
        write_history(&path, &entries).expect("write history");

        let title_matches =
            search_history_at_path(&path, "rust").expect("search title case-insensitive");
        let url_matches = search_history_at_path(&path, "DOCS.RS").expect("search URL");

        assert_eq!(title_matches.len(), 1);
        assert_eq!(title_matches[0].id, "title-match");
        assert_eq!(url_matches.len(), 1);
        assert_eq!(url_matches[0].id, "url-match");
    }

    #[test]
    fn delete_history_entry_removes_only_selected_id() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let entries = vec![
            history_entry(
                "delete-me",
                "Delete",
                "https://example.com/delete",
                "2026-06-04T09:00:00Z",
            ),
            history_entry(
                "keep-me",
                "Keep",
                "https://example.com/keep",
                "2026-06-04T08:00:00Z",
            ),
        ];
        write_history(&path, &entries).expect("write history");

        delete_history_entry_at_path(&path, "delete-me").expect("delete selected entry");

        let stored = read_history(&path).expect("read remaining history");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "keep-me");
    }

    #[test]
    fn clear_history_writes_empty_list() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let entries = vec![history_entry(
            "entry",
            "Entry",
            "https://example.com",
            "2026-06-04T09:00:00Z",
        )];
        write_history(&path, &entries).expect("write history");

        clear_history_at_path(&path).expect("clear history");

        let stored = read_history(&path).expect("read cleared history");
        assert!(stored.is_empty());
        assert_eq!(fs::read_to_string(&path).expect("read history file"), "[]");
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_failure_restores_existing_history_file() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(HISTORY_FILE_NAME);
        let missing_temp_path = directory.path().join("missing.json.tmp");
        fs::write(&path, r#"[{"id":"existing"}]"#).expect("write existing history");

        let error =
            replace_file(&missing_temp_path, &path).expect_err("replace should fail without temp");

        assert!(error.contains("failed to save history file"));
        assert_eq!(
            fs::read_to_string(&path).expect("read restored history"),
            r#"[{"id":"existing"}]"#
        );
    }
}
