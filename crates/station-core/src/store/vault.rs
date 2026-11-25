use crate::atomic::{write_bytes_atomic, write_json_atomic};
use crate::redact::redact;
use crate::state::sessions::now_ms;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use unicode_normalization::UnicodeNormalization;

const MAX_TEXT: usize = 20_000;
const MAX_MSGS: usize = 4_000;
const MAX_RAW_JSONL_BYTES: u64 = 64 * 1024 * 1024;
const ARCHIVE_VERSION: u32 = 2;

const WRAP_TAGS: &[&str] = &[
    "system-reminder",
    "ide_opened_file",
    "ide_selection",
    "command-name",
    "command-message",
    "command-args",
    "local-command-stdout",
    "local-command-stderr",
    "local-command-caveat",
];

fn is_zero(n: &usize) -> bool {
    *n == 0
}

pub fn is_valid_sid(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 80
        && sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Lowercase, accent-insensitive Vietnamese/Latin folding for search:
/// `"Tối ưu RAM"` -> `"toi uu ram"`, `"đ/Đ"` -> `"d"`.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.nfd() {
        match ch {
            '\u{0300}'..='\u{036f}' => {}
            'đ' | 'Đ' => out.push('d'),
            other => {
                for lower in other.to_lowercase() {
                    out.push(lower);
                }
            }
        }
    }
    out
}

/// Strips `<system-reminder>` and editor/command wrapper tags plus `<pasted_content>` markers.
pub fn clean_prompt(text: &str) -> String {
    static WRAP_RES: OnceLock<Vec<Regex>> = OnceLock::new();
    static PASTED_RE: OnceLock<Regex> = OnceLock::new();

    let wrap_res = WRAP_RES.get_or_init(|| {
        WRAP_TAGS
            .iter()
            .map(|tag| Regex::new(&format!(r"(?s)<{tag}>.*?</{tag}>")).unwrap())
            .collect()
    });
    let pasted_re = PASTED_RE.get_or_init(|| Regex::new(r"</?pasted_content[^>]*>").unwrap());

    let mut cur = text.to_owned();
    for re in wrap_res {
        cur = re.replace_all(&cur, "").into_owned();
    }
    pasted_re.replace_all(&cur, "").trim().to_owned()
}

fn clip_chars(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn parse_iso_ms(raw: Option<&str>) -> u64 {
    let Some(s) = raw else {
        return 0;
    };
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis().max(0) as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMessage {
    pub r: String,
    pub t: String,
    pub at: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ph: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedTranscript {
    pub v: u32,
    pub sid: String,
    pub title: String,
    pub cwd: String,
    pub start: u64,
    pub end: u64,
    pub msgs: Vec<VaultMessage>,
    pub gone: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPrompt {
    pub t: String,
    pub ph: usize,
    pub at: u64,
    pub sid: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub sid: String,
    pub agent: String,
    pub title: String,
    pub cwd: String,
    pub updated_at: u64,
    pub prompts: Vec<VaultPrompt>,
    #[serde(default)]
    pub start: u64,
    #[serde(default)]
    pub end: u64,
    #[serde(default)]
    pub n: usize,
    #[serde(default)]
    pub first: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mtime: u64,
    #[serde(default)]
    pub gone: u64,
    #[serde(default)]
    pub bytes: usize,
    #[serde(default)]
    pub scanned: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VaultFile {
    entries: HashMap<String, VaultEntry>,
}

struct ParsedTranscript {
    title: String,
    cwd: String,
    start: u64,
    end: u64,
    msgs: Vec<VaultMessage>,
}

fn parse_transcript_jsonl(text: &str) -> ParsedTranscript {
    let mut out = ParsedTranscript {
        title: String::new(),
        cwd: String::new(),
        start: 0,
        end: 0,
        msgs: Vec::new(),
    };
    let mut custom_title: Option<String> = None;
    let mut ai_title: Option<String> = None;

    for line in text.lines() {
        if !line.contains("\"type\":\"user\"")
            && !line.contains("\"type\": \"user\"")
            && !line.contains("\"type\":\"assistant\"")
            && !line.contains("\"type\": \"assistant\"")
            && !line.contains("-title\"")
        {
            continue;
        }
        let Ok(j) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = j.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "custom-title" {
            if let Some(t) = j.get("customTitle").and_then(Value::as_str) {
                custom_title = Some(t.to_owned());
            }
            continue;
        }
        if kind == "ai-title" {
            if let Some(t) = j.get("aiTitle").and_then(Value::as_str) {
                ai_title = Some(t.to_owned());
            }
            continue;
        }
        if j.get("isSidechain").and_then(Value::as_bool) == Some(true)
            || j.get("isMeta").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        if out.cwd.is_empty()
            && let Some(cwd) = j.get("cwd").and_then(Value::as_str)
        {
            out.cwd = cwd.to_owned();
        }
        let at = parse_iso_ms(j.get("timestamp").and_then(Value::as_str));
        let content = j.get("message").and_then(|m| m.get("content"));

        let (role, raw_text, ph) = match kind {
            "user" => {
                let raw = match content {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Array(arr)) => arr
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                let cleaned = clean_prompt(&raw);
                let text = if cleaned.starts_with("[Request interrupted") {
                    String::new()
                } else {
                    cleaned
                };
                let photos = content
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter(|b| b.get("type").and_then(Value::as_str) == Some("image"))
                            .count()
                    })
                    .unwrap_or(0);
                ("u", text, photos)
            }
            "assistant" => {
                let text = match content {
                    Some(Value::String(s)) => s.trim().to_owned(),
                    Some(Value::Array(arr)) => arr
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim()
                        .to_owned(),
                    _ => String::new(),
                };
                ("c", text, 0)
            }
            _ => continue,
        };

        if raw_text.is_empty() {
            continue;
        }
        let safe_text = clip_chars(&redact(&raw_text), MAX_TEXT);
        if at > 0 {
            if out.start == 0 {
                out.start = at;
            }
            out.end = out.end.max(at);
        }
        if out.msgs.len() < MAX_MSGS {
            out.msgs.push(VaultMessage {
                r: role.to_owned(),
                t: safe_text,
                at,
                ph,
            });
        }
    }

    let raw_title = custom_title.or(ai_title).unwrap_or_default();
    out.title = redact(raw_title.trim());
    out
}

fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            collect_jsonl_files(&path, out, depth + 1);
        } else if ft.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("jsonl")
        {
            out.push(path);
        }
    }
}

pub struct VaultStore {
    file: PathBuf,
    archive_dir: PathBuf,
    data: VaultFile,
    scanned_at: u64,
}

impl VaultStore {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("vault.json");
        let archive_dir = data_dir.join("vault").join("c");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<VaultFile>(&s).ok())
            .unwrap_or_default();
        Self {
            file,
            archive_dir,
            data,
            scanned_at: 0,
        }
    }

    pub fn scanned_at(&self) -> u64 {
        self.scanned_at
    }

    pub fn archive_path(&self, sid: &str) -> Option<PathBuf> {
        is_valid_sid(sid).then(|| self.archive_dir.join(format!("{sid}.jsonl.gz")))
    }

    pub fn record_prompt(
        &mut self,
        sid: &str,
        agent: &str,
        title: &str,
        cwd: &str,
        text: &str,
        photos: usize,
        now_ms: u64,
    ) {
        let safe_text = redact(&clean_prompt(text));
        let safe_title = redact(title);
        let entry = self
            .data
            .entries
            .entry(sid.to_owned())
            .or_insert_with(|| VaultEntry {
                sid: sid.to_owned(),
                agent: agent.to_owned(),
                title: safe_title.clone(),
                cwd: cwd.to_owned(),
                updated_at: now_ms,
                prompts: Vec::new(),
                start: now_ms,
                end: now_ms,
                n: 0,
                first: String::new(),
                size: 0,
                mtime: 0,
                gone: 0,
                bytes: 0,
                scanned: false,
            });
        entry.updated_at = now_ms;
        entry.end = entry.end.max(now_ms);
        if entry.start == 0 {
            entry.start = now_ms;
        }
        if !safe_title.is_empty() {
            entry.title = safe_title;
        }
        if entry.first.is_empty() && !safe_text.is_empty() {
            entry.first = clip_chars(&safe_text, 300);
        }
        entry.prompts.push(VaultPrompt {
            t: safe_text,
            ph: photos,
            at: now_ms,
            sid: sid.to_owned(),
            title: entry.title.clone(),
        });
        entry.n = entry.n.max(entry.prompts.len());
        if entry.prompts.len() > 200 {
            let drain = entry.prompts.len() - 200;
            entry.prompts.drain(..drain);
        }
        if self.data.entries.len() > 500
            && let Some(oldest) = self
                .data
                .entries
                .iter()
                .min_by_key(|(_, e)| e.updated_at)
                .map(|(k, _)| k.clone())
        {
            self.data.entries.remove(&oldest);
        }
    }

    /// Scans `claude_dir/projects/**/*.jsonl`, strips `<system-reminder>` blocks,
    /// archives raw `.jsonl` transcripts as `.jsonl.gz` via `flate2`, and updates the index.
    pub fn scan_claude_projects(&mut self, claude_dir: &Path) -> usize {
        let projects_dir = claude_dir.join("projects");
        let mut files = Vec::new();
        if projects_dir.is_dir() {
            collect_jsonl_files(&projects_dir, &mut files, 0);
        }

        let now = now_ms();
        let mut seen = HashSet::new();
        let mut kept = 0usize;

        for file in files {
            let Some(sid) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !is_valid_sid(sid) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&file) else {
                continue;
            };
            let size = meta.len();
            if size == 0 || size > MAX_RAW_JSONL_BYTES {
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            seen.insert(sid.to_owned());
            let gz_path = self.archive_dir.join(format!("{sid}.jsonl.gz"));
            if let Some(existing) = self.data.entries.get(sid)
                && existing.scanned
                && existing.size == size
                && existing.mtime == mtime
                && existing.gone == 0
                && gz_path.is_file()
            {
                continue;
            }

            let Ok(raw_bytes) = std::fs::read(&file) else {
                continue;
            };
            let raw_text = String::from_utf8_lossy(&raw_bytes);
            let parsed = parse_transcript_jsonl(&raw_text);
            if parsed.msgs.is_empty() {
                continue;
            }

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            if encoder.write_all(&raw_bytes).is_err() {
                continue;
            }
            let Ok(gz_bytes) = encoder.finish() else {
                continue;
            };
            if write_bytes_atomic(&gz_path, &gz_bytes).is_err() {
                continue;
            }

            let first = parsed
                .msgs
                .iter()
                .find(|m| m.r == "u")
                .map(|m| clip_chars(&m.t, 300))
                .unwrap_or_default();
            let end = if parsed.end > 0 { parsed.end } else { mtime.max(now) };
            let start = if parsed.start > 0 { parsed.start } else { end };
            let title = if parsed.title.is_empty() {
                Path::new(&parsed.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(redact)
                    .unwrap_or_default()
            } else {
                parsed.title
            };
            let prompts: Vec<VaultPrompt> = parsed
                .msgs
                .iter()
                .filter(|m| m.r == "u")
                .take(200)
                .map(|m| VaultPrompt {
                    t: m.t.clone(),
                    ph: m.ph,
                    at: if m.at > 0 { m.at } else { end },
                    sid: sid.to_owned(),
                    title: title.clone(),
                })
                .collect();

            self.data.entries.insert(
                sid.to_owned(),
                VaultEntry {
                    sid: sid.to_owned(),
                    agent: "claude".to_owned(),
                    title,
                    cwd: parsed.cwd,
                    updated_at: end,
                    prompts,
                    start,
                    end,
                    n: parsed.msgs.len(),
                    first,
                    size,
                    mtime,
                    gone: 0,
                    bytes: gz_bytes.len(),
                    scanned: true,
                },
            );
            kept += 1;
        }

        let mut marked_gone = false;
        for entry in self.data.entries.values_mut() {
            if entry.scanned && !seen.contains(&entry.sid) && entry.gone == 0 {
                entry.gone = now;
                marked_gone = true;
            }
        }

        self.scanned_at = now;
        if kept > 0 || marked_gone {
            let _ = self.flush();
        }
        kept
    }

    pub fn get_transcript(&self, sid: &str) -> Option<ArchivedTranscript> {
        if !is_valid_sid(sid) {
            return None;
        }
        let entry = self.data.entries.get(sid)?;
        if let Some(gz_path) = self.archive_path(sid)
            && gz_path.is_file()
            && let Ok(gz_bytes) = std::fs::read(&gz_path)
        {
            let mut decoder = GzDecoder::new(&gz_bytes[..]);
            let mut raw = String::new();
            if decoder.read_to_string(&mut raw).is_ok() {
                let parsed = parse_transcript_jsonl(&raw);
                return Some(ArchivedTranscript {
                    v: ARCHIVE_VERSION,
                    sid: entry.sid.clone(),
                    title: if parsed.title.is_empty() {
                        entry.title.clone()
                    } else {
                        parsed.title
                    },
                    cwd: if parsed.cwd.is_empty() {
                        entry.cwd.clone()
                    } else {
                        parsed.cwd
                    },
                    start: if parsed.start > 0 { parsed.start } else { entry.start },
                    end: if parsed.end > 0 { parsed.end } else { entry.end },
                    msgs: parsed.msgs,
                    gone: entry.gone,
                });
            }
        }
        Some(ArchivedTranscript {
            v: ARCHIVE_VERSION,
            sid: entry.sid.clone(),
            title: entry.title.clone(),
            cwd: entry.cwd.clone(),
            start: entry.start,
            end: entry.end,
            msgs: entry
                .prompts
                .iter()
                .map(|p| VaultMessage {
                    r: "u".to_owned(),
                    t: p.t.clone(),
                    at: p.at,
                    ph: p.ph,
                })
                .collect(),
            gone: entry.gone,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<&VaultEntry> {
        let folded_query = fold(query);
        let words: Vec<&str> = folded_query.split_whitespace().collect();
        let mut list: Vec<&VaultEntry> = self
            .data
            .entries
            .values()
            .filter(|e| {
                if words.is_empty() {
                    return true;
                }
                let mut hay = String::new();
                hay.push_str(&e.title);
                hay.push(' ');
                hay.push_str(&e.cwd);
                hay.push(' ');
                hay.push_str(&e.first);
                for p in &e.prompts {
                    hay.push('\n');
                    hay.push_str(&p.t);
                }
                let folded = fold(&hay);
                words.iter().all(|w| folded.contains(w))
            })
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list.truncate(limit);
        list
    }

    pub fn stats(&self) -> Value {
        let mut gone = 0usize;
        let mut bytes = 0usize;
        for e in self.data.entries.values() {
            if e.gone > 0 {
                gone += 1;
            }
            bytes += e.bytes;
        }
        json!({
            "n": self.data.entries.len(),
            "gone": gone,
            "bytes": bytes,
            "at": self.scanned_at,
        })
    }

    pub fn recent_prompts(&self, since_ms: u64) -> Vec<VaultPrompt> {
        let mut out = Vec::new();
        for e in self.data.entries.values() {
            for p in &e.prompts {
                if p.at >= since_ms {
                    out.push(p.clone());
                }
            }
        }
        out.sort_by_key(|p| p.at);
        out
    }

    pub fn flush(&self) -> anyhow::Result<()> {
        write_json_atomic(&self.file, &self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_redacts_secrets_searches_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = VaultStore::load(dir.path());
        v.record_prompt("s1", "claude", "API work", "/repo", "fix token sk-secret123456", 0, 1000);
        v.flush().unwrap();
        let loaded = VaultStore::load(dir.path());
        let found = loaded.search("api", 10);
        assert_eq!(found.len(), 1);
        assert!(!found[0].prompts[0].t.contains("sk-secret123456"));
    }

    #[test]
    fn fold_normalizes_vietnamese_diacritics_and_d_stroke() {
        assert_eq!(fold("Tối ưu RAM"), "toi uu ram");
        assert_eq!(
            fold("Định dạng sửa lỗi đường dẫn"),
            "dinh dang sua loi duong dan"
        );
    }

    #[test]
    fn scan_claude_projects_strips_system_reminder_archives_gz_and_searches_vietnamese() {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let claude_dir = dir.path().join("claude");
        let proj = claude_dir.join("projects").join("proj-a");
        std::fs::create_dir_all(&proj).unwrap();

        let jsonl_path = proj.join("sess-viet-001.jsonl");
        let lines = [
            r#"{"type":"custom-title","customTitle":"Tối ưu RAM server"}"#,
            r#"{"type":"user","cwd":"/repo/app","timestamp":"2026-11-25T10:00:00Z","message":{"content":"<system-reminder>hidden prompt</system-reminder>Hãy tối ưu bộ nhớ định kỳ với token sk-ant-api03-secret9999"}}"#,
            r#"{"type":"assistant","timestamp":"2026-11-25T10:00:05Z","message":{"content":[{"type":"text","text":"Đã tối ưu RAM xong."}]}}"#,
        ]
        .join("\n");
        std::fs::write(&jsonl_path, format!("{lines}\n")).unwrap();

        let mut vault = VaultStore::load(&data_dir);
        let kept = vault.scan_claude_projects(&claude_dir);
        assert_eq!(kept, 1);

        // Second scan on unchanged file is a no-op
        assert_eq!(vault.scan_claude_projects(&claude_dir), 0);

        // Gzip archive exists and decompresses cleanly
        let gz_path = data_dir.join("vault").join("c").join("sess-viet-001.jsonl.gz");
        assert!(gz_path.exists(), "expected gzip archive at {}", gz_path.display());
        let gz_bytes = std::fs::read(&gz_path).unwrap();
        let mut decoder = GzDecoder::new(&gz_bytes[..]);
        let mut raw_out = String::new();
        decoder.read_to_string(&mut raw_out).unwrap();
        assert!(raw_out.contains("sess-viet-001") || raw_out.contains("Tối ưu RAM server"));

        // Accent-insensitive Vietnamese search works across title and prompt body
        let by_title = vault.search("toi uu ram", 10);
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].sid, "sess-viet-001");
        assert!(!by_title[0].prompts[0].t.contains("system-reminder"));
        assert!(!by_title[0].prompts[0].t.contains("hidden prompt"));
        assert!(!by_title[0].prompts[0].t.contains("sk-ant-api03-secret9999"));

        let by_prompt = vault.search("dinh ky", 10);
        assert_eq!(by_prompt.len(), 1);
        assert_eq!(by_prompt[0].sid, "sess-viet-001");

        // Deleting original jsonl marks entry as gone while keeping archived transcript readable
        std::fs::remove_file(&jsonl_path).unwrap();
        let _ = vault.scan_claude_projects(&claude_dir);
        let transcript = vault
            .get_transcript("sess-viet-001")
            .expect("archived transcript should still be readable after source deletion");
        assert!(transcript.gone > 0);
        assert_eq!(transcript.msgs.len(), 2);
        assert!(!transcript.msgs[0].t.contains("hidden prompt"));
        assert!(!transcript.msgs[0].t.contains("sk-ant-api03-secret9999"));
    }
}
