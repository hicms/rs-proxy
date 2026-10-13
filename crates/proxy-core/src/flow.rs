use crate::model::{ProcessIdentity, Route};
use std::{collections::HashMap, net::SocketAddrV4};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FlowKey {
    pub local: SocketAddrV4,
    pub remote: SocketAddrV4,
}

#[derive(Clone, Debug)]
pub struct FlowDecision<R = Route> {
    pub endpoint: Option<u64>,
    pub timestamp: i64,
    pub route: Option<R>,
    pub identity: Option<ProcessIdentity>,
    pub sequence: Option<u32>,
    pub pinned_direct: bool,
}

pub struct FlowTable<R = Route> {
    decisions: HashMap<FlowKey, FlowDecision<R>>,
    next: HashMap<FlowKey, FlowDecision<R>>,
}
impl<R> Default for FlowTable<R> {
    fn default() -> Self {
        Self {
            decisions: HashMap::new(),
            next: HashMap::new(),
        }
    }
}
impl<R: Clone> FlowTable<R> {
    pub fn publish(
        &mut self,
        key: FlowKey,
        endpoint: u64,
        timestamp: i64,
        route: Option<R>,
        identity: Option<ProcessIdentity>,
    ) -> bool {
        if self
            .next
            .get(&key)
            .is_some_and(|old| timestamp <= old.timestamp)
        {
            return false;
        }
        let decision = FlowDecision {
            endpoint: Some(endpoint),
            timestamp,
            route,
            identity,
            sequence: None,
            pinned_direct: false,
        };
        if let Some(old) = self.decisions.get_mut(&key) {
            if timestamp <= old.timestamp {
                if old.pinned_direct && old.endpoint.is_none() {
                    old.endpoint = Some(endpoint);
                }
                return false;
            }
            // A newer CONNECT belongs to the next generation, never a retransmission.
            if old.pinned_direct || old.sequence.is_some() {
                self.next.insert(key, decision);
                return true;
            }
        }
        self.decisions.insert(key, decision);
        true
    }

    pub fn on_syn(
        &mut self,
        key: &FlowKey,
        sequence: u32,
        timestamp: i64,
        ttl: i64,
    ) -> Option<Option<R>> {
        if let Some(old) = self.decisions.get(key) {
            if old.sequence == Some(sequence) {
                return Some(old.route.clone());
            }
            if old.timestamp > timestamp {
                return None;
            }
            if old.sequence.is_some() {
                self.promote(key);
            }
        }
        let old = self.decisions.get_mut(key)?;
        if old.timestamp > timestamp {
            return None;
        }
        if timestamp.saturating_sub(old.timestamp) > ttl {
            self.promote(key);
            return None;
        }
        old.sequence = Some(sequence);
        Some(old.route.clone())
    }

    pub fn abandon(&mut self, key: FlowKey, sequence: u32, timestamp: i64) {
        self.decisions.insert(
            key,
            FlowDecision {
                endpoint: None,
                timestamp,
                route: None,
                identity: None,
                sequence: Some(sequence),
                pinned_direct: true,
            },
        );
    }

    pub fn get(&self, key: &FlowKey) -> Option<&FlowDecision<R>> {
        self.decisions.get(key)
    }

    pub fn close(&mut self, key: &FlowKey, endpoint: u64) {
        if self
            .next
            .get(key)
            .is_some_and(|value| value.endpoint == Some(endpoint))
        {
            self.next.remove(key);
        }
        if self
            .decisions
            .get(key)
            .is_some_and(|value| value.endpoint == Some(endpoint) && value.route.is_none())
        {
            self.promote(key);
        }
    }

    pub fn release(&mut self, key: &FlowKey, timestamp: i64) {
        if self
            .decisions
            .get(key)
            .is_some_and(|value| value.timestamp == timestamp)
        {
            self.promote(key);
        }
    }
    fn promote(&mut self, key: &FlowKey) {
        self.decisions.remove(key);
        if let Some(next) = self.next.remove(key) {
            self.decisions.insert(key.clone(), next);
        }
    }
    pub fn collect_unowned(&mut self, before: i64) {
        self.decisions
            .retain(|_, d| d.endpoint.is_some() || d.timestamp >= before);
    }
}
