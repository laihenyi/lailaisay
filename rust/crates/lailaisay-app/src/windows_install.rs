//! Inno Setup's AppMutex prevents replacing files while the desktop app is running.
use std::{ffi::c_void, io, ptr};

pub const APP_MUTEX: &str = "Local\\Tok.Desktop.Running";

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: *const c_void, initial_owner: i32, name: *const u16)
        -> *mut c_void;
}

pub fn register_running_app() -> io::Result<()> {
    let name: Vec<u16> = APP_MUTEX.encode_utf16().chain(Some(0)).collect();
    // Keep this single handle until process termination, including background worker
    // shutdown after the GUI returns. Windows releases it on process exit.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    #[test]
    fn running_marker_can_be_opened_by_the_installer() {
        register_running_app().unwrap();
        let name: Vec<u16> = APP_MUTEX.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { OpenMutexW(0x00100000, 0, name.as_ptr()) };
        assert!(!handle.is_null());
        unsafe {
            CloseHandle(handle);
        }
    }
}
