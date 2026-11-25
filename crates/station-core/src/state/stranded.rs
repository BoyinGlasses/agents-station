use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
}
