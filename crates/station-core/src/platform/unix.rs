use super::{DiskStats, MemStats, ProcRow, ProcSnapshot, kids_of};
use std::collections::{HashMap, HashSet};

/// Parses `ps` elapsed time (`DD-HH:MM:SS`, `HH:MM:SS`, or `MM:SS`) into seconds.
pub fn parse_ps_etime(raw: &str) -> u64 {
    let s = raw.trim();
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().unwrap_or(0), r),
        None => (0, s),
    };
    let parts: Vec<u64> = rest
        .split(':')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    let secs = match parts.as_slice() {
        [h, m, sec] => h * 3600 + m * 60 + sec,
        [m, sec] => m * 60 + sec,
        [sec] => *sec,
        _ => 0,
    };
    days * 86_400 + secs
}

/// Parses `ps` cumulative CPU time (`DD-HH:MM:SS`, `HH:MM:SS.cc`, or `MM:SS.cc`) into seconds.
pub fn parse_ps_cpu_time(raw: &str) -> f64 {
    let s = raw.trim();
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().unwrap_or(0.0), r),
        None => (0.0, s),
    };
    let parts: Vec<f64> = rest
        .split(':')
        .map(|p| p.parse::<f64>().unwrap_or(0.0))
        .collect();
    let secs = match parts.as_slice() {
        [h, m, sec] => h * 3600.0 + m * 60.0 + sec,
        [m, sec] => m * 60.0 + sec,
        [sec] => *sec,
        _ => 0.0,
    };
    days * 86_400.0 + secs
}

/// Parses `ps -axo pid=,ppid=,pgid=,uid=,rss=,etime=,time=,comm=,args=` output into a `ProcSnapshot`.
pub fn parse_ps_snapshot(output: &str, now_sec: u64) -> ProcSnapshot {
    let mut procs = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Robustly consume 8 whitespace-delimited columns, leaving column 9 (args) intact.
        let mut cols = Vec::with_capacity(8);
        let mut rest = trimmed;
        for _ in 0..8 {
            rest = rest.trim_start();
            let Some(idx) = rest.find(char::is_whitespace) else {
                cols.clear();
                break;
            };
            cols.push(&rest[..idx]);
            rest = &rest[idx..];
        }
        if cols.len() < 8 {
            continue;
        }
        let Ok(pid) = cols[0].parse::<u32>() else {
            continue;
        };
        let ppid = cols[1].parse::<u32>().unwrap_or(0);
        let pgid = cols[2].parse::<u32>().unwrap_or(0);
        let uid = cols[3].parse::<u32>().unwrap_or(0);
        let rss_kb = cols[4].parse::<u64>().unwrap_or(0);
        let rss = rss_kb.saturating_mul(1024);
        let up = parse_ps_etime(cols[5]);
        let cpu_sec = parse_ps_cpu_time(cols[6]);
        let comm = cols[7].to_owned();
        let args = rest.trim_start().to_owned();
        let start = now_sec.saturating_sub(up);
        procs.insert(
            pid,
            ProcRow {
                pid,
                ppid,
                pgid,
                uid,
                rss,
                mem: rss,
                start,
                up,
                cpu_sec,
                cpu_pct: 0.0,
                comm,
                args,
            },
        );
    }
    let kids = kids_of(&procs);
    ProcSnapshot { procs, kids }
}

/// Parses Linux `/proc/meminfo` into `MemStats`.
pub fn parse_linux_meminfo(raw: &str) -> MemStats {
    let mut kv: HashMap<&str, u64> = HashMap::new();
    for line in raw.lines() {
        if let Some((k, rest)) = line.split_once(':') {
            let val_kb = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0);
            kv.insert(k.trim(), val_kb.saturating_mul(1024));
        }
    }
    let total = *kv.get("MemTotal").unwrap_or(&0);
    let avail = kv
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| *kv.get("MemFree").unwrap_or(&0));
    let used = total.saturating_sub(avail);
    let swap_total = *kv.get("SwapTotal").unwrap_or(&0);
    let swap_free = *kv.get("SwapFree").unwrap_or(&0);
    let swap_used = swap_total.saturating_sub(swap_free);
    let pct = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let pressure = if pct >= 90.0 {
        "critical"
    } else if pct >= 75.0 {
        "warn"
    } else {
        "normal"
    };
    MemStats {
        total,
        used,
        compressed: 0,
        swap_total,
        swap_used,
        pressure: pressure.to_owned(),
    }
}

/// Parses macOS `vm_stat`, `sysctl -n hw.memsize`, and `sysctl -n vm.swapusage` into `MemStats`.
pub fn parse_darwin_vm_stat(vm_stat: &str, hw_memsize: u64, swap_output: &str) -> MemStats {
    let mut page_size: u64 = 4096;
    let mut pages_active: u64 = 0;
    let mut pages_wired: u64 = 0;
    let mut pages_compressed: u64 = 0;

    for line in vm_stat.lines() {
        if let Some(pos) = line.find("page size of ") {
            let rest = &line[pos + "page size of ".len()..];
            if let Some(n) = rest.split_whitespace().next().and_then(|s| s.parse::<u64>().ok()) {
                page_size = n;
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let count = v
            .trim()
            .trim_end_matches('.')
            .parse::<u64>()
            .unwrap_or(0);
        match k.trim() {
            "Pages active" => pages_active = count,
            "Pages wired down" => pages_wired = count,
            "Pages occupied by compressor" => pages_compressed = count,
            _ => {}
        }
    }

    let compressed = pages_compressed.saturating_mul(page_size);
    let used = pages_active
        .saturating_add(pages_wired)
        .saturating_add(pages_compressed)
        .saturating_mul(page_size)
        .min(hw_memsize);

    let parse_mb_field = |label: &str| -> u64 {
        let Some(pos) = swap_output.find(label) else {
            return 0;
        };
        let after = swap_output[pos + label.len()..].trim_start();
        let token = after.split_whitespace().next().unwrap_or("");
        let (num_str, mult) = if let Some(n) = token.strip_suffix('G') {
            (n, 1024.0 * 1024.0 * 1024.0)
        } else if let Some(n) = token.strip_suffix('M') {
            (n, 1024.0 * 1024.0)
        } else if let Some(n) = token.strip_suffix('K') {
            (n, 1024.0)
        } else {
            (token, 1.0)
        };
        (num_str.parse::<f64>().unwrap_or(0.0) * mult) as u64
    };
    let swap_total = parse_mb_field("total =");
    let swap_used = parse_mb_field("used =");
    let pct = if hw_memsize > 0 {
        (used as f64 / hw_memsize as f64) * 100.0
    } else {
        0.0
    };
    let pressure = if pct >= 90.0 {
        "critical"
    } else if pct >= 75.0 {
        "warn"
    } else {
        "normal"
    };
    MemStats {
        total: hw_memsize,
        used,
        compressed,
        swap_total,
        swap_used,
        pressure: pressure.to_owned(),
    }
}

/// Parses POSIX `df -Pk <path>` output into `DiskStats`.
pub fn parse_df_pk(output: &str) -> DiskStats {
    let Some(data_line) = output.lines().nth(1) else {
        return DiskStats {
            total: 0,
            free: 0,
            used: 0,
        };
    };
    let cols: Vec<&str> = data_line.split_whitespace().collect();
    if cols.len() < 4 {
        return DiskStats {
            total: 0,
            free: 0,
            used: 0,
        };
    }
    let total = cols[1].parse::<u64>().unwrap_or(0).saturating_mul(1024);
    let used = cols[2].parse::<u64>().unwrap_or(0).saturating_mul(1024);
    let free = cols[3].parse::<u64>().unwrap_or(0).saturating_mul(1024);
    DiskStats { total, free, used }
}

/// Parses `lsof -nP -iTCP -sTCP:LISTEN -Fpn` output into `pid -> listening ports`.
pub fn parse_lsof_tcp_listeners(output: &str) -> HashMap<u32, HashSet<u16>> {
    let mut out: HashMap<u32, HashSet<u16>> = HashMap::new();
    let mut cur_pid: Option<u32> = None;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            cur_pid = rest.trim().parse::<u32>().ok();
        } else if let Some(rest) = line.strip_prefix('n')
            && let Some(pid) = cur_pid
            && let Some(port_str) = rest.rsplit(':').next()
            && let Ok(port) = port_str.trim().parse::<u16>()
        {
            out.entry(pid).or_default().insert(port);
        }
    }
    out
}

/// Parses `lsof -nP -iTCP -sTCP:ESTABLISHED -Fpn` output into `local_port -> pid`.
pub fn parse_lsof_established_local_ports(output: &str) -> HashMap<u16, u32> {
    let mut out: HashMap<u16, u32> = HashMap::new();
    let mut cur_pid: Option<u32> = None;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            cur_pid = rest.trim().parse::<u32>().ok();
        } else if let Some(rest) = line.strip_prefix('n')
            && let Some(pid) = cur_pid
            && let Some((local, _remote)) = rest.split_once("->")
            && let Some(port_str) = local.rsplit(':').next()
            && let Ok(port) = port_str.trim().parse::<u16>()
        {
            out.entry(port).or_insert(pid);
        }
    }
    out
}

/// Parses Linux `ss -H -ltnp` output into `pid -> listening ports`.
pub fn parse_ss_listeners(output: &str) -> HashMap<u32, HashSet<u16>> {
    let mut out: HashMap<u32, HashSet<u16>> = HashMap::new();
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let Some(port) = cols[3].rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) else {
            continue;
        };
        let users = cols[5..].join(" ");
        let mut cursor = users.as_str();
        while let Some(pos) = cursor.find("pid=") {
            let after = &cursor[pos + 4..];
            let pid_str: String = after.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(pid) = pid_str.parse::<u32>() {
                out.entry(pid).or_default().insert(port);
            }
            cursor = &after[pid_str.len()..];
        }
    }
    out
}

#[cfg(unix)]
fn run_cmd_stdout(bin: &str, args: &[&str]) -> String {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

#[cfg(unix)]
fn unix_now_sec() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(unix)]
fn unix_terminate_pid(pid: u32, force: bool) -> bool {
    let sig = if force { "-KILL" } else { "-TERM" };
    std::process::Command::new("kill")
        .args([sig, &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn unix_is_pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(target_os = "macos")]
pub struct DarwinPlatform;

#[cfg(target_os = "macos")]
impl super::Platform for DarwinPlatform {
    fn name(&self) -> &'static str {
        "macOS"
    }

    fn processes(&self) -> ProcSnapshot {
        let out = run_cmd_stdout(
            "ps",
            &["-axo", "pid=,ppid=,pgid=,uid=,rss=,etime=,time=,comm=,args="],
        );
        parse_ps_snapshot(&out, unix_now_sec())
    }

    fn lite_procs(&self) -> HashMap<u32, ProcRow> {
        self.processes().procs
    }

    fn memory(&self) -> MemStats {
        let hw = run_cmd_stdout("sysctl", &["-n", "hw.memsize"])
            .trim()
            .parse::<u64>()
            .unwrap_or(0);
        let vm = run_cmd_stdout("vm_stat", &[]);
        let swap = run_cmd_stdout("sysctl", &["-n", "vm.swapusage"]);
        parse_darwin_vm_stat(&vm, hw, &swap)
    }

    fn disk(&self) -> DiskStats {
        parse_df_pk(&run_cmd_stdout("df", &["-Pk", "/"]))
    }

    fn listeners(&self) -> HashMap<u32, HashSet<u16>> {
        parse_lsof_tcp_listeners(&run_cmd_stdout(
            "lsof",
            &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"],
        ))
    }

    fn established(&self) -> HashMap<u16, u32> {
        parse_lsof_established_local_ports(&run_cmd_stdout(
            "lsof",
            &["-nP", "-iTCP", "-sTCP:ESTABLISHED", "-Fpn"],
        ))
    }

    fn idle_seconds(&self) -> Option<f64> {
        let out = run_cmd_stdout("ioreg", &["-c", "IOHIDSystem"]);
        for line in out.lines() {
            if let Some(pos) = line.find("\"HIDIdleTime\" = ") {
                let ns_str: String = line[pos + 16..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if let Ok(ns) = ns_str.parse::<u64>() {
                    return Some(ns as f64 / 1e9);
                }
            }
        }
        None
    }

    fn terminate_pid(&self, pid: u32, force: bool) -> bool {
        unix_terminate_pid(pid, force)
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        unix_is_pid_alive(pid)
    }
}

#[cfg(target_os = "linux")]
pub struct LinuxPlatform;

#[cfg(target_os = "linux")]
impl super::Platform for LinuxPlatform {
    fn name(&self) -> &'static str {
        "Linux"
    }

    fn processes(&self) -> ProcSnapshot {
        let out = run_cmd_stdout(
            "ps",
            &["-axo", "pid=,ppid=,pgid=,uid=,rss=,etime=,time=,comm=,args="],
        );
        parse_ps_snapshot(&out, unix_now_sec())
    }

    fn lite_procs(&self) -> HashMap<u32, ProcRow> {
        self.processes().procs
    }

    fn memory(&self) -> MemStats {
        let raw = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        parse_linux_meminfo(&raw)
    }

    fn disk(&self) -> DiskStats {
        parse_df_pk(&run_cmd_stdout("df", &["-Pk", "/"]))
    }

    fn listeners(&self) -> HashMap<u32, HashSet<u16>> {
        let ss_out = run_cmd_stdout("ss", &["-H", "-ltnp"]);
        if !ss_out.trim().is_empty() {
            return parse_ss_listeners(&ss_out);
        }
        parse_lsof_tcp_listeners(&run_cmd_stdout(
            "lsof",
            &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"],
        ))
    }

    fn established(&self) -> HashMap<u16, u32> {
        parse_lsof_established_local_ports(&run_cmd_stdout(
            "lsof",
            &["-nP", "-iTCP", "-sTCP:ESTABLISHED", "-Fpn"],
        ))
    }

    fn idle_seconds(&self) -> Option<f64> {
        let out = run_cmd_stdout("xprintidle", &[]);
        out.trim().parse::<u64>().ok().map(|ms| ms as f64 / 1000.0)
    }

    fn terminate_pid(&self, pid: u32, force: bool) -> bool {
        unix_terminate_pid(pid, force)
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists() || unix_is_pid_alive(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unix_ps_meminfo_vm_stat_df_lsof_and_ss_outputs() {
        assert_eq!(parse_ps_etime("02-01:30:10"), 2 * 86_400 + 3600 + 1800 + 10);
        assert_eq!(parse_ps_etime("05:12"), 312);
        assert!((parse_ps_cpu_time("01:02:03.50") - 3723.5).abs() < 1e-6);

        let ps_out = "  100     1   100   501 20480  01:10  00:00.50 node node /app/server.mjs\n\
                       200   100   100   501 40960  00:30  00:01.25 claude claude --resume abc\n";
        let snap = parse_ps_snapshot(ps_out, 1_700_000_000);
        assert_eq!(snap.procs.len(), 2);
        assert_eq!(snap.procs[&200].ppid, 100);
        assert_eq!(snap.procs[&200].rss, 40960 * 1024);
        assert_eq!(snap.procs[&200].args, "claude --resume abc");
        assert_eq!(snap.kids[&100], vec![200]);

        let meminfo = "MemTotal:       16384000 kB\nMemFree:         2048000 kB\nMemAvailable:    4096000 kB\nSwapTotal:       2097152 kB\nSwapFree:        1048576 kB\n";
        let lmem = parse_linux_meminfo(meminfo);
        assert_eq!(lmem.total, 16_384_000 * 1024);
        assert_eq!(lmem.used, 12_288_000 * 1024);
        assert_eq!(lmem.swap_used, 1_048_576 * 1024);
        assert_eq!(lmem.pressure, "warn");

        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages active: 100000.\nPages wired down: 50000.\nPages occupied by compressor: 25000.\n";
        let swap = "total = 2048.00M  used = 512.00M  free = 1536.00M";
        let dmem = parse_darwin_vm_stat(vm, 16 * 1024 * 1024 * 1024, swap);
        assert_eq!(dmem.compressed, 25_000 * 16_384);
        assert_eq!(dmem.used, 175_000 * 16_384);
        assert_eq!(dmem.swap_total, 2048 * 1024 * 1024);
        assert_eq!(dmem.swap_used, 512 * 1024 * 1024);

        let df = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk3 1000000 400000 600000 40% /\n";
        let disk = parse_df_pk(df);
        assert_eq!(disk.total, 1_000_000 * 1024);
        assert_eq!(disk.used, 400_000 * 1024);
        assert_eq!(disk.free, 600_000 * 1024);

        let lsof_listen = "p1234\nn127.0.0.1:4310\nn[::1]:8080\np5678\nn*:3000\n";
        let listeners = parse_lsof_tcp_listeners(lsof_listen);
        assert!(listeners[&1234].contains(&4310));
        assert!(listeners[&1234].contains(&8080));
        assert!(listeners[&5678].contains(&3000));

        let lsof_est = "p2222\nn127.0.0.1:54321->127.0.0.1:4310\n";
        let est = parse_lsof_established_local_ports(lsof_est);
        assert_eq!(est.get(&54321), Some(&2222));

        let ss_out = "LISTEN 0 128 127.0.0.1:4310 0.0.0.0:* users:((\"agents-station\",pid=9001,fd=7))\n";
        let ss_map = parse_ss_listeners(ss_out);
        assert!(ss_map[&9001].contains(&4310));
    }
}
