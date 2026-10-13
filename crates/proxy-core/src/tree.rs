use crate::{
    config::Protocol,
    model::{ProcessIdentity, ProcessInfo, ProcessView, Route},
    rules::RoutingPolicy,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default)]
pub struct ProcessTree {
    records: HashMap<ProcessIdentity, ProcessInfo>,
    live: HashMap<u32, ProcessIdentity>,
    manual: HashMap<ProcessIdentity, u32>,
    retired: HashMap<u32, u64>,
}

impl ProcessTree {
    pub fn upsert(&mut self, process: ProcessInfo) {
        let id = &process.identity;
        let sequence = id.sequence.parse::<u64>().expect("system process sequence");
        let retired = self
            .retired
            .get(&id.pid)
            .is_some_and(|old| sequence <= *old);
        let replaced = self.live.get(&id.pid).is_some_and(|old| {
            old.sequence
                .parse::<u64>()
                .expect("system process sequence")
                > sequence
        });
        if !retired && !replaced {
            self.live.insert(id.pid, id.clone());
        }
        self.records.insert(process.identity.clone(), process);
    }

    pub fn exit(&mut self, identity: &ProcessIdentity) {
        if self.live.get(&identity.pid) == Some(identity) {
            self.live.remove(&identity.pid);
        }
        let sequence = identity
            .sequence
            .parse::<u64>()
            .expect("system process sequence");
        self.retired
            .entry(identity.pid)
            .and_modify(|old| *old = (*old).max(sequence))
            .or_insert(sequence);
        // Keep ancestry until the next full reconciliation: start/exit buffers can arrive out of order.
    }

    pub fn synchronize(&mut self, processes: Vec<ProcessInfo>) {
        self.live.clear();
        for process in processes {
            self.upsert(process);
        }
        self.collect_expired();
    }

    fn ancestors(&self, identity: &ProcessIdentity) -> Vec<&ProcessInfo> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some(identity);
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            let Some(process) = self.records.get(id) else {
                break;
            };
            chain.push(process);
            current = process.parent.as_ref();
        }
        chain
    }

    fn collect_expired(&mut self) {
        let retained: HashSet<_> = self
            .live
            .values()
            .flat_map(|id| self.ancestors(id))
            .map(|p| p.identity.clone())
            .collect();
        self.records.retain(|id, _| retained.contains(id));
        self.manual.retain(|id, _| retained.contains(id));
    }

    pub fn identity(&self, pid: u32) -> Option<&ProcessIdentity> {
        self.live.get(&pid)
    }

    pub fn set_manual(&mut self, pid: u32, group: Option<u32>) -> bool {
        let Some(identity) = self.live.get(&pid).cloned() else {
            return false;
        };
        match group {
            Some(group) => {
                self.manual.insert(identity, group);
            }
            None => {
                let descendants: Vec<_> = self
                    .live
                    .values()
                    .filter(|id| self.ancestors(id).iter().any(|p| p.identity == identity))
                    .cloned()
                    .collect();
                for id in descendants {
                    self.manual.remove(&id);
                }
            }
        }
        true
    }

    pub fn uses_group(&self, group: u32) -> bool {
        self.manual.values().any(|id| *id == group)
    }

    pub fn reassign_group(&mut self, from: u32, to: u32) {
        for group in self.manual.values_mut() {
            if *group == from {
                *group = to;
            }
        }
    }

    pub fn route(
        &self,
        identity: &ProcessIdentity,
        protocol: Protocol,
        policy: &RoutingPolicy,
    ) -> Option<Route> {
        let ancestors = self.ancestors(identity);
        ancestors
            .iter()
            .find_map(|p| {
                self.manual.get(&p.identity).map(|group| Route {
                    group_id: *group,
                    rule_id: None,
                    source: "manual".into(),
                })
            })
            .or_else(|| policy.classify(&ancestors, protocol))
    }

    pub fn snapshot(&self, policy: &RoutingPolicy) -> Vec<ProcessView> {
        let mut views: Vec<_> = self
            .live
            .values()
            .filter_map(|id| {
                self.records.get(id).map(|process| ProcessView {
                    process: process.clone(),
                    route: self.route(id, Protocol::Both, policy),
                })
            })
            .collect();
        views.sort_by(|a, b| {
            a.process
                .name
                .to_lowercase()
                .cmp(&b.process.name.to_lowercase())
                .then(a.process.identity.pid.cmp(&b.process.identity.pid))
        });
        views
    }
}
