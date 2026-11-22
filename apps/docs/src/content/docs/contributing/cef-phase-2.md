---
title: CEF Phase 2 go/no-go report
description: Phase 2 native child-window rendering, Chrome vs Alloy runtime style, MV3 extension prototype, and budget measurements.
---

Status: Phase 2 rendering and extension go/no-go completed 2026-11-22 in `spikes/cef-phase2` against pinned CEF `156.0.31+ge603d97+chromium-156.0.8078.100` (`cef = "=156.7.0"`). Decision: **GO with native child windows (`set_as_child`) and Chrome-style page browsers (`RuntimeStyle::CHROME`)**; off-screen rendering (OSR) is skipped because `WindowInfo::set_as_windowless` forces `RuntimeStyle::ALLOY` in CEF 156 and native child windows already pass split-layout and overlay z-order checks.

## Verified capabilities

Run `powershell -NoProfile -ExecutionPolicy Bypass -File spikes/cef-phase2/run-spike.ps1 -Style chrome` (and `-Style alloy` for comparison).

| Check | Chrome-style child | Alloy-style child | Evidence |
| --- | --- | --- | --- |
| Zero WebView2/wry dependency | Pass | Pass | `cargo tree` in `spikes/cef-phase2` contains no `wry` or `webview2`. |
| Native messaging framing unit tests | Pass | Pass | 2/2 Rust tests pass: 4-byte LE JSON round-trip and >1 MB frame rejection. |
| Native child-window embedding | Pass (`CHROME`) | Pass (`ALLOY`) | Valid child `HWND` embedded inside Win32 `HostWindow` via `set_as_child`. |
| Overlay z-order above 4 page panes | Pass | Pass | `SetWindowPos(overlay, HWND_TOP)` verified ahead of all 4 child pane `HWND`s. |
| 4-pane split creation & resize (25 steps) | Pass (mean `8.21 ms`, p95 `10.94 ms`) | Pass (mean `4.13 ms`, p95 `5.77 ms`) | Both within 16.6 ms 60 fps frame budget. |
| MV3 service worker + stable RSA key ID | Pass | Pass | `fkjmjfpacnjjgnppeenhiolleglehgik` loaded via `--load-extension` + `--disable-extensions-except`. |
| Content-script isolation (`document_start`) | Pass | Pass | DOM marker visible to page; `window.__EXT_ISOLATED_SECRET` remains `undefined` in main world. |
| Action badge API | Pass | Pass | `setBadgeText("42")` and `setBadgeBackgroundColor("#ff5500")` read back as `"42"` and `[255, 85, 0, 255]`. |
| Direct extension tab creation (`chrome.tabs.create`) | Pass (`tab.id` returned, `/ext-created` loaded) | **Fail** (`direct_tab_id: -1`) | Alloy child browsers lack a Chrome `TabStripModel`; Chrome-style is required for `tabs.create`. |
| Popup & options pages | Pass (`tabsCount: 3`) | Pass (`tabsCount: 2`) | Both `chrome-extension://.../popup.html` and `options.html` resolve `chrome.runtime.id` and `chrome.tabs.query({})`. |
| Native messaging (stdio echo host) | Pass | Pass | `chrome.runtime.sendNativeMessage("com.zenith.phase2.echo", ...)` round-tripped via `--native-host` and temporary HKCU manifest. |

## Prototype measurements vs. Section 12.1 budgets

| Metric | Chrome-style run | Alloy-style run | Section 12.1 budget | Status |
| --- | ---: | ---: | --- | --- |
| Startup to first load | `413 ms` | `306 ms` | `<1000 ms` | Fits |
| 1-tab host private memory | `34.18 MB` | `30.65 MB` | `≤120 MB` app-owned | Fits |
| 1-tab total private memory (7 procs) | `143.78 MB` | `152.52 MB` | Lower than Chrome/Edge/Firefox/Zen (Phase 3/4 matrix) | On track |
| 4-pane host private memory | `54.09 MB` | `41.67 MB` | — | Recorded |
| 4-pane total private memory | `332.30 MB` (13 procs) | `221.44 MB` (10 procs) | — | Recorded |
| 4-pane resize p95 (25 steps) | `10.94 ms` | `5.77 ms` | Smooth resize (no lag) | Fits |
| Unpacked CEF runtime (`en-US.pak`) | `355.79 MB` (from Phase 1) | `355.79 MB` | `≤400 MB` | Fits |

## Chosen architecture and limitations for Phase 3

1. **Rendering model:** Native child windows (`WindowInfo::set_as_child`) under Zenith's Win32 host window, reusing Zenith's existing `ResizeObserver` placement and overlay-lease z-order model. OSR/shared-texture rendering is not adopted.
2. **Runtime style split:** Use `RuntimeStyle::CHROME` for web/extension page panes (required for `chrome.tabs.create` and full Chrome extension tab semantics) and `RuntimeStyle::ALLOY` for the trusted Svelte UI surface (lighter footprint and excluded from Chrome tab lists).
3. **Extension-created tab adoption (Phase 3 work):** In stock Chrome-style CEF, `chrome.tabs.create` creates a Chrome-owned tab outside `SpikeState.browsers`, which blocks `cef::shutdown()` if left open. Phase 3 must intercept or adopt extension-created browsers into Zenith's tab registry and close all Chrome browsers before `shutdown()`.
4. **Deferred to Phase 3/4:** Dark Reader and uBlock Origin Lite automated runs, cross-browser memory comparison (Chrome/Edge/Firefox/Zen), mixed-DPI/IME/accessibility matrix, and colocated GPU/sandbox packaging.
