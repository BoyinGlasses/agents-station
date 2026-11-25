use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

const SLACK_MS: u64 = 5000;
const MAX_SESSIONS: usize = 50;

fn valid_sid(sid: &str) -> bool {
    (1..=120).contains(&sid.len())
        && sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b':')
}

fn sid_disk_key(sid: &str) -> String {
    sid.replace(':', "_")
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn canonicalize_maybe_new(path: &Path) -> Option<PathBuf> {
    if let Ok(c) = std::fs::canonicalize(path) {
        return Some(c);
    }
    let mut tail = Vec::new();
    let mut cur = path;
    loop {
        if let Ok(mut resolved) = std::fs::canonicalize(cur) {
            for comp in tail.into_iter().rev() {
                match comp {
                    Component::Normal(seg) => resolved.push(seg),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if !resolved.pop() {
                            return None;
                        }
                    }
                    Component::RootDir | Component::Prefix(_) => return None,
                }
            }
            return Some(resolved);
        }
        let comp = cur.components().next_back()?;
        tail.push(comp);
        cur = cur.parent()?;
    }
}

fn resolve_target(file: &Path, cwd: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
    let cwd_path = cwd.filter(|p| !p.as_os_str().is_empty())?;
    let canon_cwd = std::fs::canonicalize(cwd_path).ok()?;
    let candidate = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd_path.join(file)
    };
    if is_symlink(&candidate) {
        return None;
    }
    let resolved = canonicalize_maybe_new(&candidate)?;
    if !resolved.starts_with(&canon_cwd) || resolved == canon_cwd || is_symlink(&resolved) {
        return None;
    }
    Some((resolved, canon_cwd))
}

fn within_cwd(path: &Path, canon_cwd: &Path) -> bool {
    canonicalize_maybe_new(path).is_some_and(|r| r.starts_with(canon_cwd) && r != canon_cwd)
}

#[derive(Debug, Clone)]
struct UndoFileRec {
    file: PathBuf,
    cwd: PathBuf,
    existed: bool,
    kept: Option<PathBuf>,
    big: bool,
}

#[derive(Debug, Clone)]
struct UndoTurn {
    id: String,
    prompt: String,
    at: u64,
    end: u64,
    files: Vec<UndoFileRec>,
    undone: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoStep {
    pub file: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoPlan {
    pub prompt: String,
    pub at: u64,
    pub end: u64,
    pub undone: u64,
    pub files: Vec<UndoStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoApplyResult {
    pub done: Vec<UndoStep>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSummary {
    pub n: usize,
    pub at: u64,
    pub done: bool,
    pub undone: u64,
}

pub struct UndoManager {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
    max_sessions: usize,
    next_turn_seq: u64,
    turns: HashMap<String, Vec<UndoTurn>>,
    session_order: VecDeque<String>,
}

impl UndoManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            max_bytes: 5_000_000,
            keep: 3,
            max_sessions: MAX_SESSIONS,
            next_turn_seq: 0,
            turns: HashMap::new(),
            session_order: VecDeque::new(),
        }
    }

    fn session_dir(&self, sid: &str) -> PathBuf {
        self.dir.join(sid_disk_key(sid))
    }

    fn turn_dir(&self, sid: &str, turn_id: &str) -> PathBuf {
        self.session_dir(sid).join(turn_id)
    }

    pub fn prompt(&mut self, sid: &str, text: &str, now_ms: u64) {
        if !valid_sid(sid) {
            return;
        }
        self.session_order.retain(|s| s != sid);
        self.session_order.push_back(sid.to_owned());

        let disk_dir = self.session_dir(sid);
        let seq = self.next_turn_seq;
        self.next_turn_seq = self.next_turn_seq.wrapping_add(1);
        let list = self.turns.entry(sid.to_owned()).or_default();
        let id = format!("{now_ms:x}_{seq}");
        list.push(UndoTurn {
            id,
            prompt: text.chars().take(200).collect(),
            at: now_ms,
            end: 0,
            files: Vec::new(),
            undone: 0,
        });
        while list.len() > self.keep {
            let old = list.remove(0);
            let _ = std::fs::remove_dir_all(disk_dir.join(&old.id));
        }

        while self.turns.len() > self.max_sessions {
            let Some(oldest) = self.session_order.pop_front() else {
                break;
            };
            let evicted_turns = self.turns.remove(&oldest);
            let evicted_key = sid_disk_key(&oldest);
            let key_still_used = self.turns.keys().any(|k| sid_disk_key(k) == evicted_key);
            if !key_still_used {
                let _ = std::fs::remove_dir_all(self.session_dir(&oldest));
            } else if let Some(turns) = evicted_turns {
                let old_dir = self.session_dir(&oldest);
                for t in turns {
                    let _ = std::fs::remove_dir_all(old_dir.join(&t.id));
                }
            }
        }
    }

    pub fn before(&mut self, sid: &str, file: &Path, cwd: Option<&Path>) {
        let Some((resolved, canon_cwd)) = resolve_target(file, cwd) else {
            return;
        };
        let Some(turn_id) = self
            .turns
            .get(sid)
            .and_then(|l| l.last())
            .filter(|t| t.undone == 0 && !t.files.iter().any(|x| x.file == resolved))
            .map(|t| (t.id.clone(), t.files.len()))
        else {
            return;
        };
        let meta = match std::fs::symlink_metadata(&resolved) {
            Ok(m) => {
                if m.file_type().is_symlink() || !m.is_file() {
                    return;
                }
                Some(m)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return,
        };
        let mut rec = UndoFileRec {
            file: resolved.clone(),
            cwd: canon_cwd,
            existed: meta.is_some(),
            kept: None,
            big: false,
        };
        if let Some(m) = meta {
            if m.len() > self.max_bytes {
                rec.big = true;
            } else {
                let td = self.turn_dir(sid, &turn_id.0);
                if std::fs::create_dir_all(&td).is_ok() {
                    let dst = td.join((turn_id.1 + 1).to_string());
                    if std::fs::copy(&resolved, &dst).is_ok() {
                        rec.kept = Some(dst);
                    } else {
                        rec.big = true;
                    }
                } else {
                    rec.big = true;
                }
            }
        }
        if let Some(t) = self.turns.get_mut(sid).and_then(|l| l.last_mut()) {
            t.files.push(rec);
        }
    }

    pub fn stop(&mut self, sid: &str, now_ms: u64) {
        if let Some(t) = self.turns.get_mut(sid).and_then(|l| l.last_mut())
            && t.end == 0
        {
            t.end = now_ms;
        }
    }

    pub fn plan(&self, sid: &str) -> Option<UndoPlan> {
        let t = self.turns.get(sid)?.last()?;
        if t.files.is_empty() {
            return None;
        }
        let mut files = Vec::with_capacity(t.files.len());
        for x in &t.files {
            if !within_cwd(&x.file, &x.cwd) {
                continue;
            }
            let st = match std::fs::symlink_metadata(&x.file) {
                Ok(m) if m.file_type().is_symlink() => continue,
                Ok(m) => Some(m),
                Err(_) => None,
            };
            let file_str = x.file.to_string_lossy().into_owned();
            if x.big {
                files.push(UndoStep { file: file_str, action: "big".into() });
                continue;
            }
            let mtime_ms = st
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|mt| mt.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let later = st.is_some() && t.end > 0 && mtime_ms > t.end.saturating_add(SLACK_MS);
            if !x.existed {
                let act = if st.is_none() {
                    "same"
                } else if later {
                    "changedAfter"
                } else {
                    "delete"
                };
                files.push(UndoStep { file: file_str, action: act.into() });
                continue;
            }
            let Some(kept_path) = &x.kept else {
                files.push(UndoStep { file: file_str, action: "big".into() });
                continue;
            };
            let Ok(was) = std::fs::read(kept_path) else {
                files.push(UndoStep { file: file_str, action: "big".into() });
                continue;
            };
            let cur = std::fs::read(&x.file).ok();
            let act = if cur.as_deref() == Some(was.as_slice()) {
                "same"
            } else if later {
                "changedAfter"
            } else {
                "restore"
            };
            files.push(UndoStep { file: file_str, action: act.into() });
        }
        Some(UndoPlan {
            prompt: t.prompt.clone(),
            at: t.at,
            end: t.end,
            undone: t.undone,
            files,
        })
    }

    pub fn apply(&mut self, sid: &str, now_ms: u64) -> UndoApplyResult {
        self.apply_filtered(sid, now_ms, None)
    }

    pub fn apply_filtered(
        &mut self,
        sid: &str,
        now_ms: u64,
        only_files: Option<&[String]>,
    ) -> UndoApplyResult {
        let Some(p) = self.plan(sid) else {
            return UndoApplyResult { done: Vec::new(), after: None };
        };
        if p.undone > 0 {
            return UndoApplyResult { done: Vec::new(), after: None };
        }
        let steps: Vec<UndoStep> = p
            .files
            .into_iter()
            .filter(|x| x.action == "restore" || x.action == "delete")
            .filter(|x| match only_files {
                Some(filter) if !filter.is_empty() => filter.iter().any(|f| {
                    if cfg!(windows) {
                        f.eq_ignore_ascii_case(&x.file)
                            || Path::new(&x.file)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.eq_ignore_ascii_case(f))
                    } else {
                        f == &x.file
                            || Path::new(&x.file)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n == f)
                    }
                }),
                _ => true,
            })
            .collect();
        let Some(turn_id) = self.turns.get(sid).and_then(|l| l.last()).map(|t| t.id.clone()) else {
            return UndoApplyResult { done: Vec::new(), after: None };
        };
        let after = self.turn_dir(sid, &turn_id).join("after");
        if !steps.is_empty() {
            let _ = std::fs::create_dir_all(&after);
        }
        let mut done = Vec::new();
        let rec_lookup: HashMap<String, (Option<PathBuf>, PathBuf)> = self
            .turns
            .get(sid)
            .and_then(|l| l.last())
            .map(|t| {
                t.files
                    .iter()
                    .map(|x| {
                        (
                            x.file.to_string_lossy().into_owned(),
                            (x.kept.clone(), x.cwd.clone()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut manifest_files = Vec::new();
        for (i, s) in steps.iter().enumerate() {
            let target = Path::new(&s.file);
            let Some((kept_src, canon_cwd)) = rec_lookup.get(&s.file) else {
                continue;
            };
            if is_symlink(target) || !within_cwd(target, canon_cwd) {
                continue;
            }
            let fname = target
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let kept_after = format!("{}-{fname}", i + 1);
            let _ = std::fs::copy(target, after.join(&kept_after));
            let ok = if s.action == "delete" {
                std::fs::remove_file(target).is_ok()
            } else if let Some(src) = kept_src {
                std::fs::read(src)
                    .and_then(|b| std::fs::write(target, b))
                    .is_ok()
            } else {
                false
            };
            if ok {
                done.push(s.clone());
            }
            manifest_files.push(json!({ "file": s.file, "was": s.action, "kept": kept_after }));
        }
        if !steps.is_empty() {
            let manifest = json!({ "prompt": p.prompt, "at": p.at, "files": manifest_files });
            let _ = std::fs::write(
                after.join("manifest.json"),
                serde_json::to_string_pretty(&manifest).unwrap_or_default(),
            );
        }
        if let Some(t) = self.turns.get_mut(sid).and_then(|l| l.last_mut()) {
            t.undone = now_ms;
        }
        UndoApplyResult {
            done,
            after: if !steps.is_empty() {
                Some(after.to_string_lossy().into_owned())
            } else {
                None
            },
        }
    }

    pub fn info(&self, sid: &str) -> Option<UndoSummary> {
        let t = self.turns.get(sid)?.last()?;
        if t.files.is_empty() {
            return None;
        }
        Some(UndoSummary {
            n: t.files.len(),
            at: t.at,
            done: t.end > 0,
            undone: t.undone,
        })
    }

    pub fn prune_stale_disk(&mut self) -> usize {
        while self.turns.len() > self.max_sessions {
            let Some(oldest) = self.session_order.pop_front() else {
                break;
            };
            let evicted_turns = self.turns.remove(&oldest);
            let evicted_key = sid_disk_key(&oldest);
            let key_still_used = self.turns.keys().any(|k| sid_disk_key(k) == evicted_key);
            if !key_still_used {
                let _ = std::fs::remove_dir_all(self.session_dir(&oldest));
            } else if let Some(turns) = evicted_turns {
                let old_dir = self.session_dir(&oldest);
                for t in turns {
                    let _ = std::fs::remove_dir_all(old_dir.join(&t.id));
                }
            }
        }

        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return 0;
        };
        let active_keys: std::collections::HashSet<String> =
            self.turns.keys().map(|k| sid_disk_key(k)).collect();
        let mut dirs: Vec<(bool, u64, String, PathBuf)> = rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let is_active = active_keys.contains(&name);
                let mtime_ms = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                (is_active, mtime_ms, name, e.path())
            })
            .collect();

        if dirs.len() <= self.max_sessions {
            return 0;
        }
        dirs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let excess = dirs.len() - self.max_sessions;
        let mut removed = 0usize;
        for (is_active, _, _, path) in dirs.into_iter().take(excess) {
            if is_active {
                continue;
            }
            if std::fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_restores_edited_file_and_deletes_created_file() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("a.txt");
        let created = dir.path().join("b.txt");
        std::fs::write(&existing, "before").unwrap();

        let mut u = UndoManager::new(dir.path().join("undo"));
        u.prompt("sess-1234", "edit files", 1000);
        u.before("sess-1234", &existing, Some(dir.path()));
        u.before("sess-1234", &created, Some(dir.path()));
        std::fs::write(&existing, "after").unwrap();
        std::fs::write(&created, "new file").unwrap();

        let plan = u.plan("sess-1234").unwrap();
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].action, "restore");
        assert_eq!(plan.files[1].action, "delete");

        let res = u.apply("sess-1234", 2000);
        assert_eq!(res.done.len(), 2);
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "before");
        assert!(!created.exists());
    }

    fn create_file_symlink(original: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(original, link)
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(original, link)
        }
    }

    #[test]
    fn undo_supports_multi_agent_colon_sids_with_sanitized_disk_dir() {
        let dir = tempfile::tempdir().unwrap();
        let undo_root = dir.path().join("undo");
        let mut u = UndoManager::new(undo_root.clone());

        for (sid, expected_dir) in [
            ("codex:sess-1", "codex_sess-1"),
            ("antigravity:c1", "antigravity_c1"),
        ] {
            let f = dir.path().join(format!("{expected_dir}.txt"));
            std::fs::write(&f, "before").unwrap();
            u.prompt(sid, "edit file", 1000);
            u.before(sid, &f, Some(dir.path()));
            std::fs::write(&f, "after").unwrap();

            assert!(
                undo_root.join(expected_dir).exists(),
                "expected sanitized disk dir for {sid}"
            );
            let plan = u.plan(sid).expect("plan must exist for colon sid");
            assert_eq!(plan.files.len(), 1);
            assert_eq!(plan.files[0].action, "restore");

            let res = u.apply(sid, 2000);
            assert_eq!(res.done.len(), 1);
            assert_eq!(std::fs::read_to_string(&f).unwrap(), "before");
        }
    }

    #[test]
    fn undo_skips_symlink_on_backup_and_revert() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        std::fs::write(&real, "original").unwrap();
        create_file_symlink(&real, &link).expect("create symlink");

        let mut u = UndoManager::new(dir.path().join("undo"));
        u.prompt("sess-sym1", "touch symlink", 1000);
        u.before("sess-sym1", &link, Some(dir.path()));
        std::fs::write(&real, "modified").unwrap();
        assert!(u.plan("sess-sym1").is_none(), "symlink must be skipped on backup");

        // TOCTOU: regular file swapped to symlink before apply/revert
        let swapped = dir.path().join("swapped.txt");
        let victim = dir.path().join("victim.txt");
        std::fs::write(&swapped, "before").unwrap();
        std::fs::write(&victim, "keep-me").unwrap();
        u.prompt("sess-sym2", "edit regular then swap", 1100);
        u.before("sess-sym2", &swapped, Some(dir.path()));
        std::fs::remove_file(&swapped).unwrap();
        create_file_symlink(&victim, &swapped).expect("create TOCTOU symlink");

        let res = u.apply("sess-sym2", 2000);
        assert!(res.done.is_empty(), "swapped symlink must never be overwritten");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep-me");
    }

    #[test]
    fn undo_rejects_path_escaping_outside_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "outside-before").unwrap();

        let mut u = UndoManager::new(dir.path().join("undo"));
        u.prompt("sess-esc1", "escape cwd", 1000);
        u.before("sess-esc1", Path::new("../outside.txt"), Some(&workspace));
        u.before("sess-esc1", &workspace.join("../outside.txt"), Some(&workspace));
        u.before("sess-esc1", &outside, Some(&workspace));
        std::fs::write(&outside, "outside-after").unwrap();

        assert!(u.plan("sess-esc1").is_none(), "path escaping cwd must be rejected");
        let res = u.apply("sess-esc1", 2000);
        assert!(res.done.is_empty());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside-after");
    }

    #[test]
    fn undo_evicts_oldest_session_folder_past_50_cap() {
        let dir = tempfile::tempdir().unwrap();
        let undo_root = dir.path().join("undo");
        let target = dir.path().join("file.txt");
        std::fs::write(&target, "v0").unwrap();

        let mut u = UndoManager::new(undo_root.clone());
        for i in 0..51usize {
            let sid = format!("codex:s{i:03}");
            u.prompt(&sid, "edit", 1000 + i as u64);
            u.before(&sid, &target, Some(dir.path()));
        }

        assert!(
            !undo_root.join("codex_s000").exists(),
            "oldest session disk folder must be evicted on 51st session"
        );
        assert!(u.plan("codex:s000").is_none(), "oldest session must be evicted from memory");
        assert!(undo_root.join("codex_s050").exists(), "newest session disk folder must exist");

        let dirs = std::fs::read_dir(&undo_root)
            .unwrap()
            .filter_map(Result::ok)
            .count();
        assert!(dirs <= 50, "expected <= 50 stored session folders, found {dirs}");
    }

    #[test]
    fn undo_rejects_missing_or_empty_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("unscoped.txt");
        std::fs::write(&target, "before").unwrap();

        let mut u = UndoManager::new(dir.path().join("undo"));
        u.prompt("codex:no-cwd", "edit without cwd", 1000);
        u.before("codex:no-cwd", &target, None);
        u.before("codex:no-cwd", &target, Some(Path::new("")));
        std::fs::write(&target, "after").unwrap();

        assert!(u.plan("codex:no-cwd").is_none(), "missing/empty cwd must be rejected");
        let res = u.apply("codex:no-cwd", 2000);
        assert!(res.done.is_empty());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "after");
    }

    #[test]
    fn undo_eviction_preserves_disk_backups_for_colliding_sid_disk_key() {
        let dir = tempfile::tempdir().unwrap();
        let undo_root = dir.path().join("undo");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "orig-a").unwrap();
        std::fs::write(&file_b, "orig-b").unwrap();

        let mut u = UndoManager::new(undo_root.clone());
        // Oldest session uses colon; second session collides on sid_disk_key ("codex_a").
        u.prompt("codex:a", "edit a", 1000);
        u.before("codex:a", &file_a, Some(dir.path()));

        u.prompt("codex_a", "edit b", 1000);
        u.before("codex_a", &file_b, Some(dir.path()));
        std::fs::write(&file_b, "changed-b").unwrap();

        // Add 49 more distinct sessions so total reaches 51 and "codex:a" is evicted.
        for i in 0..49usize {
            let sid = format!("other:{i:02}");
            u.prompt(&sid, "edit", 2000 + i as u64);
            u.before(&sid, &file_a, Some(dir.path()));
        }

        assert!(u.plan("codex:a").is_none(), "oldest session must be evicted");
        assert!(undo_root.join("codex_a").exists(), "shared disk dir must survive for active codex_a");

        let plan = u.plan("codex_a").expect("active colliding session must keep valid plan");
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].action, "restore");

        let res = u.apply("codex_a", 9000);
        assert_eq!(res.done.len(), 1);
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "orig-b");
    }

    #[test]
    fn undo_prune_stale_disk_removes_orphaned_folders_beyond_50_cap() {
        let dir = tempfile::tempdir().unwrap();
        let undo_root = dir.path().join("undo");
        std::fs::create_dir_all(&undo_root).unwrap();

        for i in 0..55usize {
            let orphan = undo_root.join(format!("orphan_{i:03}"));
            std::fs::create_dir_all(&orphan).unwrap();
            std::fs::write(orphan.join("marker"), "stale").unwrap();
        }

        let target = dir.path().join("live.txt");
        std::fs::write(&target, "orig").unwrap();
        let mut u = UndoManager::new(undo_root.clone());
        u.prompt("codex:live", "edit live", 5000);
        u.before("codex:live", &target, Some(dir.path()));

        let removed = u.prune_stale_disk();
        assert!(removed >= 6, "expected >= 6 orphaned dirs removed, got {removed}");
        assert!(undo_root.join("codex_live").exists(), "active session dir must survive");

        let remaining = std::fs::read_dir(&undo_root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .count();
        assert!(remaining <= 50, "expected <= 50 undo dirs after prune, got {remaining}");
    }
}
