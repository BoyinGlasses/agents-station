use super::{DiskStats, MemStats, Platform, ProcRow, ProcSnapshot};
use std::collections::{HashMap, HashSet};

pub struct StubPlatform;

impl Platform for StubPlatform {
    fn name(&self) -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "macOS"
        }
        #[cfg(target_os = "linux")]
        {
            "Linux"
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            "Stub"
        }
    }

    fn processes(&self) -> ProcSnapshot {
        ProcSnapshot::default()
    }

    fn lite_procs(&self) -> HashMap<u32, ProcRow> {
        HashMap::new()
    }

    fn memory(&self) -> MemStats {
        MemStats {
            total: 0,
            used: 0,
            compressed: 0,
            swap_total: 0,
            swap_used: 0,
            pressure: "normal".to_owned(),
        }
    }

    fn disk(&self) -> DiskStats {
        DiskStats {
            total: 0,
            free: 0,
            used: 0,
        }
    }

    fn listeners(&self) -> HashMap<u32, HashSet<u16>> {
        HashMap::new()
    }

    fn established(&self) -> HashMap<u16, u32> {
        HashMap::new()
    }

    fn idle_seconds(&self) -> Option<f64> {
        None
    }

    fn terminate_pid(&self, _pid: u32, _force: bool) -> bool {
        false
    }

    fn is_pid_alive(&self, _pid: u32) -> bool {
        false
    }
}
