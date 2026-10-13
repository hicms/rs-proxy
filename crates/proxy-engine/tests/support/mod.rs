use proxy_platform::windivert::{DivertHandle, Driver};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

pub struct DirectTcpPeer {
    handle: Arc<DivertHandle>,
    worker: Option<JoinHandle<()>>,
    pub received: Arc<AtomicBool>,
}
impl DirectTcpPeer {
    pub fn start(directory: &Path) -> Self {
        let driver = Driver::load(directory).unwrap();
        let handle = driver
            .open(
                "outbound and ip and tcp and ip.DstAddr == 198.19.99.4 and tcp.DstPort == 4444",
                0,
                -100,
                0,
            )
            .unwrap();
        let socket = handle.clone();
        let received = Arc::new(AtomicBool::new(false));
        let observed = received.clone();
        let worker = thread::spawn(move || {
            let mut input = vec![0; 65535];
            let mut next_seq = 3001u32;
            while let Ok((length, mut address)) = socket.receive(&mut input) {
                let ip_length = (input[0] & 15) as usize * 4;
                if length < ip_length + 20 {
                    continue;
                }
                let payload_offset = ip_length + (input[ip_length + 12] >> 4) as usize * 4;
                if payload_offset > length {
                    continue;
                }
                let flags = input[ip_length + 13];
                let incoming_sequence =
                    u32::from_be_bytes(input[ip_length + 4..ip_length + 8].try_into().unwrap());
                let data = &input[payload_offset..length];
                let (sequence, ack, reply_flags, echo) = if flags & 2 != 0 {
                    (3000, incoming_sequence.wrapping_add(1), 0x12, &[][..])
                } else if !data.is_empty() {
                    observed.store(true, Ordering::Release);
                    let sequence = next_seq;
                    next_seq = next_seq.wrapping_add(data.len() as u32);
                    (
                        sequence,
                        incoming_sequence.wrapping_add(data.len() as u32),
                        0x18,
                        data,
                    )
                } else if flags & 1 != 0 {
                    (next_seq, incoming_sequence.wrapping_add(1), 0x11, &[][..])
                } else {
                    continue;
                };
                let mut output = vec![0u8; 40 + echo.len()];
                let reply_length = output.len() as u16;
                output[0] = 0x45;
                output[2..4].copy_from_slice(&reply_length.to_be_bytes());
                output[8] = 64;
                output[9] = 6;
                output[12..16].copy_from_slice(&input[16..20]);
                output[16..20].copy_from_slice(&input[12..16]);
                output[20..22].copy_from_slice(&input[ip_length + 2..ip_length + 4]);
                output[22..24].copy_from_slice(&input[ip_length..ip_length + 2]);
                output[24..28].copy_from_slice(&sequence.to_be_bytes());
                output[28..32].copy_from_slice(&ack.to_be_bytes());
                output[32] = 0x50;
                output[33] = reply_flags;
                output[34..36].copy_from_slice(&u16::MAX.to_be_bytes());
                output[40..].copy_from_slice(echo);
                address.set_inbound();
                socket.recalculate(&mut output, &mut address).unwrap();
                socket.send(&output, &address).unwrap();
            }
        });
        Self {
            handle,
            worker: Some(worker),
            received,
        }
    }
}
impl Drop for DirectTcpPeer {
    fn drop(&mut self) {
        self.handle.shutdown_receive().unwrap();
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
