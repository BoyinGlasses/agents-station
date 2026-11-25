use bevy::prelude::*;

#[derive(Resource, Debug, Default)]
pub struct TrayState {
    pub in_tray: bool,
    pub icon_installed: bool,
    pub notifications_sent: u32,
}

pub struct TrayPlugin;

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TrayState::default());
    }
}

/// Zero-PowerShell Win32 Shell_NotifyIconW tooltip/toast helper.
#[cfg(windows)]
pub fn show_native_tray_toast(title: &str, body: &str) -> bool {
    use windows_sys::Win32::UI::Shell::{
        NIF_INFO, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    fn copy_wide(dst: &mut [u16], src: &str) {
        let max = dst.len().saturating_sub(1);
        for (i, ch) in src.encode_utf16().take(max).enumerate() {
            dst[i] = ch;
        }
    }

    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = GetForegroundWindow();
        nid.uID = 0x4310;
        nid.uFlags = NIF_TIP | NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut nid.szTip, "Agents Station (127.0.0.1:4310)");
        copy_wide(&mut nid.szInfoTitle, title);
        copy_wide(&mut nid.szInfo, body);

        let added = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        if added {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        added
    }
}

#[cfg(not(windows))]
pub fn show_native_tray_toast(_title: &str, _body: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_state_defaults_cleanly_without_spawning_shells() {
        let st = TrayState::default();
        assert!(!st.in_tray);
        assert!(!st.icon_installed);
        assert_eq!(st.notifications_sent, 0);
    }
}
