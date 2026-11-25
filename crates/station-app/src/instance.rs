use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

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
}
