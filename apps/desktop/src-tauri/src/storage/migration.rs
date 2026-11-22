//! Pre-CEF migration backups: preserves application-owned data (JSON/SQLite/vault/registry)
//! before the first CEF startup without ever mutating legacy `WebView2` directories.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use serde::{Deserialize, Serialize};

use super::{write_atomic, Paths};

const MARKER_FILE: &str = ".migrated-v1";
const ROAMING_FILES: &[&str] = &[
    "session.json",
    "settings.json",
    "window.json",
    "zenith.db",
    "vault.json",
    "vault.bin",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct MigrationMarker {
    migrated_at: i64,
    engine: String,
    #[serde(default)]
    from_legacy: bool,
    #[serde(default)]
    notice_seen: bool,
}

/// Creates a verified, idempotent pre-migration backup of application-owned state before the
/// first CEF startup. Never touches or removes legacy `WebView2/` data.
pub fn ensure_pre_cef_backup(paths: &Paths) -> Result<(), String> {
    let cef_dir = paths.cef_data();
    let marker = cef_dir.join(MARKER_FILE);
    if marker.exists() {
        return Ok(());
    }

    let backup_dir = paths.migration_backups();
    let tmp_dir = paths.roaming.join("pre-cef-backup.tmp");
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    let has_app_data = ROAMING_FILES.iter().any(|f| paths.roaming.join(f).is_file())
        || paths.roaming.join("windows").is_dir()
        || paths.extensions().join("registry.json").is_file();
    let from_legacy = paths.webview_data().is_dir() || has_app_data;

    if has_app_data && !backup_dir.exists() {
        fs::create_dir_all(&tmp_dir).map_err(|e| format!("create backup staging dir: {e}"))?;

        for name in ROAMING_FILES {
            let src = paths.roaming.join(name);
            if src.is_file() {
                let dst = tmp_dir.join(name);
                if *name == "zenith.db" {
                    backup_sqlite(&src, &dst)?;
                } else {
                    copy_verified(&src, &dst)?;
                }
            }
        }

        let src_windows = paths.roaming.join("windows");
        if src_windows.is_dir() {
            copy_dir_verified(&src_windows, &tmp_dir.join("windows"))?;
        }

        let src_registry = paths.extensions().join("registry.json");
        if src_registry.is_file() {
            let ext_dst = tmp_dir.join("extensions");
            fs::create_dir_all(&ext_dst).map_err(|e| format!("create backup extensions dir: {e}"))?;
            copy_verified(&src_registry, &ext_dst.join("registry.json"))?;
        }

        fs::rename(&tmp_dir, &backup_dir).map_err(|e| format!("commit pre-cef backup: {e}"))?;
    }

    let stamp = MigrationMarker {
        migrated_at: super::now_ms(),
        engine: "cef-v1".into(),
        from_legacy,
        notice_seen: !from_legacy,
    };
    let bytes = serde_json::to_vec(&stamp).map_err(|e| format!("serialize migration marker: {e}"))?;
    write_atomic(&marker, &bytes).map_err(|e| format!("write migration marker: {e}"))?;
    Ok(())
}

/// True when this profile upgraded from legacy WebView2 data and has not yet acknowledged the
/// one-time fresh-site-storage notice.
pub fn migration_notice_pending(paths: &Paths) -> bool {
    let marker = paths.cef_data().join(MARKER_FILE);
    let Ok(text) = fs::read_to_string(marker) else { return false };
    serde_json::from_str::<MigrationMarker>(&text)
        .map(|m| m.from_legacy && !m.notice_seen)
        .unwrap_or(false)
}

/// Marks the one-time CEF upgrade notice as acknowledged.
pub fn dismiss_migration_notice(paths: &Paths) -> Result<(), String> {
    let marker = paths.cef_data().join(MARKER_FILE);
    let mut state = fs::read_to_string(&marker)
        .ok()
        .and_then(|t| serde_json::from_str::<MigrationMarker>(&t).ok())
        .unwrap_or(MigrationMarker {
            migrated_at: super::now_ms(),
            engine: "cef-v1".into(),
            from_legacy: false,
            notice_seen: true,
        });
    state.notice_seen = true;
    let bytes = serde_json::to_vec(&state).map_err(|e| format!("serialize migration marker: {e}"))?;
    write_atomic(&marker, &bytes).map_err(|e| format!("write migration marker: {e}"))?;
    Ok(())
}

fn copy_verified(src: &Path, dst: &Path) -> Result<(), String> {
    let expected = fs::metadata(src)
        .map_err(|e| format!("stat {}: {e}", src.display()))?
        .len();
    let copied = fs::copy(src, dst)
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    if copied != expected {
        return Err(format!("backup size mismatch for {}", src.display()));
    }
    if dst.extension().and_then(|e| e.to_str()) == Some("json") && expected > 0 {
        let text = fs::read_to_string(dst)
            .map_err(|e| format!("read backup {}: {e}", dst.display()))?;
        let trimmed = text.trim_start_matches('\u{feff}').trim();
        if !trimmed.is_empty() {
            serde_json::from_str::<serde_json::Value>(trimmed)
                .map_err(|e| format!("verify json backup {}: {e}", dst.display()))?;
        }
    }
    Ok(())
}

fn copy_dir_verified(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("readdir {}: {e}", src.display()))?;
    for entry in entries.flatten() {
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_verified(&p, &target)?;
        } else if p.is_file() {
            copy_verified(&p, &target)?;
        }
    }
    Ok(())
}

fn backup_sqlite(src: &Path, dst: &Path) -> Result<(), String> {
    // Open read-only and copy via VACUUM INTO so WAL journals are consolidated cleanly.
    let conn = Connection::open(src).map_err(|e| format!("open sqlite for backup: {e}"))?;
    let escaped = dst.display().to_string().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}';"))
        .or_else(|_| fs::copy(src, dst).map(|_| ()).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e))))
        .map_err(|e| format!("sqlite backup failed: {e}"))?;
    let check = Connection::open(dst).map_err(|e| format!("reopen sqlite backup: {e}"))?;
    let result: String = check
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| format!("sqlite backup integrity check failed: {e}"))?;
    if result != "ok" {
        return Err(format!("sqlite backup failed quick_check: {result}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(tag: &str) -> Paths {
        let base = std::env::temp_dir().join(format!("zenith-cef-mig-{tag}-{}", super::super::now_ms()));
        let _ = fs::remove_dir_all(&base);
        let paths = Paths {
            roaming: base.join("roaming"),
            local: base.join("local"),
        };
        fs::create_dir_all(&paths.roaming).unwrap();
        fs::create_dir_all(&paths.local).unwrap();
        paths
    }

    #[test]
    fn backs_up_app_data_and_preserves_legacy_webview2() {
        let paths = temp_paths("preserve");
        fs::write(paths.settings(), r#"{"onboarded":true}"#).unwrap();
        fs::write(paths.session(), r#"{"tabs":[]}"#).unwrap();
        let db = Connection::open(paths.db()).unwrap();
        super::super::migrate(&db).unwrap();
        drop(db);

        let wv = paths.webview_data();
        fs::create_dir_all(&wv).unwrap();
        fs::write(wv.join("legacy.dat"), b"keep-me").unwrap();

        let ext_dir = paths.extensions();
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(ext_dir.join("registry.json"), r#"{"folders":{}}"#).unwrap();

        // Simulate an interrupted earlier backup staging folder.
        let stale_tmp = paths.roaming.join("pre-cef-backup.tmp");
        fs::create_dir_all(&stale_tmp).unwrap();
        fs::write(stale_tmp.join("partial.json"), b"{").unwrap();

        ensure_pre_cef_backup(&paths).unwrap();
        assert!(paths.cef_data().join(MARKER_FILE).exists());
        assert!(migration_notice_pending(&paths));
        assert!(paths.migration_backups().join("settings.json").exists());
        assert!(paths.migration_backups().join("session.json").exists());
        assert!(paths.migration_backups().join("zenith.db").exists());
        assert!(paths.migration_backups().join("extensions").join("registry.json").exists());
        assert!(!stale_tmp.exists());
        // Legacy WebView2 directory must remain untouched.
        assert_eq!(fs::read(wv.join("legacy.dat")).unwrap(), b"keep-me");

        // Second run is idempotent and does not overwrite existing backup.
        fs::write(paths.settings(), r#"{"onboarded":false}"#).unwrap();
        ensure_pre_cef_backup(&paths).unwrap();
        assert_eq!(
            fs::read_to_string(paths.migration_backups().join("settings.json")).unwrap(),
            r#"{"onboarded":true}"#
        );

        dismiss_migration_notice(&paths).unwrap();
        assert!(!migration_notice_pending(&paths));
        ensure_pre_cef_backup(&paths).unwrap();
        assert!(!migration_notice_pending(&paths));

        let _ = fs::remove_dir_all(paths.roaming.parent().unwrap());
    }

    #[test]
    fn fresh_profile_does_not_show_migration_notice() {
        let paths = temp_paths("fresh");
        ensure_pre_cef_backup(&paths).unwrap();
        assert!(!migration_notice_pending(&paths));
        let _ = fs::remove_dir_all(paths.roaming.parent().unwrap());
    }
}
