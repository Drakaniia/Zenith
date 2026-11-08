# Restructure spec: monorepo, feature-first frontend, domain-module backend

Status: **implemented** (2026-11-08, branch `restructure`). The living description of the result is [contributing/project-structure.md](../apps/docs/src/content/docs/contributing/project-structure.md); this file is kept as the record of the decisions.

Deviations from the plan below: the frontend's `types.ts` and `ipc.ts` were split fully (each feature has its own `types.ts` and `api.ts`, and declares its backend events by merging into `BackendEvents`); the dev preview mock lives in `app/preview/` (only `main.ts` loads it); there are additional features `window`, `blocker` and `toolbar`; the Rust `app/` also has `window.rs` and `startup.rs`; history, notes and download records were split out of `storage`; tests are colocated but stay excluded from `svelte-check` (adding `@types/node` and type-checking them is a follow-up); the import-cycle rule turned out to have no existing cycles to grandfather.

This spec turns the single-app repository into a pnpm monorepo that can hold the Zenith browser and its documentation website, and reorganizes the app's code so it can keep growing without flat folders and 1,000-line files. It is a plan for moves and splits only: **no behavior changes**.

## 1. Decisions

| Topic | Decision |
| --- | --- |
| Repo shape | pnpm monorepo: `apps/desktop` (the browser), `apps/docs` (website), `packages/*` (shared code) |
| Docs framework | Astro Starlight |
| Frontend (Svelte) | Feature-first: `src/features/<domain>/`, plus `src/shared/` and `src/platform/` |
| Backend (Rust) | Domain modules, each with its own `commands.rs`; `commands.rs` and `*_commands.rs` dissolve |
| File size | Soft limit of **600 lines**, counted without Rust `#[cfg(test)] mod tests` blocks; exceptions are documented in the file header |
| Frontend tests | Colocated `*.test.ts` next to the module under test |

## 2. Invariants (must hold after every phase)

- **No behavior change.** Moves use `git mv` so history follows the file. Splits move code verbatim; renames are limited to what the split needs.
- **Stable contracts.** Tauri command names, event names (`tab-update`, `bookmarks-changed`, `shortcut`, …), settings/session JSON shapes, SQLite schema, `%APPDATA%`/`%LOCALAPPDATA%` paths, the bundle identifier `dev.zenith.browser`, the updater feed and the signing key all stay as they are. An installed Zenith must update into the restructured build without noticing.
- **Green at every commit.** `pnpm check`, `pnpm test`, `pnpm build`, `pnpm test:rust` pass after each commit (run from the repo root once Phase 1 lands).
- **One concern per commit.** For example "move bookmarks frontend into features/bookmarks" or "split bookmarks.rs into tree/html/backup". Never mix a move with a logic edit.

## 3. Current state (2026-11-08)

- ~190 source files. `src/lib/` holds **40** flat files and `src/components/` holds **35** flat files, both over the 30-file guideline. `src-tauri/src/` has 17 flat domain files beside `host/`, `media/` and `vault/`.
- Hubs with many importers: `ui.svelte.ts` (45 importers), `browser.svelte.ts` (33), `settings.svelte.ts` (30), `ipc.ts` (25), `types.ts` (21), `actions.ts` (20), `layout.svelte.ts` (20).
- `actions.ts` ↔ `menus.ts` ↔ `bookmark-actions.ts` import each other's neighbours. The dispatcher knows every feature.
- `commands.rs` (1,183 lines) holds 92 commands from about 20 domains.
- Files over 600 lines, listed in §8.
- The working tree has 32 modified and 8 untracked files (profiles, tab strip, sidebar position). They must be committed before Phase 1 (see §9, Phase 0).

## 4. Target repository layout

```
zenith/
├─ apps/
│  ├─ desktop/                 # the browser: everything that is the repo root today
│  │  ├─ src/                  # Svelte chrome UI (§5)
│  │  ├─ src-tauri/            # Rust backend (§6); keeps Tauri's conventional name
│  │  ├─ scripts/              # app-only scripts: dev/, measure*, gen-profile-avatars.mjs
│  │  ├─ measurements/         # performance data (was /measurements)
│  │  ├─ index.html  vite.config.ts  svelte.config.js  tsconfig.json
│  │  └─ package.json          # name "@zenith/desktop", version = app version
│  └─ docs/                    # Astro Starlight website (§7)
│     └─ package.json          # name "@zenith/docs"
├─ packages/
│  └─ brand/                   # logo.svg (was assets/), icon sources, colour tokens
│     └─ package.json          # name "@zenith/brand"
├─ scripts/
│  ├─ release.mjs              # repo-level: bumps apps/desktop versions, tags
│  └─ release.test.ts          # was tests/release.test.ts
├─ .github/workflows/          # ci.yml, release.yml, docs.yml (new)
├─ package.json                # private root: workspace scripts only, no deps beyond tooling
├─ pnpm-workspace.yaml         # packages: ["apps/*", "packages/*"]
├─ README.md  todo.md
└─ (docs/ is removed once its content lives in apps/docs; see §7)
```

### Rules

- A new `packages/*` entry is created only when **two apps** consume the code. No speculative packages. `brand` qualifies because the desktop app's icons and the docs site both use the logo.
- Root `package.json` scripts delegate: `dev` → `pnpm --filter @zenith/desktop dev`, `app:dev`, `app:build`, `docs:dev` → `pnpm --filter @zenith/docs dev`, `check` / `test` / `build` → `pnpm -r …`, `test:rust`, `release`.
- `pnpm-workspace.yaml` keeps the `minimumReleaseAgeExclude` entry and its comment.

### Path references to update in Phase 1

`git grep` finds hard-coded paths in: `.github/workflows/ci.yml` (rust-cache `workspaces: apps/desktop/src-tauri -> target`), `.github/workflows/release.yml` (tauri-action project path), `.gitignore` (3), `README.md` (3), `package.json` (2), `scripts/dev/build.ps1` (2), `scripts/dev/run.ps1`, `scripts/dev/e2e.mjs`, `scripts/measure.ps1` (2), `scripts/measure-blocking.mjs` (3), `scripts/release.mjs` (3, plus its test), `src-tauri/tauri.conf.json` (its relative `../dist` stays valid because the app moves as a unit). `pnpm-lock.yaml` is regenerated, because its importers are keyed by path.

## 5. Desktop frontend: `apps/desktop/src/`

### Layers and the dependency direction

```
main.ts → app/ → features/* → shared/, platform/
```

- **`app/`**: composition root. It is the shell layout, startup, global event wiring and action registration. Nothing imports from `app/` except `main.ts`.
- **`features/<domain>/`**: everything one user-facing domain needs: components, its store (`<domain>.svelte.ts`), pure logic (`<topic>.ts`), typed backend calls (`api.ts`), types (`types.ts`) and tests. Features may import other features. `browser` and `settings` are the core features most others use. Imports must not form cycles.
- **`shared/`**: domain-free building blocks with **3 or more** consumers: UI primitives, URL helpers, motion and icons. No business logic.
- **`platform/`**: the Tauri/WebView2 bridge. It covers IPC transport, webview placement and overlay leases, and the dev-only preview mock. It contains no domain logic.

### Conventions

- Components use `PascalCase.svelte`. Modules use `kebab-case.ts`, and runes stores use `kebab-case.svelte.ts`. Tests are `<module>.test.ts` in the same folder.
- A feature folder stays flat. Subfolders exist only for a cohesive sub-area (`onboarding/steps/`, `settings/sections/`). The maximum depth is 3 levels below `src/`.
- No `index.ts` barrels (the project has none today). Consumers import the file directly.
- Aliases are `$features/*`, `$shared/*` and `$platform/*`, set in `tsconfig.json` `paths` and `vite.config.ts` `resolve.alias`. Use them in `.svelte` and `.svelte.ts` files.
- **Node-test constraint.** `node --test` strips types natively and resolves no aliases. A pure module covered by a `*.test.ts` file must therefore use only relative imports, and its value imports must carry the `.ts` extension. Type-only imports (`import type`) are unaffected. Enable `allowImportingTsExtensions` (allowed with `noEmit`).

### Target tree (old location → new location)

```
src/
├─ main.ts
├─ styles/app.css                         ← app.css
├─ app/
│  ├─ App.svelte                          ← App.svelte (slimmed; see §8)
│  ├─ app.svelte.ts                       ← lib/app.svelte.ts (startup info, incognito flag)
│  ├─ actions.ts                          ← registers every feature's actions (from lib/actions.ts)
│  └─ WindowControls.svelte               ← components/WindowControls.svelte
├─ platform/
│  ├─ ipc.ts                              ← lib/ipc.ts (transport; domain calls peel off to features/*/api.ts)
│  ├─ layout.svelte.ts                    ← lib/layout.svelte.ts
│  ├─ actions.ts                          ← new: action registry (register/run/label), no domain imports
│  └─ preview/                            ← lib/preview/mock.ts, split per domain
├─ shared/
│  ├─ url.ts  url.test.ts                 ← lib/url.ts (17 consumers)
│  ├─ favicons.ts                         ← lib/favicons.ts (bookmarks, palette, browser)
│  └─ ui/  Icon.svelte  icons.ts  Favicon.svelte  Menu.svelte  menu.svelte.ts
│          Toasts.svelte  toasts.svelte.ts  Toggle.svelte  Segmented.svelte  motion.ts
└─ features/
   ├─ browser/       browser.svelte.ts (facade) spaces.ts folders.ts tabs.ts split.ts glance.ts
   │                 session.ts sleep.ts types.ts api.ts  Content.svelte Pane.svelte Glance.svelte
   ├─ sidebar/       Sidebar TabStrip TabItem SpaceEditor Essentials FolderItem MediaBar
   │                 SidebarTop SidebarFooter SpacePane (.svelte)  sidebar-dnd.svelte.ts
   │                 sidebar-position.ts (+test)  menus.ts
   ├─ toolbar/       TopBar.svelte UrlPill.svelte  menus.ts
   ├─ palette/       Palette.svelte (+ split parts)  palette.svelte.ts  autocomplete.ts fuzzy.ts (+tests)
   ├─ page/          FindBar.svelte PermissionPrompt.svelte  find.svelte.ts permissions.svelte.ts
   ├─ bookmarks/     bookmarks.ts (+test) bookmarks.svelte.ts bookmark-actions.ts bookmark-dnd.svelte.ts
   │                 BookmarkDialog BookmarkGhost BookmarkImport BookmarkPopup BookmarksBar
   │                 BookmarksPanel FolderPicker SpacePicker BookmarksPage (.svelte)  menus.ts api.ts types.ts
   ├─ history/       HistoryPage.svelte api.ts
   ├─ downloads/     DownloadRow DownloadsPopover DownloadsPage MediaPanel (.svelte)
   │                 downloads.svelte.ts media.ts (+test) api.ts types.ts
   ├─ extensions/    extensions.svelte.ts ExtensionButtons ExtensionInstallPrompt ExtensionsPopover ExtensionsPage
   ├─ passwords/     passwords.ts (+test) vault.svelte.ts PasswordSavePrompt PasswordSuggest PasswordsPage
   │                 + everything in components/passwords/
   ├─ notes/         notes.ts (+test) notes.svelte.ts NotesPage.svelte (+ split parts)
   ├─ startpage/     startpage.ts (+test) startpage.svelte.ts wallpapers.ts NewTab StartPagePanel
   │                 StartPageControls SiteEditor
   ├─ profiles/      profiles.ts (+test) profile-picture.ts generated/profile-avatars.ts
   │                 ProfileAvatar ProfileButton ProfileDialog
   ├─ shortcuts/     shortcuts.ts (+test) ShortcutEditor.svelte
   ├─ settings/      settings.svelte.ts SettingsPage.svelte  sections/*.svelte
   ├─ onboarding/    onboarding-flow.ts (+test) onboarding.svelte.ts Onboarding Horizon Choice  steps/*
   ├─ chatbot/       chatbot.ts
   ├─ updates/       updates.svelte.ts
   └─ default-browser/ default-browser.ts
```

`scripts/gen-profile-avatars.mjs` writes to `features/profiles/generated/profile-avatars.ts`.

### Dissolving the hubs (Phase 3)

| Hub | Becomes |
| --- | --- |
| `types.ts` (21 importers) | Per-feature `types.ts`: Tab/Space/Folder/TabId → `browser/types.ts`, BookmarkNode → `bookmarks/types.ts`, DownloadItem/MediaItem → `downloads/types.ts`, ProfileEntry → `profiles/types.ts`, StartupInfo → `app/`. IPC-only shapes (LayoutSpec, PaneSpec) → `platform/` |
| `ui.svelte.ts` (45 importers) | Menus → `shared/ui/menu.svelte.ts` (with `MenuItem`, `AnchorRect`, `MenuMods`), toasts → `shared/ui/toasts.svelte.ts`, palette → `palette/palette.svelte.ts`, find bar → `page/find.svelte.ts`, `permissions` → `page/permissions.svelte.ts`, `downloads` → `downloads/downloads.svelte.ts`. Compact-mode reveal and dialog flags → `app/` or the owning feature |
| `ipc.ts` (25 importers) | `platform/ipc.ts` keeps `invoke`/`on`, error mapping and window scoping. Each domain's typed calls move to `features/<domain>/api.ts`. Peel off one domain per commit, and keep `api` re-exporting until the last caller moves |
| `actions.ts` (20 importers) | `platform/actions.ts` is a registry (`register(id, handler)`, `run(id, args)`, `label(id)`). Each feature owns an `actions.ts` that registers its handlers, and `app/actions.ts` imports them all once at startup. Components call `run()` from the platform and never import other features' handlers. This breaks the actions ↔ menus ↔ bookmark-actions cycle |
| `menus.ts` (13 importers) | `sidebar/menus.ts` (tab, space, folder, essentials), `bookmarks/menus.ts`, `toolbar/menus.ts`. Each builds `MenuItem[]` and uses `run()` for commands |

### Tests

- `tests/*.test.ts` move next to their modules (e.g. `tests/bookmarks.test.ts` → `src/features/bookmarks/bookmarks.test.ts`). `tests/release.test.ts` → `scripts/release.test.ts` at the repo root.
- `apps/desktop/package.json`: `"test": "node --test \"src/**/*.test.ts\""`.
- `tsconfig.json` `include` already covers `src/**/*.ts`, so colocated tests would become type-checked and need `@types/node`. Phase 2 excludes `src/**/*.test.ts` so the move stays a pure move. A follow-up adds `@types/node` and removes the exclusion, so tests are type-checked for the first time.

## 6. Desktop backend: `apps/desktop/src-tauri/src/`

### Conventions

- One folder per domain using `mod.rs` (the existing style in `host/`, `media/`, `vault/`). Each domain folder has a `commands.rs` holding its `#[tauri::command]` functions.
- `CmdResult` and the command error mapping move to `app/command.rs`. The `generate_handler!` list moves to `app/handlers.rs` and references `bookmarks::commands::bookmarks_list` and so on.
- Unit tests stay inline. When code plus tests exceed 600 lines, the test module moves to `<module>/tests.rs` (`#[cfg(test)] mod tests;`).
- No module named `windows`, because it would clash with the `windows` crate. Use `window`, `session` or `platform::win`.
- `impl Vault` and `impl Host` may span several files in their module. Fields the other files need become `pub(super)`.

### Target tree

```
src/
├─ main.rs                 # ≤ 80 lines: mod declarations, then app::run()
├─ app/                    # mod.rs (run, builder, plugins), setup.rs, single_instance.rs,
│                          # maintenance.rs (--purge-incognito etc.), command.rs, handlers.rs,
│                          # commands.rs (ui_ready, app_quit, window_*, chrome_devtools, pick_*,
│                          #   open_path, show_in_folder, clipboard_write)
├─ host/                   # tab host (WebView2); split in §8
│  ├─ mod.rs  state.rs  config.rs  focus.rs  chrome.rs  placement.rs  devtools.rs
│  │  accelerators.rs  debug.rs  commands.rs (tab_*, set_layout, focus_view, find_in_page,
│  │  permission_respond, clear_browsing_data, debug_*)
│  ├─ tab/  mod.rs  events.rs  permissions.rs  find.rs  sleep.rs        ← host/tab.rs
│  ├─ autofill.rs  page_tools.rs  downloads.rs                           (unchanged)
├─ session/                ← sessions.rs + session_load/save, window_open/undo_close, windows_restore
├─ incognito/              ← incognito.rs + incognito_open
├─ settings/               # mod.rs, wallpapers.rs, commands.rs (apply_settings, settings_*, wallpaper_*)
├─ shortcuts/              ← keys.rs, hotkey.rs + set_shortcuts
├─ storage/                ← store.rs: db.rs (SQLite worker), snapshot.rs (atomic JSON)
├─ history/                # history queries from store.rs + history_* commands
├─ bookmarks/              ← bookmarks.rs; split in §8; + bookmarks_*, favicon_fetch
├─ notes/                  # note storage + notes_list, note_save, note_delete
├─ importer/               ← importer.rs: mod.rs, chromium.rs, firefox.rs, commands.rs
├─ downloads/              # downloads_* commands (WebView2 wiring stays in host/downloads.rs)
├─ media/                  # existing; job.rs split in §8; + media_* commands
├─ blocker/                ← blocker.rs + blocker_* commands; resources/blocklist.txt unchanged
├─ extensions/             ← host/extensions.rs, host/crx.rs + ext_* commands
├─ vault/                  # existing; split in §8; commands.rs ← password_commands.rs
├─ profiles/               ← profiles.rs, profile_commands.rs; split in §8
├─ updates/                ← updates.rs + update_* commands
├─ search/                 # search_suggest command + suggestions() from net.rs
└─ platform/
   ├─ net.rs               ← net.rs (WinHTTP GET)
   ├─ urls.rs              ← urls.rs (URL validation for palette and other apps)
   └─ win/                 ← win.rs: default_browser.rs, shell.rs, memory.rs, commands.rs
                             (default_browser_*, memory_stats, system_memory_load)
```

## 7. Docs website: `apps/docs/` (Astro Starlight)

```
apps/docs/
├─ astro.config.mjs        # Starlight: title "Zenith", sidebar groups below, logo from @zenith/brand
├─ package.json            # @astrojs/starlight, astro; @astrojs/svelte only if live demos are added
├─ public/                 # favicon, og image
└─ src/
   ├─ content.config.ts
   ├─ assets/              # screenshots
   ├─ components/          # custom Astro components (download button, shortcut table)
   └─ content/docs/
      ├─ index.mdx                         # splash landing page with a download CTA
      ├─ getting-started/  install.md  first-run.md  import-from-other-browsers.md
      ├─ guides/           spaces-and-tabs.md  split-view.md  glance.md  bookmarks.md
      │                    passwords.md  downloads-and-video.md  extensions.md  profiles.md
      │                    incognito.md  notes.md  start-page.md  ad-blocking.md
      ├─ reference/        keyboard-shortcuts.md  settings.md  command-line.md
      └─ contributing/     architecture.md     ← docs/ARCHITECTURE.md
                           extensions.md       ← docs/EXTENSIONS.md
                           releasing.md        ← docs/RELEASING.md
                           zen-ui.md           ← docs/ZEN-UI.md
                           project-structure.md (end state of this spec)
                           measurements.md     ← measurements/README.md, blocking.md
```

- `docs/` is migrated with `git mv` and then deleted. Links in `README.md` and in Rust doc comments (e.g. `updates.rs` → "see docs/RELEASING.md") are updated.
- New `.github/workflows/docs.yml` builds `apps/docs` on changes under `apps/docs/**` or `packages/brand/**`. Deployment is an open question (§10).
- Later, not in this restructure: generate `reference/keyboard-shortcuts.md` and `reference/settings.md` from the app's `shortcuts.ts` / `settings.svelte.ts` defaults. When that happens, the shared data moves to a `packages/` entry rather than letting docs import from `apps/desktop`.

## 8. Split plan (files over 600 lines)

Line counts exclude Rust test modules. Every split moves code verbatim along the seams below. Seams marked *(confirm)* are inferred from structure and are verified when the phase starts.

### Backend

| File | Lines | Split |
| --- | --- | --- |
| `bookmarks.rs` | 1,533 + 533 tests | `mod.rs` (types Kind/Node/NewNode/NodePatch, pub API), `schema.rs` (migrate, legacy conversion), `validate.rs` (clean_*, prepare), `tree.rs` (list/add/update/move/reorder/remove/import_paths), `html.rs` (Netscape export and parser), `backup.rs` (JSON backup/restore, daily backups, date helpers), `favicon.rs`, `commands.rs`, `tests.rs` |
| `media/job.rs` | 1,438 + 380 tests | `job/mod.rs` (Job, Request, start/run/action/active/forget), `fetch.rs` (retries, ranges, segments, files), `record.rs` (live HLS), `output.rs` (ffmpeg, reserve/place, Mark of the Web), `state.rs` (Running, snapshot, publish/update/settle), `tests.rs` |
| `host/tab.rs` | 1,407 | `tab/mod.rs` (TabPatch, TabShared, Tab, CreateOptions, create/close/navigate/command), `events.rs` (`wire`, a ~480-line function, broken into `wire_navigation`, `wire_new_window`, `wire_resources` (blocking/media), `wire_permissions`, …), `permissions.rs`, `find.rs`, `sleep.rs` |
| `host/mod.rs` | 1,329 | `mod.rs` (Host, Win, with_host, post), `state.rs` (app/paths/db, emit helpers, tab↔window map), `config.rs` (RuntimeConfig, BlockerState), `focus.rs`, `chrome.rs` (attach_chrome, detach_window, shutdown), `placement.rs` (rects, place/clip view, on_resize, set_layout), `devtools.rs` (CDP, stray-target watch), `accelerators.rs`, `debug.rs` |
| `vault/mod.rs` | 1,226 + 105 tests | `mod.rs` (global with/try_with/status/broadcast/lock/timer), `types.rs` (Status, EntryInfo, Account, …), `origin.rs` (normalize/display/match), `vault.rs` (open, setup, unlock, master and recovery), `entries.rs` (list/reveal/add/update/delete, accounts, fill, capture, never-save), `transfer.rs` (CSV export, backup, preview/import, read_backup), `offers.rs` (save offers, pending imports), `tests.rs` |
| `commands.rs` | 1,183 | Dissolved into the domain `commands.rs` files in §6 |
| `profiles.rs` | 803 + 197 tests | `mod.rs` (Active, init, choose, requested, identifier/aumid), `store.rs` (List load/save/repair, create/update/delete, clean_name/new_id), `icons.rs`, `launch.rs` (lock, is_running, launch), `shortcuts.rs` (.lnk files), `commands.rs`, `tests.rs` |
| `media/mod.rs` | 775 + 89 tests | Report/item model vs. page scan vs. tool detection *(confirm)*, + `commands.rs` |
| `host/extensions.rs` | 637 | `extensions/mod.rs` (list/enable/remove), `install.rs` (folder/crx/store, offers), `updates.rs` (Web Store update checks) *(confirm)* |
| `media/dash.rs` | 624 + 125 tests | Tests to `dash/tests.rs`; MPD parsing vs. segment-template expansion *(confirm)* |
| `media/mp4.rs` | 592 + 168 tests | Under the limit; tests move to `mp4/tests.rs` only |

### Frontend

| File | Lines | Split |
| --- | --- | --- |
| `SettingsPage.svelte` | 1,249 | Shell (nav and scroll-spy over the 10 sections) + one component per section. General, Appearance, Privacy, Tabs & memory, Default browser and About go to `settings/sections/`. Bookmarks, Downloads, Notes and Shortcuts become `BookmarksSettings.svelte` etc. in their own features (as `passwords/AutofillSettings.svelte` already is) |
| `browser.svelte.ts` | 1,063 | Keep `BrowserStore` and its public API as a facade. Method bodies delegate to `spaces.ts`, `folders.ts`, `tabs.ts` (wake/activate/close/reopen/pin/move), `split.ts`, `glance.ts` and `session.ts` (snapshot/restore/startPrivate). Each takes the store as its first argument. The 33 importers do not change |
| `App.svelte` | 1,011 | `app/App.svelte` (layout) + `app/startup.ts` (ui_ready, restore, first-run) + `app/events.ts` (IPC listeners) + `app/Overlays.svelte` (prompts, popovers, dialogs) *(confirm)* |
| `BookmarksPage.svelte` | 984 | Library shell + `LibraryTree.svelte`, `LibraryList.svelte`, `LibraryDetails.svelte` *(confirm)* |
| `actions.ts` | 778 | Registry in `platform/actions.ts` + per-feature `actions.ts` (§5) |
| `menus.ts` | 739 | Per-feature `menus.ts` (§5) |
| `preview/mock.ts` | 698 | `platform/preview/` split by the same domains as `api.ts` |
| `StartPagePanel.svelte` | 678 | Panel shell + `WallpaperPicker.svelte` + `LayoutOptions.svelte` *(confirm)* |
| `NotesPage.svelte` | 664 | `NotesPage` + `NoteList.svelte` + `NoteEditor.svelte` *(confirm)* |
| `Palette.svelte` | 653 | `Palette` + `PaletteResults.svelte` + `palette-sources.ts` (tabs, bookmarks, history and command providers) |
| `ProfileDialog.svelte` | 619 | `ProfileDialog` + `ProfileForm.svelte` + `AvatarPicker.svelte` *(confirm)* |
| `scripts/dev/e2e.mjs` | 1,452 | `e2e/index.mjs` (runner), `e2e/harness.mjs` (CDP and instance helpers), `e2e/cases/*.mjs` grouped by area |

**Watchlist (500–600 lines, do not split now):** `NewTab.svelte` 585, `BookmarksPanel.svelte` 573, `SpacePane.svelte` 569, `TabStrip.svelte` 535, `store.rs` 534, `host/autofill.rs` 520, `TabItem.svelte` 506, `PasswordList.svelte` 502.

## 9. Phases

Each phase is its own branch or PR and lands only when green. Phase 7 can run in parallel with phases 2–6.

0. **Baseline.** Commit the current work in progress as its own commits. Tag `pre-restructure`. Record the passing test counts from `pnpm check`, `pnpm test`, `pnpm build` and `pnpm test:rust`.
1. **Monorepo shell.** `git mv` the app into `apps/desktop/`, `assets/logo.svg` into `packages/brand/` and `measurements/` into the app. Add the root `package.json` and the workspace globs, fix every path in §4, and regenerate the lockfile. Verify the four checks, `pnpm app:build`, and an e2e smoke run (`scripts/dev/run.ps1`, `e2e.mjs`). Confirm that the release workflow still produces a signed installer and `latest.json` (dry-run tag on a fork or a draft release).
2. **Frontend moves.** Create `app/`, `platform/`, `shared/` and `features/*`, and add the aliases. `git mv` every file per §5 and rewrite imports only. Colocate tests. Use one commit per feature.
3. **Frontend hubs.** Split `types.ts`, `ui.svelte.ts`, `ipc.ts` (peeled one domain at a time), `actions.ts` and `menus.ts` per §5.
4. **Frontend splits.** Split the frontend files listed in §8.
5. **Backend domains.** Create `app/` and move `CmdResult` and `handlers.rs`. Then work one domain per commit: move its files and its commands out of `commands.rs` into `<domain>/commands.rs`. `commands.rs` is deleted when it is empty.
6. **Backend splits.** Split the backend files listed in §8.
7. **Docs site.** Scaffold Starlight in `apps/docs` and migrate `docs/` and the measurement notes. Write the getting-started pages and add `docs.yml`.
8. **Guardrails.**
   - Add `scripts/check-structure.mjs` to CI. It checks the soft 600-line limit (with header-documented exceptions), at most 30 files per directory, nothing importing `app/` except `main.ts`, no `features/*` imports from `platform/preview`, and no import cycles between features.
   - Rewrite the `contributing/architecture.md` source map.
   - Add an `AGENTS.md` that states these rules.

## 10. Open questions

- **Docs hosting:** Cloudflare Pages or GitHub Pages, and which domain? This decides the deploy step in `docs.yml` and Starlight's `site` URL.
- **Landing vs. docs:** Is the docs site also the product's landing page (Starlight splash `index.mdx`), or will a separate marketing site exist later?
- **Docs versioning:** Do docs track only the latest release, or is a page per released version wanted?
- **Repository name:** Should the repo be renamed (e.g. `zenith`) when it becomes a monorepo? GitHub redirects old URLs, but the updater endpoint in `tauri.conf.json` must keep resolving.
