# Measurements

Reference machine: Windows 11 (build 26200), WebView2 runtime 156.0.4314.58, release build of Zenith 0.1.0. The system was under memory pressure during the runs (75–95 % of RAM in use by other applications), which tends to make Chromium trim memory more eagerly, so treat the absolute numbers as approximate and compare runs made on the same machine.

Re-run with `powershell -ExecutionPolicy Bypass -File scripts/measure.ps1` (memory and startup, appends to `memory.csv`) and `node scripts/measure-blocking.mjs` (ad blocking, writes `blocking.md`, needs the test instance from `scripts/dev/run.ps1`). Both use a throwaway profile and never touch your own.

## Size

| Artifact                                               | Size    | Target  |
| ------------------------------------------------------ | ------- | ------- |
| NSIS installer `Zenith_0.1.0_x64-setup.exe`            | 1.78 MB | < 10 MB |
| `zenith.exe` (release, `opt-level="z"`, LTO, stripped) | 4.4 MB  |         |

The installer does not bundle WebView2; it downloads the Evergreen bootstrapper only when the runtime is missing (it ships with Windows 11).

## Startup

Time from process start until the browser window is shown with its UI (`perf.json`, written when `ZENITH_PERF=1`): **650–900 ms** across all scenarios below (target < 1 s). Restored tabs other than the active one start as sleeping, so the session size does not slow startup.

## Memory

Private bytes summed over `zenith.exe` and every process it starts (WebView2 browser, GPU, utility and renderer processes). Pages: example.com/.org/.net, iana.org, rfc-editor.org (10 tabs adds info.cern.ch, w3.org, whatwg.org and two more iana/rfc pages).

| Scenario                                        | Processes | Private total | zenith.exe | WebView2 browser | GPU    | Utility | Renderers             |
| ----------------------------------------------- | --------- | ------------- | ---------- | ---------------- | ------ | ------- | --------------------- |
| Idle (new-tab page, Mica)                       | 7         | 231 MB        | 16 MB      | 39 MB            | 120 MB | 21 MB   | 1 × 32 MB (chrome UI) |
| Idle, Solid material                            | 7         | 236 MB        | 16 MB      | 40 MB            | 123 MB | 21 MB   | 1 × 32 MB             |
| 1 tab                                           | 8         | 284 MB        | 16 MB      | 45 MB            | 142 MB | 21 MB   | 2 = 57 MB             |
| 5 tabs                                          | 12        | 425 MB        | 17 MB      | 63 MB            | 143 MB | 23 MB   | 6 = 177 MB            |
| 10 tabs                                         | 17        | 577 MB        | 17 MB      | 86 MB            | 135 MB | 24 MB   | 11 = 312 MB           |
| 5 tabs, 4 asleep (after 1 min)                  | 8         | 292 MB        | 16 MB      | 54 MB            | 106 MB | 22 MB   | 2 = 90 MB             |
| Microsoft Edge, 1 tab, fresh profile (baseline) | 18        | 548 MB        |            |                  | 185 MB |         | 11                    |

Reading the numbers:

- **App overhead with one tab**, as the plan defines it (Rust process + chrome webview + WebView2 browser process): 16 + 32 + 45 = **93 MB**, within the provisional 120 MB budget.
- The WebView2 **GPU process** (110–145 MB) is the largest single item. It is the same with the Solid material (no transparency), so it is WebView2's own baseline, not a cost of Mica. Edge's GPU process is larger (185 MB).
- Each additional page costs roughly one renderer (25–35 MB for these light pages; real-world sites cost more).
- **Tab sleeping works as intended**: with four of five tabs asleep, memory falls from 425 MB to 292 MB, close to the one-tab figure.
- One tab in Zenith uses about half of what one tab in a fresh Edge profile uses (Edge runs extra services and preloads).

Raw rows, including dates, are in [`memory.csv`](memory.csv).

## Ad and tracker blocking

See [`blocking.md`](blocking.md): on six news and media sites, blocking cut requests that reach the network from 1,093 to 270 (−75 %).
