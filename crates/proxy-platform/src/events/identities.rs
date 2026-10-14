use proxy_core::model::ProcessIdentity;
use std::collections::HashMap;

const CAPACITY: usize = 16_384;

struct Entry {
    key: u64,
    timestamp: u64,
    identity: Option<ProcessIdentity>,
}

#[derive(Default)]
pub struct Identities {
    entries: HashMap<u32, Entry>,
    latest: u64,
    discarded_through: u64,
    needs_sync: bool,
}

impl Identities {
    pub fn started(&mut self, key: u64, timestamp: u64, identity: ProcessIdentity) -> bool {
        if !self.admit(identity.pid, timestamp) {
            return false;
        }
        if let Some(previous) = self.entries.get_mut(&identity.pid) {
            if previous.key == key && previous.identity.as_ref() == Some(&identity) {
                previous.timestamp = previous.timestamp.min(timestamp);
                return false;
            }
            if previous.timestamp >= timestamp {
                return false;
            }
        }
        self.entries.insert(
            identity.pid,
            Entry {
                key,
                timestamp,
                identity: Some(identity),
            },
        );
        true
    }

    pub fn exited(&mut self, pid: u32, key: u64, timestamp: u64) -> Option<ProcessIdentity> {
        if !self.admit(pid, timestamp) {
            return None;
        }
        if self
            .entries
            .get(&pid)
            .is_some_and(|entry| entry.timestamp > timestamp)
        {
            return None;
        }
        let previous = self.entries.insert(
            pid,
            Entry {
                key,
                timestamp,
                identity: None,
            },
        );
        previous
            .filter(|entry| entry.key == key)
            .and_then(|entry| entry.identity)
    }

    fn admit(&mut self, pid: u32, timestamp: u64) -> bool {
        self.observe(timestamp);
        if self.entries.len() >= CAPACITY && !self.entries.contains_key(&pid) {
            self.forget_history();
        }
        timestamp > self.discarded_through
    }

    pub fn observe(&mut self, timestamp: u64) {
        self.latest = self.latest.max(timestamp);
    }

    pub fn forget_history(&mut self) {
        // Keep a watermark so delayed events cannot recreate identities discarded after loss.
        self.discarded_through = self.latest;
        self.entries.clear();
        self.needs_sync = true;
    }

    pub fn take_sync_required(&mut self) -> bool {
        std::mem::take(&mut self.needs_sync)
    }
}
