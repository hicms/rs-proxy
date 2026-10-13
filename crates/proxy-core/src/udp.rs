use crate::{flow::FlowKey, model::ProcessIdentity};
use std::{collections::HashMap, net::SocketAddrV4};

#[derive(Clone)]
pub struct UdpOwner {
    pub endpoint: u64,
    pub identity: ProcessIdentity,
    pub timestamp: i64,
    pub local: SocketAddrV4,
}
struct Binding {
    owner: UdpOwner,
    closed: Option<i64>,
    seeded: bool,
}
pub enum UdpLookup<'a> {
    Pending,
    Stale,
    Found(&'a UdpOwner),
}
#[derive(Default)]
pub struct UdpTable {
    bindings: HashMap<u64, Binding>,
    destinations: HashMap<FlowKey, u64>,
}
impl UdpTable {
    pub fn bind(&mut self, owner: UdpOwner, seeded: bool, remote: Option<SocketAddrV4>) {
        if !seeded {
            let candidates: Vec<_> = self
                .bindings
                .iter()
                .filter(|(_, b)| {
                    b.seeded
                        && b.closed.is_none()
                        && b.owner.identity == owner.identity
                        && compatible(b.owner.local, owner.local)
                })
                .map(|(id, _)| *id)
                .collect();
            if candidates.len() == 1 {
                self.bindings.remove(&candidates[0]);
            }
        }
        let endpoint = owner.endpoint;
        if let Some(remote) = remote {
            self.destinations.insert(
                FlowKey {
                    local: owner.local,
                    remote,
                },
                endpoint,
            );
        }
        if let Some(existing) = self.bindings.get_mut(&endpoint)
            && existing.closed.is_none()
            && existing.owner.identity == owner.identity
        {
            existing.owner.local = owner.local;
            return;
        }
        self.bindings.insert(
            endpoint,
            Binding {
                owner,
                closed: None,
                seeded,
            },
        );
    }
    pub fn close(&mut self, endpoint: u64, pid: u32, local: SocketAddrV4, timestamp: i64) {
        let candidates: Vec<_> = self
            .bindings
            .iter()
            .filter(|(_, b)| {
                b.seeded
                    && b.closed.is_none()
                    && b.owner.identity.pid == pid
                    && compatible(b.owner.local, local)
            })
            .map(|(id, _)| *id)
            .collect();
        let seed = (candidates.len() == 1).then(|| candidates[0]);
        for (id, b) in &mut self.bindings {
            if (*id == endpoint || seed == Some(*id)) && b.owner.timestamp <= timestamp {
                b.closed = Some(timestamp);
            }
        }
    }
    pub fn lookup(&self, key: &FlowKey, timestamp: i64) -> UdpLookup<'_> {
        let explicit = self
            .destinations
            .get(key)
            .and_then(|id| self.bindings.get(id))
            .filter(|b| {
                b.owner.timestamp <= timestamp && b.closed.is_none_or(|end| timestamp <= end)
            });
        let matching = |b: &&Binding| {
            b.owner.local.port() == key.local.port()
                && (b.owner.local.ip().is_unspecified() || b.owner.local.ip() == key.local.ip())
        };
        let mut candidates = self.bindings.values().filter(matching);
        let binding = if let Some(explicit) = explicit {
            explicit
        } else {
            let mut eligible = self.bindings.values().filter(matching).filter(|b| {
                b.owner.timestamp <= timestamp && b.closed.is_none_or(|end| timestamp <= end)
            });
            let Some(first) = eligible.next() else {
                return if candidates.next().is_some() {
                    UdpLookup::Stale
                } else {
                    UdpLookup::Pending
                };
            };
            if eligible.next().is_some() {
                return UdpLookup::Stale;
            }
            first
        };
        if binding.closed.is_some() || timestamp < binding.owner.timestamp {
            UdpLookup::Stale
        } else {
            UdpLookup::Found(&binding.owner)
        }
    }
    pub fn active(&self, owner: &UdpOwner) -> bool {
        self.bindings.get(&owner.endpoint).is_some_and(|b| {
            b.closed.is_none()
                && b.owner.identity == owner.identity
                && b.owner.timestamp == owner.timestamp
        })
    }
    pub fn collect(&mut self, before: i64) {
        self.bindings
            .retain(|_, b| b.closed.is_none_or(|closed| closed >= before));
        self.destinations
            .retain(|_, id| self.bindings.contains_key(id));
    }
}
fn compatible(bound: SocketAddrV4, actual: SocketAddrV4) -> bool {
    bound.port() == actual.port() && (bound.ip().is_unspecified() || bound.ip() == actual.ip())
}
