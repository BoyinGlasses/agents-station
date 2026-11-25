use crate::atomic::write_json_atomic;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_IMAGES: usize = 400;
pub const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageResolveError {
    InvalidId,
    NotFound,
    TooLarge,
}

pub fn mime_for_ext(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

pub fn is_valid_image_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_safe_abs_path(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
}

fn has_any_symlink(path: &Path) -> bool {
    let mut cur = PathBuf::new();
    for comp in path.components() {
        cur.push(comp);
        if matches!(comp, Component::Normal(_))
            && std::fs::symlink_metadata(&cur).is_ok_and(|m| m.file_type().is_symlink())
        {
            return true;
        }
    }
    false
}

/// Opens the target image without following leaf/ancestor symlinks, validates
/// regular-file metadata on the open handle, and enforces a strict byte cap.
pub fn read_image_bytes_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, ImageResolveError> {
    if !is_safe_abs_path(path) || has_any_symlink(path) {
        return Err(ImageResolveError::NotFound);
    }
    let meta_link = std::fs::symlink_metadata(path).map_err(|_| ImageResolveError::NotFound)?;
    if meta_link.file_type().is_symlink() || !meta_link.is_file() {
        return Err(ImageResolveError::NotFound);
    }
    let file = File::open(path).map_err(|_| ImageResolveError::NotFound)?;
    let meta = file.metadata().map_err(|_| ImageResolveError::NotFound)?;
    if !meta.is_file() {
        return Err(ImageResolveError::NotFound);
    }
    if meta.len() > max_bytes {
        return Err(ImageResolveError::TooLarge);
    }
    let mut buf = Vec::with_capacity((meta.len().min(max_bytes)) as usize);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|_| ImageResolveError::NotFound)?;
    if buf.len() as u64 > max_bytes {
        return Err(ImageResolveError::TooLarge);
    }
    Ok(buf)
}

pub struct ImageStore {
    file: PathBuf,
    dir: PathBuf,
    map: HashMap<String, PathBuf>,
    order: VecDeque<String>,
}

impl ImageStore {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("images.json");
        let dir = data_dir.join("images");
        let raw: HashMap<String, String> = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut map = HashMap::new();
        let mut order = VecDeque::new();
        for (id, p) in raw {
            if map.len() >= MAX_IMAGES {
                break;
            }
            if !is_valid_image_id(&id) {
                continue;
            }
            let pb = PathBuf::from(p);
            if mime_for_ext(&pb).is_some() && is_safe_abs_path(&pb) && !has_any_symlink(&pb) {
                map.insert(id.clone(), pb);
                order.push_back(id);
            }
        }
        Self {
            file,
            dir,
            map,
            order,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn register(&mut self, file: &Path) -> Option<String> {
        mime_for_ext(file)?;
        if !is_safe_abs_path(file) || has_any_symlink(file) {
            return None;
        }
        let abs = std::fs::canonicalize(file).ok()?;
        mime_for_ext(&abs)?;
        if has_any_symlink(&abs) {
            return None;
        }
        let meta = std::fs::symlink_metadata(&abs).ok()?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return None;
        }
        let key_str = abs.to_string_lossy();
        let digest = Sha256::digest(key_str.as_bytes());
        let id: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
        if self.map.get(&id).is_some_and(|existing| existing == &abs) {
            return Some(id);
        }
        self.map.insert(id.clone(), abs);
        self.order.retain(|x| x != &id);
        self.order.push_back(id.clone());
        while self.map.len() > MAX_IMAGES {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        let _ = self.save();
        Some(id)
    }

    pub fn save_bytes(&mut self, ext: &str, bytes: &[u8]) -> anyhow::Result<String> {
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            anyhow::bail!("image too large");
        }
        let clean_ext = ext.trim_start_matches('.').to_ascii_lowercase();
        let probe = PathBuf::from(format!("x.{clean_ext}"));
        if mime_for_ext(&probe).is_none() {
            anyhow::bail!("unsupported image extension");
        }
        std::fs::create_dir_all(&self.dir)?;
        let digest = Sha256::digest(bytes);
        let id: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
        let target = self.dir.join(format!("{id}.{clean_ext}"));
        std::fs::write(&target, bytes)?;
        let abs = std::fs::canonicalize(&target)?;
        if has_any_symlink(&abs) {
            anyhow::bail!("symlink target rejected");
        }
        self.map.insert(id.clone(), abs);
        self.order.retain(|x| x != &id);
        self.order.push_back(id.clone());
        while self.map.len() > MAX_IMAGES {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        let _ = self.save();
        Ok(id)
    }

    /// Lock-light lookup returning `(path, mime)` without touching the filesystem.
    pub fn lookup(&self, id: &str) -> Result<(PathBuf, &'static str), ImageResolveError> {
        if !is_valid_image_id(id) {
            return Err(ImageResolveError::InvalidId);
        }
        let path = self.map.get(id).ok_or(ImageResolveError::NotFound)?;
        let mime = mime_for_ext(path).ok_or(ImageResolveError::NotFound)?;
        Ok((path.clone(), mime))
    }

    pub fn resolve(&self, id: &str) -> Result<(PathBuf, &'static str), ImageResolveError> {
        let (path, mime) = self.lookup(id)?;
        if !is_safe_abs_path(&path) || has_any_symlink(&path) {
            return Err(ImageResolveError::NotFound);
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|_| ImageResolveError::NotFound)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ImageResolveError::NotFound);
        }
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(ImageResolveError::TooLarge);
        }
        Ok((path, mime))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let ser: HashMap<&str, String> = self
            .map
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_string_lossy().into_owned()))
            .collect();
        write_json_atomic(&self.file, &ser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_store_registers_resolves_persists_and_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("demo.webp");
        std::fs::write(&img, b"RIFFdemo").unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, b"plain").unwrap();

        let mut store = ImageStore::load(dir.path());
        assert!(store.register(&txt).is_none());
        let id = store.register(&img).expect("webp must register");
        assert!(is_valid_image_id(&id));

        let (resolved, mime) = store.resolve(&id).unwrap();
        assert_eq!(mime, "image/webp");
        assert_eq!(std::fs::read(resolved).unwrap(), b"RIFFdemo");

        assert_eq!(store.resolve("../secret"), Err(ImageResolveError::InvalidId));
        assert_eq!(store.resolve("a/b"), Err(ImageResolveError::InvalidId));
        assert_eq!(store.resolve("deadbeef00112233"), Err(ImageResolveError::NotFound));

        let reloaded = ImageStore::load(dir.path());
        assert!(reloaded.resolve(&id).is_ok());
    }

    #[test]
    fn image_store_rejects_relative_parent_missing_symlink_caps_load_and_bounds_read() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let real = sub.join("real.png");
        std::fs::write(&real, b"PNGDATA").unwrap();

        let mut store = ImageStore::load(dir.path());
        assert!(store.register(Path::new("relative.png")).is_none(), "relative path must be rejected");
        assert!(store.register(&sub.join("../sub/real.png")).is_none(), "parent-dir .. must be rejected");
        assert!(store.register(&dir.path().join("missing.png")).is_none(), "missing file must be rejected");

        let link = dir.path().join("link.png");
        #[cfg(windows)]
        let sym_ok = std::os::windows::fs::symlink_file(&real, &link).is_ok();
        #[cfg(unix)]
        let sym_ok = std::os::unix::fs::symlink(&real, &link).is_ok();
        if sym_ok {
            assert!(store.register(&link).is_none(), "symlink file must be rejected");
        }

        // Persisted images.json with > MAX_IMAGES entries and relative/.. paths must be filtered & capped on load
        let mut fake: HashMap<String, String> = HashMap::new();
        for i in 0..(MAX_IMAGES + 25) {
            fake.insert(format!("{i:016x}"), real.to_string_lossy().into_owned());
        }
        fake.insert("aaaaaaaaaaaaaaaa".into(), "relative.png".into());
        fake.insert("bbbbbbbbbbbbbbbb".into(), sub.join("../sub/real.png").to_string_lossy().into_owned());
        std::fs::write(dir.path().join("images.json"), serde_json::to_string(&fake).unwrap()).unwrap();
        let loaded = ImageStore::load(dir.path());
        assert!(loaded.len() <= MAX_IMAGES, "load must enforce MAX_IMAGES cap, got {}", loaded.len());
        assert_eq!(loaded.resolve("aaaaaaaaaaaaaaaa"), Err(ImageResolveError::NotFound));
        assert_eq!(loaded.resolve("bbbbbbbbbbbbbbbb"), Err(ImageResolveError::NotFound));

        // Bounded read rejects oversized files without unbounded allocation
        let big = dir.path().join("big.png");
        std::fs::write(&big, vec![0u8; 64]).unwrap();
        assert_eq!(read_image_bytes_bounded(&big, 32), Err(ImageResolveError::TooLarge));
        assert_eq!(read_image_bytes_bounded(&big, 64).unwrap().len(), 64);
    }
}
