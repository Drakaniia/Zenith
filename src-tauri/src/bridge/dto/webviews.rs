use serde::{Deserialize, Serialize};

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
