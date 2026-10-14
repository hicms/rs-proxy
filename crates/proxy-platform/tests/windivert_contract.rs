use proxy_platform::windivert::Address;
use std::{io, mem, net::SocketAddrV4};

#[path = "../src/windivert/errors.rs"]
mod errors;

#[test]
fn socket_address_decodes_the_public_c_abi() {
    // WinDivert 2.2: union begins at 16, Socket.Protocol is at union offset 56.
    let mut raw = [0u8; 80];
    raw[..8].copy_from_slice(&12345678i64.to_ne_bytes());
    raw[8..12].copy_from_slice(&(3u32 | (4 << 8) | (1 << 17)).to_ne_bytes());
    raw[16..24].copy_from_slice(&0x123456789abcdef0u64.to_ne_bytes());
    raw[32..36].copy_from_slice(&9876u32.to_ne_bytes());
    raw[36..40].copy_from_slice(&0xc000020au32.to_ne_bytes());
    raw[52..56].copy_from_slice(&0xcb007114u32.to_ne_bytes());
    raw[68..70].copy_from_slice(&49152u16.to_ne_bytes());
    raw[70..72].copy_from_slice(&443u16.to_ne_bytes());
    raw[72] = 6;

    assert_eq!(mem::size_of::<Address>(), 80);
    assert_eq!(mem::align_of::<Address>(), 8);
    // Address contains only integer fields and bytes; every bit pattern is valid.
    let mut address = unsafe { mem::transmute::<[u8; 80], Address>(raw) };
    assert!(address.outbound());
    let event = address.socket_event().unwrap();
    assert_eq!(event.timestamp, 12345678);
    assert_eq!(event.kind, 4);
    assert_eq!(event.endpoint, 0x123456789abcdef0);
    assert_eq!(event.pid, 9876);
    assert_eq!(
        event.local,
        "192.0.2.10:49152".parse::<SocketAddrV4>().unwrap()
    );
    assert_eq!(
        event.remote,
        "203.0.113.20:443".parse::<SocketAddrV4>().unwrap()
    );
    assert_eq!(event.protocol, 6);
    address.set_inbound();
    assert!(!address.outbound());

    raw[8] = 0;
    let network = unsafe { mem::transmute::<[u8; 80], Address>(raw) };
    assert!(network.socket_event().is_none());
    raw[8] = 3;
    raw[10] |= 1 << 4;
    let ipv6 = unsafe { mem::transmute::<[u8; 80], Address>(raw) };
    assert!(ipv6.socket_event().is_none());
}

#[test]
fn driver_open_failures_keep_their_actual_cause_and_os_error() {
    for (code, expected) in [
        (2, "驱动文件"),
        (5, "管理员"),
        (87, "过滤表达式"),
        (577, "数字签名"),
        (654, "不兼容"),
        (1060, "禁止自动安装"),
        (1275, "驱动被系统阻止"),
        (1753, "Base Filtering Engine"),
        (12345, "原始系统错误"),
    ] {
        let error = errors::open(io::Error::from_raw_os_error(code));
        assert!(error.to_string().contains(expected), "{error:#}");
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().raw_os_error(),
            Some(code)
        );
        if code != 5 {
            assert!(!error.to_string().contains("管理员"));
        }
    }
}

#[test]
fn driver_compatibility_checks_major_and_minor_without_claiming_a_patch() {
    assert!(errors::require_supported_version(2, 2).is_ok());
    for (major, minor) in [(1, 4), (2, 0), (2, 1), (2, 3), (3, 0)] {
        let error = errors::require_supported_version(major, minor).unwrap_err();
        assert!(error.to_string().contains(&format!("{major}.{minor}")));
        assert!(error.to_string().contains("需要 2.2 接口"));
    }
}
