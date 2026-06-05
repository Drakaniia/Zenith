use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

use super::atomic_json::write_json_atomic;

pub fn read_json_or_default<T>(path: &Path) -> Result<T, String>
where
    T: Default + DeserializeOwned,
{
    if !path.exists() {
        return Ok(T::default());
    }

    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("failed to read JSON: {error}"))?;
    Ok(serde_json::from_str(&contents).unwrap_or_default())
}

pub fn write_json<T>(path: &Path, value: &T, context: &str) -> Result<(), String>
where
    T: Serialize,
{
    let json = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize {context}: {error}"))?;
    write_json_atomic(path, &json, context)
}
