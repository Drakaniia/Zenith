use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub default_search_engine: String,
    pub homepage: String,
    pub theme: String,
    pub javascript_enabled: bool,
    pub show_status_bar: bool,
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
