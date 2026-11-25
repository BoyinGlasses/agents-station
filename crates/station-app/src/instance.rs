use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

pub const GUI_MUTEX_NAME: &str = "Local\\agents-station-gui";
pub const SHOW_EVENT_NAME: &str = "Local\\agents-station-show";

static HELD_NAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn held_set() -> &'static Mutex<HashSet<String>> {
    HELD_NAMES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub struct InstanceLock {
    name: String,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: Win32 mutex HANDLE is owned solely by this struct and closed once in Drop.
#[cfg(windows)]
unsafe impl Send for InstanceLock {}

impl InstanceLock {
    pub fn try_acquire(mutex_name: &str) -> Result<Option<Self>> {
        let mut set = held_set().lock().expect("instance lock set poisoned");
        if set.contains(mutex_name) {
            return Ok(None);
        }

        #[cfg(windows)]
        let handle = {
            use windows_sys::Win32::Foundation::{
                CloseHandle, ERROR_ALREADY_EXISTS, GetLastError,
            };
            use windows_sys::Win32::System::Threading::CreateMutexW;

            let wide: Vec<u16> = mutex_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: null security attributes, non-owned initially, valid NUL-terminated UTF-16 name.
            let h = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
            if h.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            // SAFETY: immediate GetLastError after CreateMutexW.
            let err = unsafe { GetLastError() };
            if err == ERROR_ALREADY_EXISTS {
                // SAFETY: close duplicate handle opened for existing mutex.
                unsafe { CloseHandle(h) };
                return Ok(None);
            }
            h
        };

        set.insert(mutex_name.to_owned());
        Ok(Some(Self {
            name: mutex_name.to_owned(),
            #[cfg(windows)]
            handle,
        }))
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // SAFETY: handle is valid and owned uniquely by self.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
        if let Ok(mut set) = held_set().lock() {
            set.remove(&self.name);
        }
    }
}

/// Cross-process named event listener (`Local\agents-station-show`) that wakes the
/// running TrayDaemon when a second `agents-station.exe` launch asks to show the GUI.
pub struct ShowEventListener {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    state: std::sync::Arc<(Mutex<bool>, std::sync::Condvar)>,
}

// SAFETY: Win32 event HANDLE is owned uniquely by this struct and closed once in Drop.
#[cfg(windows)]
unsafe impl Send for ShowEventListener {}

#[cfg(not(windows))]
static NON_WIN_EVENTS: OnceLock<
    Mutex<std::collections::HashMap<String, std::sync::Arc<(Mutex<bool>, std::sync::Condvar)>>>,
> = OnceLock::new();

impl ShowEventListener {
    pub fn create(event_name: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::CreateEventW;

            let wide: Vec<u16> = event_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: null security attrs, auto-reset (0), initially non-signaled (0), valid UTF-16 name.
            let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide.as_ptr()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(Self { handle })
        }
        #[cfg(not(windows))]
        {
            let map = NON_WIN_EVENTS.get_or_init(|| Mutex::new(Default::default()));
            let state = std::sync::Arc::new((Mutex::new(false), std::sync::Condvar::new()));
            map.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(event_name.to_owned(), state.clone());
            Ok(Self { state })
        }
    }

    pub fn wait_timeout(&self, timeout_ms: u32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
            use windows_sys::Win32::System::Threading::WaitForSingleObject;

            // SAFETY: self.handle is a valid Win32 event HANDLE owned by self.
            unsafe { WaitForSingleObject(self.handle, timeout_ms) == WAIT_OBJECT_0 }
        }
        #[cfg(not(windows))]
        {
            let (lock, cvar) = &*self.state;
            let mut signaled = lock.lock().unwrap_or_else(|e| e.into_inner());
            if *signaled {
                *signaled = false;
                return true;
            }
            let (mut guard, _) = cvar
                .wait_timeout(signaled, std::time::Duration::from_millis(timeout_ms as u64))
                .unwrap_or_else(|e| e.into_inner());
            if *guard {
                *guard = false;
                true
            } else {
                false
            }
        }
    }
}

impl Drop for ShowEventListener {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // SAFETY: handle is valid and owned uniquely by self.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

pub fn signal_show_event(event_name: &str) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};

        let wide: Vec<u16> = event_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: valid UTF-16 event name; handle closed immediately after SetEvent.
        let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide.as_ptr()) };
        if handle.is_null() {
            return false;
        }
        let ok = unsafe { SetEvent(handle) != 0 };
        unsafe { CloseHandle(handle) };
        ok
    }
    #[cfg(not(windows))]
    {
        let Some(map) = NON_WIN_EVENTS.get() else {
            return false;
        };
        let entry = map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(event_name)
            .cloned();
        if let Some(state) = entry {
            let (lock, cvar) = &*state;
            *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
            cvar.notify_one();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_instance_lock_returns_none_until_first_drops() {
        let name = format!("Local\\agents-station-test-{}", std::process::id());
        let first = InstanceLock::try_acquire(&name).unwrap();
        assert!(first.is_some());
        let second = InstanceLock::try_acquire(&name).unwrap();
        assert!(second.is_none());
        drop(first);
        let third = InstanceLock::try_acquire(&name).unwrap();
        assert!(third.is_some());
    }

    #[test]
    fn signaling_show_event_wakes_listener() {
        let name = format!("{}-test-{}", SHOW_EVENT_NAME, std::process::id());
        let listener = ShowEventListener::create(&name).unwrap();
        let name_bg = name.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            signal_show_event(&name_bg)
        });
        assert!(
            listener.wait_timeout(2_000),
            "ShowEventListener must wake when named event is signaled"
        );
        assert!(handle.join().unwrap());
    }
}
