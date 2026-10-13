use crate::windows::Handle;
use anyhow::{Context, Result, bail};
use proxy_core::model::{ProcessIdentity, ProcessInfo};
use std::{
    collections::HashSet,
    ffi::c_void,
    io,
    mem::{size_of, size_of_val},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{ERROR_NO_MORE_FILES, HANDLE},
    System::{Diagnostics::ToolHelp::*, Threading::*},
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        buffer: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}
#[repr(C)]
#[derive(Default)]
struct BasicInfo {
    exit_status: usize,
    peb: usize,
    affinity: usize,
    base_priority: usize,
    pid: usize,
    parent: usize,
}
#[repr(C)]
struct UnicodeString {
    length: u16,
    capacity: u16,
    buffer: *const u16,
}

fn identity(handle: &Handle, pid: u32) -> Result<ProcessIdentity> {
    let mut sequence = 0u64;
    let status = unsafe {
        NtQueryInformationProcess(
            handle.raw(),
            92,
            (&mut sequence as *mut u64).cast(),
            8,
            ptr::null_mut(),
        )
    };
    if status < 0 {
        bail!("无法读取进程 {pid} 的系统序号 (NTSTATUS {status:#x})");
    }
    Ok(ProcessIdentity {
        pid,
        sequence: sequence.to_string(),
    })
}

pub(crate) fn hold_identity(expected: &ProcessIdentity) -> Result<Option<Handle>> {
    let handle = match Handle::new(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, expected.pid)
    }) {
        Ok(handle) => handle,
        Err(error) if error.raw_os_error() == Some(87) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    Ok((identity(&handle, expected.pid)? == *expected).then_some(handle))
}
fn command_line(handle: &Handle) -> Result<String> {
    let mut size = 0;
    unsafe {
        NtQueryInformationProcess(handle.raw(), 60, ptr::null_mut(), 0, &mut size);
    }
    if size == 0 {
        bail!("无法读取进程命令行长度");
    }
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    let status = unsafe {
        NtQueryInformationProcess(
            handle.raw(),
            60,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    };
    if status < 0 {
        bail!("无法读取进程命令行 (NTSTATUS {status:#x})");
    }
    if (size as usize) < size_of::<UnicodeString>() {
        bail!("命令行返回值过短");
    }
    let text = unsafe { &*buffer.as_ptr().cast::<UnicodeString>() };
    let start = buffer.as_ptr() as usize;
    let end = start + size_of_val(buffer.as_slice());
    let address = text.buffer as usize;
    if text.length == 0 {
        return Ok(String::new());
    }
    if address < start
        || address
            .checked_add(text.length as usize)
            .is_none_or(|p| p > end)
        || text.length % 2 != 0
    {
        bail!("命令行返回值越界");
    }
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text.buffer, text.length as usize / 2)
    }))
}
pub fn query(pid: u32) -> Result<ProcessInfo> {
    query_before(pid, None)
}
fn query_before(pid: u32, latest_creation: Option<u64>) -> Result<ProcessInfo> {
    let handle = Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })
        .with_context(|| format!("打开进程 {pid}"))?;
    if let Some(latest) = latest_creation {
        use windows_sys::Win32::Foundation::FILETIME;
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe {
            GetProcessTimes(
                handle.raw(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error()).context("校验 socket 进程创建时间");
        }
        let created = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        if created > latest {
            bail!("socket 事件对应的 PID 已被复用");
        }
    }
    let id = identity(&handle, pid)?;
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle.raw(), 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error()).context("读取进程路径");
    }
    let image_path = String::from_utf16_lossy(&path[..length as usize]);
    let name = image_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&image_path)
        .to_string();
    let mut basic = BasicInfo::default();
    let result = unsafe {
        NtQueryInformationProcess(
            handle.raw(),
            0,
            (&mut basic as *mut BasicInfo).cast(),
            size_of::<BasicInfo>() as u32,
            ptr::null_mut(),
        )
    };
    let parent = if result >= 0 && basic.parent != 0 && basic.parent != pid as usize {
        Handle::new(unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, basic.parent as u32)
        })
        .ok()
        .and_then(|parent| identity(&parent, basic.parent as u32).ok())
        .filter(|parent| {
            parent.sequence.parse::<u64>().unwrap_or(u64::MAX)
                < id.sequence.parse::<u64>().unwrap_or(0)
        })
    } else {
        None
    };
    let cmdline = command_line(&handle).unwrap_or_else(|error| {
        tracing::debug!(pid, %error, "进程命令行不可用");
        String::new()
    });
    Ok(ProcessInfo {
        identity: id,
        parent,
        name,
        image_path,
        cmdline,
    })
}
pub fn query_ancestors(pid: u32) -> Result<Vec<ProcessInfo>> {
    ancestors(query(pid)?)
}
pub fn query_socket_ancestors(pid: u32, event_timestamp: i64) -> Result<Vec<ProcessInfo>> {
    use windows_sys::Win32::{
        Foundation::FILETIME, System::SystemInformation::GetSystemTimePreciseAsFileTime,
    };
    let mut wall = FILETIME::default();
    unsafe {
        GetSystemTimePreciseAsFileTime(&mut wall);
    }
    let elapsed = crate::windivert::timestamp().saturating_sub(event_timestamp);
    if elapsed < 0 {
        bail!("socket 事件时间戳无效");
    }
    let wall = (u64::from(wall.dwHighDateTime) << 32) | u64::from(wall.dwLowDateTime);
    let delta = (elapsed as u128 * 10_000_000 / crate::windivert::frequency() as u128) as u64;
    ancestors(query_before(pid, Some(wall.saturating_sub(delta)))?)
}
fn ancestors(process: ProcessInfo) -> Result<Vec<ProcessInfo>> {
    let pid = process.identity.pid;
    let mut chain = vec![process];
    let mut visited = HashSet::from([pid]);
    while let Some(parent) = chain.last().and_then(|p| p.parent.clone()) {
        if !visited.insert(parent.pid) {
            break;
        }
        match query(parent.pid) {
            Ok(process) if process.identity == parent => chain.push(process),
            _ => break,
        }
    }
    chain.reverse();
    Ok(chain)
}
pub fn snapshot() -> Result<Vec<ProcessInfo>> {
    let handle = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(handle.raw(), &mut entry) } == 0 {
        return Err(io::Error::last_os_error()).context("枚举进程");
    }
    let mut processes = Vec::new();
    loop {
        if let Ok(process) = query(entry.th32ProcessID) {
            processes.push(process);
        }
        if unsafe { Process32NextW(handle.raw(), &mut entry) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
                return Err(error.into());
            }
            break;
        }
    }
    Ok(processes)
}
