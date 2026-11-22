//! Phase 2 CEF rendering (native child windows, 4-pane split, overlay z-order) and MV3 extension go/no-go prototype.

mod native_host;
mod server;
mod win_host;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cef::args::Args;
use cef::*;
use serde::Serialize;
use windows::Win32::Foundation::HWND;

use server::{ExtensionSignals, FixtureServer};
use win_host::{sample_process_tree_memory, HostWindow, MemorySnapshot, ResizeStats};

const CEF_PINNED_VERSION: &str = "156.0.31+ge603d97+chromium-156.0.8078.100";
const EXPECTED_EXT_ID: &str = "fkjmjfpacnjjgnppeenhiolleglehgik";

#[derive(Debug, Clone, Default, Serialize)]
struct Phase2Report {
    cef_version: String,
    requested_primary_style: String,
    actual_primary_style: Option<String>,
    actual_alloy_probe_style: Option<String>,
    chrome_child_embed_ok: bool,
    alloy_child_embed_ok: bool,
    startup_to_first_load_ms: Option<u64>,
    overlay_above_pages_ok: bool,
    split_4pane_created: usize,
    split_resize_stats: ResizeStats,
    memory_1tab: MemorySnapshot,
    memory_4pane: MemorySnapshot,
    extensions: ExtensionSignals,
    ext_created_tab_observed_by_cef: bool,
}

impl Phase2Report {
    fn all_passed(&self) -> bool {
        let ext = &self.extensions;
        (self.chrome_child_embed_ok || self.alloy_child_embed_ok)
            && self.overlay_above_pages_ok
            && self.split_4pane_created >= 4
            && ext.mv3_worker_alive
            && ext.content_script_injected
            && ext.content_script_isolated
            && ext.action_badge_ok
            && ext.direct_tab_id.is_some()
            && ext.direct_tab_loaded
            && ext.popup_ok
            && ext.options_ok
            && ext.native_messaging_ok
    }
}

struct CliOptions {
    profile_dir: PathBuf,
    cef_dir: Option<PathBuf>,
    ext_dir: PathBuf,
    primary_style: RuntimeStyle,
}

fn parse_cli() -> CliOptions {
    let args: Vec<String> = env::args().collect();
    let mut profile_dir = env::temp_dir().join("zenith-cef-phase2-profile");
    let mut cef_dir = env::var_os("CEF_PATH").map(PathBuf::from);
    let mut ext_dir = PathBuf::from("fixtures/zenith-phase2-ext");
    let mut primary_style = RuntimeStyle::CHROME;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--profile-dir" if i + 1 < args.len() => {
                profile_dir = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--cef-dir" if i + 1 < args.len() => {
                cef_dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--ext-dir" if i + 1 < args.len() => {
                ext_dir = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--style" if i + 1 < args.len() => {
                primary_style = match args[i + 1].as_str() {
                    "alloy" => RuntimeStyle::ALLOY,
                    "default" => RuntimeStyle::DEFAULT,
                    _ => RuntimeStyle::CHROME,
                };
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    CliOptions {
        profile_dir,
        cef_dir,
        ext_dir,
        primary_style,
    }
}

fn style_name(style: RuntimeStyle) -> String {
    if style == RuntimeStyle::CHROME {
        "CHROME".to_string()
    } else if style == RuntimeStyle::ALLOY {
        "ALLOY".to_string()
    } else {
        "DEFAULT".to_string()
    }
}

struct SpikeState {
    started_at: Instant,
    fixture_port: u16,
    primary_style: RuntimeStyle,
    host: Mutex<Option<HostWindow>>,
    browsers: Mutex<HashMap<i32, Browser>>,
    pane_hwnds: Mutex<Vec<HWND>>,
    signals: Arc<Mutex<ExtensionSignals>>,
    report: Mutex<Phase2Report>,
    stage2_launched: Mutex<bool>,
    ext_pages_launched: Mutex<bool>,
    closing: Mutex<bool>,
}

wrap_task! {
    struct ForceQuitTask;

    impl Task {
        fn execute(&self) {
            eprintln!("[phase2] force-quitting CEF message loop after close timeout");
            quit_message_loop();
        }
    }
}

impl SpikeState {
    fn host_hwnd_raw(&self) -> Option<cef::sys::HWND> {
        self.host
            .lock()
            .ok()
            .and_then(|h| h.as_ref().map(|w| cef::sys::HWND(w.hwnd.0.cast())))
    }

    fn close_all(&self) {
        if let Ok(mut closing) = self.closing.lock() {
            if *closing {
                return;
            }
            *closing = true;
        }
        let list: Vec<Browser> = self
            .browsers
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        eprintln!("[phase2] close_all: closing {} tracked browsers", list.len());
        if list.is_empty() {
            quit_message_loop();
            return;
        }
        for b in list {
            if let Some(host) = b.host() {
                host.close_browser(1);
            }
        }
        // Fallback: extension-created Chrome windows aren't in `self.browsers`, so guarantee loop exit.
        let mut fallback = ForceQuitTask::new();
        post_delayed_task(ThreadId::UI, Some(&mut fallback), 1500);
    }
}

#[derive(Clone, Copy)]
enum BrowserKind {
    PrimaryPane,
    SplitPane,
    ExtPage,
}

wrap_display_handler! {
    struct Phase2DisplayHandler;

    impl DisplayHandler {
        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let id = browser.map(|b| b.identifier()).unwrap_or(-1);
            let msg = message.map(|m| m.to_string()).unwrap_or_default();
            eprintln!("[console id={id} L{line}] {msg}");
            0
        }
    }
}

wrap_load_handler! {
    struct Phase2LoadHandler {
        state: Arc<SpikeState>,
        kind: BrowserKind,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            if frame.is_none_or(|f| f.is_main() == 0) {
                return;
            }
            if matches!(self.kind, BrowserKind::PrimaryPane) {
                let elapsed = self.state.started_at.elapsed().as_millis() as u64;
                if let Ok(mut rep) = self.state.report.lock() {
                    if rep.startup_to_first_load_ms.is_none() {
                        rep.startup_to_first_load_ms = Some(elapsed);
                        rep.memory_1tab = sample_process_tree_memory();
                    }
                }
            }
        }
    }
}

wrap_life_span_handler! {
    struct Phase2LifeSpanHandler {
        state: Arc<SpikeState>,
        kind: BrowserKind,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let id = browser.identifier();
            if let Ok(mut map) = self.state.browsers.lock() {
                map.insert(id, browser.clone());
            }

            if let Some(host) = browser.host() {
                let actual = host.runtime_style();
                let hwnd = HWND(host.window_handle().0.cast());
                eprintln!(
                    "[phase2] on_after_created id={id} style={} hwnd={:?}",
                    style_name(actual),
                    hwnd.0
                );
                match self.kind {
                    BrowserKind::PrimaryPane => {
                        if let Ok(mut rep) = self.state.report.lock() {
                            rep.actual_primary_style = Some(style_name(actual));
                            if actual == RuntimeStyle::CHROME {
                                rep.chrome_child_embed_ok = !hwnd.0.is_null();
                            } else {
                                rep.alloy_child_embed_ok = !hwnd.0.is_null();
                            }
                        }
                        if !hwnd.0.is_null() {
                            if let Ok(mut panes) = self.state.pane_hwnds.lock() {
                                panes.push(hwnd);
                            }
                        }
                    }
                    BrowserKind::SplitPane => {
                        if let Ok(mut rep) = self.state.report.lock() {
                            if rep.actual_alloy_probe_style.is_none() {
                                rep.actual_alloy_probe_style = Some(style_name(actual));
                            }
                            if actual == RuntimeStyle::ALLOY {
                                rep.alloy_child_embed_ok = !hwnd.0.is_null();
                            }
                        }
                        if !hwnd.0.is_null() {
                            if let Ok(mut panes) = self.state.pane_hwnds.lock() {
                                panes.push(hwnd);
                            }
                        }
                    }
                    BrowserKind::ExtPage => {}
                }
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let id = browser.identifier();
                if let Ok(mut map) = self.state.browsers.lock() {
                    map.remove(&id);
                    if map.is_empty() {
                        quit_message_loop();
                    }
                }
            }
        }
    }
}

wrap_client! {
    struct Phase2Client {
        state: Arc<SpikeState>,
        kind: BrowserKind,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(Phase2DisplayHandler::new())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(Phase2LoadHandler::new(self.state.clone(), self.kind))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(Phase2LifeSpanHandler::new(self.state.clone(), self.kind))
        }
    }
}

fn spawn_child_browser(
    state: &Arc<SpikeState>,
    url: &str,
    bounds: Rect,
    style: RuntimeStyle,
    kind: BrowserKind,
) {
    let Some(parent) = state.host_hwnd_raw() else {
        return;
    };
    let mut win_info = WindowInfo::default().set_as_child(parent, &bounds);
    win_info.runtime_style = style;
    let mut client = Phase2Client::new(state.clone(), kind);
    let settings = BrowserSettings::default();
    browser_host_create_browser(
        Some(&win_info),
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&settings),
        None,
        None,
    );
}

wrap_task! {
    struct DriverTickTask {
        state: Arc<SpikeState>,
        ticks_left: u32,
    }

    impl Task {
        fn execute(&self) {
            // Sync latest extension signals into report.
            let sig = self.state.signals.lock().map(|s| s.clone()).unwrap_or_default();
            if let Ok(mut rep) = self.state.report.lock() {
                rep.extensions = sig.clone();
                if sig.direct_tab_loaded {
                    rep.ext_created_tab_observed_by_cef = true;
                }
            }

            // Launch popup & options pages once primary page has loaded.
            let first_loaded = self
                .state
                .report
                .lock()
                .map(|r| r.startup_to_first_load_ms.is_some())
                .unwrap_or(false);
            if first_loaded {
                let launch_ext = self
                    .state
                    .ext_pages_launched
                    .lock()
                    .map(|mut f| {
                        let first = !*f;
                        *f = true;
                        first
                    })
                    .unwrap_or(false);
                if launch_ext {
                    let ext_id = sig
                        .extension_id
                        .as_deref()
                        .unwrap_or(EXPECTED_EXT_ID);
                    let popup_url = format!(
                        "chrome-extension://{ext_id}/popup.html?port={}",
                        self.state.fixture_port
                    );
                    let options_url = format!(
                        "chrome-extension://{ext_id}/options.html?port={}",
                        self.state.fixture_port
                    );
                    spawn_child_browser(
                        &self.state,
                        &popup_url,
                        Rect { x: 900, y: 10, width: 320, height: 240 },
                        RuntimeStyle::CHROME,
                        BrowserKind::ExtPage,
                    );
                    spawn_child_browser(
                        &self.state,
                        &options_url,
                        Rect { x: 900, y: 260, width: 320, height: 240 },
                        RuntimeStyle::CHROME,
                        BrowserKind::ExtPage,
                    );
                }
            }

            // Stage 2: create 3 additional Alloy child panes to form a 4-pane split and benchmark resize + overlay.
            let ready_for_split = first_loaded
                && (sig.mv3_worker_alive || self.ticks_left <= 30);
            if ready_for_split {
                let launch_split = self
                    .state
                    .stage2_launched
                    .lock()
                    .map(|mut f| {
                        let first = !*f;
                        *f = true;
                        first
                    })
                    .unwrap_or(false);
                if launch_split {
                    let rects = [
                        Rect { x: 640, y: 0, width: 640, height: 400 },
                        Rect { x: 0, y: 400, width: 640, height: 400 },
                        Rect { x: 640, y: 400, width: 640, height: 400 },
                    ];
                    for (idx, rect) in rects.into_iter().enumerate() {
                        let url = format!("http://127.0.0.1:{}/pane/{}", self.state.fixture_port, idx + 1);
                        spawn_child_browser(
                            &self.state,
                            &url,
                            rect,
                            RuntimeStyle::ALLOY,
                            BrowserKind::SplitPane,
                        );
                    }
                }
            }

            // Once 4 panes have HWNDs, run resize benchmark and overlay z-order check once.
            let panes: Vec<HWND> = self
                .state
                .pane_hwnds
                .lock()
                .map(|p| p.clone())
                .unwrap_or_default();
            if panes.len() >= 4 {
                let need_bench = self
                    .state
                    .report
                    .lock()
                    .map(|r| r.split_4pane_created < 4)
                    .unwrap_or(false);
                if need_bench {
                    if let Ok(host_guard) = self.state.host.lock() {
                        if let Some(host) = host_guard.as_ref() {
                            let stats = host.benchmark_split_resize(&panes, 25);
                            let overlay_ok = host.raise_and_verify_overlay(&panes);
                            let mem4 = sample_process_tree_memory();
                            if let Ok(mut rep) = self.state.report.lock() {
                                rep.split_4pane_created = panes.len();
                                rep.split_resize_stats = stats;
                                rep.overlay_above_pages_ok = overlay_ok;
                                rep.memory_4pane = mem4;
                            }
                        }
                    }
                }
            }

            let done = self.state.report.lock().map(|r| r.all_passed()).unwrap_or(false);
            if done || self.ticks_left == 0 {
                self.state.close_all();
                return;
            }

            let mut next = DriverTickTask::new(self.state.clone(), self.ticks_left - 1);
            post_delayed_task(ThreadId::UI, Some(&mut next), 250);
        }
    }
}

wrap_browser_process_handler! {
    struct Phase2BrowserProcessHandler {
        state: Arc<SpikeState>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            match HostWindow::create(1280, 800) {
                Ok(host) => {
                    if let Ok(mut slot) = self.state.host.lock() {
                        *slot = Some(host);
                    }
                }
                Err(e) => {
                    eprintln!("HostWindow::create failed: {e}");
                    quit_message_loop();
                    return;
                }
            }

            let page_url = format!("http://127.0.0.1:{}/page", self.state.fixture_port);
            spawn_child_browser(
                &self.state,
                &page_url,
                Rect { x: 0, y: 0, width: 640, height: 400 },
                self.state.primary_style,
                BrowserKind::PrimaryPane,
            );

            let mut tick = DriverTickTask::new(self.state.clone(), 48);
            post_delayed_task(ThreadId::UI, Some(&mut tick), 250);
        }
    }
}

wrap_app! {
    struct Phase2App {
        state: Option<Arc<SpikeState>>,
        ext_dir: Option<PathBuf>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Only configure browser-process extension switches (empty process_type).
            let is_browser = process_type.is_none_or(|p| p.to_string().is_empty());
            if !is_browser {
                return;
            }
            let (Some(cmd), Some(ext_dir)) = (command_line, &self.ext_dir) else {
                return;
            };
            let ext_str = ext_dir.to_string_lossy();
            cmd.append_switch_with_value(
                Some(&CefString::from("load-extension")),
                Some(&CefString::from(ext_str.as_ref())),
            );
            // Block external Chrome registry extensions from leaking into the Zenith profile.
            cmd.append_switch_with_value(
                Some(&CefString::from("disable-extensions-except")),
                Some(&CefString::from(ext_str.as_ref())),
            );
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            self.state
                .as_ref()
                .map(|s| Phase2BrowserProcessHandler::new(s.clone()))
        }
    }
}

fn main() -> ExitCode {
    if env::args().any(|a| a == "--native-host") {
        let code = native_host::run_stdio_host();
        return ExitCode::from(code as u8);
    }

    let started_at = Instant::now();
    let args = Args::new();
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    let mut sub_app = Phase2App::new(None, None);
    let sub_exit = execute_process(
        Some(args.as_main_args()),
        Some(&mut sub_app),
        std::ptr::null_mut(),
    );
    if sub_exit >= 0 {
        return ExitCode::from(sub_exit as u8);
    }

    let cli = parse_cli();
    if let Err(e) = fs::create_dir_all(&cli.profile_dir) {
        eprintln!("failed to create profile dir: {e}");
        return ExitCode::from(2);
    }
    let ext_dir = fs::canonicalize(&cli.ext_dir).unwrap_or(cli.ext_dir);
    // Strip Windows extended-length prefix (\\?\) because Chromium's --load-extension rejects it.
    let ext_dir = PathBuf::from(ext_dir.to_string_lossy().trim_start_matches(r"\\?\"));

    let signals = Arc::new(Mutex::new(ExtensionSignals::default()));
    let fixture_port = match FixtureServer::start(signals.clone()) {
        Ok(srv) => srv.addr.port(),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let state = Arc::new(SpikeState {
        started_at,
        fixture_port,
        primary_style: cli.primary_style,
        host: Mutex::new(None),
        browsers: Mutex::new(HashMap::new()),
        pane_hwnds: Mutex::new(Vec::new()),
        signals,
        report: Mutex::new(Phase2Report {
            cef_version: CEF_PINNED_VERSION.to_string(),
            requested_primary_style: style_name(cli.primary_style),
            ..Default::default()
        }),
        stage2_launched: Mutex::new(false),
        ext_pages_launched: Mutex::new(false),
        closing: Mutex::new(false),
    });

    let mut settings = Settings::default();
    // shortcut: sandbox disabled in standalone spike until bootstrap.exe integration, re-enable in Phase 3 packaging
    settings.no_sandbox = 1;
    settings.log_severity = LogSeverity::WARNING;
    settings.root_cache_path = CefString::from(cli.profile_dir.to_string_lossy().as_ref());
    settings.cache_path = CefString::from(
        cli.profile_dir
            .join("cef-cache")
            .to_string_lossy()
            .as_ref(),
    );
    if let Some(cef_dir) = &cli.cef_dir {
        settings.resources_dir_path = CefString::from(cef_dir.to_string_lossy().as_ref());
        settings.locales_dir_path =
            CefString::from(cef_dir.join("locales").to_string_lossy().as_ref());
    }

    let mut browser_app = Phase2App::new(Some(state.clone()), Some(ext_dir));
    let init_ok = initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut browser_app),
        std::ptr::null_mut(),
    );
    if init_ok == 0 {
        eprintln!("cef::initialize failed");
        return ExitCode::from(3);
    }

    run_message_loop();

    let rep = state.report.lock().map(|r| r.clone()).unwrap_or_default();
    println!(
        "{}",
        serde_json::to_string_pretty(&rep).unwrap_or_else(|_| "{}".to_string())
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());

    if let Ok(mut host) = state.host.lock() {
        *host = None;
    }
    shutdown();

    if rep.all_passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
