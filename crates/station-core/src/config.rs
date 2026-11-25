use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 4310;
pub const DEFAULT_HOST: &str = "127.0.0.1";

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn resolve_data_dir(root: &Path, env_override: Option<&str>) -> PathBuf {
    if let Some(ov) = env_override
        && !ov.trim().is_empty()
    {
        return PathBuf::from(ov);
    }
    let is_node_modules = root
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("node_modules"));
    if is_node_modules {
        home_dir().join(".my-office")
    } else {
        root.join("data")
    }
}

pub fn resolve_claude_dir(env_override: Option<&str>) -> PathBuf {
    if let Some(ov) = env_override
        && !ov.trim().is_empty()
    {
        return PathBuf::from(ov);
    }
    home_dir().join(".claude")
}

/// Resolves default runtime `data/` dir using `OFFICE_DATA_DIR` or anchoring to the executable's install root.
pub fn default_data_dir() -> PathBuf {
    let env_dir = std::env::var("OFFICE_DATA_DIR").ok();
    let root = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    resolve_data_dir(&root, env_dir.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_data_dir_honors_override_and_node_modules() {
        let root = Path::new("D:/mystation/agents-station");
        assert_eq!(
            resolve_data_dir(root, Some("D:/tmp/custom-data")),
            PathBuf::from("D:/tmp/custom-data")
        );
        assert_eq!(resolve_data_dir(root, None), root.join("data"));

        let nm = Path::new("C:/Users/me/AppData/Roaming/npm/node_modules/my-office");
        assert_eq!(resolve_data_dir(nm, None), home_dir().join(".my-office"));
    }
}
