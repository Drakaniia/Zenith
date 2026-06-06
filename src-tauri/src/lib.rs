pub mod bridge;
pub mod core;
pub mod features;
pub mod persistence;
pub mod runtime;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_devtools::init())
        .plugin(tauri_plugin_devtools_app::init());

    builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(generate_ampui_handler!())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            disable_main_webview_native_devtools(app)?;

            runtime::shortcuts::install(app.handle())?;
            Ok(())
        })
        .on_menu_event(runtime::shortcuts::handle_menu_event)
        .run(tauri::generate_context!())
        .expect("error while running Rust Browser");
}

#[cfg(target_os = "windows")]
fn disable_main_webview_native_devtools(app: &tauri::App) -> Result<(), std::io::Error> {
    use std::{sync::mpsc, time::Duration};
    use tauri::Manager;

    let Some(main_webview) = app.get_webview("main") else {
        return Ok(());
    };

    let (sender, receiver) = mpsc::channel();

    main_webview
        .with_webview(move |platform_webview| {
            let result = (|| -> Result<(), std::io::Error> {
                // SAFETY: WebView2 COM methods must run on the webview thread; Tauri executes
                // this closure through `with_webview` on that owning thread.
                unsafe {
                    let core = platform_webview
                        .controller()
                        .CoreWebView2()
                        .map_err(|error| {
                            std::io::Error::other(format!(
                                "disable_main_webview_native_devtools(main): {error}"
                            ))
                        })?;
                    let settings = core.Settings().map_err(|error| {
                        std::io::Error::other(format!(
                            "disable_main_webview_native_devtools(main): {error}"
                        ))
                    })?;

                    settings.SetAreDevToolsEnabled(false).map_err(|error| {
                        std::io::Error::other(format!(
                            "disable_main_webview_native_devtools(main): {error}"
                        ))
                    })?;
                }

                Ok(())
            })();

            let _ = sender.send(result);
        })
        .map_err(|error| {
            std::io::Error::other(format!(
                "disable_main_webview_native_devtools(main): {error}"
            ))
        })?;

    receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("disable_main_webview_native_devtools(main): {error}"),
            )
        })?
}
