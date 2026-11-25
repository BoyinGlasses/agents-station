use crate::catalog::LaunchTarget;
use anyhow::{Result, bail};
use std::collections::HashMap;

pub const SW_RESTORE_CMD: i32 = 9;
pub const WS_EX_TOOLWINDOW_FLAG: u32 = 0x0000_0080;

pub fn lookup_running_window(
    snapshot: &HashMap<String, (u32, isize)>,
    process_names: &[String],
) -> Option<(u32, isize)> {
    process_names
        .iter()
        .find_map(|name| snapshot.get(&name.to_ascii_lowercase()).copied())
}

pub(crate) fn is_eligible_top_level_window(visible: bool, has_owner: bool, ex_style: u32) -> bool {
    visible && !has_owner && (ex_style & WS_EX_TOOLWINDOW_FLAG) == 0
}

pub(crate) fn focus_window_with<I, S, F>(
    hwnd: isize,
    is_iconic: I,
    mut show_window: S,
    mut set_foreground: F,
) where
    I: FnOnce(isize) -> bool,
    S: FnMut(isize, i32),
    F: FnMut(isize),
{
    // Only restore minimized (iconic) windows so maximized IDE windows stay maximized.
    if is_iconic(hwnd) {
        show_window(hwnd, SW_RESTORE_CMD);
    }
    set_foreground(hwnd);
}

pub trait WindowLauncher: Send + Sync {
    fn snapshot_running_windows(&self) -> HashMap<String, (u32, isize)>;

    fn find_running_window(&self, process_names: &[String]) -> Option<(u32, isize)> {
        if process_names.is_empty() {
            return None;
        }
        let running = self.snapshot_running_windows();
        lookup_running_window(&running, process_names)
    }

    fn focus_window(&self, hwnd: isize) -> Result<()>;
    fn launch_target(&self, target: &LaunchTarget) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Win32WindowLauncher;

#[cfg(windows)]
mod win32_impl {
    use super::*;
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
        WS_EX_TOOLWINDOW,
    };

    const _: () = assert!(SW_RESTORE_CMD == SW_RESTORE);
    const _: () = assert!(WS_EX_TOOLWINDOW_FLAG == WS_EX_TOOLWINDOW);

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct EnumState {
        pid_to_name: HashMap<u32, String>,
        running: HashMap<String, (u32, isize)>,
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam as *mut EnumState) };
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        if !visible {
            return 1;
        }
        let has_owner = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        if !is_eligible_top_level_window(visible, has_owner, ex_style) {
            return 1;
        }
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if let Some(name) = state.pid_to_name.get(&pid) {
            state
                .running
                .entry(name.clone())
                .or_insert((pid, hwnd as isize));
        }
        1
    }

    impl WindowLauncher for Win32WindowLauncher {
        fn snapshot_running_windows(&self) -> HashMap<String, (u32, isize)> {
            let mut sys = System::new();
            sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing(),
            );
            let mut pid_to_name = HashMap::with_capacity(sys.processes().len());
            for (pid, proc_) in sys.processes() {
                let name = proc_.name().to_string_lossy().to_ascii_lowercase();
                if !name.is_empty() {
                    pid_to_name.insert(pid.as_u32(), name);
                }
            }
            if pid_to_name.is_empty() {
                return HashMap::new();
            }
            let mut state = EnumState {
                pid_to_name,
                running: HashMap::new(),
            };
            unsafe {
                EnumWindows(Some(enum_cb), (&mut state as *mut EnumState) as LPARAM);
            }
            state.running
        }

        fn focus_window(&self, hwnd: isize) -> Result<()> {
            focus_window_with(
                hwnd,
                |h| unsafe { IsIconic(h as HWND) != 0 },
                |h, cmd| unsafe {
                    ShowWindow(h as HWND, cmd);
                },
                |h| unsafe {
                    SetForegroundWindow(h as HWND);
                },
            );
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
    fn snapshot_running_windows(&self) -> HashMap<String, (u32, isize)> {
        HashMap::new()
    }

    fn focus_window(&self, _hwnd: isize) -> Result<()> {
        Ok(())
    }

    fn launch_target(&self, target: &LaunchTarget) -> Result<()> {
        bail!("Win32WindowLauncher is only available on Windows ({target:?})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_window_skips_sw_restore_for_non_minimized_window() {
        let mut restore_calls: Vec<(isize, i32)> = Vec::new();
        let mut fg_calls: Vec<isize> = Vec::new();

        focus_window_with(
            0x1234,
            |_hwnd| false,
            |hwnd, cmd| restore_calls.push((hwnd, cmd)),
            |hwnd| fg_calls.push(hwnd),
        );

        assert!(
            restore_calls.is_empty(),
            "non-minimized (including maximized) window must NOT receive SW_RESTORE"
        );
        assert_eq!(fg_calls, vec![0x1234]);
    }

    #[test]
    fn focus_window_calls_sw_restore_for_minimized_window() {
        let mut restore_calls: Vec<(isize, i32)> = Vec::new();
        let mut fg_calls: Vec<isize> = Vec::new();

        focus_window_with(
            0x5678,
            |_hwnd| true,
            |hwnd, cmd| restore_calls.push((hwnd, cmd)),
            |hwnd| fg_calls.push(hwnd),
        );

        assert_eq!(restore_calls, vec![(0x5678, SW_RESTORE_CMD)]);
        assert_eq!(fg_calls, vec![0x5678]);
    }

    #[test]
    fn top_level_window_filter_skips_invisible_owned_and_tool_windows() {
        assert!(is_eligible_top_level_window(true, false, 0));
        assert!(!is_eligible_top_level_window(false, false, 0));
        assert!(!is_eligible_top_level_window(true, true, 0));
        assert!(!is_eligible_top_level_window(true, false, WS_EX_TOOLWINDOW_FLAG));
    }
}
