use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    UI::Shell::IsUserAnAdmin,
};

pub(crate) struct Handle(HANDLE);
impl Handle {
    pub(crate) fn new(raw: HANDLE) -> io::Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}
pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub(crate) fn status(code: u32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32))
    }
}
// Only called for strings whose allocation and terminator are owned by Windows.
pub(crate) unsafe fn wide_string(value: *const u16) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut len = 0;
    unsafe {
        while ptr::read(value.add(len)) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(value, len))
    }
}
