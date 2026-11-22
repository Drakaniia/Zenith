---
title: CEF Phase 1 spike report
description: Phase 1 standalone CEF runtime, trusted bridge, isolation, lifecycle, and runtime-size measurements.
---

Status: Phase 1 core integration spike completed 2026-11-22 in `spikes/cef-phase1` against pinned official prebuilt CEF `156.0.31+ge603d97+chromium-156.0.8078.100` (`cef = "=156.7.0"`). NSIS packaging, signed updater testing, and custom H.264/AAC source builds were intentionally deferred per stakeholder scope choice.

## Verified capabilities

Run `powershell -NoProfile -ExecutionPolicy Bypass -File spikes/cef-phase1/run-spike.ps1` after exporting CEF with `export-cef-dir --force "$env:LOCALAPPDATA\zenith-cef\156.0.31"`.

| Check | Result | Evidence |
| --- | --- | --- |
| Zero WebView2/wry dependency | Pass | `cargo tree` in `spikes/cef-phase1` contains no `wry` or `webview2` crates. |
| Security & path unit tests | Pass | 4/4 Rust tests pass: strict `zenith://app/` origin check, 5-layer invoke authorization, generation tombstoning, path traversal rejection, per-profile Windows mutex lock. |
| Subprocess & per-profile single-instance | Pass | Single-exe `cef::execute_process` handles renderer/GPU/utility launches; second launch on the same `--profile-dir` exits `10`. |
| Trusted UI custom scheme + typed bridge | Pass | `zenith://app/` served via `SchemeHandlerFactory` (`STANDARD \| SECURE \| DISPLAY_ISOLATED \| FETCH_ENABLED`). V8 bridge injects only into `TrustedUi` main frames with a per-launch 256-bit random token and command allowlist. |
| Bridge rejection tests | Pass | Bad token returns `invalid_token`; unlisted command returns `unknown_command`; backend-to-UI `spike-ping` event delivers payload. |
| Untrusted page & subframe isolation | Pass | Separate HTTP browser confirms `window.__ZENITH_BRIDGE__` and `__zenith_native_send` are `undefined` and `fetch("zenith://app/index.html")` is blocked natively (`Not allowed to load local resource`). Sandboxed iframe inside trusted UI has no bridge and cannot read `window.parent.__ZENITH_BRIDGE__`. |
| Fail-closed trusted navigation & popups | Pass | Navigating trusted UI to `http://127.0.0.1` is cancelled in `on_before_browse`; trusted popups are denied in `on_before_popup`. |
| Crash detection & recovery | Pass | Navigating untrusted browser to `chrome://crash` fires `on_render_process_terminated`, bumps generation, rejects stale generation id, and reloads `/recovered`. |

## Measured runtime size (`156.0.31`, Windows x64 minimal)

| Payload | Unpacked size | Budget (`≤400 MB` installed) |
| --- | ---: | --- |
| Runtime base (`libcef.dll`, `chrome_elf.dll`, ANGLE/DXC/SwiftShader/Vulkan DLLs, `icudtl.dat`, `v8_context_snapshot.bin`, `.pak` files) | 355.21 MB | Fits (~44 MB headroom) |
| All CEF locales (`locales/*.pak`) | 49.80 MB | Exceeds budget (405.01 MB total before `zenith.exe`) |
| `en-US.pak` only | 0.58 MB | Fits (355.79 MB total + app binary) |

## Architectural findings for Phase 2–3

1. **External Chrome registry extensions:** stock CEF 156 (Chrome runtime default) probes `Software\Google\Chrome\Extensions\*` on Windows (`external_registry_loader_win.cc`). Phase 2/4 must explicitly disable external registry extension loading so installed Chrome extensions never leak into Zenith profiles.
2. **Runtime colocation and Windows sandbox:** launching the spike binary outside the CEF DLL directory caused GPU process exits (`-2147483645`) and software compositing fallback, and `no_sandbox = 1` was used as a standalone spike shortcut. Production packaging must colocate runtime DLLs beside the executable and integrate `bootstrap.exe`/CEF Windows sandbox before Phase 3/5.
3. **Locale pruning required for budget:** shipping all CEF locale `.pak` files pushes unpacked size to `405.01 MB` before `zenith.exe`. Packaging must prune locales to supported UI languages or request an explicit budget revision.
4. **Custom scheme MIME rule:** CEF `Response::set_mime_type` requires bare MIME types (for example `"text/html"`, not `"text/html; charset=utf-8"`).

## Open items deferred from Phase 1

- Packaging `zenith.exe` + pinned CEF runtime into an NSIS installer and testing signed atomic app+engine updates.
- Custom H.264/AAC codec build/patch pipeline (deferred to separate CI/build pipeline).
- Wiring the real `apps/desktop/src-tauri` command registry onto the CEF bridge (Phase 2 chooses native child windows vs off-screen rendering and Chrome-style vs Alloy-style extension compatibility first).
