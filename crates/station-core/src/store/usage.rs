use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Hard cap: <= 1 MB streaming read buffer (vs 8 MB in legacy Node.js).
const CHUNK_BYTES: usize = 1024 * 1024;
const MAX_CACHED_FILES: usize = 128;
const MAX_SEEN_PER_FILE: usize = 400;

fn price_of(model: &str) -> Option<(f64, f64, f64)> {
    let m = model.to_ascii_lowercase();
    if m.contains("fable-5-1") || m.contains("mythos-5-1") {
        Some((10.0, 50.0, 0.25))
    } else if m.contains("fable-5") || m.contains("mythos-5") {
        Some((10.0, 50.0, 1.0))
    } else if m.contains("opus-5-5") {
        Some((4.0, 20.0, 0.2))
    } else if m.contains("opus-5") || m.contains("opus-4-5") || m.contains("opus-4-6") || m.contains("opus-4-7") || m.contains("opus-4-8") || m.contains("opus-4-9") {
        Some((5.0, 25.0, 0.5))
    } else if m.contains("opus-4") || m.contains("opus-3") {
        Some((15.0, 75.0, 1.5))
    } else if m.contains("sonnet-5") {
        Some((2.0, 10.0, 0.2))
    } else if m.contains("sonnet") {
        Some((3.0, 15.0, 0.3))
    } else if m.contains("haiku-4") {
        Some((1.0, 5.0, 0.1))
    } else if m.contains("haiku-3-5") {
        Some((0.8, 4.0, 0.08))
    } else if m.contains("haiku") {
        Some((0.25, 1.25, 0.03))
    } else {
        None
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUsage {
    pub cost: f64,
    #[serde(rename = "in")]
    pub input: u64,
    pub out: u64,
    pub ctx: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<u64>,
}

pub fn ctx_max(u: &SessionUsage) -> u64 {
    if u.model.to_ascii_lowercase().contains("[1m]") || u.ctx > 200_000 {
        1_000_000
    } else {
        200_000
    }
}

#[derive(Default)]
struct FileCursor {
    offset: u64,
    seen: HashMap<String, u64>,
    seen_order: VecDeque<String>,
}

pub struct UsageScanner {
    files: HashMap<PathBuf, FileCursor>,
    order: VecDeque<PathBuf>,
    sessions: HashMap<String, SessionUsage>,
}

impl Default for UsageScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageScanner {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            order: VecDeque::new(),
            sessions: HashMap::new(),
        }
    }

    fn find_transcript(sid: &str, claude_dir: &Path) -> Option<PathBuf> {
        if sid.is_empty()
            || sid.len() > 120
            || !sid
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        let projects = claude_dir.join("projects");
        let rd = std::fs::read_dir(projects).ok()?;
        let fname = format!("{sid}.jsonl");
        for entry in rd.flatten().take(200) {
            let candidate = entry.path().join(&fname);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    pub fn scan_session(
        &mut self,
        sid: &str,
        transcript_path: Option<&Path>,
        claude_dir: &Path,
    ) -> Option<SessionUsage> {
        let path = transcript_path
            .filter(|p| p.is_file())
            .map(Path::to_path_buf)
            .or_else(|| Self::find_transcript(sid, claude_dir))?;
        let mut cur = self.sessions.remove(sid).unwrap_or_default();
        self.scan_file(&mut cur, &path, true);
        let out = cur.clone();
        self.sessions.insert(sid.to_owned(), cur);
        Some(out)
    }

    pub fn usage_of(&self, sid: &str) -> Option<&SessionUsage> {
        self.sessions.get(sid)
    }

    fn read_new_lines(&mut self, path: &Path) -> Option<Vec<String>> {
        let meta = std::fs::metadata(path).ok()?;
        let size = meta.len();
        if !self.files.contains_key(path) {
            self.files.insert(path.to_path_buf(), FileCursor::default());
            self.order.push_back(path.to_path_buf());
            while self.files.len() > MAX_CACHED_FILES {
                if let Some(old) = self.order.pop_front() {
                    self.files.remove(&old);
                }
            }
        }
        let cur = self.files.get_mut(path)?;
        if size < cur.offset {
            cur.offset = 0;
            cur.seen.clear();
            cur.seen_order.clear();
        }
        if size == cur.offset {
            return None;
        }
        let to_read = (size - cur.offset).min(CHUNK_BYTES as u64) as usize;
        let mut f = File::open(path).ok()?;
        f.seek(SeekFrom::Start(cur.offset)).ok()?;
        let mut buf = vec![0u8; to_read];
        let n = f.read(&mut buf).ok()?;
        buf.truncate(n);
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else {
            if n == CHUNK_BYTES {
                cur.offset += n as u64;
                return Some(Vec::new());
            }
            return None;
        };
        cur.offset += (end + 1) as u64;
        let text = String::from_utf8_lossy(&buf[..end]);
        Some(text.split('\n').map(str::to_owned).collect())
    }

    pub fn scan_file(&mut self, u: &mut SessionUsage, path: &Path, is_main: bool) -> bool {
        let Some(lines) = self.read_new_lines(path) else {
            return false;
        };
        let mut changed = false;
        let cur = self.files.get_mut(path).unwrap();
        for line in lines {
            if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
                continue;
            }
            let Ok(rec) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if rec.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(msg) = rec.get("message") else {
                continue;
            };
            let Some(usage) = msg.get("usage") else {
                continue;
            };
            let Some(model) = msg.get("model").and_then(Value::as_str) else {
                continue;
            };
            if model.is_empty() || model == "<synthetic>" {
                continue;
            }
            let id = msg
                .get("id")
                .or_else(|| rec.get("uuid"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let out = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
            let prev = cur.seen.get(&id).copied();
            if prev.is_some_and(|p| out <= p) {
                continue;
            }
            let price = price_of(model);
            let fast = if usage.get("speed").and_then(Value::as_str) == Some("fast") {
                2.0
            } else {
                1.0
            };
            let is_sidechain = rec.get("isSidechain").and_then(Value::as_bool).unwrap_or(false);
            if prev.is_none() {
                let cc = usage.get("cache_creation");
                let w1 = cc
                    .and_then(|c| c.get("ephemeral_1h_input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let total_cw = usage
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let w5 = cc
                    .and_then(|c| c.get("ephemeral_5m_input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| total_cw.saturating_sub(w1));
                let inp = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let cr = usage
                    .get("cache_read_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let web = usage
                    .get("server_tool_use")
                    .and_then(|s| s.get("web_search_requests"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                u.input += inp + w5 + w1 + cr;
                if let Some((pi, _po, pcr)) = price {
                    u.cost += fast
                        * ((inp as f64) * pi
                            + (w5 as f64) * pi * 1.25
                            + (w1 as f64) * pi * 2.0
                            + (cr as f64) * pcr)
                        / 1e6
                        + (web as f64) * 0.01;
                }
                if is_main && !is_sidechain {
                    u.ctx = inp + w5 + w1 + cr + out;
                    u.model = model.to_owned();
                }
            } else if is_main && !is_sidechain {
                u.ctx += out.saturating_sub(prev.unwrap_or(0));
            }
            let add = out.saturating_sub(prev.unwrap_or(0));
            u.out += add;
            if let Some((_pi, po, _pcr)) = price {
                u.cost += fast * (add as f64) * po / 1e6;
            }
            cur.seen.insert(id.clone(), out);
            cur.seen_order.push_back(id);
            while cur.seen.len() > MAX_SEEN_PER_FILE {
                if let Some(old) = cur.seen_order.pop_front() {
                    cur.seen.remove(&old);
                }
            }
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_scanner_dedupes_streamed_messages_and_prices_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("s1.jsonl");
        let lines = [
            r#"{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":1000000,"output_tokens":100000,"cache_read_input_tokens":0}}}"#,
            r#"{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":1000000,"output_tokens":200000,"cache_read_input_tokens":0}}}"#,
        ]
        .join("\n")
            + "\n";
        std::fs::write(&file, lines).unwrap();

        let mut scanner = UsageScanner::new();
        let mut u = SessionUsage::default();
        assert!(scanner.scan_file(&mut u, &file, true));
        // Input priced once ($2/M), output total 200k ($10/M -> $2) = $4.00
        assert!((u.cost - 4.0).abs() < 1e-6, "cost={}", u.cost);
        assert_eq!(u.out, 200_000);
        assert_eq!(ctx_max(&u), 1_000_000);
    }
}
