pub mod stubs;
pub mod unix;
#[cfg(windows)]
pub mod win32;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcRow {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub uid: u32,
    pub rss: u64,
    pub mem: u64,
    pub start: u64,
    pub up: u64,
    pub cpu_sec: f64,
    pub cpu_pct: f32,
    pub comm: String,
    pub args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemStats {
    pub total: u64,
    pub used: u64,
    pub compressed: u64,
    #[serde(rename = "swapTotal")]
    pub swap_total: u64,
    #[serde(rename = "swapUsed")]
    pub swap_used: u64,
    pub pressure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStats {
    pub total: u64,
    pub free: u64,
    pub used: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ProcSnapshot {
    pub procs: HashMap<u32, ProcRow>,
    pub kids: HashMap<u32, Vec<u32>>,
}

pub fn kids_of(procs: &HashMap<u32, ProcRow>) -> HashMap<u32, Vec<u32>> {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs.values() {
        if p.ppid != 0 && p.ppid != p.pid {
            kids.entry(p.ppid).or_default().push(p.pid);
        }
    }
    kids
}

pub fn folder_bytes_walk(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file()
                && let Ok(meta) = entry.metadata()
            {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

pub fn read_claude_login(claude_dir: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(claude_dir.join(".credentials.json")).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    v.get("claudeAiOauth").cloned().filter(|x| !x.is_null())
}

pub trait Platform: Send + Sync {
    fn name(&self) -> &'static str;
    fn processes(&self) -> ProcSnapshot;
    fn lite_procs(&self) -> HashMap<u32, ProcRow>;
    fn memory(&self) -> MemStats;
    fn disk(&self) -> DiskStats;
    fn listeners(&self) -> HashMap<u32, HashSet<u16>>;
    fn established(&self) -> HashMap<u16, u32>;
    fn idle_seconds(&self) -> Option<f64>;
    fn folder_bytes(&self, path: &Path) -> u64 {
        folder_bytes_walk(path)
    }
    fn claude_login(&self, claude_dir: &Path) -> Option<Value> {
        read_claude_login(claude_dir)
    }
    fn terminate_pid(&self, pid: u32, force: bool) -> bool;
    fn is_pid_alive(&self, pid: u32) -> bool;
}

pub fn default_platform() -> Arc<dyn Platform> {
    #[cfg(windows)]
    {
        Arc::new(win32::Win32Platform::new())
    }
    #[cfg(target_os = "macos")]
    {
        Arc::new(unix::DarwinPlatform)
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(unix::LinuxPlatform)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Arc::new(stubs::StubPlatform)
    }
}
