use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCli {
    pub file: PathBuf,
    pub args: Vec<String>,
    pub protocol: Option<String>,
}

pub fn resolve_cli_command(file: &Path, depth: u8) -> Option<ResolvedCli> {
    if depth > 4 || !file.is_file() {
        return None;
    }
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "cmd" | "bat") {
        static QUOTED: OnceLock<Regex> = OnceLock::new();
        let re = QUOTED.get_or_init(|| Regex::new(r#""([^"\r\n]+)""#).unwrap());
        let text = std::fs::read_to_string(file).ok()?;
        let dir = file.parent()?.to_string_lossy();
        for caps in re.captures_iter(&text) {
            let target = caps[1]
                .replace("%dp0%", &format!("{dir}\\"))
                .replace("%~dp0", &format!("{dir}\\"))
                .replace("%DP0%", &format!("{dir}\\"))
                .replace("%~DP0", &format!("{dir}\\"));
            if target.contains('%') {
                continue;
            }
            let p = Path::new(&target);
            if !p.is_absolute() {
                continue;
            }
            let lower = target.to_ascii_lowercase();
            if lower.ends_with("\\node.exe") || lower.ends_with("/node") || lower.ends_with("\\node") {
                continue;
            }
            if let Some(found) = resolve_cli_command(p, depth + 1) {
                return Some(found);
            }
        }
        return None;
    }
    Some(ResolvedCli {
        file: file.to_path_buf(),
        args: Vec::new(),
        protocol: None,
    })
}

pub fn find_in_path(names: &[&str]) -> Option<ResolvedCli> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file()
                && let Some(r) = resolve_cli_command(&candidate, 0)
            {
                return Some(r);
            }
        }
    }
    None
}
