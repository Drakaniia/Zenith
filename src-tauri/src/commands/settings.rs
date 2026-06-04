use crate::models::AppSettings;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tauri::Manager;

const SETTINGS_FILE_NAME: &str = "settings.json";

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(SETTINGS_FILE_NAME))
        .map_err(|error| format!("failed to resolve app data directory: {error}"))
}

fn read_settings(path: &Path) -> Result<AppSettings, String> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read settings file: {error}"))?;

    Ok(serde_json::from_str(&contents).unwrap_or_else(|_| AppSettings::default()))
}

fn write_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "settings path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create settings directory: {error}"))?;

    let temp_path = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("failed to serialize settings: {error}"))?;

    {
        let mut temp_file = fs::File::create(&temp_path)
            .map_err(|error| format!("failed to create temporary settings file: {error}"))?;
        temp_file
            .write_all(&json)
            .map_err(|error| format!("failed to write temporary settings file: {error}"))?;
        temp_file
            .sync_all()
            .map_err(|error| format!("failed to flush temporary settings file: {error}"))?;
    }

    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to replace existing settings file: {error}"))?;
    }

    fs::rename(temp_path, path).map_err(|error| format!("failed to save settings file: {error}"))
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp_path, path).map_err(|error| format!("failed to save settings file: {error}"))
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    read_settings(&settings_path(&app)?)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    write_settings(&settings_path(&app)?, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_json_returns_default_settings() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(SETTINGS_FILE_NAME);

        let settings = read_settings(&path).expect("read missing settings");

        assert_eq!(settings.default_search_engine, "yandex");
        assert_eq!(settings.homepage, "https://yandex.com");
        assert_eq!(settings.theme, "dark");
        assert!(settings.javascript_enabled);
        assert!(settings.show_status_bar);
    }

    #[test]
    fn saving_settings_writes_valid_json() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join("nested").join(SETTINGS_FILE_NAME);
        let settings = AppSettings {
            default_search_engine: "google".to_string(),
            homepage: "https://example.com".to_string(),
            theme: "light".to_string(),
            javascript_enabled: false,
            show_status_bar: false,
        };

        write_settings(&path, &settings).expect("write settings");
        let contents = fs::read_to_string(path).expect("read settings json");
        let decoded: AppSettings = serde_json::from_str(&contents).expect("decode settings json");

        assert_eq!(
            decoded.default_search_engine,
            settings.default_search_engine
        );
        assert_eq!(decoded.homepage, settings.homepage);
        assert_eq!(decoded.theme, settings.theme);
        assert_eq!(decoded.javascript_enabled, settings.javascript_enabled);
        assert_eq!(decoded.show_status_bar, settings.show_status_bar);
    }

    #[test]
    fn invalid_settings_file_falls_back_to_default_without_panic() {
        let directory = tempfile::tempdir().expect("create temp dir");
        let path = directory.path().join(SETTINGS_FILE_NAME);
        fs::write(&path, "{not valid json").expect("write invalid json");

        let settings = read_settings(&path).expect("read invalid settings");

        assert_eq!(settings.default_search_engine, "yandex");
        assert_eq!(settings.homepage, "https://yandex.com");
        assert_eq!(settings.theme, "dark");
        assert!(settings.javascript_enabled);
        assert!(settings.show_status_bar);
    }
}
