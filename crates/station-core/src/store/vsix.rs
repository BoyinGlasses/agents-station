use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixInfo {
    pub publisher: String,
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
}

pub fn find_installed_claude_vsix(home: &Path) -> Vec<VsixInfo> {
    let editors = [
        ".vscode",
        ".vscode-insiders",
        ".cursor",
        ".windsurf",
        ".antigravity",
        ".antigravity-ide",
    ];
    let mut out = Vec::new();
    for ed in editors {
        let ext_root = home.join(ed).join("extensions");
        let Ok(entries) = std::fs::read_dir(ext_root) else {
            continue;
        };
        for e in entries.flatten() {
            let fname = e.file_name().to_string_lossy().into_owned();
            if let Some(rest) = fname.strip_prefix("anthropic.claude-code-") {
                let ver = rest.split('-').next().unwrap_or("").to_owned();
                out.push(VsixInfo {
                    publisher: "anthropic".to_owned(),
                    name: "claude-code".to_owned(),
                    version: ver,
                    dir: e.path(),
                });
            }
        }
    }
    out
}
