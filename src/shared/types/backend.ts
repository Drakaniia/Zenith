export interface Tab {
  id: string;
  title: string;
  url: string;
  favicon_url: string | null;
  is_loading: boolean;
  can_go_back: boolean;
  can_go_forward: boolean;
}

export interface Bookmark {
  id: string;
  title: string;
  url: string;
  created_at: string;
}

export interface HistoryEntry {
  id: string;
  title: string;
  url: string;
  visited_at: string;
}

export interface DownloadItem {
  id: string;
  filename: string;
  url: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: string;
  started_at: string;
}

export interface AppSettings {
  default_search_engine: string;
  homepage: string;
  theme: string;
  javascript_enabled: boolean;
  show_status_bar: boolean;
}

export interface PageMetadata {
  title: string;
  favicon_url: string | null;
}

export interface DownloadProgress {
  id: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: string;
}

export interface WebviewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FindResult {
  active_match: number;
  total_matches: number;
}

export interface WindowState {
  is_minimized: boolean;
  is_maximized: boolean;
}
