use crate::bridge::dto::Bookmark;
use chrono::Utc;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tauri::Manager;
use uuid::Uuid;

const BOOKMARKS_FILE_NAME: &str = "bookmarks.json";

fn bookmarks_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(BOOKMARKS_FILE_NAME))
        .map_err(|error| format!("failed to resolve app data directory: {error}"))
}

fn read_bookmarks(path: &Path) -> Result<Vec<Bookmark>, String> {
    match path.try_exists() {
        Ok(false) => return Ok(Vec::new()),
        Ok(true) => {}
        Err(error) => return Err(format!("failed to check bookmarks file: {error}")),
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read bookmarks file: {error}"))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("failed to parse bookmarks file: {error}"))
}

fn write_bookmarks(path: &Path, bookmarks: &[Bookmark]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "bookmarks path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create bookmarks directory: {error}"))?;

    let temp_path = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(bookmarks)
        .map_err(|error| format!("failed to serialize bookmarks: {error}"))?;

    {
        let mut temp_file = fs::File::create(&temp_path)
            .map_err(|error| format!("failed to create temporary bookmarks file: {error}"))?;
        temp_file
            .write_all(&json)
            .map_err(|error| format!("failed to write temporary bookmarks file: {error}"))?;
        temp_file
            .sync_all()
            .map_err(|error| format!("failed to flush temporary bookmarks file: {error}"))?;
    }

    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    if !path
        .try_exists()
        .map_err(|error| format!("failed to check existing bookmarks file: {error}"))?
    {
        return fs::rename(temp_path, path)
            .map_err(|error| format!("failed to save bookmarks file: {error}"));
    }

    let backup_path = path.with_extension("json.bak");
    if backup_path
        .try_exists()
        .map_err(|error| format!("failed to check bookmarks backup file: {error}"))?
    {
        fs::remove_file(&backup_path)
            .map_err(|error| format!("failed to remove stale bookmarks backup file: {error}"))?;
    }

    fs::rename(path, &backup_path)
        .map_err(|error| format!("failed to back up existing bookmarks file: {error}"))?;

    match fs::rename(temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(save_error) => match fs::rename(&backup_path, path) {
            Ok(()) => Err(format!("failed to save bookmarks file: {save_error}")),
            Err(restore_error) => Err(format!(
                "failed to save bookmarks file: {save_error}; failed to restore bookmarks backup: {restore_error}"
            )),
        },
    }
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path).map_err(|error| format!("failed to save bookmarks file: {error}"))
}

fn save_bookmark_at_path(path: &Path, title: String, url: String) -> Result<Bookmark, String> {
    let mut bookmarks = read_bookmarks(path)?;

    if let Some(existing) = bookmarks.iter_mut().find(|bookmark| bookmark.url == url) {
        existing.title = title;
        let updated = existing.clone();
        write_bookmarks(path, &bookmarks)?;
        return Ok(updated);
    }

    let bookmark = Bookmark {
        id: Uuid::new_v4().to_string(),
        title,
        url,
        created_at: Utc::now().to_rfc3339(),
    };

    bookmarks.push(bookmark.clone());
    write_bookmarks(path, &bookmarks)?;

    Ok(bookmark)
}

fn delete_bookmark_at_path(path: &Path, id: &str) -> Result<(), String> {
    let mut bookmarks = read_bookmarks(path)?;
    bookmarks.retain(|bookmark| bookmark.id != id);
    write_bookmarks(path, &bookmarks)
}

fn is_bookmarked_at_path(path: &Path, url: &str) -> Result<bool, String> {
    Ok(read_bookmarks(path)?
        .iter()
        .any(|bookmark| bookmark.url == url))
}

#[tauri::command]
pub fn get_bookmarks(app: tauri::AppHandle) -> Result<Vec<Bookmark>, String> {
    read_bookmarks(&bookmarks_path(&app)?)
}

#[tauri::command]
pub fn save_bookmark(
    app: tauri::AppHandle,
    title: String,
    url: String,
) -> Result<Bookmark, String> {
    save_bookmark_at_path(&bookmarks_path(&app)?, title, url)
}

#[tauri::command]
pub fn delete_bookmark(app: tauri::AppHandle, id: String) -> Result<(), String> {
    delete_bookmark_at_path(&bookmarks_path(&app)?, &id)
}

#[tauri::command]
pub fn is_bookmarked(app: tauri::AppHandle, url: String) -> Result<bool, String> {
    is_bookmarked_at_path(&bookmarks_path(&app)?, &url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    #[test]
    fn missing_bookmarks_json_returns_empty_list() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(BOOKMARKS_FILE_NAME);

        let bookmarks = read_bookmarks(&path).expect("read missing bookmarks");

        assert!(bookmarks.is_empty());
    }

    #[test]
    fn save_creates_bookmark_with_uuid_and_iso_timestamp() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("nested").join(BOOKMARKS_FILE_NAME);

        let bookmark = save_bookmark_at_path(
            &path,
            "Example".to_string(),
            "https://example.com".to_string(),
        )
        .expect("save bookmark");

        Uuid::parse_str(&bookmark.id).expect("bookmark id is UUID");
        DateTime::parse_from_rfc3339(&bookmark.created_at)
            .expect("created_at is RFC3339 timestamp");
        assert_eq!(bookmark.title, "Example");
        assert_eq!(bookmark.url, "https://example.com");

        let stored = read_bookmarks(&path).expect("read saved bookmarks");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, bookmark.id);
    }

    #[test]
    fn duplicate_url_updates_title_instead_of_creating_duplicate() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(BOOKMARKS_FILE_NAME);

        let first = save_bookmark_at_path(
            &path,
            "Old Title".to_string(),
            "https://example.com".to_string(),
        )
        .expect("save first bookmark");
        let second = save_bookmark_at_path(
            &path,
            "New Title".to_string(),
            "https://example.com".to_string(),
        )
        .expect("update duplicate bookmark");

        let stored = read_bookmarks(&path).expect("read updated bookmarks");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, first.id);
        assert_eq!(stored[0].id, second.id);
        assert_eq!(stored[0].title, "New Title");
        assert_eq!(stored[0].created_at, first.created_at);
    }

    #[test]
    fn delete_removes_only_matching_id() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(BOOKMARKS_FILE_NAME);
        let first = save_bookmark_at_path(
            &path,
            "First".to_string(),
            "https://example.com".to_string(),
        )
        .expect("save first bookmark");
        let second = save_bookmark_at_path(
            &path,
            "Second".to_string(),
            "https://www.rust-lang.org".to_string(),
        )
        .expect("save second bookmark");

        delete_bookmark_at_path(&path, &first.id).expect("delete first bookmark");

        let stored = read_bookmarks(&path).expect("read remaining bookmarks");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, second.id);
    }

    #[test]
    fn is_bookmarked_matches_normalized_url_string_exactly() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(BOOKMARKS_FILE_NAME);
        save_bookmark_at_path(
            &path,
            "Example".to_string(),
            "https://example.com".to_string(),
        )
        .expect("save bookmark");

        assert!(
            is_bookmarked_at_path(&path, "https://example.com").expect("check exact bookmarked")
        );
        assert!(!is_bookmarked_at_path(&path, "https://example.com/")
            .expect("check non-exact bookmarked"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_failure_restores_existing_bookmarks_file() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(BOOKMARKS_FILE_NAME);
        let missing_temp_path = directory.path().join("missing.json.tmp");
        fs::write(&path, r#"[{"id":"existing"}]"#).expect("write existing bookmarks");

        let error =
            replace_file(&missing_temp_path, &path).expect_err("replace should fail without temp");

        assert!(error.contains("failed to save bookmarks file"));
        assert_eq!(
            fs::read_to_string(&path).expect("read restored bookmarks"),
            r#"[{"id":"existing"}]"#
        );
    }
}
