use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoInfo {
    pub root: String,
    pub name: String,
    pub branch: String,
    pub dirty: usize,
    pub ahead: u32,
    pub head: String,
    pub upstream: String,
    pub ci: Option<String>,
    #[serde(rename = "ciName")]
    pub ci_name: String,
    #[serde(rename = "ciUrl")]
    pub ci_url: String,
    pub born: Option<String>,
}

fn run_git(args: &[&str], cwd: &Path) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub struct GitTracker {
    roots: HashMap<String, String>,
    repos: HashMap<String, RepoInfo>,
}

impl Default for GitTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl GitTracker {
    pub fn new() -> Self {
        Self {
            roots: HashMap::new(),
            repos: HashMap::new(),
        }
    }

    pub fn root_of(&mut self, cwd: &str) -> String {
        if cwd.is_empty() {
            return String::new();
        }
        if let Some(cached) = self.roots.get(cwd) {
            return cached.clone();
        }
        let root = run_git(&["rev-parse", "--show-toplevel"], Path::new(cwd))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        self.roots.insert(cwd.to_owned(), root.clone());
        root
    }

    pub fn poll_cwd(&mut self, cwd: &str) -> Option<RepoInfo> {
        let root = self.root_of(cwd);
        if root.is_empty() {
            return None;
        }
        let rpath = Path::new(&root);
        let status = run_git(
            &["status", "--porcelain=v1", "--branch", "--untracked-files=normal"],
            rpath,
        )?;
        let lines: Vec<&str> = status.lines().filter(|l| !l.is_empty()).collect();
        let branch = lines
            .first()
            .copied()
            .unwrap_or("")
            .trim_start_matches("## ")
            .trim_start_matches("No commits yet on ")
            .split("...")
            .next()
            .unwrap_or("")
            .trim()
            .to_owned();
        let dirty = lines.len().saturating_sub(1);
        let head = run_git(&["rev-parse", "HEAD"], rpath)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        let upstream = run_git(&["rev-parse", "@{u}"], rpath)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        let ahead = run_git(&["rev-list", "--count", "@{u}..HEAD"], rpath)
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let name = rpath
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&root)
            .to_owned();

        let prev = self.repos.get(&root);
        let info = RepoInfo {
            root: root.clone(),
            name,
            branch,
            dirty,
            ahead,
            head,
            upstream,
            ci: prev.and_then(|p| p.ci.clone()),
            ci_name: prev.map(|p| p.ci_name.clone()).unwrap_or_default(),
            ci_url: prev.map(|p| p.ci_url.clone()).unwrap_or_default(),
            born: prev.and_then(|p| p.born.clone()),
        };
        self.repos.insert(root, info.clone());
        Some(info)
    }

    pub fn repo_of(&self, cwd: &str) -> Option<&RepoInfo> {
        let root = self.roots.get(cwd)?;
        self.repos.get(root)
    }

    pub fn all_repos(&self) -> Vec<RepoInfo> {
        self.repos.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_tracker_reads_current_workspace_repo() {
        let mut gt = GitTracker::new();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let info = gt.poll_cwd(root.to_str().unwrap()).expect("workspace is a git repo");
        assert!(!info.branch.is_empty());
        assert!(!info.head.is_empty());
    }
}
