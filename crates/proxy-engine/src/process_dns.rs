use crate::{dns, packet, relay::UdpPacket, state::Shared};
use anyhow::{Context, Result};
use proxy_platform::windivert::DivertHandle;

pub(crate) async fn relay(
    packet: UdpPacket,
    shared: &Shared,
    network: &DivertHandle,
) -> Result<()> {
    let owner = &packet.route.owner;
    if !shared.state.lock().unwrap().udp_active(owner) {
        return Ok(());
    }
    let meter = shared
        .traffic
        .register(&owner.identity, &packet.route.traffic_group);
    let response = dns::udp_response(
        dns::exchange(
            &packet.route.group,
            packet.route.destination.address,
            &packet.payload,
            Some(&meter),
        )
        .await
        .with_context(|| {
            format!(
                "进程 {} 的 DNS（代理组 {}，目标 {}，上游 {}）",
                owner.identity.pid,
                packet.route.group.name,
                packet.remote,
                packet.route.destination.address
            )
        })?,
    )?;
    let mut reply = packet::udp_reply(packet.remote, packet.local, &response)
        .context("DNS 回复超过 IPv4 长度限制")?;
    let mut address = packet.address;
    address.set_inbound();
    network.recalculate(&mut reply, &mut address)?;
    let state = shared.state.lock().unwrap();
    if !state.udp_active(owner) {
        return Ok(());
    }
    network.send(&reply, &address)?;
    meter.record(0, response.len());
    Ok(())
}
