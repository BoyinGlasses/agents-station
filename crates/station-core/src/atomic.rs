use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent dir for {}", path.display()))?;
    }
    let pid = std::process::id();
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let tmp = path.with_file_name(format!("{file_name}.{pid}.{seq}.tmp"));

    std::fs::write(&tmp, bytes).with_context(|| format!("write temp {}", tmp.display()))?;

    if let Err(first_err) = std::fs::rename(&tmp, path) {
        if path.exists() {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path).with_context(|| {
                format!(
                    "atomic replace {} failed after {first_err}",
                    path.display()
                )
            })?;
        } else {
            let _ = std::fs::remove_file(&tmp);
            return Err(first_err)
                .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()));
        }
    }
    Ok(())
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("serialize json")?;
    write_bytes_atomic(path, json.as_bytes())
}

pub fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let backup = path.with_file_name(format!("{file_name}.backup-{ts}"));
    std::fs::copy(path, &backup)
        .with_context(|| format!("backup {} -> {}", path.display(), backup.display()))?;
    Ok(Some(backup))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_json_atomic_replaces_existing_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested/state.json");
        write_json_atomic(&target, &serde_json::json!({ "v": 1 })).unwrap();
        write_json_atomic(&target, &serde_json::json!({ "v": 2 })).unwrap();

        let text = std::fs::read_to_string(&target).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["v"], 2);

        let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }

    #[test]
    fn backup_file_creates_timestamped_copy() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        std::fs::write(&target, "{\"a\":1}").unwrap();
        let bak = backup_file(&target).unwrap().expect("backup created");
        assert!(bak.file_name().unwrap().to_string_lossy().starts_with("settings.json.backup-"));
        assert_eq!(std::fs::read_to_string(bak).unwrap(), "{\"a\":1}");
    }
}
