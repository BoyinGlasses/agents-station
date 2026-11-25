use crate::catalog::LaunchTarget;
use anyhow::{Result, bail};

pub trait WindowLauncher: Send + Sync {
    fn find_running_window(&self, process_names: &[String]) -> Option<(u32, isize)>;
    fn focus_window(&self, hwnd: isize) -> Result<()>;
    fn launch_target(&self, target: &LaunchTarget) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Win32WindowLauncher;

#[cfg(windows)]
mod win32_impl {
    use super::*;
    use std::collections::HashSet;
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SW_SHOWNORMAL,
        SetForegroundWindow, ShowWindow,
    };

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct EnumState {
        pids: HashSet<u32>,
        found: Option<(u32, isize)>,
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam as *mut EnumState) };
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if state.pids.contains(&pid) {
            state.found = Some((pid, hwnd as isize));
            return 0;
        }
        1
    }

    impl WindowLauncher for Win32WindowLauncher {
        fn find_running_window(&self, process_names: &[String]) -> Option<(u32, isize)> {
            if process_names.is_empty() {
                return None;
            }
            let targets: HashSet<String> = process_names
                .iter()
                .map(|n| n.to_ascii_lowercase())
                .collect();
            let mut sys = System::new();
            sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing(),
            );
            let mut pids = HashSet::new();
            for (pid, proc_) in sys.processes() {
                let name = proc_.name().to_string_lossy().to_ascii_lowercase();
                if targets.contains(&name) {
                    pids.insert(pid.as_u32());
                }
            }
            if pids.is_empty() {
                return None;
            }
            let mut state = EnumState { pids, found: None };
            unsafe {
                EnumWindows(Some(enum_cb), (&mut state as *mut EnumState) as LPARAM);
            }
            state.found
        }

        fn focus_window(&self, hwnd: isize) -> Result<()> {
            let h = hwnd as HWND;
            unsafe {
                ShowWindow(h, SW_RESTORE);
                SetForegroundWindow(h);
            }
            Ok(())
        }

        fn launch_target(&self, target: &LaunchTarget) -> Result<()> {
            let file_str = match target {
                LaunchTarget::Exe(p) | LaunchTarget::Shortcut(p) => {
                    p.to_string_lossy().into_owned()
                }
                LaunchTarget::MsixAumid { aumid, .. } => format!("shell:AppsFolder\\{aumid}"),
            };
            let verb = to_wide("open");
            let file = to_wide(&file_str);
            let hinst = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    file.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    SW_SHOWNORMAL,
                )
            };
            if (hinst as isize) <= 32 {
                bail!("ShellExecuteW failed for {file_str} (code {})", hinst as isize);
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
impl WindowLauncher for Win32WindowLauncher {
    fn find_running_window(&self, _process_names: &[String]) -> Option<(u32, isize)> {
        None
    }

    fn focus_window(&self, _hwnd: isize) -> Result<()> {
        Ok(())
    }

    fn launch_target(&self, target: &LaunchTarget) -> Result<()> {
        bail!("Win32WindowLauncher is only available on Windows ({target:?})")
    }
}
