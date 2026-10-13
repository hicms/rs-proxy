use anyhow::{Context, Result, bail};
use libloading::Library;
use std::{
    ffi::{CString, c_void},
    io,
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    sync::Arc,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Address {
    pub timestamp: i64,
    flags: u32,
    reserved: u32,
    data: [u8; 64],
}
impl Default for Address {
    fn default() -> Self {
        Self {
            timestamp: 0,
            flags: 0,
            reserved: 0,
            data: [0; 64],
        }
    }
}
const _: () = assert!(std::mem::size_of::<Address>() == 80 && std::mem::align_of::<Address>() == 8);

#[derive(Debug)]
pub struct SocketEvent {
    pub timestamp: i64,
    pub kind: u8,
    pub endpoint: u64,
    pub pid: u32,
    pub local: SocketAddrV4,
    pub remote: SocketAddrV4,
    pub protocol: u8,
}
impl Address {
    pub fn outbound(&self) -> bool {
        self.flags & (1 << 17) != 0
    }
    pub fn set_inbound(&mut self) {
        self.flags &= !(1 << 17);
    }
    pub fn set_loopback_outbound(&mut self) {
        self.flags |= (1 << 17) | (1 << 18);
        self.data[..4].copy_from_slice(&1u32.to_ne_bytes());
        self.data[4..8].fill(0);
    }
    pub fn socket_event(&self) -> Option<SocketEvent> {
        if self.flags & 0xff != 3 || self.flags & (1 << 20) != 0 {
            return None;
        }
        let number =
            |offset: usize| u32::from_ne_bytes(self.data[offset..offset + 4].try_into().unwrap());
        let port =
            |offset: usize| u16::from_ne_bytes(self.data[offset..offset + 2].try_into().unwrap());
        Some(SocketEvent {
            timestamp: self.timestamp,
            kind: ((self.flags >> 8) & 0xff) as u8,
            endpoint: u64::from_ne_bytes(self.data[..8].try_into().unwrap()),
            pid: number(16),
            local: SocketAddrV4::new(Ipv4Addr::from(number(20)), port(52)),
            remote: SocketAddrV4::new(Ipv4Addr::from(number(36)), port(54)),
            protocol: self.data[56],
        })
    }
}
type Open = unsafe extern "system" fn(*const u8, u32, i16, u64) -> isize;
type Recv = unsafe extern "system" fn(isize, *mut c_void, u32, *mut u32, *mut Address) -> i32;
type Send = unsafe extern "system" fn(isize, *const c_void, u32, *mut u32, *const Address) -> i32;
type Shutdown = unsafe extern "system" fn(isize, u32) -> i32;
type Close = unsafe extern "system" fn(isize) -> i32;
type Checksums = unsafe extern "system" fn(*mut c_void, u32, *mut Address, u64) -> i32;
type SetParam = unsafe extern "system" fn(isize, u32, u64) -> i32;

pub struct Driver {
    _library: Library,
    open: Open,
    recv: Recv,
    send: Send,
    shutdown: Shutdown,
    close: Close,
    checksums: Checksums,
    set_param: SetParam,
}
impl Driver {
    pub fn load(directory: &Path) -> Result<Arc<Self>> {
        let path = directory
            .join("WinDivert.dll")
            .canonicalize()
            .context("缺少 WinDivert.dll，请运行资源准备脚本")?;
        // The absolute packaged path prevents resolution through the current directory.
        unsafe {
            let library = Library::new(&path).context("加载 WinDivert")?;
            Ok(Arc::new(Self {
                open: *library.get(b"WinDivertOpen\0")?,
                recv: *library.get(b"WinDivertRecv\0")?,
                send: *library.get(b"WinDivertSend\0")?,
                shutdown: *library.get(b"WinDivertShutdown\0")?,
                close: *library.get(b"WinDivertClose\0")?,
                checksums: *library.get(b"WinDivertHelperCalcChecksums\0")?,
                set_param: *library.get(b"WinDivertSetParam\0")?,
                _library: library,
            }))
        }
    }
    pub fn open(
        self: &Arc<Self>,
        filter: &str,
        layer: u32,
        priority: i16,
        flags: u64,
    ) -> Result<Arc<DivertHandle>> {
        let filter = CString::new(filter)?;
        let handle = unsafe { (self.open)(filter.as_ptr().cast(), layer, priority, flags) };
        if handle == -1 {
            return Err(io::Error::last_os_error())
                .context("打开 WinDivert，请确认以管理员身份运行");
        }
        Ok(Arc::new(DivertHandle {
            driver: self.clone(),
            handle,
        }))
    }
}
pub struct DivertHandle {
    driver: Arc<Driver>,
    handle: isize,
}
impl DivertHandle {
    pub fn receive(&self, packet: &mut [u8]) -> io::Result<(usize, Address)> {
        let mut length = 0;
        let mut address = Address::default();
        if unsafe {
            (self.driver.recv)(
                self.handle,
                if packet.is_empty() {
                    std::ptr::null_mut()
                } else {
                    packet.as_mut_ptr().cast()
                },
                packet.len() as u32,
                &mut length,
                &mut address,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if length as usize > packet.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "驱动包长度越界"));
        }
        Ok((length as usize, address))
    }
    pub fn send(&self, packet: &[u8], address: &Address) -> io::Result<()> {
        let mut sent = 0;
        if unsafe {
            (self.driver.send)(
                self.handle,
                packet.as_ptr().cast(),
                packet.len() as u32,
                &mut sent,
                address,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if sent as usize != packet.len() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "驱动未注入完整包"));
        }
        Ok(())
    }
    pub fn recalculate(&self, packet: &mut [u8], address: &mut Address) -> Result<()> {
        if unsafe {
            (self.driver.checksums)(packet.as_mut_ptr().cast(), packet.len() as u32, address, 0)
        } == 0
        {
            bail!("数据包校验和计算失败");
        }
        Ok(())
    }
    pub fn set_queue(&self, length: u64, milliseconds: u64) -> io::Result<()> {
        for (parameter, value) in [(0, length), (1, milliseconds)] {
            if unsafe { (self.driver.set_param)(self.handle, parameter, value) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    pub fn shutdown_receive(&self) -> io::Result<()> {
        if unsafe { (self.driver.shutdown)(self.handle, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for DivertHandle {
    fn drop(&mut self) {
        unsafe {
            (self.driver.close)(self.handle);
        }
    }
}
pub fn timestamp() -> i64 {
    let mut value = 0;
    unsafe {
        QueryPerformanceCounter(&mut value);
    }
    value
}
pub fn frequency() -> i64 {
    let mut value = 0;
    unsafe {
        QueryPerformanceFrequency(&mut value);
    }
    value
}
