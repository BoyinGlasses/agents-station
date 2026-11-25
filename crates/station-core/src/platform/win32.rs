use super::{DiskStats, MemStats, Platform, ProcRow, ProcSnapshot, kids_of};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

const MIB_TCP_STATE_LISTEN: u32 = 2;
const MIB_TCP_STATE_ESTAB: u32 = 5;
const STILL_ACTIVE: u32 = 259;
const CACHE_TTL: Duration = Duration::from_secs(2);

struct CachedReading {
    at: Instant,
    procs: HashMap<u32, ProcRow>,
    listeners: HashMap<u32, HashSet<u16>>,
    established: HashMap<u16, u32>,
}

struct Win32Inner {
    sys: System,
    cached: Option<CachedReading>,
}

pub struct Win32Platform {
    inner: Mutex<Win32Inner>,
}

impl Win32Platform {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Win32Inner {
                sys: System::new(),
                cached: None,
            }),
        }
    }

    fn refresh_if_stale(&self) -> (HashMap<u32, ProcRow>, HashMap<u32, HashSet<u16>>, HashMap<u16, u32>) {
        let mut guard = self.inner.lock().expect("Win32Platform lock poisoned");
        if let Some(c) = &guard.cached
            && c.at.elapsed() < CACHE_TTL
        {
            return (c.procs.clone(), c.listeners.clone(), c.established.clone());
        }

        guard.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        );

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut procs = HashMap::with_capacity(guard.sys.processes().len());
        for (pid_obj, proc_) in guard.sys.processes() {
            let pid = pid_obj.as_u32();
            let ppid = proc_.parent().map(|p| p.as_u32()).unwrap_or(0);
            let mem = proc_.memory();
            let start = proc_.start_time();
            let up = if start > 0 {
                now_sec.saturating_sub(start)
            } else {
                proc_.run_time()
            };
            let comm = proc_
                .exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| proc_.name().to_string_lossy().into_owned());
            let args = if proc_.cmd().is_empty() {
                proc_.name().to_string_lossy().into_owned()
            } else {
                proc_
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            procs.insert(
                pid,
                ProcRow {
                    pid,
                    ppid,
                    pgid: pid,
                    uid: 1,
                    rss: mem,
                    mem,
                    start,
                    up,
                    cpu_sec: (proc_.accumulated_cpu_time() as f64) / 1000.0,
                    cpu_pct: proc_.cpu_usage(),
                    comm,
                    args,
                },
            );
        }

        let (listeners, established) = read_tcp_tables();
        guard.cached = Some(CachedReading {
            at: Instant::now(),
            procs: procs.clone(),
            listeners: listeners.clone(),
            established: established.clone(),
        });
        (procs, listeners, established)
    }
}

impl Default for Win32Platform {
    fn default() -> Self {
        Self::new()
    }
}

fn read_tcp_tables() -> (HashMap<u32, HashSet<u16>>, HashMap<u16, u32>) {
    let mut listeners: HashMap<u32, HashSet<u16>> = HashMap::new();
    let mut established: HashMap<u16, u32> = HashMap::new();
    collect_tcp_v4(&mut listeners, &mut established);
    collect_tcp_v6(&mut listeners, &mut established);
    (listeners, established)
}

fn collect_tcp_v4(
    listeners: &mut HashMap<u32, HashSet<u16>>,
    established: &mut HashMap<u16, u32>,
) {
    let mut size: u32 = 0;
    // SAFETY: probing required buffer size with null pointer.
    let rc = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc != ERROR_INSUFFICIENT_BUFFER || size == 0 {
        return;
    }
    const _: () = assert!(
        std::mem::align_of::<MIB_TCPTABLE_OWNER_PID>() <= std::mem::align_of::<u32>()
    );
    let words = (size as usize).saturating_add(1024).div_ceil(size_of::<u32>());
    let mut buf = vec![0u32; words];
    let buf_bytes = buf.len() * size_of::<u32>();
    let Ok(mut cap) = u32::try_from(buf_bytes) else {
        return;
    };
    // SAFETY: 4-byte aligned buffer allocated with `cap` bytes.
    let rc2 = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut c_void,
            &mut cap,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc2 != NO_ERROR {
        return;
    }
    let table_ptr = buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
    let table_offset = std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table);
    // SAFETY: buf is 4-byte aligned Vec<u32> of >= 1024 bytes, populated by GetExtendedTcpTable.
    let count = unsafe { (*table_ptr).dwNumEntries } as usize;
    // SAFETY: raw pointer projection avoids narrowing provenance via a 1-row shared reference.
    let base_ptr = unsafe { std::ptr::addr_of!((*table_ptr).table).cast::<MIB_TCPROW_OWNER_PID>() };
    let max_rows = buf_bytes.saturating_sub(table_offset) / size_of::<MIB_TCPROW_OWNER_PID>();
    for i in 0..count.min(max_rows) {
        // SAFETY: bounded by count.min(max_rows) within buf.
        let row = unsafe { &*base_ptr.add(i) };
        let port = u16::from_be(row.dwLocalPort as u16);
        if port == 0 {
            continue;
        }
        if row.dwState == MIB_TCP_STATE_LISTEN && row.dwOwningPid != 0 {
            listeners.entry(row.dwOwningPid).or_default().insert(port);
        } else if row.dwState == MIB_TCP_STATE_ESTAB {
            *established.entry(port).or_insert(0) += 1;
        }
    }
}

fn collect_tcp_v6(
    listeners: &mut HashMap<u32, HashSet<u16>>,
    established: &mut HashMap<u16, u32>,
) {
    let mut size: u32 = 0;
    // SAFETY: probing required buffer size with null pointer.
    let rc = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET6 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc != ERROR_INSUFFICIENT_BUFFER || size == 0 {
        return;
    }
    const _: () = assert!(
        std::mem::align_of::<MIB_TCP6TABLE_OWNER_PID>() <= std::mem::align_of::<u32>()
    );
    let words = (size as usize).saturating_add(1024).div_ceil(size_of::<u32>());
    let mut buf = vec![0u32; words];
    let buf_bytes = buf.len() * size_of::<u32>();
    let Ok(mut cap) = u32::try_from(buf_bytes) else {
        return;
    };
    // SAFETY: 4-byte aligned buffer allocated with `cap` bytes.
    let rc2 = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut c_void,
            &mut cap,
            0,
            AF_INET6 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc2 != NO_ERROR {
        return;
    }
    let table_ptr = buf.as_ptr().cast::<MIB_TCP6TABLE_OWNER_PID>();
    let table_offset = std::mem::offset_of!(MIB_TCP6TABLE_OWNER_PID, table);
    // SAFETY: buf is 4-byte aligned Vec<u32> of >= 1024 bytes, populated by GetExtendedTcpTable.
    let count = unsafe { (*table_ptr).dwNumEntries } as usize;
    // SAFETY: raw pointer projection avoids narrowing provenance via a 1-row shared reference.
    let base_ptr = unsafe { std::ptr::addr_of!((*table_ptr).table).cast::<MIB_TCP6ROW_OWNER_PID>() };
    let max_rows = buf_bytes.saturating_sub(table_offset) / size_of::<MIB_TCP6ROW_OWNER_PID>();
    for i in 0..count.min(max_rows) {
        // SAFETY: bounded by count.min(max_rows) within buf.
        let row = unsafe { &*base_ptr.add(i) };
        let port = u16::from_be(row.dwLocalPort as u16);
        if port == 0 {
            continue;
        }
        if row.dwState == MIB_TCP_STATE_LISTEN && row.dwOwningPid != 0 {
            listeners.entry(row.dwOwningPid).or_default().insert(port);
        } else if row.dwState == MIB_TCP_STATE_ESTAB {
            *established.entry(port).or_insert(0) += 1;
        }
    }
}

impl Platform for Win32Platform {
    fn name(&self) -> &'static str {
        "Windows"
    }

    fn processes(&self) -> ProcSnapshot {
        let (procs, _, _) = self.refresh_if_stale();
        let kids = kids_of(&procs);
        ProcSnapshot { procs, kids }
    }

    fn lite_procs(&self) -> HashMap<u32, ProcRow> {
        self.refresh_if_stale().0
    }

    fn memory(&self) -> MemStats {
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: valid zeroed MEMORYSTATUSEX with dwLength set.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
        if ok == 0 {
            return MemStats {
                total: 0,
                used: 0,
                compressed: 0,
                swap_total: 0,
                swap_used: 0,
                pressure: "normal".to_owned(),
            };
        }
        let total = status.ullTotalPhys;
        let used = total.saturating_sub(status.ullAvailPhys);
        let left = if total > 0 {
            1.0 - (used as f64 / total as f64)
        } else {
            1.0
        };
        let pressure = if left < 0.08 {
            "critical"
        } else if left < 0.18 {
            "warn"
        } else {
            "normal"
        };
        let swap_total = status.ullTotalPageFile.saturating_sub(total);
        let commit_used = status.ullTotalPageFile.saturating_sub(status.ullAvailPageFile);
        let swap_used = commit_used.saturating_sub(used);
        MemStats {
            total,
            used,
            compressed: 0,
            swap_total,
            swap_used,
            pressure: pressure.to_owned(),
        }
    }

    fn disk(&self) -> DiskStats {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
        let wide: Vec<u16> = format!("{drive}\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut avail = 0u64;
        let mut total = 0u64;
        let mut free = 0u64;
        // SAFETY: valid NUL-terminated UTF-16 drive root and output pointers.
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
        if ok == 0 {
            return DiskStats {
                total: 0,
                free: 0,
                used: 0,
            };
        }
        DiskStats {
            total,
            free: avail,
            used: total.saturating_sub(avail),
        }
    }

    fn listeners(&self) -> HashMap<u32, HashSet<u16>> {
        // Fresh TCP read ensures newly bound sockets are observed immediately.
        read_tcp_tables().0
    }

    fn established(&self) -> HashMap<u16, u32> {
        read_tcp_tables().1
    }

    fn idle_seconds(&self) -> Option<f64> {
        let mut info = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: cbSize initialized to size_of::<LASTINPUTINFO>().
        let ok = unsafe { GetLastInputInfo(&mut info) };
        if ok == 0 {
            return None;
        }
        // SAFETY: GetTickCount takes no arguments.
        let now_tick = unsafe { GetTickCount() };
        let elapsed_ms = now_tick.wrapping_sub(info.dwTime);
        Some((elapsed_ms as f64) / 1000.0)
    }

    fn terminate_pid(&self, pid: u32, _force: bool) -> bool {
        // SAFETY: OpenProcess/TerminateProcess/CloseHandle with valid Win32 handle check.
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h.is_null() {
                return false;
            }
            let ok = TerminateProcess(h, 1) != 0;
            CloseHandle(h);
            ok
        }
    }

    fn is_pid_alive(&self, pid: u32) -> bool {
        // SAFETY: querying process exit status via PROCESS_QUERY_LIMITED_INFORMATION.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code: u32 = 0;
            let alive = GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE;
            CloseHandle(h);
            alive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn win32_platform_reads_live_memory_disk_procs_idle_and_tcp_listeners() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        let plat = Win32Platform::new();

        let mem = plat.memory();
        assert!(mem.total > 0, "expected total memory > 0");
        assert!(mem.used <= mem.total);

        let dsk = plat.disk();
        assert!(dsk.total > 0, "expected disk total > 0");

        let procs = plat.processes();
        let my_pid = std::process::id();
        assert!(procs.procs.contains_key(&my_pid), "current PID missing from processes()");

        let idle = plat.idle_seconds();
        assert!(idle.is_some_and(|s| s >= 0.0), "idle_seconds={idle:?}");

        let list = plat.listeners();
        let my_ports = list.get(&my_pid).expect("current PID must have listening ports");
        assert!(my_ports.contains(&port), "bound port {port} not found in {my_ports:?}");
        drop(listener);
    }

    #[test]
    fn win32_is_pid_alive_returns_false_for_exited_zombie_process() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let plat = Win32Platform::new();
        assert!(plat.is_pid_alive(std::process::id()));

        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("spawn short-lived child");
        let pid = child.id();
        let _ = child.wait().expect("wait for child exit");

        let alive = plat.is_pid_alive(pid);
        drop(child);
        assert!(!alive, "exited zombie PID {pid} must not be reported alive");
    }
}
