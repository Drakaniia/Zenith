use crate::bridge::{
    dto::{DownloadItem, DownloadProgress},
    events,
};
use chrono::Utc;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

static DOWNLOADS: OnceLock<Mutex<Vec<DownloadRecord>>> = OnceLock::new();
static DOWNLOAD_APP: OnceLock<tauri::AppHandle> = OnceLock::new();

#[derive(Clone)]
struct DownloadRecord {
    item: DownloadItem,
    cancellation_requested: Arc<AtomicBool>,
    #[allow(dead_code)] // Reserved for Task 19 open-file/show-in-folder actions.
    local_path: Option<PathBuf>,
}

fn downloads() -> &'static Mutex<Vec<DownloadRecord>> {
    DOWNLOADS.get_or_init(|| Mutex::new(Vec::new()))
}

fn lock_downloads() -> Result<std::sync::MutexGuard<'static, Vec<DownloadRecord>>, String> {
    downloads()
        .lock()
        .map_err(|_| "download queue lock is poisoned".to_string())
}

fn download_progress(item: &DownloadItem) -> DownloadProgress {
    DownloadProgress {
        id: item.id.clone(),
        total_bytes: item.total_bytes,
        downloaded_bytes: item.downloaded_bytes,
        status: item.status.clone(),
    }
}

fn snapshot_downloads() -> Result<Vec<DownloadItem>, String> {
    Ok(lock_downloads()?
        .iter()
        .map(|record| record.item.clone())
        .collect())
}

fn clear_completed_download_records() -> Result<(), String> {
    lock_downloads()?.retain(|record| record.item.status != "completed");
    Ok(())
}

fn cancel_download_record(id: &str) -> Result<DownloadProgress, String> {
    let mut records = lock_downloads()?;
    let record = records
        .iter_mut()
        .find(|record| record.item.id == id)
        .ok_or_else(|| format!("download not found: {id}"))?;

    record.cancellation_requested.store(true, Ordering::SeqCst);
    if record.item.status != "completed" {
        record.item.status = "failed".to_string();
    }

    Ok(download_progress(&record.item))
}

fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("filename cannot be empty".to_string());
    }

    if filename.contains('/') || filename.contains('\\') {
        return Err("filename cannot contain path separators".to_string());
    }

    Ok(())
}

fn create_download_record(url: String, filename: String, local_path: PathBuf) -> DownloadRecord {
    DownloadRecord {
        item: DownloadItem {
            id: Uuid::new_v4().to_string(),
            filename,
            url,
            total_bytes: None,
            downloaded_bytes: 0,
            status: "downloading".to_string(),
            started_at: Utc::now().to_rfc3339(),
        },
        cancellation_requested: Arc::new(AtomicBool::new(false)),
        local_path: Some(local_path),
    }
}

fn update_download<F>(id: &str, update: F) -> Result<Option<DownloadProgress>, String>
where
    F: FnOnce(&mut DownloadRecord),
{
    let mut records = lock_downloads()?;
    let Some(record) = records.iter_mut().find(|record| record.item.id == id) else {
        return Ok(None);
    };

    update(record);
    Ok(Some(download_progress(&record.item)))
}

fn emit_download_progress(app: &tauri::AppHandle, payload: DownloadProgress) {
    let _ = events::emit_global(app, events::DOWNLOAD_PROGRESS, payload);
}

async fn run_download(
    app: tauri::AppHandle,
    id: String,
    url: String,
    local_path: PathBuf,
    cancellation_requested: Arc<AtomicBool>,
) {
    let result = async {
        let client = reqwest::Client::new();
        let mut response = client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("failed to start download: {error}"))?;

        response = response
            .error_for_status()
            .map_err(|error| format!("download request failed: {error}"))?;

        let total_bytes = response.content_length();
        if let Some(payload) = update_download(&id, |record| {
            record.item.total_bytes = total_bytes;
        })? {
            emit_download_progress(&app, payload);
        }

        let mut file = tokio::fs::File::create(&local_path)
            .await
            .map_err(|error| format!("failed to create download file: {error}"))?;

        loop {
            if cancellation_requested.load(Ordering::SeqCst) {
                return Err("download cancelled".to_string());
            }

            let next_chunk = tokio::select! {
                chunk = response.chunk() => {
                    chunk.map_err(|error| format!("failed to read download chunk: {error}"))?
                }
                _ = wait_for_cancellation(Arc::clone(&cancellation_requested)) => {
                    return Err("download cancelled".to_string());
                }
            };

            let Some(chunk) = next_chunk else {
                break;
            };

            file.write_all(&chunk)
                .await
                .map_err(|error| format!("failed to write download chunk: {error}"))?;

            let chunk_len = chunk.len() as u64;
            if let Some(payload) = update_download(&id, |record| {
                record.item.downloaded_bytes =
                    record.item.downloaded_bytes.saturating_add(chunk_len);
            })? {
                emit_download_progress(&app, payload);
            }
        }

        file.flush()
            .await
            .map_err(|error| format!("failed to flush download file: {error}"))?;

        Ok::<(), String>(())
    }
    .await;

    let final_status = if result.is_ok() && !cancellation_requested.load(Ordering::SeqCst) {
        "completed"
    } else {
        "failed"
    };

    if let Some(payload) = update_download(&id, |record| {
        record.item.status = final_status.to_string();
    })
    .unwrap_or(None)
    {
        emit_download_progress(&app, payload);
    }
}

async fn wait_for_cancellation(cancellation_requested: Arc<AtomicBool>) {
    while !cancellation_requested.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tauri::command]
pub fn get_downloads() -> Result<Vec<DownloadItem>, String> {
    snapshot_downloads()
}

#[tauri::command]
pub async fn start_download(
    app: tauri::AppHandle,
    url: String,
    filename: String,
) -> Result<String, String> {
    validate_filename(&filename)?;

    let _ = DOWNLOAD_APP.set(app.clone());

    let downloads_dir =
        dirs::download_dir().ok_or_else(|| "failed to resolve Downloads directory".to_string())?;
    let local_path = downloads_dir.join(&filename);
    let record = create_download_record(url.clone(), filename, local_path.clone());
    let id = record.item.id.clone();
    let cancellation_requested = Arc::clone(&record.cancellation_requested);

    lock_downloads()?.push(record);

    tauri::async_runtime::spawn(run_download(
        app,
        id.clone(),
        url,
        local_path,
        cancellation_requested,
    ));

    Ok(id)
}

#[tauri::command]
pub fn cancel_download(id: String) -> Result<(), String> {
    let payload = cancel_download_record(&id)?;
    if let Some(app) = DOWNLOAD_APP.get() {
        emit_download_progress(app, payload);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_completed_downloads() -> Result<(), String> {
    clear_completed_download_records()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_guard() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock")
    }

    fn reset_downloads_for_test() {
        lock_downloads().expect("lock downloads").clear();
    }

    fn record(id: &str, status: &str, downloaded_bytes: u64) -> DownloadRecord {
        DownloadRecord {
            item: DownloadItem {
                id: id.to_string(),
                filename: format!("{id}.txt"),
                url: format!("https://example.com/{id}.txt"),
                total_bytes: Some(100),
                downloaded_bytes,
                status: status.to_string(),
                started_at: "2026-06-04T08:10:00Z".to_string(),
            },
            cancellation_requested: Arc::new(AtomicBool::new(false)),
            local_path: Some(PathBuf::from(format!("{id}.txt"))),
        }
    }

    #[test]
    fn get_downloads_returns_current_queue_snapshot() {
        let _guard = test_guard();
        reset_downloads_for_test();
        lock_downloads().expect("lock downloads").extend([
            record("download-1", "downloading", 25),
            record("download-2", "failed", 40),
        ]);

        let downloads = get_downloads().expect("get downloads");

        assert_eq!(downloads.len(), 2);
        assert_eq!(downloads[0].id, "download-1");
        assert_eq!(downloads[0].status, "downloading");
        assert_eq!(downloads[0].downloaded_bytes, 25);
        assert_eq!(downloads[1].id, "download-2");
    }

    #[test]
    fn clear_completed_downloads_removes_only_completed_records() {
        let _guard = test_guard();
        reset_downloads_for_test();
        lock_downloads().expect("lock downloads").extend([
            record("active", "downloading", 10),
            record("done", "completed", 100),
            record("failed", "failed", 50),
            record("paused", "paused", 60),
        ]);

        clear_completed_downloads().expect("clear completed downloads");
        let downloads = get_downloads().expect("get downloads");

        let ids = downloads
            .iter()
            .map(|download| download.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["active", "failed", "paused"]);
    }

    #[test]
    fn cancel_download_marks_existing_download_failed_and_sets_cancellation_flag() {
        let _guard = test_guard();
        reset_downloads_for_test();
        lock_downloads()
            .expect("lock downloads")
            .push(record("active", "downloading", 10));

        let payload = cancel_download_record("active").expect("cancel download");

        let records = lock_downloads().expect("lock downloads");
        let cancelled = records
            .iter()
            .find(|record| record.item.id == "active")
            .expect("cancelled record");
        assert_eq!(cancelled.item.status, "failed");
        assert!(cancelled.cancellation_requested.load(Ordering::SeqCst));
        assert_eq!(payload.id, "active");
        assert_eq!(payload.status, "failed");
        assert_eq!(payload.downloaded_bytes, 10);
    }

    #[test]
    fn cancel_download_missing_id_returns_clear_error() {
        let _guard = test_guard();
        reset_downloads_for_test();

        let error = cancel_download("missing".to_string()).expect_err("missing download fails");

        assert_eq!(error, "download not found: missing");
    }
}
