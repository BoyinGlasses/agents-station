use crate::platform::{DiskStats, MemStats, Platform, ProcRow, ProcSnapshot};
use crate::redact::redact;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const GB: u64 = 1024 * 1024 * 1024;
const PORT_SEEN_MAX: usize = 1024;
const TREND_MAX: usize = 31;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn dev_bin_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(node|bun|deno|python\d*(\.\d+)?|Python|ruby|php|java|redis-server|postgres|mysqld|uvicorn|gunicorn|hugo)$").unwrap()
    })
}

fn tree_bin_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(node|npm|npx|pnpm|yarn|bun|sh)$").unwrap())
}

fn bin_of(comm: &str) -> String {
    let base = comm
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(comm)
        .to_owned();
    base.strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".EXE"))
        .unwrap_or(&base)
        .to_owned()
}

fn dev_label(p: &ProcRow) -> String {
    let a = &p.args;
    if a.contains("next-server") || a.contains("next dev") {
        return "next dev".to_owned();
    }
    if a.contains("vite") {
        return "vite".to_owned();
    }
    if a.contains("webpack") {
        return "webpack".to_owned();
    }
    if a.contains("storybook") {
        return "storybook".to_owned();
    }
    let bin = bin_of(&p.comm);
    let script = a
        .split_whitespace()
        .skip(1)
        .find(|x| !x.is_empty() && !x.starts_with('-'));
    match script {
        Some(s) => {
            let file = Path::new(s)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(s);
            format!("{bin} {file}")
        }
        None => bin,
    }
}

fn descendants(kids: &HashMap<u32, Vec<u32>>, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = kids.get(&root).cloned().unwrap_or_default();
    while let Some(c) = stack.pop() {
        out.push(c);
        if let Some(ch) = kids.get(&c) {
            stack.extend_from_slice(ch);
        }
    }
    out
}

fn tree_mem(snap: &ProcSnapshot, root: u32) -> u64 {
    let mut sum = snap.procs.get(&root).map(|p| p.mem).unwrap_or(0);
    for pid in descendants(&snap.kids, root) {
        if let Some(p) = snap.procs.get(&pid) {
            sum = sum.saturating_add(p.mem);
        }
    }
    sum
}

fn tree_cpu(snap: &ProcSnapshot, root: u32) -> f32 {
    let mut sum = snap.procs.get(&root).map(|p| p.cpu_pct).unwrap_or(0.0);
    for pid in descendants(&snap.kids, root) {
        if let Some(p) = snap.procs.get(&pid) {
            sum += p.cpu_pct;
        }
    }
    sum
}

fn protected_pids(snap: &ProcSnapshot) -> HashSet<u32> {
    let mut set = HashSet::new();
    set.insert(0);
    set.insert(1);
    let me = std::process::id();
    set.insert(me);
    let mut cur = snap.procs.get(&me);
    let mut hops = 0;
    while let Some(p) = cur {
        if p.ppid <= 1 || hops > 32 {
            break;
        }
        set.insert(p.ppid);
        cur = snap.procs.get(&p.ppid);
        hops += 1;
    }
    set
}

fn root_of(snap: &ProcSnapshot, pid: u32) -> u32 {
    let mut cur = snap.procs.get(&pid);
    let mut hops = 0;
    while let Some(p) = cur {
        if hops > 20 {
            break;
        }
        let Some(parent) = snap.procs.get(&p.ppid) else {
            break;
        };
        if parent.pid <= 1 || !tree_bin_re().is_match(&bin_of(&parent.comm)) {
            break;
        }
        cur = Some(parent);
        hops += 1;
    }
    cur.map(|p| p.pid).unwrap_or(pid)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevServerRow {
    pub pid: u32,
    pub start: u64,
    pub label: String,
    pub ports: Vec<u16>,
    pub dir: String,
    pub up: u64,
    pub cpu: i64,
    pub mem: u64,
    pub conns: u32,
    pub idle: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub t: u64,
    pub used: u64,
    pub total: u64,
    pub swap: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupItem {
    pub id: String,
    pub title: String,
    pub note: String,
    pub size: u64,
    #[serde(skip_serializing)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysSnapshot {
    pub at: u64,
    pub mem: MemStats,
    pub disk: DiskStats,
    pub tight: bool,
    #[serde(rename = "diskTight")]
    pub disk_tight: bool,
    pub battery: Option<Value>,
    pub cpu: Value,
    #[serde(rename = "devServers")]
    pub dev_servers: Vec<DevServerRow>,
    pub claude: Vec<Value>,
    pub apps: Vec<Value>,
    pub orphans: Vec<Value>,
    pub tips: Vec<Value>,
    pub held: u64,
    pub trend: Vec<TrendPoint>,
    pub cleanups: Vec<CleanupItem>,
    #[serde(rename = "cleanupsAt")]
    pub cleanups_at: u64,
}

struct MonitorInner {
    port_seen: HashMap<u16, u64>,
    trend: Vec<TrendPoint>,
    cleanups: Vec<CleanupItem>,
    cleanups_at: u64,
    started_at: u64,
}

pub struct SystemMonitor {
    platform: Arc<dyn Platform>,
    claude_dir: PathBuf,
    inner: Mutex<MonitorInner>,
}

impl SystemMonitor {
    pub fn new(platform: Arc<dyn Platform>, claude_dir: PathBuf) -> Self {
        Self {
            platform,
            claude_dir,
            inner: Mutex::new(MonitorInner {
                port_seen: HashMap::new(),
                trend: Vec::new(),
                cleanups: Vec::new(),
                cleanups_at: 0,
                started_at: now_ms(),
            }),
        }
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }

    pub fn track_owners(&self) {
        let est = self.platform.established();
        let now = now_ms();
        let mut inner = self.inner.lock().expect("SystemMonitor lock poisoned");
        for &port in est.keys() {
            inner.port_seen.insert(port, now);
        }
        if inner.port_seen.len() > PORT_SEEN_MAX {
            let mut entries: Vec<(u16, u64)> = inner.port_seen.drain().collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            entries.truncate(PORT_SEEN_MAX);
            inner.port_seen = entries.into_iter().collect();
        }
        let mem = self.platform.memory();
        inner.trend.push(TrendPoint {
            t: now,
            used: mem.used,
            total: mem.total,
            swap: mem.swap_used,
        });
        if inner.trend.len() > TREND_MAX {
            let drain = inner.trend.len() - TREND_MAX;
            inner.trend.drain(..drain);
        }
    }

    pub fn scan_cleanups(&self) -> Vec<CleanupItem> {
        let mut items = Vec::new();
        let tmp_claude = std::env::temp_dir().join("claude");
        if tmp_claude.exists() {
            let size = self.platform.folder_bytes(&tmp_claude);
            if size > 0 {
                items.push(CleanupItem {
                    id: "claude-tmp".to_owned(),
                    title: "Stale Claude temp folders".to_owned(),
                    note: "Temporary files left by closed sessions".to_owned(),
                    size,
                    paths: vec![tmp_claude],
                });
            }
        }
        items.sort_by(|a, b| b.size.cmp(&a.size));
        let mut inner = self.inner.lock().expect("SystemMonitor lock poisoned");
        inner.cleanups = items.clone();
        inner.cleanups_at = now_ms();
        items
    }

    pub fn snapshot(&self) -> SysSnapshot {
        let now = now_ms();
        let snap = self.platform.processes();
        let mem = self.platform.memory();
        let disk = self.platform.disk();
        let ports = self.platform.listeners();
        let est = self.platform.established();
        let guard = protected_pids(&snap);

        let mut inner = self.inner.lock().expect("SystemMonitor lock poisoned");
        for &port in est.keys() {
            inner.port_seen.insert(port, now);
        }

        let mut dev_roots: HashMap<u32, (u32, HashSet<u16>)> = HashMap::new();
        for (pid, pset) in &ports {
            let Some(p) = snap.procs.get(pid) else {
                continue;
            };
            if guard.contains(pid) {
                continue;
            }
            let b = bin_of(&p.comm);
            if !dev_bin_re().is_match(&b)
                && !p.args.contains("next-server")
                && !p.args.contains("vite")
                && !p.args.contains("webpack")
            {
                continue;
            }
            let root = root_of(&snap, *pid);
            if guard.contains(&root) {
                continue;
            }
            let entry = dev_roots.entry(root).or_insert_with(|| (*pid, HashSet::new()));
            entry.1.extend(pset.iter().copied());
        }

        let mut dev_servers: Vec<DevServerRow> = dev_roots
            .into_iter()
            .filter_map(|(root, (listener_pid, pset))| {
                let p = snap.procs.get(&listener_pid)?;
                let r = snap.procs.get(&root)?;
                let mut sorted_ports: Vec<u16> = pset.into_iter().collect();
                sorted_ports.sort_unstable();
                let conns: u32 = sorted_ports
                    .iter()
                    .map(|pt| est.get(pt).copied().unwrap_or(0))
                    .sum();
                let last_use = sorted_ports
                    .iter()
                    .filter_map(|pt| inner.port_seen.get(pt).copied())
                    .max()
                    .unwrap_or(0);
                let idle = if conns > 0 {
                    0
                } else {
                    let base = last_use
                        .max(inner.started_at)
                        .max(now.saturating_sub(r.up * 1000));
                    now.saturating_sub(base) / 1000
                };
                Some(DevServerRow {
                    pid: root,
                    start: r.start,
                    label: redact(&dev_label(p)),
                    ports: sorted_ports,
                    dir: String::new(),
                    up: r.up,
                    cpu: tree_cpu(&snap, root).round() as i64,
                    mem: tree_mem(&snap, root),
                    conns,
                    idle,
                })
            })
            .collect();
        dev_servers.sort_by(|a, b| b.mem.cmp(&a.mem));

        let claude = self.read_claude_sessions(&snap, now);
        let held: u64 = snap.procs.values().map(|p| p.mem).sum();
        let total_cpu: f32 = snap.procs.values().map(|p| p.cpu_pct).sum();
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let tight = mem.pressure != "normal" || mem.swap_used > 4 * GB;
        let disk_tight = disk.total > 0 && (disk.free as f64 / disk.total as f64) < 0.15;

        SysSnapshot {
            at: now,
            mem,
            disk,
            tight,
            disk_tight,
            battery: None,
            cpu: json!({
                "cores": cores,
                "percent": ((total_cpu / cores as f32).round() as i64).clamp(0, 100),
                "load": 0.0,
            }),
            dev_servers,
            claude,
            apps: Vec::new(),
            orphans: Vec::new(),
            tips: Vec::new(),
            held,
            trend: inner.trend.clone(),
            cleanups: inner.cleanups.clone(),
            cleanups_at: inner.cleanups_at,
        }
    }

    fn read_claude_sessions(&self, snap: &ProcSnapshot, now: u64) -> Vec<Value> {
        let sess_dir = self.claude_dir.join("sessions");
        let Ok(entries) = std::fs::read_dir(sess_dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(j) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(pid) = j.get("pid").and_then(Value::as_u64).map(|x| x as u32) else {
                continue;
            };
            if !self.platform.is_pid_alive(pid) {
                continue;
            }
            let cwd = j.get("cwd").and_then(Value::as_str).unwrap_or("");
            let name = j
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    Path::new(cwd)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("claude")
                        .to_owned()
                });
            let last = j
                .get("statusUpdatedAt")
                .or_else(|| j.get("updatedAt"))
                .or_else(|| j.get("startedAt"))
                .and_then(Value::as_u64)
                .unwrap_or(now);
            let proc_row = snap.procs.get(&pid);
            out.push(json!({
                "pid": pid,
                "start": proc_row.map(|r| r.start),
                "name": redact(&name),
                "dir": redact(cwd),
                "busy": j.get("status").and_then(Value::as_str) == Some("busy"),
                "idle": now.saturating_sub(last) / 1000,
                "mem": tree_mem(snap, pid),
                "remote": j.get("bridgeSessionId").is_some_and(|v| !v.is_null()),
            }));
        }
        out
    }

    pub fn action(&self, action: &str, target: Option<&Value>, force: bool, start: Option<u64>) -> Value {
        match action {
            "kill" => {
                let Some(pid) = target.and_then(Value::as_u64).map(|x| x as u32) else {
                    return json!({ "ok": false, "message": "Invalid PID" });
                };
                let snap = self.snapshot();
                let offered = snap.dev_servers.iter().any(|d| d.pid == pid)
                    || snap
                        .claude
                        .iter()
                        .any(|c| c.get("pid").and_then(Value::as_u64) == Some(pid as u64));
                if !offered {
                    return json!({ "ok": false, "message": "Target not offered" });
                }
                let procs = self.platform.processes();
                if protected_pids(&procs).contains(&pid) {
                    return json!({ "ok": false, "message": "Protected process" });
                }
                if let Some(cur) = procs.procs.get(&pid)
                    && let Some(st) = start
                    && cur.start.abs_diff(st) > 3
                {
                    return json!({ "ok": false, "message": "Process restarted; refresh" });
                }
                let ok = self.platform.terminate_pid(pid, force);
                json!({ "ok": ok, "message": if ok { "Stopped" } else { "Could not stop" } })
            }
            "clean" => {
                let Some(id) = target.and_then(Value::as_str) else {
                    return json!({ "ok": false, "message": "Missing cleanup id" });
                };
                let list = self.scan_cleanups();
                let Some(item) = list.into_iter().find(|c| c.id == id) else {
                    return json!({ "ok": false, "message": "Cleanup item not found" });
                };
                for p in &item.paths {
                    let _ = std::fs::remove_dir_all(p);
                }
                json!({ "ok": true, "message": "Cleaned", "freed": item.size })
            }
            _ => json!({ "ok": false, "message": "Unknown system action" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::default_platform;

    #[test]
    fn system_monitor_snapshot_and_dev_label_redaction() {
        let dir = tempfile::tempdir().unwrap();
        let mon = SystemMonitor::new(default_platform(), dir.path().to_path_buf());
        mon.track_owners();
        let snap = mon.snapshot();
        assert!(snap.mem.total > 0);
        assert!(snap.at > 0);

        let fake_proc = ProcRow {
            pid: 99999,
            ppid: 1,
            pgid: 99999,
            uid: 1,
            rss: 1024,
            mem: 1024,
            start: 100,
            up: 10,
            cpu_sec: 0.0,
            cpu_pct: 0.0,
            comm: "node.exe".into(),
            args: "node sk-1234567890abcdef.js".into(),
        };
        let label = redact(&dev_label(&fake_proc));
        assert!(!label.contains("sk-1234567890abcdef"), "label leaked secret: {label}");
    }
}
