use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

pub fn write_json_atomic(path: &Path, bytes: &[u8], context: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{context} path has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {context} directory: {error}"))?;

    let temp_path = path.with_extension("json.tmp");

    {
        let mut temp_file = fs::File::create(&temp_path)
            .map_err(|error| format!("failed to create temporary {context} file: {error}"))?;
        temp_file
            .write_all(bytes)
            .map_err(|error| format!("failed to write temporary {context} file: {error}"))?;
        temp_file
            .sync_all()
            .map_err(|error| format!("failed to flush temporary {context} file: {error}"))?;
    }

    replace_file(&temp_path, path, context).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(windows)]
pub fn replace_file(temp_path: &Path, path: &Path, context: &str) -> Result<(), String> {
    if !path
        .try_exists()
        .map_err(|error| format!("failed to check existing {context} file: {error}"))?
    {
        return fs::rename(temp_path, path)
            .map_err(|error| format!("failed to save {context} file: {error}"));
    }

    let backup_path = backup_path(path);
    if backup_path
        .try_exists()
        .map_err(|error| format!("failed to check {context} backup file: {error}"))?
    {
        fs::remove_file(&backup_path)
            .map_err(|error| format!("failed to remove stale {context} backup file: {error}"))?;
    }

    fs::rename(path, &backup_path)
        .map_err(|error| format!("failed to back up existing {context} file: {error}"))?;

    match fs::rename(temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(save_error) => match fs::rename(&backup_path, path) {
            Ok(()) => Err(format!("failed to save {context} file: {save_error}")),
            Err(restore_error) => Err(format!(
                "failed to save {context} file: {save_error}; failed to restore {context} backup: {restore_error}"
            )),
        },
    }
}

#[cfg(not(windows))]
pub fn replace_file(temp_path: &Path, path: &Path, context: &str) -> Result<(), String> {
    fs::rename(temp_path, path).map_err(|error| format!("failed to save {context} file: {error}"))
}
