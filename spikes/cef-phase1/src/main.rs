//! Phase 1 CEF spike entrypoint: subprocess dispatch, single-instance lock, profile setup, and self-test.

mod bridge;
mod cef_app;
mod security;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use cef::args::Args;
use cef::*;

use bridge::FixtureServer;
use cef_app::{SpikeApp, SpikeState};
use security::{BridgeToken, SingleInstanceLock};

struct CliOptions {
    self_test: bool,
    hold_lock_ms: Option<u64>,
    profile_dir: PathBuf,
    ui_dir: Option<PathBuf>,
    cef_dir: Option<PathBuf>,
}

fn parse_cli() -> CliOptions {
    let args: Vec<String> = env::args().collect();
    let mut self_test = false;
    let mut hold_lock_ms = None;
    let mut profile_dir = env::temp_dir().join("zenith-cef-phase1-profile");
    let mut ui_dir = None;
    let mut cef_dir = env::var_os("CEF_PATH").map(PathBuf::from);

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--self-test" => self_test = true,
            "--hold-lock-ms" if i + 1 < args.len() => {
                hold_lock_ms = args[i + 1].parse::<u64>().ok();
                i += 1;
            }
            "--profile-dir" if i + 1 < args.len() => {
                profile_dir = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--ui-dir" if i + 1 < args.len() => {
                ui_dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--cef-dir" if i + 1 < args.len() => {
                cef_dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    CliOptions {
        self_test,
        hold_lock_ms,
        profile_dir,
        ui_dir,
        cef_dir,
    }
}

fn main() -> ExitCode {
    let args = Args::new();

    // Verify CEF API hash before any CEF call.
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    // CEF subprocesses (renderer, GPU, utility, network) re-launch this executable with `--type=...`.
    // Intercept them before single-instance locking or browser setup.
    let mut sub_app = SpikeApp::new(None, Arc::new(Mutex::new(HashMap::new())));
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

    let _lock = match SingleInstanceLock::acquire(&cli.profile_dir) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            eprintln!("SINGLE_INSTANCE_BLOCKED: profile already active");
            return ExitCode::from(10);
        }
        Err(e) => {
            eprintln!("single-instance error: {e}");
            return ExitCode::from(2);
        }
    };

    if let Some(ms) = cli.hold_lock_ms {
        println!("LOCK_HELD");
        std::thread::sleep(std::time::Duration::from_millis(ms));
        return ExitCode::SUCCESS;
    }

    let token = match BridgeToken::generate() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let untrusted_slot = Arc::new(Mutex::new(None));
    let recovered_slot = Arc::new(Mutex::new(false));
    let fixture_port = if cli.self_test {
        match FixtureServer::start(untrusted_slot.clone(), recovered_slot.clone()) {
            Ok(srv) => srv.addr.port(),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        }
    } else {
        0
    };

    let state = SpikeState::new(
        cli.self_test,
        cli.ui_dir,
        fixture_port,
        token,
        untrusted_slot,
        recovered_slot,
    );

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

    let mut browser_app = SpikeApp::new(Some(state.clone()), Arc::new(Mutex::new(HashMap::new())));
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
    shutdown();

    if cli.self_test {
        let rep = state.report.lock().map(|r| r.clone()).unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string_pretty(&rep).unwrap_or_else(|_| "{}".to_string())
        );
        if rep.all_passed() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        }
    } else {
        ExitCode::SUCCESS
    }
}
