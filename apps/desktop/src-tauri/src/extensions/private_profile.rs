//! Incognito extension policy synchronization for CEF incognito RequestContexts.
//!
//! Policy preferences (`extensions_in_incognito` + per-extension `incognito_off` in
//! `registry.json`) are preserved; full per-extension Chrome incognito toggle wiring is
//! completed in the Phase 4 extension bridge.

pub fn sync() {
    // shortcut: CEF Chrome-runtime per-extension incognito permission toggle is wired in Phase 4.
}
