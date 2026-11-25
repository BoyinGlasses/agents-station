use crate::redact::redact;
use crate::shared::quick_choices::{QuickChoice, quick_choices};
use crate::state::bg_sessions::{BgProbeResult, probe_claude_bg};
use crate::state::git::CREATE_NO_WINDOW;
use crate::store::agent_rpc::{encode_rpc_request, parse_rpc_line};
use crate::store::cli_command::{ResolvedCli, find_in_path};
use crate::store::shots::shots_in;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

const MAX_TEXT: usize = 4000;
const MAX_CONCURRENT: usize = 3;
const MAX_PER_HOUR: usize = 12;
const MAX_COMPLETED: usize = 30;
pub const MAX_TASK_OUTPUT_BYTES: u64 = 1024 * 1024;
pub const DEFAULT_TASK_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct TaskCommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub stdin_payload: Option<String>,
}

pub type CommandOverride =
    Arc<dyn Fn(&TaskStartRequest, &str) -> Option<TaskCommandSpec> + Send + Sync>;

pub fn resolve_agent_cli(agent: &str) -> Option<ResolvedCli> {
    let env_key = match agent {
        "claude" => Some("OFFICE_CLAUDE_BIN"),
        "codex" => Some("OFFICE_CODEX_BIN"),
        "opencode" => Some("OFFICE_OPENCODE_BIN"),
        "antigravity" => Some("OFFICE_ANTIGRAVITY_BIN"),
        "zcode" => Some("OFFICE_ZCODE_BIN"),
        _ => None,
    };
    if let Some(k) = env_key
        && let Ok(v) = std::env::var(k)
    {
        let p = PathBuf::from(v);
        if p.is_file() {
            return Some(ResolvedCli {
                file: p,
                args: Vec::new(),
                protocol: (agent == "antigravity").then(|| "acp".to_owned()),
            });
        }
    }
    let names: &[&str] = match agent {
        "claude" => &["claude.exe", "claude.cmd", "claude"],
        "codex" => &["codex.exe", "codex.cmd", "codex"],
        "opencode" => &["opencode.exe", "opencode.cmd", "opencode"],
        "antigravity" => &["agy.exe", "agy"],
        "zcode" => &["zcode.exe", "zcode"],
        _ => return None,
    };
    let mut found = find_in_path(names)?;
    if agent == "antigravity" {
        found.protocol = Some("acp".to_owned());
    }
    Some(found)
}

pub fn available_agents() -> Vec<Value> {
    const ALL: [(&str, &str); 5] = [
        ("claude", "Claude Code"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("antigravity", "Antigravity"),
        ("zcode", "ZCode"),
    ];
    ALL.iter()
        .filter(|(id, _)| resolve_agent_cli(id).is_some())
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect()
}

pub fn build_command_for_cli(cli: &ResolvedCli, req: &TaskStartRequest) -> Option<TaskCommandSpec> {
    let prompt: String = req.text.trim().chars().take(MAX_TEXT).collect();
    if prompt.is_empty() {
        return None;
    }
    let mut args = cli.args.clone();
    let mut stdin_payload = None;
    match req.agent.as_str() {
        "claude" => {
            args.push("--output-format".into());
            args.push("text".into());
            if !req.model.is_empty() {
                args.push("--model".into());
                args.push(req.model.clone());
            }
            if req.fork {
                args.push("--fork-session".into());
            }
            args.push("-p".into());
            args.push("--".into());
            args.push(prompt);
        }
        "codex" => {
            args.push("exec".into());
            if !req.model.is_empty() {
                args.push("-m".into());
                args.push(req.model.clone());
            }
            args.push("--".into());
            args.push(prompt);
        }
        "opencode" => {
            args.push("run".into());
            if !req.model.is_empty() {
                args.push("-m".into());
                args.push(req.model.clone());
            }
            args.push("--".into());
            args.push(prompt);
        }
        "antigravity" => {
            args.push("--acp".into());
            let init = encode_rpc_request(
                1,
                "initialize",
                json!({ "clientInfo": { "name": "agents-station" } }),
            );
            let turn = encode_rpc_request(
                2,
                "session/prompt",
                json!({ "cwd": req.dir, "prompt": [{ "type": "text", "text": prompt }] }),
            );
            stdin_payload = Some(format!("{init}{turn}"));
        }
        "zcode" => {
            args.push("-p".into());
            args.push("--".into());
            args.push(prompt);
        }
        _ => return None,
    }
    Some(TaskCommandSpec {
        program: cli.file.clone(),
        args,
        stdin_payload,
    })
}

fn default_command_for(req: &TaskStartRequest) -> Option<TaskCommandSpec> {
    let cli = resolve_agent_cli(&req.agent)?;
    build_command_for_cli(&cli, req)
}

fn valid_model_id(m: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._:/#-]{0,199}$").unwrap());
    re.is_match(m)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub agent: String,
    pub dir: String,
    pub text: String,
    pub model: String,
    pub at: u64,
    pub sid: Option<String>,
    pub mode: String,
    pub status: String,
    pub result: String,
    pub error: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<QuickChoice>,
}

#[derive(Debug, Clone)]
pub struct TaskStartRequest {
    pub dir: String,
    pub text: String,
    pub agent: String,
    pub model: String,
    pub fork: bool,
}

pub struct TaskManager {
    running: HashMap<String, TaskRecord>,
    completed: VecDeque<TaskRecord>,
    kill_handles: HashMap<String, oneshot::Sender<()>>,
    recent: VecDeque<u64>,
    binds: HashMap<String, (String, String)>,
    bg_probe: BgProbeResult,
    command_override: Option<CommandOverride>,
    timeout: Duration,
    seq: u64,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
            completed: VecDeque::new(),
            kill_handles: HashMap::new(),
            recent: VecDeque::new(),
            binds: HashMap::new(),
            bg_probe: BgProbeResult::default(),
            command_override: None,
            timeout: DEFAULT_TASK_TIMEOUT,
            seq: 0,
        }
    }

    pub fn set_command_override(&mut self, f: CommandOverride) {
        self.command_override = Some(f);
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn set_bg_probe(&mut self, probe: BgProbeResult) {
        self.bg_probe = probe;
    }

    pub fn bg_probe(&self) -> &BgProbeResult {
        &self.bg_probe
    }

    pub fn probe_bg(&mut self) -> BgProbeResult {
        if let Some(cli) = resolve_agent_cli("claude") {
            self.bg_probe = probe_claude_bg(&cli.file);
        } else {
            self.bg_probe = BgProbeResult::default();
        }
        self.bg_probe.clone()
    }

    fn resolve_spec(&self, req: &TaskStartRequest, task_id: &str) -> Option<TaskCommandSpec> {
        if let Some(f) = &self.command_override {
            return f(req, task_id);
        }
        default_command_for(req)
    }

    pub fn validate_and_register(&mut self, req: TaskStartRequest, now_ms: u64) -> Value {
        let text: String = req.text.trim().chars().take(MAX_TEXT).collect();
        if text.is_empty() {
            return json!({ "ok": false, "code": "empty" });
        }
        if self.running.len() >= MAX_CONCURRENT {
            return json!({ "ok": false, "code": "busy" });
        }
        while let Some(&oldest) = self.recent.front() {
            if now_ms.saturating_sub(oldest) > 3_600_000 {
                self.recent.pop_front();
            } else {
                break;
            }
        }
        if self.recent.len() >= MAX_PER_HOUR {
            return json!({ "ok": false, "code": "tooMany" });
        }
        let agent = if req.agent.is_empty() {
            "claude"
        } else {
            req.agent.as_str()
        };
        if !matches!(agent, "claude" | "codex" | "opencode" | "antigravity" | "zcode") {
            return json!({ "ok": false, "code": "noAgent" });
        }
        if matches!(agent, "antigravity" | "zcode") && req.fork {
            return json!({ "ok": false, "code": "noAgent" });
        }
        if !req.model.is_empty() && (!valid_model_id(&req.model) || agent == "zcode") {
            return json!({ "ok": false, "code": "badModel" });
        }
        if req.dir.is_empty() || !Path::new(&req.dir).is_dir() {
            return json!({ "ok": false, "code": "noDir" });
        }

        self.seq += 1;
        let id = format!("{now_ms:x}_{}", self.seq);
        let rec = TaskRecord {
            id: id.clone(),
            agent: agent.to_owned(),
            dir: req.dir,
            text: redact(&text).chars().take(200).collect(),
            model: req.model,
            at: now_ms,
            sid: None,
            mode: "print".to_owned(),
            status: "running".to_owned(),
            result: String::new(),
            error: String::new(),
            shots: Vec::new(),
            choices: Vec::new(),
        };
        self.running.insert(id, rec.clone());
        self.recent.push_back(now_ms);
        json!({ "ok": true, "task": rec })
    }

    pub fn bind(&mut self, job_id: &str, sid: &str, inbox: &str) {
        if !job_id.is_empty() {
            self.binds
                .insert(job_id.to_owned(), (sid.to_owned(), inbox.to_owned()));
            if let Some(t) = self.running.get_mut(job_id) {
                t.sid = Some(sid.to_owned());
            }
        }
    }

    pub fn finish(&mut self, id: &str) -> Option<TaskRecord> {
        if let Some(tx) = self.kill_handles.remove(id) {
            let _ = tx.send(());
        }
        self.binds.remove(id);
        let rec = self.running.remove(id)?;
        self.push_completed(rec.clone());
        Some(rec)
    }

    pub fn stop(&mut self, id: &str) -> bool {
        if let Some(tx) = self.kill_handles.remove(id) {
            let _ = tx.send(());
        }
        self.binds.remove(id);
        if let Some(mut rec) = self.running.remove(id) {
            rec.status = "stopped".to_owned();
            self.push_completed(rec);
            true
        } else {
            false
        }
    }

    fn push_completed(&mut self, rec: TaskRecord) {
        self.completed.push_back(rec);
        while self.completed.len() > MAX_COMPLETED {
            self.completed.pop_front();
        }
    }

    pub fn complete(&mut self, id: &str, ok: bool, stdout: &str, stderr: &str) -> Option<TaskRecord> {
        self.complete_with_status(id, if ok { "done" } else { "failed" }, stdout, stderr)
    }

    pub fn complete_with_status(
        &mut self,
        id: &str,
        status: &str,
        stdout: &str,
        stderr: &str,
    ) -> Option<TaskRecord> {
        self.kill_handles.remove(id);
        self.binds.remove(id);
        let mut rec = self.running.remove(id)?;
        let extracted = shots_in(stdout);
        rec.shots = extracted.urls;
        rec.choices = quick_choices(&extracted.text);
        if let Some(rpc_val) = extracted.text.lines().rev().find_map(parse_rpc_line)
            && let Some(r) = rpc_val.get("result").and_then(Value::as_str)
        {
            rec.result = redact(r).chars().take(2000).collect();
        } else {
            rec.result = redact(&extracted.text).chars().take(2000).collect();
        }
        rec.error = redact(stderr.trim()).chars().take(400).collect();
        rec.status = status.to_owned();
        self.push_completed(rec.clone());
        Some(rec)
    }

    pub fn get(&self, id: &str) -> Option<TaskRecord> {
        self.running.get(id).cloned()
    }

    pub fn of_sid(&self, sid: &str) -> Option<TaskRecord> {
        self.running
            .values()
            .find(|t| t.sid.as_deref() == Some(sid))
            .cloned()
    }

    pub fn inbox_of(&self, id: &str) -> Option<String> {
        self.binds
            .get(id)
            .map(|(_, ib)| ib.clone())
            .filter(|s| !s.is_empty())
    }

    pub fn list(&self) -> Vec<&TaskRecord> {
        self.running.values().collect()
    }

    pub fn recent_completed(&self) -> Vec<&TaskRecord> {
        self.completed.iter().collect()
    }
}

/// Validates, registers, and spawns a child CLI task with `CREATE_NO_WINDOW`.
/// Drains stdout/stderr concurrently with bounded RAM caps and an execution timeout
/// so pipes never deadlock and `TaskManager` never wedges at `MAX_CONCURRENT` (3).
pub fn start_and_spawn(
    tasks: Arc<Mutex<TaskManager>>,
    mut req: TaskStartRequest,
    now_ms: u64,
    on_done: Option<Arc<dyn Fn(TaskRecord) + Send + Sync>>,
) -> Value {
    req.text = req.text.trim().chars().take(MAX_TEXT).collect();
    let (res, task_id, spec, dir, timeout) = {
        let mut guard = tasks.lock().unwrap_or_else(|e| e.into_inner());
        let reg = guard.validate_and_register(req.clone(), now_ms);
        if reg.get("ok").and_then(Value::as_bool) != Some(true) {
            return reg;
        }
        let id = reg["task"]["id"].as_str().unwrap_or("").to_owned();
        let spec = guard.resolve_spec(&req, &id);
        let Some(spec) = spec else {
            // Release slot immediately so manager never wedges!
            guard.running.remove(&id);
            guard.recent.pop_back();
            return json!({ "ok": false, "code": "noAgent" });
        };
        (reg, id, spec, req.dir.clone(), guard.timeout)
    };

    let has_stdin = spec.stdin_payload.is_some();
    let mut cmd = tokio::process::Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&dir)
        .stdin(if has_stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let mut guard = tasks.lock().unwrap_or_else(|e| e.into_inner());
            guard.running.remove(&task_id);
            guard.recent.pop_back();
            return json!({
                "ok": false,
                "code": "noAgent",
                "error": e.to_string(),
            });
        }
    };

    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    {
        let mut guard = tasks.lock().unwrap_or_else(|e| e.into_inner());
        guard.kill_handles.insert(task_id.clone(), kill_tx);
    }

    let tasks_bg = Arc::clone(&tasks);
    let id_bg = task_id.clone();
    let stdin_data = spec.stdin_payload;
    tokio::spawn(async move {
        let mut child = child;
        if let (Some(payload), Some(mut stdin)) = (stdin_data, child.stdin.take()) {
            let _ = stdin.write_all(payload.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let drain_out = async move {
            let mut buf = Vec::new();
            if let Some(out) = stdout_pipe {
                let _ = out.take(MAX_TASK_OUTPUT_BYTES).read_to_end(&mut buf).await;
            }
            buf
        };
        let drain_err = async move {
            let mut buf = Vec::new();
            if let Some(err) = stderr_pipe {
                let _ = err.take(MAX_TASK_OUTPUT_BYTES).read_to_end(&mut buf).await;
            }
            buf
        };

        enum Outcome {
            Finished(std::io::Result<std::process::ExitStatus>, Vec<u8>, Vec<u8>),
            TimedOut,
            Killed,
        }

        let outcome = {
            let run_fut = async {
                let (status, out_buf, err_buf) = tokio::join!(child.wait(), drain_out, drain_err);
                (status, out_buf, err_buf)
            };
            tokio::select! {
                res = tokio::time::timeout(timeout, run_fut) => match res {
                    Ok((status, out_buf, err_buf)) => Outcome::Finished(status, out_buf, err_buf),
                    Err(_) => Outcome::TimedOut,
                },
                _ = kill_rx => Outcome::Killed,
            }
        };

        match outcome {
            Outcome::Finished(status, out_buf, err_buf) => {
                let ok = status.map(|s| s.success()).unwrap_or(false);
                let out_s = String::from_utf8_lossy(&out_buf);
                let err_s = String::from_utf8_lossy(&err_buf);
                let finished = {
                    let mut guard = tasks_bg.lock().unwrap_or_else(|e| e.into_inner());
                    guard.complete(&id_bg, ok, &out_s, &err_s)
                };
                if let (Some(rec), Some(cb)) = (finished, on_done) {
                    cb(rec);
                }
            }
            Outcome::TimedOut => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let finished = {
                    let mut guard = tasks_bg.lock().unwrap_or_else(|e| e.into_inner());
                    guard.complete_with_status(&id_bg, "timeout", "", "task timed out")
                };
                if let (Some(rec), Some(cb)) = (finished, on_done) {
                    cb(rec);
                }
            }
            Outcome::Killed => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let mut guard = tasks_bg.lock().unwrap_or_else(|e| e.into_inner());
                guard.stop(&id_bg);
            }
        }
    });

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_manager_validates_agent_model_dir_and_caps() {
        let dir = tempfile::tempdir().unwrap();
        let mut tm = TaskManager::new();

        let bad_model = tm.validate_and_register(
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "do work".into(),
                agent: "zcode".into(),
                model: "gpt-5".into(),
                fork: false,
            },
            1000,
        );
        assert_eq!(bad_model["code"], "badModel");

        let ok = tm.validate_and_register(
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "fix bug with sk-1234567890".into(),
                agent: "claude".into(),
                model: "sonnet".into(),
                fork: false,
            },
            2000,
        );
        assert_eq!(ok["ok"], true);
        let id = ok["task"]["id"].as_str().unwrap().to_owned();
        assert!(!ok["task"]["text"].as_str().unwrap().contains("sk-1234567890"));
        assert!(tm.finish(&id).is_some());
        assert!(tm.list().is_empty());
    }

    #[test]
    fn command_builder_isolates_leading_dash_prompt_supports_fork_acp_and_requires_is_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file_not_dir = dir.path().join("plain.txt");
        std::fs::write(&file_not_dir, "hi").unwrap();

        let mut tm = TaskManager::new();
        let non_dir = tm.validate_and_register(
            TaskStartRequest {
                dir: file_not_dir.to_string_lossy().into_owned(),
                text: "hello".into(),
                agent: "claude".into(),
                model: String::new(),
                fork: false,
            },
            1000,
        );
        assert_eq!(non_dir["code"], "noDir", "file path must be rejected as non-dir");

        let cli = ResolvedCli {
            file: PathBuf::from("claude"),
            args: Vec::new(),
            protocol: None,
        };
        let spec = build_command_for_cli(
            &cli,
            &TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "--dangerously-skip-permissions evil".into(),
                agent: "claude".into(),
                model: "sonnet".into(),
                fork: true,
            },
        )
        .expect("claude spec");
        assert!(spec.args.contains(&"--fork-session".to_owned()));
        let sep_pos = spec
            .args
            .iter()
            .position(|a| a == "--")
            .expect("must include -- end-of-options before prompt");
        assert_eq!(spec.args[sep_pos + 1], "--dangerously-skip-permissions evil");

        let acp_cli = ResolvedCli {
            file: PathBuf::from("agy"),
            args: Vec::new(),
            protocol: Some("acp".into()),
        };
        let acp_spec = build_command_for_cli(
            &acp_cli,
            &TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "check repo".into(),
                agent: "antigravity".into(),
                model: String::new(),
                fork: false,
            },
        )
        .expect("acp spec");
        let stdin = acp_spec
            .stdin_payload
            .expect("antigravity ACP must send JSON-RPC initialize + prompt on stdin");
        assert!(stdin.contains("\"method\":\"initialize\""));
        assert!(stdin.contains("check repo"));
    }

    #[tokio::test]
    async fn start_and_spawn_drains_large_stdout_without_deadlocking_and_enforces_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let tm = Arc::new(Mutex::new(TaskManager::new()));

        // 1. Emit > 16 KB to stdout (exceeds 4 KB Windows pipe buffer!) + shots & choices
        let big_file = dir.path().join("big.txt");
        let mut payload = "X".repeat(20_000);
        payload.push_str("\nSCREENSHOT: http://localhost:3000/ui\n1. First\n2. Second\n");
        std::fs::write(&big_file, &payload).unwrap();
        let big_path = big_file.clone();
        #[cfg(windows)]
        let big_cmd = move |_req: &TaskStartRequest, _id: &str| {
            Some(TaskCommandSpec {
                program: PathBuf::from("cmd.exe"),
                args: vec!["/C".into(), "type".into(), big_path.to_string_lossy().into_owned()],
                stdin_payload: None,
            })
        };
        #[cfg(not(windows))]
        let big_cmd = move |_req: &TaskStartRequest, _id: &str| {
            Some(TaskCommandSpec {
                program: PathBuf::from("cat"),
                args: vec![big_path.to_string_lossy().into_owned()],
                stdin_payload: None,
            })
        };
        tm.lock().unwrap().set_command_override(Arc::new(big_cmd));

        let res = start_and_spawn(
            Arc::clone(&tm),
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "emit big output".into(),
                agent: "claude".into(),
                model: String::new(),
                fork: false,
            },
            1000,
            None,
        );
        assert_eq!(res["ok"], true);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while !tm.lock().unwrap().list().is_empty() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "large stdout child deadlocked instead of draining concurrently"
            );
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
        {
            let g = tm.lock().unwrap();
            let last = g.recent_completed().last().copied().expect("completed record");
            assert_eq!(last.status, "done");
            assert_eq!(last.shots, vec!["http://localhost:3000/ui"]);
            assert_eq!(last.choices.len(), 2);
        }

        // 2. Timeout kills slow child and frees concurrency slot
        #[cfg(windows)]
        let slow_cmd = |_req: &TaskStartRequest, _id: &str| {
            Some(TaskCommandSpec {
                program: PathBuf::from("ping.exe"),
                args: vec!["-n".into(), "15".into(), "127.0.0.1".into()],
                stdin_payload: None,
            })
        };
        #[cfg(not(windows))]
        let slow_cmd = |_req: &TaskStartRequest, _id: &str| {
            Some(TaskCommandSpec {
                program: PathBuf::from("sleep"),
                args: vec!["15".into()],
                stdin_payload: None,
            })
        };
        {
            let mut g = tm.lock().unwrap();
            g.set_timeout(std::time::Duration::from_millis(200));
            g.set_command_override(Arc::new(slow_cmd));
        }
        let res2 = start_and_spawn(
            Arc::clone(&tm),
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "slow task".into(),
                agent: "claude".into(),
                model: String::new(),
                fork: false,
            },
            2000,
            None,
        );
        assert_eq!(res2["ok"], true);
        let deadline2 = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while !tm.lock().unwrap().list().is_empty() {
            assert!(
                tokio::time::Instant::now() < deadline2,
                "timed-out child must be killed and release concurrency slot"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let last2 = tm.lock().unwrap().recent_completed().last().copied().unwrap().clone();
        assert_eq!(last2.status, "timeout");
    }
}
