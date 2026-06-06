use crate::{
    bridge::{
        dto::{FindResult, WebviewBounds, WebviewFindRequest},
        events,
    },
    core::{ids::BROWSER_WEBVIEW_LABEL_PREFIX, url::is_http_or_https_scheme},
};
use serde::{Deserialize, Serialize};
use std::{sync::mpsc, time::Duration};
use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, Position, Rect, Size, Webview, WebviewUrl,
};

const FIND_CALLBACK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize)]
struct BrowserWebviewNavigationEvent {
    label: String,
    url: String,
}

#[derive(Clone, Debug, Serialize)]
struct BrowserWebviewTitleEvent {
    label: String,
    title: String,
}

#[derive(Clone, Debug, Serialize)]
struct BrowserWebviewLoadEvent {
    label: String,
    url: String,
    state: BrowserWebviewLoadState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum BrowserWebviewLoadState {
    Started,
    Finished,
}

#[derive(Debug, Deserialize)]
struct FindScriptResult {
    active_match: u32,
    total_matches: u32,
}

#[tauri::command]
pub async fn create_browser_webview(
    app: AppHandle,
    shell_webview: Webview,
    label: String,
    url: String,
    bounds: WebviewBounds,
    javascript_enabled: bool,
    incognito: bool,
) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    validate_bounds("create_browser_webview", &label, &bounds)?;
    let parsed_url = parse_remote_url("create_browser_webview", &label, &url)?;

    if app.get_webview(&label).is_some() {
        return Err(format!(
            "create_browser_webview({label}): webview label already exists"
        ));
    }

    let shell_label = shell_webview.label().to_string();
    let parent_window = shell_webview.window();

    let navigation_app = app.clone();
    let navigation_shell_label = shell_label.clone();
    let navigation_label = label.clone();
    let mut builder = WebviewBuilder::new(label.clone(), WebviewUrl::External(parsed_url))
        .on_navigation(move |next_url| {
            if !is_supported_browser_webview_url(next_url) {
                return false;
            }

            let payload = BrowserWebviewNavigationEvent {
                label: navigation_label.clone(),
                url: next_url.to_string(),
            };
            let _ = events::emit_to_shell(
                &navigation_app,
                navigation_shell_label.clone(),
                events::BROWSER_WEBVIEW_NAVIGATION,
                payload,
            );
            true
        });

    let title_app = app.clone();
    let title_shell_label = shell_label.clone();
    let title_label = label.clone();
    builder = builder.on_document_title_changed(move |_webview, title| {
        let payload = BrowserWebviewTitleEvent {
            label: title_label.clone(),
            title,
        };
        let _ = events::emit_to_shell(
            &title_app,
            title_shell_label.clone(),
            events::BROWSER_WEBVIEW_TITLE,
            payload,
        );
    });

    let load_app = app.clone();
    let load_shell_label = shell_label.clone();
    let load_label = label.clone();
    builder = builder.on_page_load(move |_webview, payload| {
        let state = match payload.event() {
            PageLoadEvent::Started => BrowserWebviewLoadState::Started,
            PageLoadEvent::Finished => BrowserWebviewLoadState::Finished,
        };
        let event_payload = BrowserWebviewLoadEvent {
            label: load_label.clone(),
            url: payload.url().to_string(),
            state,
        };
        let _ = events::emit_to_shell(
            &load_app,
            load_shell_label.clone(),
            events::BROWSER_WEBVIEW_LOAD,
            event_payload,
        );
    });

    if incognito {
        builder = builder.incognito(true);
    }

    if !javascript_enabled {
        builder = builder.disable_javascript();
    }

    let browser_webview = parent_window
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        )
        .map_err(|error| format!("create_browser_webview({label}): {error}"))?;

    install_browser_shortcut_bridge(&app, &browser_webview, &shell_label, &label)?;

    Ok(())
}

#[tauri::command]
pub fn navigate_browser_webview(app: AppHandle, label: String, url: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    let webview = browser_webview(&app, "navigate_browser_webview", &label)?;
    let parsed_url = parse_remote_url("navigate_browser_webview", &label, &url)?;

    webview
        .navigate(parsed_url)
        .map_err(|error| format!("navigate_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn reload_browser_webview(app: AppHandle, label: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "reload_browser_webview", &label)?
        .reload()
        .map_err(|error| format!("reload_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn close_browser_webview(app: AppHandle, label: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "close_browser_webview", &label)?
        .close()
        .map_err(|error| format!("close_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn close_all_browser_webviews(app: AppHandle) -> Result<(), String> {
    let browser_webviews = app
        .webviews()
        .into_iter()
        .filter(|(label, _)| label.starts_with(BROWSER_WEBVIEW_LABEL_PREFIX))
        .collect::<Vec<_>>();
    let mut errors = Vec::new();

    for (label, webview) in browser_webviews {
        if let Err(error) = webview.hide() {
            errors.push(format!("hide_browser_webview({label}): {error}"));
        }

        if let Err(error) = webview.close() {
            errors.push(format!("close_browser_webview({label}): {error}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[tauri::command]
pub fn show_browser_webview(app: AppHandle, label: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "show_browser_webview", &label)?
        .show()
        .map_err(|error| format!("show_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn hide_browser_webview(app: AppHandle, label: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "hide_browser_webview", &label)?
        .hide()
        .map_err(|error| format!("hide_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn set_browser_webview_bounds(
    app: AppHandle,
    label: String,
    bounds: WebviewBounds,
) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    validate_bounds("set_browser_webview_bounds", &label, &bounds)?;
    browser_webview(&app, "set_browser_webview_bounds", &label)?
        .set_bounds(bounds_to_rect(bounds))
        .map_err(|error| format!("set_browser_webview_bounds({label}): {error}"))
}

#[tauri::command]
pub fn get_browser_webview_url(app: AppHandle, label: String) -> Result<String, String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "get_browser_webview_url", &label)?
        .url()
        .map(|url| url.to_string())
        .map_err(|error| format!("get_browser_webview_url({label}): {error}"))
}

#[tauri::command]
pub fn eval_browser_webview(app: AppHandle, label: String, script: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "eval_browser_webview", &label)?
        .eval(script)
        .map_err(|error| format!("eval_browser_webview({label}): {error}"))
}

#[tauri::command]
pub fn find_in_browser_webview(
    app: AppHandle,
    request: WebviewFindRequest,
) -> Result<FindResult, String> {
    validate_browser_webview_label(&request.label)?;
    let webview = browser_webview(&app, "find_in_browser_webview", &request.label)?;
    let script = find_script(&request)?;
    let (sender, receiver) = mpsc::channel();

    webview
        .eval_with_callback(script, move |value| {
            let _ = sender.send(value);
        })
        .map_err(|error| {
            format!(
                "find_in_browser_webview({}): find in page is unavailable on this platform: {error}",
                request.label
            )
        })?;

    let raw = receiver.recv_timeout(FIND_CALLBACK_TIMEOUT).map_err(|error| {
        format!(
            "find_in_browser_webview({}): find in page is unavailable on this platform: callback did not return within {} seconds: {error}",
            request.label,
            FIND_CALLBACK_TIMEOUT.as_secs()
        )
    })?;

    let result: FindScriptResult = serde_json::from_str(&raw).map_err(|error| {
        format!(
            "find_in_browser_webview({}): failed to decode find result: {error}",
            request.label
        )
    })?;

    Ok(FindResult {
        active_match: result.active_match,
        total_matches: result.total_matches,
    })
}

#[tauri::command]
pub fn clear_browser_webview_find(app: AppHandle, label: String) -> Result<(), String> {
    validate_browser_webview_label(&label)?;
    browser_webview(&app, "clear_browser_webview_find", &label)?
        .eval(
            r#"
            (() => {
              delete window.__ampuiFindState;
              const selection = window.getSelection && window.getSelection();
              if (selection) {
                selection.removeAllRanges();
              }
            })();
            "#,
        )
        .map_err(|error| format!("clear_browser_webview_find({label}): {error}"))
}

fn browser_webview(app: &AppHandle, command: &str, label: &str) -> Result<Webview, String> {
    app.get_webview(label)
        .ok_or_else(|| format!("{command}({label}): webview was not found"))
}

fn parse_remote_url(command: &str, label: &str, url: &str) -> Result<tauri::Url, String> {
    let parsed = tauri::Url::parse(url)
        .map_err(|error| format!("{command}({label}): invalid URL '{url}': {error}"))?;

    if is_supported_browser_webview_url(&parsed) {
        Ok(parsed)
    } else {
        let scheme = parsed.scheme();
        Err(format!(
            "{command}({label}): unsupported URL scheme '{scheme}'; browser webviews only accept http and https URLs"
        ))
    }
}

fn is_supported_browser_webview_url(url: &tauri::Url) -> bool {
    is_http_or_https_scheme(url.scheme())
}

fn validate_bounds(command: &str, label: &str, bounds: &WebviewBounds) -> Result<(), String> {
    if !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
    {
        return Err(format!(
            "{command}({label}): webview bounds must contain only finite values"
        ));
    }

    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err(format!(
            "{command}({label}): webview bounds width and height must be greater than zero"
        ));
    }

    Ok(())
}

fn bounds_to_rect(bounds: WebviewBounds) -> Rect {
    Rect {
        position: Position::Logical(LogicalPosition::new(bounds.x, bounds.y)),
        size: Size::Logical(LogicalSize::new(bounds.width, bounds.height)),
    }
}

#[cfg(target_os = "windows")]
fn install_browser_shortcut_bridge(
    app: &AppHandle,
    webview: &Webview,
    shell_label: &str,
    label: &str,
) -> Result<(), String> {
    configure_browser_webview_settings(webview, label)?;
    install_browser_accelerator_handler(app, webview, shell_label, label)
}

#[cfg(not(target_os = "windows"))]
fn install_browser_shortcut_bridge(
    _app: &AppHandle,
    _webview: &Webview,
    _shell_label: &str,
    _label: &str,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_browser_webview_settings(webview: &Webview, label: &str) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;

    let (sender, receiver) = mpsc::channel();
    let label_for_webview = label.to_string();
    let label_for_dispatch = label_for_webview.clone();

    webview
        .with_webview(move |platform_webview| {
            let label = label_for_webview;
            let result = (|| -> Result<(), String> {
                // SAFETY: WebView2 COM methods must run on the webview thread; Tauri executes
                // this closure through `with_webview` on that owning thread.
                unsafe {
                    let core = platform_webview
                        .controller()
                        .CoreWebView2()
                        .map_err(|error| {
                            format!("configure_browser_webview_settings({label}): {error}")
                        })?;
                    let settings = core.Settings().map_err(|error| {
                        format!("configure_browser_webview_settings({label}): {error}")
                    })?;

                    settings.SetAreDevToolsEnabled(false).map_err(|error| {
                        format!("configure_browser_webview_settings({label}): {error}")
                    })?;

                    match settings.cast::<ICoreWebView2Settings3>() {
                        Ok(settings) => settings
                            .SetAreBrowserAcceleratorKeysEnabled(false)
                            .map_err(|error| {
                                format!("configure_browser_webview_settings({label}): {error}")
                            }),
                        Err(_) => Ok(()),
                    }
                }
            })();

            let _ = sender.send(result);
        })
        .map_err(|error| {
            format!("configure_browser_webview_settings({label_for_dispatch}): {error}")
        })?;

    receiver
        .recv_timeout(FIND_CALLBACK_TIMEOUT)
        .map_err(|error| {
            format!("configure_browser_webview_settings({label_for_dispatch}): {error}")
        })?
}

#[cfg(target_os = "windows")]
fn install_browser_accelerator_handler(
    app: &AppHandle,
    webview: &Webview,
    shell_label: &str,
    label: &str,
) -> Result<(), String> {
    use webview2_com::{
        AcceleratorKeyPressedEventHandler,
        Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
            COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
        },
    };

    let (sender, receiver) = mpsc::channel();
    let label_for_webview = label.to_string();
    let label_for_dispatch = label_for_webview.clone();
    let shell_label_for_handler = shell_label.to_string();
    let app_for_handler = app.clone();

    webview
        .with_webview(move |platform_webview| {
            let label = label_for_webview;
            let result = (|| -> Result<(), String> {
                let controller = platform_webview.controller();
                let event_handler =
                    AcceleratorKeyPressedEventHandler::create(Box::new(move |_sender, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };

                        let mut event_kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
                        let mut virtual_key = 0;

                        unsafe {
                            if args.KeyEventKind(&mut event_kind).is_err()
                                || args.VirtualKey(&mut virtual_key).is_err()
                            {
                                return Ok(());
                            }
                        }

                        if event_kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                            && event_kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                        {
                            return Ok(());
                        }

                        let input = crate::runtime::shortcuts::NativeShortcutKey {
                            virtual_key,
                            ctrl: is_windows_virtual_key_down(VK_CONTROL),
                            shift: is_windows_virtual_key_down(VK_SHIFT),
                            alt: is_windows_virtual_key_down(VK_MENU),
                            meta: is_windows_virtual_key_down(VK_LWIN)
                                || is_windows_virtual_key_down(VK_RWIN),
                        };

                        if let Some(command) =
                            crate::runtime::shortcuts::command_for_native_key(input)
                        {
                            unsafe {
                                let _ = args.SetHandled(true);
                            }
                            let _ = crate::runtime::shortcuts::emit_shortcut_to_shell(
                                &app_for_handler,
                                &shell_label_for_handler,
                                command,
                            );
                        }

                        Ok(())
                    }));
                let mut token = 0;

                unsafe {
                    controller
                        .add_AcceleratorKeyPressed(&event_handler, &mut token)
                        .map_err(|error| {
                            format!("install_browser_accelerator_handler({label}): {error}")
                        })?;
                }

                Ok(())
            })();

            let _ = sender.send(result);
        })
        .map_err(|error| {
            format!("install_browser_accelerator_handler({label_for_dispatch}): {error}")
        })?;

    receiver
        .recv_timeout(FIND_CALLBACK_TIMEOUT)
        .map_err(|error| {
            format!("install_browser_accelerator_handler({label_for_dispatch}): {error}")
        })?
}

#[cfg(target_os = "windows")]
fn is_windows_virtual_key_down(virtual_key: i32) -> bool {
    extern "system" {
        fn GetKeyState(n_virt_key: i32) -> i16;
    }

    unsafe { GetKeyState(virtual_key) < 0 }
}

#[cfg(target_os = "windows")]
const VK_SHIFT: i32 = 0x10;
#[cfg(target_os = "windows")]
const VK_CONTROL: i32 = 0x11;
#[cfg(target_os = "windows")]
const VK_MENU: i32 = 0x12;
#[cfg(target_os = "windows")]
const VK_LWIN: i32 = 0x5b;
#[cfg(target_os = "windows")]
const VK_RWIN: i32 = 0x5c;

fn validate_browser_webview_label(label: &str) -> Result<(), String> {
    let id = label.strip_prefix(BROWSER_WEBVIEW_LABEL_PREFIX).ok_or_else(|| {
        format!("invalid browser webview label '{label}': expected 'tab_' followed by 32 lowercase hex characters")
    })?;

    if id.len() == 32
        && id
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        Ok(())
    } else {
        Err(format!(
            "invalid browser webview label '{label}': expected 'tab_' followed by 32 lowercase hex characters"
        ))
    }
}

fn find_script(request: &WebviewFindRequest) -> Result<String, String> {
    let query = serde_json::to_string(&request.query)
        .map_err(|error| format!("find_in_browser_webview({}): {error}", request.label))?;
    let direction = serde_json::to_string(&request.direction)
        .map_err(|error| format!("find_in_browser_webview({}): {error}", request.label))?;
    let case_sensitive = request.case_sensitive;

    Ok(format!(
        r#"
        (() => {{
          const query = {query};
          const direction = {direction};
          const caseSensitive = {case_sensitive};
          const normalize = (value) => caseSensitive ? value : value.toLocaleLowerCase();
          const bodyText = document.body ? (document.body.innerText || document.body.textContent || "") : "";
          const needle = normalize(query);
          const haystack = normalize(bodyText);
          const key = JSON.stringify([query, caseSensitive]);

          if (!query || !needle) {{
            delete window.__ampuiFindState;
            const selection = window.getSelection && window.getSelection();
            if (selection) {{
              selection.removeAllRanges();
            }}
            return {{ active_match: 0, total_matches: 0 }};
          }}

          let totalMatches = 0;
          let offset = 0;
          while (true) {{
            const index = haystack.indexOf(needle, offset);
            if (index === -1) {{
              break;
            }}
            totalMatches += 1;
            offset = index + Math.max(needle.length, 1);
          }}

          if (totalMatches === 0) {{
            window.__ampuiFindState = {{ key, activeMatch: 0, totalMatches: 0 }};
            return {{ active_match: 0, total_matches: 0 }};
          }}

          const previousState = window.__ampuiFindState;
          const backwards = ["previous", "backward", "backwards", "up"].includes(String(direction).toLocaleLowerCase());
          let activeMatch;
          if (previousState && previousState.key === key) {{
            const previousMatch = Number(previousState.activeMatch || 0);
            activeMatch = backwards
              ? (previousMatch + totalMatches - 1) % totalMatches
              : (previousMatch + 1) % totalMatches;
          }} else {{
            activeMatch = backwards ? totalMatches - 1 : 0;
          }}

          if (typeof window.find === "function") {{
            window.find(query, caseSensitive, backwards, true, false, false, false);
          }}

          window.__ampuiFindState = {{ key, activeMatch, totalMatches }};
          return {{
            active_match: activeMatch + 1,
            total_matches: totalMatches
          }};
        }})();
        "#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_label() -> &'static str {
        "tab_0123456789abcdef0123456789abcdef"
    }

    #[test]
    fn parse_remote_url_accepts_only_http_and_https() {
        assert!(parse_remote_url("test", valid_label(), "http://example.com").is_ok());
        assert!(parse_remote_url("test", valid_label(), "https://example.com").is_ok());

        for url in [
            "file:///C:/Windows/System32/drivers/etc/hosts",
            "data:text/html,hello",
            "javascript:alert(1)",
            "about:blank",
        ] {
            let error = parse_remote_url("test", valid_label(), url)
                .expect_err("unsupported scheme should fail");
            assert!(error.contains("unsupported URL scheme"));
        }
    }

    #[test]
    fn validate_browser_webview_label_requires_tab_prefix_and_uuid_hex() {
        assert!(validate_browser_webview_label(valid_label()).is_ok());

        for label in [
            "main",
            "tab_0123456789abcdef0123456789abcde",
            "tab_0123456789abcdef0123456789abcdeg",
            "tab_0123456789ABCDEF0123456789ABCDEF",
        ] {
            assert!(validate_browser_webview_label(label).is_err());
        }
    }

    #[test]
    fn validate_bounds_rejects_non_finite_or_non_positive_dimensions() {
        assert!(validate_bounds(
            "test",
            valid_label(),
            &WebviewBounds {
                x: 0.0,
                y: 0.0,
                width: 640.0,
                height: 480.0,
            },
        )
        .is_ok());

        for bounds in [
            WebviewBounds {
                x: f64::NAN,
                y: 0.0,
                width: 640.0,
                height: 480.0,
            },
            WebviewBounds {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 480.0,
            },
            WebviewBounds {
                x: 0.0,
                y: 0.0,
                width: 640.0,
                height: -1.0,
            },
        ] {
            assert!(validate_bounds("test", valid_label(), &bounds).is_err());
        }
    }

    #[test]
    fn find_script_json_escapes_query_and_direction() {
        let request = WebviewFindRequest {
            label: valid_label().to_string(),
            query: "\"; window.__pwned = true; //".to_string(),
            direction: "next\"; alert(1); //".to_string(),
            case_sensitive: true,
        };

        let script = find_script(&request).expect("build find script");
        let encoded_query = serde_json::to_string(&request.query).expect("encode query");
        let encoded_direction =
            serde_json::to_string(&request.direction).expect("encode direction");

        assert!(script.contains(&format!("const query = {encoded_query};")));
        assert!(script.contains(&format!("const direction = {encoded_direction};")));
    }
}
