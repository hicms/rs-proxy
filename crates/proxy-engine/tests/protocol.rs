use proxy_engine::{
    packet::{Packet, udp_reply},
    socks::{self, SocksAddress},
};

#[test]
fn datagrams_keep_each_remote_source_and_reject_fragments() {
    for remote in ["198.18.0.1:53", "198.18.0.2:443", "[::1]:5353"] {
        let remote = remote.parse().unwrap();
        let encoded = socks::encode_datagram(remote, b"payload").unwrap();
        assert_eq!(
            socks::decode_datagram(&encoded).unwrap(),
            (SocksAddress::Ip(remote), b"payload".as_slice())
        );
        let mut fragment = encoded;
        fragment[2] = 1;
        assert!(socks::decode_datagram(&fragment).is_err());
    }
    for invalid in [vec![], vec![0, 0, 0, 1, 127], vec![0, 0, 0, 3, 10, b'a']] {
        assert!(socks::decode_datagram(&invalid).is_err());
    }
}

#[test]
fn packet_parser_rejects_truncation_fragmentation_and_malformed_lengths() {
    let source = "198.18.0.1:53".parse().unwrap();
    let destination = "10.0.0.2:40000".parse().unwrap();
    let valid = udp_reply(source, destination, b"dns").unwrap();
    let parsed = Packet::parse(&valid).unwrap();
    assert_eq!(parsed.key.local, source);
    assert_eq!(parsed.key.remote, destination);
    assert_eq!(&valid[parsed.payload..parsed.length], b"dns");
    for length in 0..valid.len() {
        assert!(Packet::parse(&valid[..length]).is_none());
    }
    let mut invalid = valid.clone();
    invalid[6] = 0x20;
    assert!(Packet::parse(&invalid).is_none());
    let mut invalid = valid;
    invalid[24] = 0xff;
    assert!(Packet::parse(&invalid).is_none());
}
