use proxy_core::flow::FlowKey;
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Clone, Debug)]
pub struct Packet {
    pub key: FlowKey,
    pub protocol: u8,
    pub transport: usize,
    pub payload: usize,
    pub length: usize,
    pub flags: u8,
    pub sequence: u32,
}
impl Packet {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 20 || bytes[0] >> 4 != 4 {
            return None;
        }
        let transport = (bytes[0] & 15) as usize * 4;
        let length = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
        if transport < 20
            || length > bytes.len()
            || length < transport + 8
            || u16::from_be_bytes([bytes[6], bytes[7]]) & 0x3fff != 0
        {
            return None;
        }
        let protocol = bytes[9];
        let (payload, flags, sequence) = match protocol {
            6 => {
                if length < transport + 20 {
                    return None;
                }
                let header = (bytes[transport + 12] >> 4) as usize * 4;
                if header < 20 || transport + header > length {
                    return None;
                }
                (
                    transport + header,
                    bytes[transport + 13],
                    u32::from_be_bytes(bytes[transport + 4..transport + 8].try_into().ok()?),
                )
            }
            17 => {
                let udp_length =
                    u16::from_be_bytes([bytes[transport + 4], bytes[transport + 5]]) as usize;
                if udp_length < 8 || transport + udp_length != length {
                    return None;
                }
                (transport + 8, 0, 0)
            }
            _ => return None,
        };
        let source = Ipv4Addr::new(bytes[12], bytes[13], bytes[14], bytes[15]);
        let destination = Ipv4Addr::new(bytes[16], bytes[17], bytes[18], bytes[19]);
        let source_port = u16::from_be_bytes([bytes[transport], bytes[transport + 1]]);
        let destination_port = u16::from_be_bytes([bytes[transport + 2], bytes[transport + 3]]);
        Some(Self {
            key: FlowKey {
                local: SocketAddrV4::new(source, source_port),
                remote: SocketAddrV4::new(destination, destination_port),
            },
            protocol,
            transport,
            payload,
            length,
            flags,
            sequence,
        })
    }
    pub fn initial_syn(&self) -> bool {
        self.protocol == 6 && self.flags & 0x12 == 2
    }
    pub fn rewrite(&self, bytes: &mut [u8], source: SocketAddrV4, destination: SocketAddrV4) {
        bytes[12..16].copy_from_slice(&source.ip().octets());
        bytes[16..20].copy_from_slice(&destination.ip().octets());
        bytes[self.transport..self.transport + 2].copy_from_slice(&source.port().to_be_bytes());
        bytes[self.transport + 2..self.transport + 4]
            .copy_from_slice(&destination.port().to_be_bytes());
    }
}

pub fn udp_reply(
    source: SocketAddrV4,
    destination: SocketAddrV4,
    payload: &[u8],
) -> Option<Vec<u8>> {
    let length = u16::try_from(payload.len() + 28).ok()?;
    let mut bytes = vec![0u8; length as usize];
    bytes[0] = 0x45;
    bytes[2..4].copy_from_slice(&length.to_be_bytes());
    bytes[8] = 64;
    bytes[9] = 17;
    bytes[12..16].copy_from_slice(&source.ip().octets());
    bytes[16..20].copy_from_slice(&destination.ip().octets());
    bytes[20..22].copy_from_slice(&source.port().to_be_bytes());
    bytes[22..24].copy_from_slice(&destination.port().to_be_bytes());
    bytes[24..26].copy_from_slice(&(length - 20).to_be_bytes());
    bytes[28..].copy_from_slice(payload);
    Some(bytes)
}
