use crate::cli::GpuPreference;
use anyhow::Result;
use bevy::prelude::*;
use std::process::Child;

pub const TRAY_WINDOW_CLASS: &str = "AgentsStationTrayHost";
pub const TRAY_ICON_UID: u32 = 0x4310;
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
const WM_TRAY_CALLBACK: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;
#[cfg(windows)]
const WM_APP_SHOW_GUI: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;
#[cfg(windows)]
const WM_APP_QUIT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 3;

#[cfg(windows)]
const IDM_OPEN_3D: usize = 1001;
#[cfg(windows)]
const IDM_OPEN_2D: usize = 1002;
#[cfg(windows)]
const IDM_QUIT: usize = 1003;

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

#[derive(Debug, Clone)]
pub struct TrayWorkerConfig {
    pub host: String,
    pub port: u16,
    pub gpu: GpuPreference,
    pub default_lite: bool,
}

impl Default for TrayWorkerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 4310,
            gpu: GpuPreference::Auto,
            default_lite: false,
        }
    }
}

struct TrayController {
    cfg: TrayWorkerConfig,
    worker: Option<(Child, bool)>,
    last_lite: bool,
    #[cfg(windows)]
    taskbar_created_msg: u32,
    #[cfg(windows)]
    nid: Option<windows_sys::Win32::UI::Shell::NOTIFYICONDATAW>,
    #[cfg(windows)]
    icon_installed: bool,
}

impl TrayController {
    fn new(cfg: TrayWorkerConfig) -> Self {
        let last_lite = cfg.default_lite;
        Self {
            cfg,
            worker: None,
            last_lite,
            #[cfg(windows)]
            taskbar_created_msg: 0,
            #[cfg(windows)]
            nid: None,
            #[cfg(windows)]
            icon_installed: false,
        }
    }

    fn reap_exited_worker(&mut self) {
        if let Some((child, _)) = &mut self.worker {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    self.worker = None;
                }
                Ok(None) => {}
            }
        }
    }

    fn spawn_or_focus_worker(&mut self, requested_lite: Option<bool>) {
        self.reap_exited_worker();
        let target_lite = requested_lite.unwrap_or(self.last_lite);

        if let Some((child, current_lite)) = &mut self.worker {
            if requested_lite.is_none() || *current_lite == target_lite {
                #[cfg(windows)]
                focus_pid_window(child.id());
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
            self.worker = None;
        }

        if let Ok(child) = spawn_gui_worker(&self.cfg, target_lite) {
            self.last_lite = target_lite;
            self.worker = Some((child, target_lite));
        }
    }

    fn stop_worker(&mut self) {
        if let Some((mut child, _)) = self.worker.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn spawn_gui_worker(cfg: &TrayWorkerConfig, lite: bool) -> Result<Child> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--gui-worker")
        .arg("--host")
        .arg(&cfg.host)
        .arg("--port")
        .arg(cfg.port.to_string())
        .arg("--gpu")
        .arg(cfg.gpu.as_cli_arg());
    if lite {
        cmd.arg("--lite");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(cmd.spawn()?)
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn copy_wide(dst: &mut [u16], src: &str) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    for (i, ch) in src.encode_utf16().take(max).enumerate() {
        dst[i] = ch;
    }
}

#[cfg(windows)]
fn focus_pid_window(target_pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
        ShowWindow,
    };

    struct FindState {
        pid: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam as *mut FindState) };
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == state.pid {
            state.hwnd = hwnd;
            return 0;
        }
        1
    }

    let mut state = FindState {
        pid: target_pid,
        hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(enum_cb), (&mut state as *mut FindState) as LPARAM);
        if !state.hwnd.is_null() {
            ShowWindow(state.hwnd, SW_RESTORE);
            SetForegroundWindow(state.hwnd);
            return true;
        }
    }
    false
}

/// Builds a real 32x32 ARGB terracotta icon (`#D66440` badge with cream inner station dot)
/// via `CreateDIBSection` + `CreateIconIndirect`, freeing temporary GDI bitmaps immediately.
#[cfg(windows)]
pub fn create_terracotta_icon(size: i32) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_BITFIELDS, BITMAPINFO, BITMAPV5HEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
        DeleteObject, GetDC, ReleaseDC,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    let sz = size.clamp(16, 64);
    unsafe {
        let mut header: BITMAPV5HEADER = std::mem::zeroed();
        header.bV5Size = std::mem::size_of::<BITMAPV5HEADER>() as u32;
        header.bV5Width = sz;
        header.bV5Height = -sz; // top-down
        header.bV5Planes = 1;
        header.bV5BitCount = 32;
        header.bV5Compression = BI_BITFIELDS;
        header.bV5RedMask = 0x00FF_0000;
        header.bV5GreenMask = 0x0000_FF00;
        header.bV5BlueMask = 0x0000_00FF;
        header.bV5AlphaMask = 0xFF00_0000;

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hdc = GetDC(std::ptr::null_mut());
        let color_bmp = CreateDIBSection(
            hdc,
            (&header as *const BITMAPV5HEADER).cast::<BITMAPINFO>(),
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        ReleaseDC(std::ptr::null_mut(), hdc);
        if color_bmp.is_null() || bits.is_null() {
            if !color_bmp.is_null() {
                DeleteObject(color_bmp);
            }
            return None;
        }

        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (sz * sz) as usize);
        let center = (sz as f32 - 1.0) * 0.5;
        let outer_r = sz as f32 * 0.46;
        let inner_r = sz as f32 * 0.19;
        for y in 0..sz {
            for x in 0..sz {
                let dx = x as f32 - center;
                let dy = y as f32 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                let argb = if dist <= inner_r {
                    // Warm cream inner dot (#F5F0E8)
                    0xFF_F5_F0_E8u32
                } else if dist <= outer_r {
                    // Terracotta body (#D66440)
                    0xFF_D6_64_40u32
                } else if dist <= outer_r + 1.0 {
                    // Soft AA rim
                    let a = ((outer_r + 1.0 - dist).clamp(0.0, 1.0) * 255.0) as u32;
                    let r = (0xD6 * a) / 255;
                    let g = (0x64 * a) / 255;
                    let b = (0x40 * a) / 255;
                    (a << 24) | (r << 16) | (g << 8) | b
                } else {
                    0x00_00_00_00u32
                };
                pixels[(y * sz + x) as usize] = argb;
            }
        }

        let mask_bmp = CreateBitmap(sz, sz, 1, 1, std::ptr::null());
        if mask_bmp.is_null() {
            DeleteObject(color_bmp);
            return None;
        }

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let hicon = CreateIconIndirect(&info);
        DeleteObject(color_bmp);
        DeleteObject(mask_bmp);

        if hicon.is_null() {
            None
        } else {
            Some(hicon)
        }
    }
}

#[cfg(windows)]
unsafe fn show_tray_context_menu(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_SEPARATOR, MF_STRING,
        PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
        TrackPopupMenu, WM_NULL,
    };

    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        let open_3d = to_wide("Open 3D Office");
        let open_2d = to_wide("Open 2D Lite");
        let quit = to_wide("Quit");
        AppendMenuW(menu, MF_STRING, IDM_OPEN_3D, open_3d.as_ptr());
        AppendMenuW(menu, MF_STRING, IDM_OPEN_2D, open_2d.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, IDM_QUIT, quit.as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}

#[cfg(windows)]
unsafe extern "system" fn tray_wndproc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, PostQuitMessage, WM_COMMAND,
        WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    };

    unsafe {
        let ctrl_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayController;
        match msg {
            WM_TRAY_CALLBACK => {
                let mouse_msg = (lparam as u32) & 0xFFFF;
                if mouse_msg == WM_LBUTTONUP || mouse_msg == WM_LBUTTONDBLCLK {
                    if !ctrl_ptr.is_null() {
                        (*ctrl_ptr).spawn_or_focus_worker(None);
                    }
                    return 0;
                }
                if mouse_msg == WM_RBUTTONUP || mouse_msg == WM_CONTEXTMENU {
                    show_tray_context_menu(hwnd);
                    return 0;
                }
            }
            WM_COMMAND => {
                let cmd_id = wparam & 0xFFFF;
                match cmd_id {
                    IDM_OPEN_3D => {
                        if !ctrl_ptr.is_null() {
                            (*ctrl_ptr).spawn_or_focus_worker(Some(false));
                        }
                        return 0;
                    }
                    IDM_OPEN_2D => {
                        if !ctrl_ptr.is_null() {
                            (*ctrl_ptr).spawn_or_focus_worker(Some(true));
                        }
                        return 0;
                    }
                    IDM_QUIT => {
                        PostQuitMessage(0);
                        return 0;
                    }
                    _ => {}
                }
            }
            WM_APP_SHOW_GUI => {
                if !ctrl_ptr.is_null() {
                    (*ctrl_ptr).spawn_or_focus_worker(None);
                }
                return 0;
            }
            WM_APP_QUIT => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {
                if !ctrl_ptr.is_null() {
                    let ctrl = &mut *ctrl_ptr;
                    if ctrl.taskbar_created_msg != 0 && msg == ctrl.taskbar_created_msg {
                        if let Some(nid) = ctrl.nid.as_ref() {
                            use windows_sys::Win32::UI::Shell::{NIM_ADD, Shell_NotifyIconW};
                            if Shell_NotifyIconW(NIM_ADD, nid) != 0 {
                                ctrl.icon_installed = true;
                            }
                        }
                        return 0;
                    }
                }
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Returns `true` when Windows Explorer's taskbar (`Shell_TrayWnd`) is reachable in the
/// current desktop session.
pub fn has_shell_tray() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
        let cls = to_wide("Shell_TrayWnd");
        unsafe { !FindWindowW(cls.as_ptr(), std::ptr::null()).is_null() }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Persistent Win32 system tray daemon hosting a hidden message window
/// (`"AgentsStationTrayHost"`), a real 32x32 ARGB terracotta `HICON`, right-click popup menu,
/// and `--gui-worker` process lifecycle management.
pub struct TrayDaemon {
    #[cfg(windows)]
    hwnd: windows_sys::Win32::Foundation::HWND,
    #[cfg(windows)]
    hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    #[cfg(windows)]
    nid: windows_sys::Win32::UI::Shell::NOTIFYICONDATAW,
    deleted: bool,
    controller: Box<TrayController>,
}

impl TrayDaemon {
    pub fn new(port: u16, cfg: TrayWorkerConfig) -> Result<Self> {
        #[cfg(windows)]
        {
            use anyhow::anyhow;
            use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError};
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows_sys::Win32::UI::Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW, Shell_NotifyIconW,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyIcon, DestroyWindow, GWLP_USERDATA, RegisterClassW,
                RegisterWindowMessageW, SetWindowLongPtrW, WNDCLASSW, WS_OVERLAPPED,
            };

            let class_w = to_wide(TRAY_WINDOW_CLASS);
            let title_w = to_wide("Agents Station Tray Host");
            let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };

            let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
            wc.lpfnWndProc = Some(tray_wndproc);
            wc.hInstance = hinstance;
            wc.lpszClassName = class_w.as_ptr();

            let atom = unsafe { RegisterClassW(&wc) };
            if atom == 0 {
                let err = unsafe { GetLastError() };
                if err != ERROR_CLASS_ALREADY_EXISTS {
                    return Err(anyhow!("RegisterClassW failed with Win32 error {err}"));
                }
            }

            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class_w.as_ptr(),
                    title_w.as_ptr(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }

            let Some(hicon) = create_terracotta_icon(32) else {
                unsafe { DestroyWindow(hwnd) };
                return Err(anyhow!("CreateIconIndirect failed for terracotta tray icon"));
            };

            let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = TRAY_ICON_UID;
            nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY_CALLBACK;
            nid.hIcon = hicon;
            copy_wide(&mut nid.szTip, &format!("Agents Station (127.0.0.1:{port})"));

            let taskbar_w = to_wide("TaskbarCreated");
            let taskbar_created_msg = unsafe { RegisterWindowMessageW(taskbar_w.as_ptr()) };

            let mut controller = Box::new(TrayController::new(cfg));
            controller.taskbar_created_msg = taskbar_created_msg;
            controller.nid = Some(nid);

            let icon_installed = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) != 0 };
            if !icon_installed && has_shell_tray() {
                let err = unsafe { GetLastError() };
                unsafe {
                    DestroyWindow(hwnd);
                    DestroyIcon(hicon);
                }
                return Err(anyhow!("Shell_NotifyIconW(NIM_ADD) failed (Win32 err {err})"));
            }
            controller.icon_installed = icon_installed;

            let ctrl_ptr: *mut TrayController = &mut *controller;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctrl_ptr as isize);
            }

            Ok(Self {
                hwnd,
                hicon,
                nid,
                deleted: false,
                controller,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = port;
            Ok(Self {
                deleted: false,
                controller: Box::new(TrayController::new(cfg)),
            })
        }
    }

    pub fn hwnd_raw(&self) -> isize {
        #[cfg(windows)]
        {
            self.hwnd as isize
        }
        #[cfg(not(windows))]
        {
            0
        }
    }

    pub fn hicon_raw(&self) -> isize {
        #[cfg(windows)]
        {
            self.hicon as isize
        }
        #[cfg(not(windows))]
        {
            0
        }
    }

    pub fn nid_flags(&self) -> u32 {
        #[cfg(windows)]
        {
            self.nid.uFlags
        }
        #[cfg(not(windows))]
        {
            0
        }
    }

    pub fn icon_installed(&self) -> bool {
        #[cfg(windows)]
        {
            self.controller.icon_installed && !self.deleted
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn was_deleted(&self) -> bool {
        self.deleted
    }

    pub fn spawn_initial_worker(&mut self) {
        let lite = self.controller.cfg.default_lite;
        self.controller.spawn_or_focus_worker(Some(lite));
    }

    pub fn run_message_loop(&mut self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, MSG, TranslateMessage,
            };
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }

    pub fn cleanup(&mut self) {
        if self.deleted {
            return;
        }
        self.deleted = true;
        self.controller.stop_worker();

        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::Shell::{NIM_DELETE, Shell_NotifyIconW};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DestroyIcon, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
            };
            unsafe {
                if self.controller.icon_installed {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
                    self.controller.icon_installed = false;
                }
                if !self.hwnd.is_null() {
                    SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                    DestroyWindow(self.hwnd);
                    self.hwnd = std::ptr::null_mut();
                }
                if !self.hicon.is_null() {
                    DestroyIcon(self.hicon);
                    self.hicon = std::ptr::null_mut();
                }
            }
        }
    }
}

impl Drop for TrayDaemon {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Posts a wake message to a running `TrayDaemon`'s hidden message window.
pub fn post_show_gui(hwnd_raw: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
        if hwnd_raw == 0 {
            return false;
        }
        unsafe { PostMessageW(hwnd_raw as HWND, WM_APP_SHOW_GUI, 0, 0) != 0 }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd_raw;
        false
    }
}

/// Asks the running parent `"AgentsStationTrayHost"` window to quit (used when user closes
/// `--gui-worker` with `close_to_tray` disabled).
pub fn request_parent_quit() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
        let class_w = to_wide(TRAY_WINDOW_CLASS);
        unsafe {
            let hwnd = FindWindowW(class_w.as_ptr(), std::ptr::null());
            if hwnd.is_null() {
                return false;
            }
            PostMessageW(hwnd, WM_APP_QUIT, 0, 0) != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Sends a balloon toast via the existing persistent tray icon when running under `TrayDaemon`.
#[cfg(windows)]
pub fn show_native_tray_toast(title: &str, body: &str) -> bool {
    use windows_sys::Win32::UI::Shell::{
        NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let class_w = to_wide(TRAY_WINDOW_CLASS);
    unsafe {
        let hwnd = FindWindowW(class_w.as_ptr(), std::ptr::null());
        if hwnd.is_null() {
            return false;
        }
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_UID;
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut nid.szInfoTitle, title);
        copy_wide(&mut nid.szInfo, body);
        Shell_NotifyIconW(NIM_MODIFY, &nid) != 0
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

    #[cfg(windows)]
    #[test]
    fn tray_daemon_creates_valid_hicon_and_message_hwnd_without_premature_delete() {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Gdi::DeleteObject;
        use windows_sys::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetIconInfo, HICON, ICONINFO,
        };

        let daemon = TrayDaemon::new(4310, TrayWorkerConfig::default()).unwrap();
        assert_ne!(daemon.hwnd_raw(), 0, "expected valid hidden Win32 message HWND");
        assert_ne!(daemon.hicon_raw(), 0, "expected valid Win32 HICON from CreateIconIndirect");
        assert_eq!(daemon.nid_flags(), NIF_ICON | NIF_MESSAGE | NIF_TIP);
        assert!(!daemon.was_deleted());

        let mut cls_buf = [0u16; 64];
        let cls_len = unsafe {
            GetClassNameW(
                daemon.hwnd_raw() as HWND,
                cls_buf.as_mut_ptr(),
                cls_buf.len() as i32,
            )
        } as usize;
        assert_eq!(String::from_utf16_lossy(&cls_buf[..cls_len]), TRAY_WINDOW_CLASS);

        let mut info: ICONINFO = unsafe { std::mem::zeroed() };
        let ok = unsafe { GetIconInfo(daemon.hicon_raw() as HICON, &mut info) };
        assert_ne!(ok, 0, "GetIconInfo must succeed for real CreateIconIndirect HICON");
        unsafe {
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
        }

        if has_shell_tray() {
            assert!(
                daemon.icon_installed(),
                "Shell_NotifyIconW(NIM_ADD) must stay installed without immediate NIM_DELETE"
            );
        }
    }
}
