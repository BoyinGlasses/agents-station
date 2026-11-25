use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const SLACK_MS: u64 = 5000;

fn valid_sid(sid: &str) -> bool {
    (4..=80).contains(&sid.len())
        && sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Debug, Clone)]
struct UndoFileRec {
    file: PathBuf,
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
    turns: HashMap<String, Vec<UndoTurn>>,
}

impl UndoManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            max_bytes: 5_000_000,
            keep: 3,
            turns: HashMap::new(),
        }
    }

    fn turn_dir(&self, sid: &str, turn_id: &str) -> PathBuf {
        self.dir.join(sid).join(turn_id)
    }

    pub fn prompt(&mut self, sid: &str, text: &str, now_ms: u64) {
        if !valid_sid(sid) {
            return;
        }
        let list = self.turns.entry(sid.to_owned()).or_default();
        let id = format!("{now_ms:x}_{}", list.len());
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
            let _ = std::fs::remove_dir_all(self.dir.join(sid).join(&old.id));
        }
    }

    pub fn before(&mut self, sid: &str, file: &Path) {
        let Some(turn_id) = self
            .turns
            .get(sid)
            .and_then(|l| l.last())
            .filter(|t| t.undone == 0 && !t.files.iter().any(|x| x.file == file))
            .map(|t| (t.id.clone(), t.files.len()))
        else {
            return;
        };
        let meta = std::fs::metadata(file).ok();
        if meta.as_ref().is_some_and(|m| !m.is_file()) {
            return;
        }
        let mut rec = UndoFileRec {
            file: file.to_path_buf(),
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
                    if std::fs::copy(file, &dst).is_ok() {
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
            let file_str = x.file.to_string_lossy().into_owned();
            if x.big {
                files.push(UndoStep { file: file_str, action: "big".into() });
                continue;
            }
            let st = std::fs::metadata(&x.file).ok();
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
            .collect();
        let Some(turn_id) = self.turns.get(sid).and_then(|l| l.last()).map(|t| t.id.clone()) else {
            return UndoApplyResult { done: Vec::new(), after: None };
        };
        let after = self.turn_dir(sid, &turn_id).join("after");
        if !steps.is_empty() {
            let _ = std::fs::create_dir_all(&after);
        }
        let mut done = Vec::new();
        let kept_lookup: HashMap<String, PathBuf> = self
            .turns
            .get(sid)
            .and_then(|l| l.last())
            .map(|t| {
                t.files
                    .iter()
                    .filter_map(|x| Some((x.file.to_string_lossy().into_owned(), x.kept.clone()?)))
                    .collect()
            })
            .unwrap_or_default();

        let mut manifest_files = Vec::new();
        for (i, s) in steps.iter().enumerate() {
            let fname = Path::new(&s.file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let kept_after = format!("{}-{fname}", i + 1);
            let _ = std::fs::copy(&s.file, after.join(&kept_after));
            let ok = if s.action == "delete" {
                std::fs::remove_file(&s.file).is_ok()
            } else if let Some(src) = kept_lookup.get(&s.file) {
                std::fs::read(src)
                    .and_then(|b| std::fs::write(&s.file, b))
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
        u.before("sess-1234", &existing);
        u.before("sess-1234", &created);
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
}
