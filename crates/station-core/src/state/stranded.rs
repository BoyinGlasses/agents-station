use crate::state::git::CREATE_NO_WINDOW;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_REPOS: usize = 30;
const MAX_PER_REPO: usize = 40;
const MAX_ITEMS: usize = 60;
const CACHE_TTL_MS: u64 = 45_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub path: String,
    pub head: String,
    pub branch: String,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    #[serde(rename = "lockReason")]
    pub lock_reason: String,
    pub prunable: bool,
    #[serde(rename = "prunableReason")]
    pub prunable_reason: String,
    pub main: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatusParsed {
    pub branch: String,
    pub detached: bool,
    pub upstream: String,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrandedItem {
    pub id: String,
    pub path: String,
    pub repo: String,
    pub branch: String,
    pub main: bool,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
    pub dirty: usize,
    pub ahead: u32,
    pub stashes: usize,
    pub ok: bool,
    #[serde(rename = "inUse")]
    pub in_use: bool,
    pub removable: bool,
    pub work: bool,
}

pub fn id_of(path: &str) -> String {
    let h = Sha256::digest(path.as_bytes());
    h.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

pub fn parse_worktrees(out: &str) -> Vec<WorktreeEntry> {
    let z = out.contains('\0');
    let fields: Vec<&str> = if z {
        out.split('\0').collect()
    } else {
        out.lines().collect()
    };
    let mut list: Vec<WorktreeEntry> = Vec::new();
    let mut cur: Option<WorktreeEntry> = None;
    for f in fields {
        if f.is_empty() {
            if let Some(c) = cur.take() {
                list.push(c);
            }
            continue;
        }
        let (key, val) = match f.split_once(' ') {
            Some((k, v)) => (k, v),
            None => (f, ""),
        };
        if key == "worktree" {
            if let Some(c) = cur.take() {
                list.push(c);
            }
            cur = Some(WorktreeEntry {
                path: val.to_owned(),
                main: list.is_empty(),
                ..Default::default()
            });
            continue;
        }
        if let Some(c) = &mut cur {
            match key {
                "HEAD" => c.head = val.to_owned(),
                "branch" => c.branch = val.strip_prefix("refs/heads/").unwrap_or(val).to_owned(),
                "bare" => c.bare = true,
                "detached" => c.detached = true,
                "locked" => {
                    c.locked = true;
                    c.lock_reason = val.trim_matches('"').to_owned();
                }
                "prunable" => {
                    c.prunable = true;
                    c.prunable_reason = val.trim_matches('"').to_owned();
                }
                _ => {}
            }
        }
    }
    if let Some(c) = cur {
        list.push(c);
    }
    list
}

pub fn parse_status(out: &str) -> GitStatusParsed {
    let parts: Vec<&str> = out.split('\0').collect();
    let mut res = GitStatusParsed::default();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i];
        i += 1;
        if p.is_empty() {
            continue;
        }
        if let Some(hdr) = p.strip_prefix("## ") {
            let trimmed = hdr
                .strip_prefix("No commits yet on ")
                .or_else(|| hdr.strip_prefix("Initial commit on "))
                .unwrap_or(hdr);
            let (before_bracket, bracket) = match trimmed.split_once(" [") {
                Some((a, b)) => (a, b.trim_end_matches(']')),
                None => (trimmed, ""),
            };
            let (br, up) = match before_bracket.split_once("...") {
                Some((a, b)) => (a, b),
                None => (before_bracket, ""),
            };
            if br == "HEAD (no branch)" {
                res.detached = true;
            } else {
                res.branch = br.to_owned();
            }
            let mut gone = false;
            for bit in bracket.split(',').map(str::trim) {
                if bit == "gone" {
                    gone = true;
                } else if let Some(n) = bit.strip_prefix("ahead ") {
                    res.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = bit.strip_prefix("behind ") {
                    res.behind = n.parse().unwrap_or(0);
                }
            }
            res.gone = gone;
            if !gone {
                res.upstream = up.to_owned();
            } else {
                res.ahead = 0;
            }
            continue;
        }
        if p.len() >= 3 {
            res.files.push(p[3..].to_owned());
            if p.starts_with('R') || p.starts_with('C') {
                i += 1;
            }
        }
    }
    res
}

pub fn can_remove(w: &StrandedItem) -> bool {
    w.ok && !w.main && !w.bare && !w.locked && !w.prunable && !w.in_use && w.dirty == 0 && w.ahead == 0
}

pub fn holds_work(w: &StrandedItem) -> bool {
    w.dirty > 0 || w.ahead > 0 || (w.main && w.stashes > 0) || !w.ok
}

fn run_git_cmd(args: &[&str], cwd: &Path) -> Result<String, String> {
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
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

fn norm_path(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let na = norm_path(a);
    let nb = norm_path(b);
    if cfg!(windows) {
        na.to_string_lossy()
            .eq_ignore_ascii_case(&nb.to_string_lossy())
    } else {
        na == nb
    }
}

fn path_in_use(target: &Path, in_use_dirs: &[String]) -> bool {
    let nt = norm_path(target);
    in_use_dirs.iter().any(|d| {
        if d.is_empty() {
            return false;
        }
        let nd = norm_path(Path::new(d));
        if cfg!(windows) {
            let t_low = nt.to_string_lossy().to_ascii_lowercase();
            let d_low = nd.to_string_lossy().to_ascii_lowercase();
            d_low == t_low || d_low.starts_with(&format!("{t_low}\\"))
        } else {
            nd == nt || nd.starts_with(&nt)
        }
    })
}

#[derive(Debug, Clone)]
pub struct StrandedEntry {
    pub item: StrandedItem,
    pub repo_root: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrandedSnapshot {
    pub ok: bool,
    pub at: u64,
    pub repos: usize,
    pub items: Vec<StrandedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrandedRemoveResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

pub struct StrandedScanner {
    last: StrandedSnapshot,
    by_id: HashMap<String, StrandedEntry>,
}

impl Default for StrandedScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl StrandedScanner {
    pub fn new() -> Self {
        Self {
            last: StrandedSnapshot {
                ok: true,
                at: 0,
                repos: 0,
                items: Vec::new(),
            },
            by_id: HashMap::new(),
        }
    }

    fn read_worktree(
        w: &WorktreeEntry,
        repo_name: &str,
        remotes: usize,
        main_head: &str,
        stashes: usize,
        in_use_dirs: &[String],
    ) -> StrandedItem {
        let wp = Path::new(&w.path);
        let id = id_of(&w.path);
        let mut item = StrandedItem {
            id,
            path: w.path.clone(),
            repo: repo_name.to_owned(),
            branch: w.branch.clone(),
            main: w.main,
            bare: w.bare,
            locked: w.locked,
            prunable: w.prunable,
            dirty: 0,
            ahead: 0,
            stashes: if w.main { stashes } else { 0 },
            ok: false,
            in_use: path_in_use(wp, in_use_dirs),
            removable: false,
            work: false,
        };
        let Ok(st_out) = run_git_cmd(
            &["status", "--porcelain=v1", "-z", "--branch", "--untracked-files=normal"],
            wp,
        ) else {
            item.work = holds_work(&item);
            return item;
        };
        let st = parse_status(&st_out);
        item.dirty = st.files.len();
        if item.branch.is_empty() && !st.branch.is_empty() {
            item.branch = st.branch.clone();
        }
        if !st.upstream.is_empty() {
            item.ahead = st.ahead;
        } else if remotes > 0 {
            let Ok(cnt) = run_git_cmd(&["rev-list", "--count", "HEAD", "--not", "--remotes"], wp)
            else {
                item.work = holds_work(&item);
                return item;
            };
            item.ahead = cnt.trim().parse::<u32>().unwrap_or(0);
        } else if !w.main && !main_head.is_empty() {
            let Ok(cnt) = run_git_cmd(&["rev-list", "--count", "HEAD", "--not", main_head], wp)
            else {
                item.work = holds_work(&item);
                return item;
            };
            item.ahead = cnt.trim().parse::<u32>().unwrap_or(0);
        }
        item.ok = true;
        item.removable = can_remove(&item);
        item.work = holds_work(&item);
        item
    }

    pub fn scan(
        &mut self,
        dirs: &[String],
        in_use_dirs: &[String],
        now_ms: u64,
        force: bool,
    ) -> StrandedSnapshot {
        if !force && self.last.at > 0 && now_ms.saturating_sub(self.last.at) < CACHE_TTL_MS {
            return self.last.clone();
        }

        let mut repos: HashMap<PathBuf, PathBuf> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        for d in dirs.iter().take(80) {
            if repos.len() >= MAX_REPOS || !seen.insert(d.clone()) {
                continue;
            }
            let p = Path::new(d);
            if !p.is_absolute() || !p.is_dir() {
                continue;
            }
            if let Ok(common_raw) = run_git_cmd(&["rev-parse", "--git-common-dir"], p) {
                let common = norm_path(&p.join(common_raw.trim()));
                repos.entry(common).or_insert_with(|| p.to_path_buf());
            }
        }

        // Also inspect <repo>/.claude/worktrees/*
        let base_commons: Vec<PathBuf> = repos.keys().cloned().collect();
        for common in base_commons {
            if let Some(top) = common.parent() {
                let wt_dir = top.join(".claude").join("worktrees");
                if let Ok(rd) = std::fs::read_dir(&wt_dir) {
                    for e in rd.flatten().take(30) {
                        if repos.len() >= MAX_REPOS {
                            break;
                        }
                        let sub = e.path();
                        if sub.is_dir() && sub.join(".git").exists() {
                            if let Ok(c_raw) = run_git_cmd(&["rev-parse", "--git-common-dir"], &sub) {
                                let c = norm_path(&sub.join(c_raw.trim()));
                                repos.entry(c).or_insert(sub);
                            }
                        }
                    }
                }
            }
        }

        let mut next_by_id: HashMap<String, StrandedEntry> = HashMap::new();
        let mut items: Vec<StrandedItem> = Vec::new();
        for (common, from) in &repos {
            let wl = run_git_cmd(&["worktree", "list", "--porcelain", "-z"], from)
                .or_else(|_| run_git_cmd(&["worktree", "list", "--porcelain"], from));
            let Ok(wl_out) = wl else {
                continue;
            };
            let all = parse_worktrees(&wl_out);
            let Some(main_wt) = all.first() else {
                continue;
            };
            let root_path = if main_wt.bare {
                common.clone()
            } else {
                norm_path(Path::new(&main_wt.path))
            };
            let repo_name = root_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .trim_end_matches(".git")
                .to_owned();
            let remotes = run_git_cmd(&["remote"], &root_path)
                .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0);
            let stashes = if main_wt.bare {
                0
            } else {
                run_git_cmd(&["stash", "list", "--format=%gd"], &root_path)
                    .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
                    .unwrap_or(0)
            };
            let main_head = if main_wt.bare { "" } else { main_wt.head.as_str() };

            for w in all.iter().take(MAX_PER_REPO) {
                if w.bare || w.prunable {
                    continue;
                }
                let wp = norm_path(Path::new(&w.path));
                if !wp.exists() {
                    continue;
                }
                let mut w_norm = w.clone();
                w_norm.path = wp.to_string_lossy().into_owned();
                let item = Self::read_worktree(
                    &w_norm,
                    &repo_name,
                    remotes,
                    main_head,
                    stashes,
                    in_use_dirs,
                );
                next_by_id.insert(
                    item.id.clone(),
                    StrandedEntry {
                        item: item.clone(),
                        repo_root: root_path.clone(),
                    },
                );
                items.push(item);
            }
        }

        self.by_id = next_by_id;
        let mut shown: Vec<StrandedItem> = items
            .into_iter()
            .filter(|it| it.work || it.removable)
            .collect();
        shown.sort_by(|a, b| b.work.cmp(&a.work).then_with(|| a.path.cmp(&b.path)));
        shown.truncate(MAX_ITEMS);
        self.last = StrandedSnapshot {
            ok: true,
            at: now_ms,
            repos: repos.len(),
            items: shown,
        };
        self.last.clone()
    }

    pub fn entry(&self, id: &str) -> Option<StrandedEntry> {
        self.by_id.get(id).cloned()
    }

    pub fn dir_of(&self, dir: &str) -> Option<String> {
        let want = norm_path(Path::new(dir));
        self.last
            .items
            .iter()
            .find(|it| same_path(Path::new(&it.path), &want))
            .map(|it| it.path.clone())
    }

    pub fn remove(&mut self, id: &str, in_use_dirs: &[String]) -> StrandedRemoveResult {
        let Some(was) = self.by_id.get(id).cloned() else {
            return StrandedRemoveResult {
                ok: false,
                code: Some("gone"),
                why: None,
                path: None,
                branch: None,
            };
        };
        let busy = path_in_use(Path::new(&was.item.path), in_use_dirs);
        let mut check_was = was.item.clone();
        check_was.in_use = busy;
        if !can_remove(&check_was) {
            return StrandedRemoveResult {
                ok: false,
                code: Some(if busy { "inUse" } else { "notClean" }),
                why: None,
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            };
        }

        // Re-verify live worktree state before removing (no --force!)
        let wl = run_git_cmd(&["worktree", "list", "--porcelain", "-z"], &was.repo_root)
            .or_else(|_| run_git_cmd(&["worktree", "list", "--porcelain"], &was.repo_root));
        let Ok(wl_out) = wl else {
            return StrandedRemoveResult {
                ok: false,
                code: Some("gone"),
                why: None,
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            };
        };
        let all = parse_worktrees(&wl_out);
        let Some(fresh_wt) = all
            .iter()
            .find(|w| same_path(Path::new(&w.path), Path::new(&was.item.path)))
        else {
            return StrandedRemoveResult {
                ok: false,
                code: Some("gone"),
                why: None,
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            };
        };
        if fresh_wt.main {
            return StrandedRemoveResult {
                ok: false,
                code: Some("gone"),
                why: None,
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            };
        }
        let remotes = run_git_cmd(&["remote"], &was.repo_root)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        let main_head = all
            .first()
            .filter(|m| !m.bare)
            .map(|m| m.head.as_str())
            .unwrap_or("");
        let mut fresh_norm = fresh_wt.clone();
        fresh_norm.path = norm_path(Path::new(&fresh_wt.path))
            .to_string_lossy()
            .into_owned();
        let fresh_item = Self::read_worktree(
            &fresh_norm,
            &was.item.repo,
            remotes,
            main_head,
            0,
            in_use_dirs,
        );
        if !can_remove(&fresh_item) {
            return StrandedRemoveResult {
                ok: false,
                code: Some(if fresh_item.in_use { "inUse" } else { "notClean" }),
                why: None,
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            };
        }

        // Never pass --force: git itself refuses dirty/locked worktrees as a second safety barrier.
        let wt_arg = strip_verbatim_prefix(Path::new(&fresh_norm.path));
        let wt_arg_str = wt_arg.to_string_lossy();
        match run_git_cmd(&["worktree", "remove", "--", &wt_arg_str], &was.repo_root) {
            Ok(_) => {
                self.by_id.remove(id);
                self.last.items.retain(|it| it.id != id);
                self.last.at = 0;
                StrandedRemoveResult {
                    ok: true,
                    code: None,
                    why: None,
                    path: Some(was.item.path),
                    branch: Some(was.item.branch),
                }
            }
            Err(err) => StrandedRemoveResult {
                ok: false,
                code: Some("failed"),
                why: Some(err.chars().take(200).collect()),
                path: Some(was.item.path),
                branch: Some(was.item.branch),
            },
        }
    }
}

pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

pub fn open_folder(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err("folder not found".to_owned());
    }
    let clean = strip_verbatim_prefix(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(&clean)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("--")
            .arg(&clean)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(&clean)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktrees_status_and_remove_safety() {
        let wt = parse_worktrees("worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo-wt\nHEAD def\ndetached\nlocked reason\n");
        assert_eq!(wt.len(), 2);
        assert!(wt[0].main);
        assert_eq!(wt[0].branch, "main");
        assert!(!wt[1].main && wt[1].detached && wt[1].locked);

        let st = parse_status("## main...origin/main [ahead 2, behind 1]\0 M a.rs\0?? b.rs\0");
        assert_eq!(st.branch, "main");
        assert_eq!(st.upstream, "origin/main");
        assert_eq!(st.ahead, 2);
        assert_eq!(st.behind, 1);
        assert_eq!(st.files, vec!["a.rs", "b.rs"]);

        let clean_wt = StrandedItem { ok: true, main: false, ..Default::default() };
        assert!(can_remove(&clean_wt));
        let main_wt = StrandedItem { ok: true, main: true, ..Default::default() };
        assert!(!can_remove(&main_wt));
    }

    #[test]
    fn strip_verbatim_prefix_handles_drive_and_unc_paths() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\D:\mystation\repo")),
            PathBuf::from(r"D:\mystation\repo")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\proj")),
            PathBuf::from(r"\\server\share\proj")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"D:\plain\path")),
            PathBuf::from(r"D:\plain\path")
        );
    }
}
