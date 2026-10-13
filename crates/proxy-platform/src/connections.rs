use crate::windows::status;
use anyhow::{Result, bail};
use proxy_core::{config::Protocol, model::Connection};
use std::{ffi::c_void, mem::size_of, net::Ipv4Addr};
use windows_sys::Win32::{
    Foundation::ERROR_INSUFFICIENT_BUFFER, NetworkManagement::IpHelper::*,
    Networking::WinSock::AF_INET,
};

fn rows<T: Copy>(call: impl Fn(*mut c_void, &mut u32) -> u32) -> Result<Vec<T>> {
    let mut size = 0;
    let first = call(std::ptr::null_mut(), &mut size);
    if first != ERROR_INSUFFICIENT_BUFFER {
        status(first)?;
    }
    loop {
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let result = call(buffer.as_mut_ptr().cast(), &mut size);
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        status(result)?;
        if size < 4 {
            return Ok(Vec::new());
        }
        let count = unsafe { *buffer.as_ptr().cast::<u32>() } as usize;
        if count > (size as usize - 4) / size_of::<T>() {
            bail!("网络表长度无效");
        }
        let first = unsafe { buffer.as_ptr().cast::<u8>().add(4).cast::<T>() };
        return Ok((0..count)
            .map(|i| unsafe { first.add(i).read_unaligned() })
            .collect());
    }
}
pub fn snapshot() -> Result<Vec<Connection>> {
    let tcp = rows::<MIB_TCPROW_OWNER_PID>(|ptr, size| unsafe {
        GetExtendedTcpTable(ptr, size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0)
    })?;
    let udp = rows::<MIB_UDPROW_OWNER_PID>(|ptr, size| unsafe {
        GetExtendedUdpTable(ptr, size, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0)
    })?;
    let mut connections: Vec<_> = tcp
        .into_iter()
        .map(|row| Connection {
            pid: row.dwOwningPid,
            protocol: Protocol::Tcp,
            local_address: Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).to_string(),
            local_port: u16::from_be(row.dwLocalPort as u16),
            remote_address: Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes()).to_string(),
            remote_port: u16::from_be(row.dwRemotePort as u16),
            state: match row.dwState {
                1 => "CLOSED",
                2 => "LISTEN",
                3 => "SYN_SENT",
                4 => "SYN_RECEIVED",
                5 => "ESTABLISHED",
                6 => "FIN_WAIT_1",
                7 => "FIN_WAIT_2",
                8 => "CLOSE_WAIT",
                9 => "CLOSING",
                10 => "LAST_ACK",
                11 => "TIME_WAIT",
                12 => "DELETE_TCB",
                _ => "UNKNOWN",
            }
            .into(),
        })
        .collect();
    connections.extend(udp.into_iter().map(|row| Connection {
        pid: row.dwOwningPid,
        protocol: Protocol::Udp,
        local_address: Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).to_string(),
        local_port: u16::from_be(row.dwLocalPort as u16),
        remote_address: String::new(),
        remote_port: 0,
        state: "BOUND".into(),
    }));
    Ok(connections)
}
