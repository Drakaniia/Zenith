pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod navigation;
pub mod settings;
pub mod tabs;
pub mod webviews;
pub mod windows;

pub use bookmarks::Bookmark;
pub use downloads::{DownloadItem, DownloadProgress};
pub use history::HistoryEntry;
pub use navigation::PageMetadata;
pub use settings::AppSettings;
pub use tabs::Tab;
pub use webviews::{FindResult, WebviewBounds, WebviewCreateRequest, WebviewFindRequest};
pub use windows::WindowState;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_and_deserializes_bookmark() {
        let bookmark = Bookmark {
            id: "bookmark-1".to_string(),
            title: "Example".to_string(),
            url: "https://example.com".to_string(),
            created_at: "2026-06-04T08:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&bookmark).expect("serialize bookmark");
        let decoded: Bookmark = serde_json::from_str(&json).expect("deserialize bookmark");

        assert_eq!(decoded.id, bookmark.id);
        assert_eq!(decoded.title, bookmark.title);
        assert_eq!(decoded.url, bookmark.url);
        assert_eq!(decoded.created_at, bookmark.created_at);
    }

    #[test]
    fn serializes_and_deserializes_history_entry() {
        let entry = history::HistoryEntry {
            id: "history-1".to_string(),
            title: "Rust".to_string(),
            url: "https://www.rust-lang.org".to_string(),
            visited_at: "2026-06-04T08:05:00Z".to_string(),
        };

        let json = serde_json::to_string(&entry).expect("serialize history entry");
        let decoded: history::HistoryEntry =
            serde_json::from_str(&json).expect("deserialize history entry");

        assert_eq!(decoded.id, entry.id);
        assert_eq!(decoded.title, entry.title);
        assert_eq!(decoded.url, entry.url);
        assert_eq!(decoded.visited_at, entry.visited_at);
    }

    #[test]
    fn serializes_and_deserializes_download_item() {
        let download = DownloadItem {
            id: "download-1".to_string(),
            filename: "example.txt".to_string(),
            url: "https://example.com/example.txt".to_string(),
            total_bytes: Some(1024),
            downloaded_bytes: 512,
            status: "downloading".to_string(),
            started_at: "2026-06-04T08:10:00Z".to_string(),
        };

        let json = serde_json::to_string(&download).expect("serialize download item");
        let decoded: DownloadItem = serde_json::from_str(&json).expect("deserialize download item");

        assert_eq!(decoded.id, download.id);
        assert_eq!(decoded.filename, download.filename);
        assert_eq!(decoded.url, download.url);
        assert_eq!(decoded.total_bytes, download.total_bytes);
        assert_eq!(decoded.downloaded_bytes, download.downloaded_bytes);
        assert_eq!(decoded.status, download.status);
        assert_eq!(decoded.started_at, download.started_at);
    }

    #[test]
    fn serializes_and_deserializes_default_app_settings() {
        let settings = AppSettings::default();

        let json = serde_json::to_string(&settings).expect("serialize app settings");
        let decoded: AppSettings = serde_json::from_str(&json).expect("deserialize app settings");

        assert_eq!(decoded.default_search_engine, "yandex");
        assert_eq!(decoded.homepage, "browser://home");
        assert_eq!(decoded.theme, "dark");
        assert!(decoded.javascript_enabled);
        assert!(decoded.show_status_bar);
    }
}
