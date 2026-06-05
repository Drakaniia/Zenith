# Tauri + Svelte Browser Shell Folder Structure

Status: Proposed  
Last verified against the working tree: 2026-06-05  
Scope: `src/` and `src-tauri/` architecture for ampui

This document proposes a scalable folder structure for ampui, a desktop browser shell built with
Tauri v2, Rust, Svelte 5 runes, TypeScript, and Vite.

It is not a claim that the current repository already matches this structure. The live tree should
be migrated feature by feature.

## Goals

- Keep system behavior, IPC, and UI reactivity from spreading across unrelated files.
- Make debugging fast by giving every feature one obvious Rust module, one TypeScript bridge, and
  one Svelte state module.
- Support future complexity such as multi-window orchestration, richer networking, high-frequency
  download streams, and synchronized shell state.
- Keep remote page webviews separate from the trusted shell UI and avoid broad IPC access.
- Make shared JSON contracts visible and testable.

## Chosen Pattern

Use a **Domain-Oriented Modular Monolith** with an explicit **Bridge** module.

This fits the Tauri/Svelte lifecycle better than a purely layer-based layout:

- Tauri runs native system behavior in Rust: webviews, windows, filesystem, networking,
  persistence, and download orchestration.
- Svelte runs the trusted shell UI: components, reactive state, interaction handling, and view
  composition.
- The Bridge is the adapter between those worlds. It owns Tauri `invoke`, events, channels, command
  names, event names, and DTO imports.

Each feature is a deep module. Callers should not need to know whether a feature uses a JSON store,
native webview calls, event listeners, or channels internally.

## Proposed `src/` Tree

```text
src/
|-- main.ts
|-- app/
|   |-- App.svelte
|   |-- bootstrap.ts
|   `-- styles.css
|
|-- shell/
|   |-- components/
|   |   |-- TitleBar.svelte
|   |   |-- NavigationBar.svelte
|   |   `-- StatusBar.svelte
|   `-- state/
|       `-- shell.svelte.ts
|
|-- bridge/
|   |-- invoke.ts
|   |-- errors.ts
|   |-- commands/
|   |   |-- bookmarks.ts
|   |   |-- downloads.ts
|   |   |-- history.ts
|   |   |-- navigation.ts
|   |   |-- settings.ts
|   |   |-- tabs.ts
|   |   `-- windows.ts
|   |-- events/
|   |   |-- browser-webviews.ts
|   |   `-- downloads.ts
|   `-- channels/
|       `-- downloads.ts
|
|-- shared/
|   |-- types/
|   |   |-- backend.ts
|   |   |-- events.ts
|   |   `-- ids.ts
|   `-- utils/
|       |-- internal-urls.ts
|       `-- url.ts
|
|-- features/
|   |-- tabs/
|   |   |-- components/
|   |   |   |-- TabBar.svelte
|   |   |   |-- TabItem.svelte
|   |   |   `-- WebviewHost.svelte
|   |   |-- state/
|   |   |   `-- tabs.svelte.ts
|   |   |-- model/
|   |   |   `-- types.ts
|   |   |-- actions/
|   |   |   `-- tab-actions.ts
|   |   `-- index.ts
|   |
|   |-- history/
|   |   |-- components/
|   |   |   `-- HistoryPanel.svelte
|   |   |-- state/
|   |   |   `-- history.svelte.ts
|   |   |-- model/
|   |   |   `-- types.ts
|   |   |-- actions/
|   |   |   `-- history-actions.ts
|   |   `-- index.ts
|   |
|   |-- downloads/
|   |   |-- components/
|   |   |   `-- DownloadManager.svelte
|   |   |-- state/
|   |   |   `-- downloads.svelte.ts
|   |   |-- model/
|   |   |   `-- types.ts
|   |   |-- actions/
|   |   |   `-- download-actions.ts
|   |   `-- index.ts
|   |
|   |-- bookmarks/
|   |-- settings/
|   `-- find-in-page/
|
|-- lib/
|   |-- ui/
|   `-- utils/
|
`-- tests/
    |-- unit/
    |-- integration/
    `-- fixtures/
```

### Frontend Module Roles

`app/` owns root application composition and startup.

`shell/` owns always-visible shell UI that is not specific to one feature.

`features/*/components/` contains Svelte components. Components render state and send user intent.
They should not import raw Tauri APIs.

`features/*/state/*.svelte.ts` contains Svelte 5 rune state. These files coordinate feature state,
derived values, and event subscriptions.

`features/*/actions/` contains user-intent functions that combine state changes and Bridge calls.
Use this when the action is more than a direct state mutation.

`features/*/model/` contains UI-only types and feature-specific constants. Backend DTOs do not live
here.

`bridge/` is the only frontend module allowed to import `@tauri-apps/api/core`,
`@tauri-apps/api/event`, or Tauri webview/window APIs.

`shared/types/backend.ts` contains TypeScript mirrors of Rust DTOs. Prefer generated types once the
project adopts a generator. Until then, maintain the file manually and treat it as part of the IPC
contract.

`lib/ui/` is for generic reusable UI primitives only. If a component knows about tabs, downloads,
history, settings, or webviews, it belongs in a feature folder.

## Proposed `src-tauri/` Tree

```text
src-tauri/
|-- src/
|   |-- main.rs
|   |-- lib.rs
|   |-- app.rs
|   |
|   |-- bridge/
|   |   |-- mod.rs
|   |   |-- registry.rs
|   |   |-- events.rs
|   |   `-- dto/
|   |       |-- bookmarks.rs
|   |       |-- downloads.rs
|   |       |-- history.rs
|   |       |-- settings.rs
|   |       |-- tabs.rs
|   |       `-- windows.rs
|   |
|   |-- core/
|   |   |-- error.rs
|   |   |-- ids.rs
|   |   |-- paths.rs
|   |   |-- time.rs
|   |   `-- url.rs
|   |
|   |-- runtime/
|   |   |-- app_state.rs
|   |   |-- webviews.rs
|   |   `-- windows.rs
|   |
|   |-- persistence/
|   |   |-- app_data.rs
|   |   |-- atomic_json.rs
|   |   `-- json_store.rs
|   |
|   `-- features/
|       |-- tabs/
|       |   |-- mod.rs
|       |   |-- domain.rs
|       |   |-- service.rs
|       |   |-- commands.rs
|       |   |-- events.rs
|       |   `-- tests.rs
|       |
|       |-- history/
|       |   |-- mod.rs
|       |   |-- domain.rs
|       |   |-- service.rs
|       |   |-- store.rs
|       |   |-- commands.rs
|       |   `-- tests.rs
|       |
|       |-- bookmarks/
|       |-- downloads/
|       |-- navigation/
|       |-- settings/
|       `-- windows/
|
|-- tests/
|   |-- bookmarks_integration.rs
|   |-- downloads_integration.rs
|   `-- history_integration.rs
|
|-- capabilities/
|   `-- default.json
|
|-- tauri.conf.json
|-- Cargo.toml
`-- build.rs
```

### Rust Module Roles

`main.rs` stays thin and calls the library entrypoint.

`lib.rs` owns Tauri builder setup, plugin registration, managed state registration, and command
registry wiring. It should not contain feature behavior.

`bridge/registry.rs` owns the `tauri::generate_handler!` list. This keeps command registration
centralized without making `lib.rs` a long command index.

`bridge/dto/` owns structs and enums that cross IPC. These types should derive `Serialize` and/or
`Deserialize` as needed.

`bridge/events.rs` owns event names and typed emit helpers. Event name strings should not be copied
through feature services.

`core/` owns cross-feature primitives: error types, path helpers, ID validation, time helpers, and
URL normalization primitives.

`runtime/` owns concrete Tauri adapters for native webviews, windows, and process-level app state.
This is where direct `WebviewBuilder`, `AppHandle`, and `WebviewWindow` logic belongs when it is not
specific to a single feature.

`persistence/` owns storage mechanics, including app data paths, atomic JSON writes, temp files, and
platform-specific replacement behavior.

`features/*/domain.rs` owns domain invariants and pure behavior.

`features/*/service.rs` owns orchestration. It can call persistence, runtime adapters, and event
emitters. It should be testable without constructing the full Tauri app whenever possible.

`features/*/commands.rs` owns `#[tauri::command]` functions. Command handlers should be thin:
deserialize input, call the service, return a serializable result.

`features/*/store.rs` owns feature-specific persistence if that persistence is not generic enough
for `persistence/`.

## File Placement Guidelines

### Core Browser Logic

Place core browser logic in Rust:

- Webview lifecycle: `src-tauri/src/runtime/webviews.rs` and `features/tabs/service.rs`.
- Window lifecycle: `src-tauri/src/runtime/windows.rs` and `features/windows/service.rs`.
- URL normalization and metadata fetching: `src-tauri/src/features/navigation/`.
- History, bookmarks, settings persistence: feature services plus `persistence/`.
- Download orchestration: `src-tauri/src/features/downloads/`.

Do not place durable browser behavior in Svelte components. The UI can request behavior and reflect
state, but Rust should own system effects.

### IPC Command Handlers

Place command handlers in `src-tauri/src/features/*/commands.rs`.

Rules:

- Every command must be registered through `bridge/registry.rs`.
- Commands return `Result<T, AppError>` or a project-standard serializable error type.
- Async command parameters should use owned types such as `String`, not borrowed types.
- Commands should not perform direct JSON file writes or webview operations inline when a service
  module can own that behavior.

### Svelte Components

Place Svelte components in one of three places:

- `shell/components/` for always-visible shell UI.
- `features/*/components/` for feature-specific UI.
- `lib/ui/` for generic, domain-free UI primitives.

Components should use Svelte 5 patterns:

- `$props()` for props.
- `$state()` for local reactive state.
- `$derived()` for computed state.
- Callback props for child-to-parent events.
- `onclick` and other event attributes rather than legacy event directives.

### Svelte `.svelte.ts` State Modules

Place reusable reactive state in `features/*/state/*.svelte.ts`.

Rules:

- Use `.svelte.ts` only when the module needs runes.
- Keep raw `invoke`, `listen`, and `Channel` calls out of state modules. Import Bridge wrappers
  instead.
- Keep component rendering concerns out of state modules.
- Event subscriptions must expose or use cleanup functions so listeners can be removed.

### Shared TypeScript Types

Place TypeScript IPC contract types in `src/shared/types/backend.ts`.

Rules:

- Backend DTO names should mirror Rust DTO names.
- Use the same field naming policy the Rust serde layer uses.
- Keep UI-only extensions in `features/*/model/types.ts`.
- Do not mix Tauri wrapper functions with DTO declarations.

The current repository has many DTOs and Tauri wrappers in `src/lib/utils/tauri.ts`. The target
structure splits that file into `shared/types/*`, `bridge/commands/*`, `bridge/events/*`, and
`bridge/channels/*`.

### Tests

Rust tests:

- Pure unit tests live beside the module with `#[cfg(test)]`.
- Feature integration tests live in `src-tauri/tests/`.
- Store tests should use temporary directories and verify atomic write behavior.
- Command tests should cover serialization and error behavior where feasible.

TypeScript and Svelte tests:

- The current `package.json` does not define a TS test command.
- After a test runner is added, place feature unit tests near feature code or in `src/tests/unit/`.
- Place Bridge contract tests under `src/tests/integration/`.
- Component tests should focus on rendering, callbacks, and state binding, not native Tauri effects.

## IPC Strategy

Tauri IPC should be organized by intent:

- Use commands for request/response operations such as `get_history`, `save_settings`, or
  `navigate_browser_webview`.
- Use events for small lifecycle notifications such as webview navigation, title changes, load
  status, and download status changes.
- Use channels for ordered high-frequency streams such as download progress, child process output,
  or future networking telemetry.

### Bridge Rules

Only `src/bridge/` may import Tauri APIs.

```text
Svelte component
  -> feature action
    -> feature state
      -> bridge command/event/channel wrapper
        -> Tauri IPC
          -> Rust command
            -> Rust feature service
```

No component should call `invoke` directly.

No feature state module should spell command names as raw strings.

No Rust service should depend on TypeScript naming. The contract is the serialized DTO and the
registered command/event name.

### Suggested Bridge Shape

```ts
// src/bridge/invoke.ts
import { invoke } from '@tauri-apps/api/core';

export type BackendCommand =
  | 'get_history'
  | 'search_history'
  | 'save_settings'
  | 'create_browser_webview'
  | 'navigate_browser_webview';

export function invokeCommand<T>(
  command: BackendCommand,
  args?: Record<string, unknown>
): Promise<T> {
  return invoke<T>(command, args);
}
```

```ts
// src/bridge/commands/history.ts
import type { HistoryEntry } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function getHistory(): Promise<HistoryEntry[]> {
  return invokeCommand<HistoryEntry[]>('get_history');
}

export function searchHistory(query: string): Promise<HistoryEntry[]> {
  return invokeCommand<HistoryEntry[]>('search_history', { query });
}
```

```ts
// src/bridge/events/browser-webviews.ts
import { listen } from '@tauri-apps/api/event';
import type { BrowserWebviewNavigationEvent } from '../../shared/types/events';

export const BROWSER_WEBVIEW_NAVIGATION_EVENT = 'browser-webview-navigation';

export function onBrowserWebviewNavigation(
  handler: (event: BrowserWebviewNavigationEvent) => void
): Promise<() => void> {
  return listen<BrowserWebviewNavigationEvent>(BROWSER_WEBVIEW_NAVIGATION_EVENT, (event) => {
    handler(event.payload);
  });
}
```

### Rust Command Shape

```rust
// src-tauri/src/features/history/commands.rs
use crate::bridge::dto::history::HistoryEntryDto;
use crate::core::error::AppError;

#[tauri::command]
pub async fn get_history() -> Result<Vec<HistoryEntryDto>, AppError> {
    // Call the history service here.
    todo!()
}
```

The command file is an adapter. The service owns behavior.

## Import Rules

- `features/*/components/` may import from its own feature state/actions/model, `shell/`,
  `shared/`, and `lib/ui/`.
- `features/*/state/` may import from `bridge/`, `shared/`, and its own feature model.
- `bridge/` may import Tauri APIs and `shared/types`, but not Svelte components or feature state.
- `shared/` may not import from `features/`, `bridge/`, or Tauri APIs.
- Rust `features/*/commands.rs` may call its own service, but should not call another feature's
  command handler.
- Rust services may depend on `core/`, `runtime/`, `persistence/`, `bridge/dto/`, and another
  feature's public module only when there is a real domain dependency.

## Debugging Map

For a failed command:

1. Check `src/bridge/commands/<feature>.ts` for command name and argument shape.
2. Check `src-tauri/src/bridge/registry.rs` for registration.
3. Check `src-tauri/src/features/<feature>/commands.rs` for IPC deserialization.
4. Check `src-tauri/src/features/<feature>/service.rs` for behavior.
5. Check `src-tauri/src/persistence/` or `runtime/` if the failure is storage or native runtime
   related.

For a missing event:

1. Check the event constant in `src/bridge/events/<feature>.ts`.
2. Check the typed Rust emitter in `src-tauri/src/bridge/events.rs`.
3. Check listener cleanup in the `.svelte.ts` state module.
4. Check whether the event should be webview-specific instead of global.

For a stale UI state issue:

1. Check the feature `.svelte.ts` module.
2. Check whether Bridge calls are awaited or intentionally fire-and-forget.
3. Check whether Rust emits a completion event after system work finishes.

## Migration Notes For The Current Repo

The current repository already has useful modules, but several target seams are still flat:

- `src/lib/utils/tauri.ts` should be split into Bridge wrappers and shared DTO types.
- `src/lib/state/*.svelte.ts` should move feature by feature into `src/features/*/state/`.
- `src/lib/components/*.svelte` should move into `shell/components/` or feature component folders.
- `src-tauri/src/models.rs` can become `src-tauri/src/bridge/dto/mod.rs` and re-export feature DTOs.
- `src-tauri/src/commands/*.rs` can move gradually into `src-tauri/src/features/*/commands.rs`.
- `src-tauri/src/lib.rs` should keep builder setup but delegate command registration to
  `bridge/registry.rs`.

Do not perform the migration as one broad mechanical move. Move one feature at a time, keep command
names stable, and run the verified Rust and Svelte checks after each feature migration.

## Recommended Adoption Order

1. Create `src/bridge/` and move raw Tauri calls out of `src/lib/utils/tauri.ts`.
2. Create `src/shared/types/backend.ts` and move DTO declarations there.
3. Move the tabs frontend module into `src/features/tabs/`.
4. Move history Rust code into `src-tauri/src/features/history/`.
5. Extract `src-tauri/src/persistence/atomic_json.rs` before moving bookmarks and settings.
6. Move downloads last because it combines persistence-like state, networking, filesystem output,
   and progress events.

## Verification Commands

Use only commands verified for this repository:

```text
pnpm check
pnpm build
```

```text
cd src-tauri
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

At the time this document was written, `package.json` did not define `pnpm test` or `pnpm lint`.

## References

- Tauri IPC: https://v2.tauri.app/concept/inter-process-communication/
- Tauri calls from Rust to frontend, events, and channels:
  https://v2.tauri.app/develop/calling-frontend/
- Tauri runtime authority and capabilities:
  https://v2.tauri.app/security/runtime-authority/
- Svelte 5 runes: https://svelte.dev/docs/svelte/what-are-runes
- Svelte `.svelte.ts` modules: https://svelte.dev/docs/svelte/svelte-js-files
