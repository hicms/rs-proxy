use crate::windows::status;
use anyhow::{Context, Result, bail};
use proxy_core::{
    config::Protocol,
    flow::FlowKey,
    model::{Connection, ProcessIdentity},
};
use std::{
    ffi::c_void,
    mem::{offset_of, size_of},
    net::{Ipv4Addr, SocketAddrV4},
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NOT_FOUND},
    NetworkManagement::IpHelper::*,
    Networking::WinSock::{AF_INET, AF_INET6},
};

fn rows<T: Copy>(offset: usize, call: impl Fn(*mut c_void, &mut u32) -> u32) -> Result<Vec<T>> {
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
        if (size as usize) < offset {
            return Ok(Vec::new());
        }
        let count = unsafe { *buffer.as_ptr().cast::<u32>() } as usize;
        if count > (size as usize - offset) / size_of::<T>() {
            bail!("网络表长度无效");
        }
        let first = unsafe { buffer.as_ptr().cast::<u8>().add(offset).cast::<T>() };
        return Ok((0..count)
            .map(|i| unsafe { first.add(i).read_unaligned() })
            .collect());
    }
}
pub fn snapshot() -> Result<Vec<Connection>> {
    let tcp = rows::<MIB_TCPROW_OWNER_PID>(
        offset_of!(MIB_TCPTABLE_OWNER_PID, table),
        |ptr, size| unsafe {
            GetExtendedTcpTable(ptr, size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0)
        },
    )?;
    let udp = rows::<MIB_UDPROW_OWNER_PID>(
        offset_of!(MIB_UDPTABLE_OWNER_PID, table),
        |ptr, size| unsafe {
            GetExtendedUdpTable(ptr, size, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0)
        },
    )?;
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

pub struct TcpConnection {
    pub pid: u32,
    pub key: FlowKey,
    created: i64,
    state: u32,
}
impl TcpConnection {
    pub fn is_listener(&self) -> bool {
        self.state == MIB_TCP_STATE_LISTEN as u32
    }
    pub fn is_established(&self) -> bool {
        self.state == MIB_TCP_STATE_ESTAB as u32
    }
    fn row(&self) -> MIB_TCPROW_LH {
        MIB_TCPROW_LH {
            Anonymous: MIB_TCPROW_LH_0 {
                dwState: self.state,
            },
            dwLocalAddr: u32::from_ne_bytes(self.key.local.ip().octets()),
            dwLocalPort: self.key.local.port().to_be() as u32,
            dwRemoteAddr: u32::from_ne_bytes(self.key.remote.ip().octets()),
            dwRemotePort: self.key.remote.port().to_be() as u32,
        }
    }
}

pub fn tcp_snapshot() -> Result<Vec<TcpConnection>> {
    let table = rows::<MIB_TCPROW_OWNER_MODULE>(
        offset_of!(MIB_TCPTABLE_OWNER_MODULE, table),
        |ptr, size| unsafe {
            GetExtendedTcpTable(ptr, size, 0, AF_INET as u32, TCP_TABLE_OWNER_MODULE_ALL, 0)
        },
    )?;
    Ok(table
        .into_iter()
        .map(|row| TcpConnection {
            pid: row.dwOwningPid,
            key: FlowKey {
                local: SocketAddrV4::new(
                    Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()),
                    u16::from_be(row.dwLocalPort as u16),
                ),
                remote: SocketAddrV4::new(
                    Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes()),
                    u16::from_be(row.dwRemotePort as u16),
                ),
            },
            created: row.liCreateTimestamp,
            state: row.dwState,
        })
        .collect())
}

pub fn tcp6_listener_owners() -> Result<Vec<(u32, u16)>> {
    let listeners = rows::<MIB_TCP6ROW_OWNER_PID>(
        offset_of!(MIB_TCP6TABLE_OWNER_PID, table),
        |ptr, size| unsafe {
            GetExtendedTcpTable(
                ptr,
                size,
                0,
                AF_INET6 as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        },
    )?;
    Ok(listeners
        .into_iter()
        .map(|row| (row.dwOwningPid, u16::from_be(row.dwLocalPort as u16)))
        .collect())
}

pub fn disconnect_tcp(connection: &TcpConnection, identity: &ProcessIdentity) -> Result<bool> {
    if connection.pid != identity.pid || !connection.is_established() {
        return Ok(false);
    }
    // Keeping this handle alive prevents PID reuse while checking and deleting the connection.
    let Some(_process) = crate::process::hold_identity(identity)? else {
        return Ok(false);
    };
    let mut row = connection.row();
    let mut options = TCP_ESTATS_SYN_OPTS_ROS_v0::default();
    let code = unsafe {
        GetPerTcpConnectionEStats(
            &row,
            TcpConnectionEstatsSynOpts,
            std::ptr::null_mut(),
            0,
            0,
            (&mut options as *mut TCP_ESTATS_SYN_OPTS_ROS_v0).cast(),
            0,
            size_of::<TCP_ESTATS_SYN_OPTS_ROS_v0>() as u32,
            std::ptr::null_mut(),
            0,
            0,
        )
    };
    if code == ERROR_NOT_FOUND {
        return Ok(false);
    }
    status(code).context("无法确认 TCP 连接方向")?;
    if !options.ActiveOpen {
        return Ok(false);
    }
    // IP Helper has no atomic owner-bound delete. Recheck this exact generation immediately before it.
    let still_current = tcp_snapshot()?.into_iter().any(|current| {
        current.pid == connection.pid
            && current.key == connection.key
            && current.created == connection.created
            && current.is_established()
    });
    if !still_current {
        return Ok(false);
    }
    row.Anonymous.dwState = MIB_TCP_STATE_DELETE_TCB as u32;
    let code = unsafe { SetTcpEntry(&row) };
    if code == ERROR_NOT_FOUND {
        return Ok(false);
    }
    status(code).context("断开 TCP 连接")?;
    Ok(true)
}
