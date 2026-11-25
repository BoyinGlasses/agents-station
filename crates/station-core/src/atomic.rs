use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
fn replace_file_atomic(tmp: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let src_wide: Vec<u16> = tmp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dst_wide: Vec<u16> = dst
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `src_wide` and `dst_wide` are valid null-terminated UTF-16 buffers
    // alive for the full duration of the synchronous `MoveFileExW` call.
    let ok = unsafe {
        MoveFileExW(
            src_wide.as_ptr(),
            dst_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file_atomic(tmp: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, dst)
}

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

    let write_res = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp);
        return Err(err).with_context(|| format!("write temp {}", tmp.display()));
    }

    if let Err(err) = replace_file_atomic(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err).with_context(|| {
            format!("atomic replace {} -> {}", tmp.display(), path.display())
        });
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

    #[test]
    fn write_json_atomic_overwrites_unicode_path_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("café-状態.json");
        for v in 1..=3 {
            write_json_atomic(&target, &serde_json::json!({ "v": v })).unwrap();
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed["v"], 3);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }

    #[cfg(windows)]
    #[test]
    fn write_json_atomic_never_destroys_target_or_leaks_tmp_when_locked() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("critical.json");
        write_json_atomic(&target, &serde_json::json!({ "v": 1 })).unwrap();

        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&target)
            .unwrap();

        let res = write_json_atomic(&target, &serde_json::json!({ "v": 2 }));
        drop(reader);

        assert!(res.is_err(), "locked target without delete share should fail cleanly");
        assert!(
            target.exists(),
            "crash-safe atomic replace must never delete existing target file"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed["v"], 1);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }
}
