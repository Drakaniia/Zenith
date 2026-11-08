# Working in this repository

Zenith is a pnpm monorepo: the browser in `apps/desktop` (Svelte 5 UI in `src/`, Rust backend in `src-tauri/`), its website in `apps/docs` (Astro Starlight), shared assets in `packages/`. The full layout and rules are in `apps/docs/src/content/docs/contributing/project-structure.md`; read it before adding files.

## Where code goes

- UI: `apps/desktop/src/features/<domain>/` for anything belonging to one domain (components, `<domain>.svelte.ts` store, pure `.ts` logic with colocated `*.test.ts`, `api.ts`, `types.ts`, `actions.ts`, `menus.ts`). `shared/` only for domain-free code used by 3+ features; `platform/` only for the backend bridge; `app/` is the composition root and nothing else imports it.
- Backend: one module folder per domain under `apps/desktop/src-tauri/src/`, with its commands in `<domain>/commands.rs` and registered in `app/handlers.rs`.
- Docs: user guides and references in `apps/docs/src/content/docs/`; contributor docs in its `contributing/`.

## Rules

- Files stay under 600 lines (Rust: excluding the test module); folders under 30 files; at most 3 levels below `src/`.
- Imports flow `main.ts → app → features → shared / platform`. Plain `.ts` modules use relative imports (the Node test runner resolves no aliases); `.svelte`/`.svelte.ts` files use `$features`, `$shared`, `$platform` across folders.
- No barrels (`index.ts`). No module named `windows` in Rust.
- Tauri command names, event names, settings/session JSON shapes and storage paths are a contract with installed copies: don't rename them casually.

## Before you finish

From the repository root: `pnpm check`, `pnpm test`, `pnpm check:structure`, `pnpm --filter @zenith/desktop build`, and for Rust changes `pnpm test:rust`. For docs changes, `pnpm docs:build`.
