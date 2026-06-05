use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub url: String,
    pub favicon_url: Option<String>,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    pub url: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub url: String,
    pub visited_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DownloadItem {
    pub id: String,
    pub filename: String,
    pub url: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: String,
    pub started_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub default_search_engine: String,
    pub homepage: String,
    pub theme: String,
    pub javascript_enabled: bool,
    pub show_status_bar: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageMetadata {
    pub title: String,
    pub favicon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DownloadProgress {
    pub id: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebviewBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebviewCreateRequest {
    pub label: String,
    pub url: String,
    pub bounds: WebviewBounds,
    pub javascript_enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebviewFindRequest {
    pub label: String,
    pub query: String,
    pub direction: String,
    pub case_sensitive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FindResult {
    pub active_match: u32,
    pub total_matches: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowState {
    pub is_minimized: bool,
    pub is_maximized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            default_search_engine: "yandex".to_string(),
            homepage: "browser://home".to_string(),
            theme: "dark".to_string(),
            javascript_enabled: true,
            show_status_bar: true,
        }
    }
}

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
        let entry = HistoryEntry {
            id: "history-1".to_string(),
            title: "Rust".to_string(),
            url: "https://www.rust-lang.org".to_string(),
            visited_at: "2026-06-04T08:05:00Z".to_string(),
        };

        let json = serde_json::to_string(&entry).expect("serialize history entry");
        let decoded: HistoryEntry = serde_json::from_str(&json).expect("deserialize history entry");

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
