use crate::core::ids::{BROWSER_WEBVIEW_LABEL_PREFIX, BROWSER_WINDOW_LABEL_PREFIX};
use tauri::{AppHandle, Manager, Webview};

const MAIN_WEBVIEW_LABEL: &str = "main";

#[tauri::command]
pub fn toggle_devtools(
    app: AppHandle,
    webview: Webview,
    label: Option<String>,
) -> Result<(), String> {
    let target_label = resolve_devtools_target_label(label.as_deref(), webview.label())?;
    let target = app
        .get_webview(&target_label)
        .ok_or_else(|| format!("toggle_devtools({target_label}): webview was not found"))?;

    toggle_webview_devtools(&target);
    Ok(())
}

fn resolve_devtools_target_label(
    requested_label: Option<&str>,
    current_webview_label: &str,
) -> Result<String, String> {
    let target_label = requested_label.unwrap_or(current_webview_label);

    if is_managed_devtools_target_label(target_label) {
        Ok(target_label.to_string())
    } else {
        Err(format!(
            "toggle_devtools({target_label}): unsupported webview label"
        ))
    }
}

fn is_managed_devtools_target_label(label: &str) -> bool {
    label == MAIN_WEBVIEW_LABEL
        || has_prefixed_hex_id(label, BROWSER_WEBVIEW_LABEL_PREFIX)
        || has_prefixed_hex_id(label, BROWSER_WINDOW_LABEL_PREFIX)
}

fn has_prefixed_hex_id(label: &str, prefix: &str) -> bool {
    let Some(id) = label.strip_prefix(prefix) else {
        return false;
    };

    id.len() == 32
        && id
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

#[cfg(target_os = "windows")]
fn toggle_webview_devtools(webview: &Webview) {
    webview.open_devtools();
}

#[cfg(not(target_os = "windows"))]
fn toggle_webview_devtools(webview: &Webview) {
    if webview.is_devtools_open() {
        webview.close_devtools();
    } else {
        webview.open_devtools();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_tab_label() -> &'static str {
        "tab_0123456789abcdef0123456789abcdef"
    }

    fn valid_shell_window_label() -> &'static str {
        "browser_window_0123456789abcdef0123456789abcdef"
    }

    #[test]
    fn devtools_target_defaults_to_current_shell_webview() {
        assert_eq!(
            resolve_devtools_target_label(None, MAIN_WEBVIEW_LABEL).expect("main is valid"),
            MAIN_WEBVIEW_LABEL
        );
        assert_eq!(
            resolve_devtools_target_label(None, valid_shell_window_label())
                .expect("managed shell window is valid"),
            valid_shell_window_label()
        );
    }

    #[test]
    fn devtools_target_accepts_managed_browser_webview_label() {
        assert_eq!(
            resolve_devtools_target_label(Some(valid_tab_label()), MAIN_WEBVIEW_LABEL)
                .expect("managed tab webview is valid"),
            valid_tab_label()
        );
    }

    #[test]
    fn devtools_target_rejects_unmanaged_labels() {
        for label in [
            "",
            "settings",
            "tab_0123456789abcdef0123456789abcde",
            "tab_0123456789ABCDEF0123456789ABCDEF",
            "browser_window_0123456789abcdef0123456789abcde",
        ] {
            assert!(
                resolve_devtools_target_label(Some(label), MAIN_WEBVIEW_LABEL).is_err(),
                "{label} should be rejected"
            );
        }
    }

    #[test]
    fn devtools_target_rejects_unmanaged_current_webview_label() {
        assert!(resolve_devtools_target_label(None, "settings").is_err());
    }

    #[test]
    fn managed_devtools_target_labels_are_explicit() {
        assert!(is_managed_devtools_target_label(MAIN_WEBVIEW_LABEL));
        assert!(is_managed_devtools_target_label(valid_tab_label()));
        assert!(is_managed_devtools_target_label(valid_shell_window_label()));
        assert!(!is_managed_devtools_target_label("tab_short"));
        assert!(!is_managed_devtools_target_label("main_extra"));
    }
}
