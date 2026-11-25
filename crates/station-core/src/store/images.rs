use crate::atomic::write_json_atomic;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

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

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
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
            if is_valid_image_id(&id) {
                let pb = PathBuf::from(p);
                if mime_for_ext(&pb).is_some() {
                    map.insert(id.clone(), pb);
                    order.push_back(id);
                }
            }
        }
        Self {
            file,
            dir,
            map,
            order,
        }
    }

    pub fn register(&mut self, file: &Path) -> Option<String> {
        mime_for_ext(file)?;
        if is_symlink(file) {
            return None;
        }
        let abs = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        if is_symlink(&abs) {
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
        let abs = std::fs::canonicalize(&target).unwrap_or(target);
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

    pub fn resolve(&self, id: &str) -> Result<(PathBuf, &'static str), ImageResolveError> {
        if !is_valid_image_id(id) {
            return Err(ImageResolveError::InvalidId);
        }
        let path = self.map.get(id).ok_or(ImageResolveError::NotFound)?;
        let mime = mime_for_ext(path).ok_or(ImageResolveError::NotFound)?;
        let meta = std::fs::symlink_metadata(path).map_err(|_| ImageResolveError::NotFound)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ImageResolveError::NotFound);
        }
        if meta.len() > MAX_IMAGE_BYTES {
            return Err(ImageResolveError::TooLarge);
        }
        Ok((path.clone(), mime))
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
}
