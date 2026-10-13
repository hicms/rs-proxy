use proxy_core::{
    config::ProxyGroup,
    model::{GroupTraffic, ProcessIdentity, ProcessTraffic, TrafficSnapshot, TrafficStats},
    tree::ProcessTree,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Default)]
struct Bytes {
    uploaded: AtomicU64,
    downloaded: AtomicU64,
}

#[derive(Default)]
struct Sample {
    bytes: Arc<Bytes>,
    previous: [u64; 2],
    rates: [f64; 2],
}
impl Sample {
    fn counts(&self) -> [u64; 2] {
        [
            self.bytes.uploaded.load(Ordering::Relaxed),
            self.bytes.downloaded.load(Ordering::Relaxed),
        ]
    }
    fn sample(&mut self, seconds: f64) {
        let counts = self.counts();
        for (index, count) in counts.into_iter().enumerate() {
            self.rates[index] = count.saturating_sub(self.previous[index]) as f64 / seconds;
        }
        self.previous = counts;
    }
    fn snapshot(&self, running: bool) -> TrafficStats {
        let [uploaded, downloaded] = self.counts();
        TrafficStats {
            uploaded_bytes: uploaded as f64,
            downloaded_bytes: downloaded as f64,
            upload_bytes_per_second: if running { self.rates[0] } else { 0.0 },
            download_bytes_per_second: if running { self.rates[1] } else { 0.0 },
        }
    }
}

/// A group lifetime, retained by existing sessions after the configuration removes its ID.
#[derive(Clone, Debug)]
pub struct TrafficGroup {
    id: u32,
    generation: u64,
    bytes: Arc<Bytes>,
}
impl TrafficGroup {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

pub enum Direction {
    Upload,
    Download,
}

#[derive(Clone, Debug)]
pub struct TrafficMeter {
    counters: [Arc<Bytes>; 4],
}
impl TrafficMeter {
    pub async fn transfer<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &self,
        mut reader: R,
        mut writer: W,
        direction: Direction,
    ) -> std::io::Result<()> {
        let mut bytes = vec![0; 32768];
        loop {
            let count = reader.read(&mut bytes).await?;
            if count == 0 {
                return writer.shutdown().await;
            }
            let mut offset = 0;
            while offset < count {
                let written = writer.write(&bytes[offset..count]).await?;
                if written == 0 {
                    return Err(std::io::ErrorKind::WriteZero.into());
                }
                match direction {
                    Direction::Upload => self.record(written, 0),
                    Direction::Download => self.record(0, written),
                }
                offset += written;
            }
        }
    }
    pub fn record(&self, uploaded: usize, downloaded: usize) {
        for counter in &self.counters {
            if uploaded != 0 {
                counter
                    .uploaded
                    .fetch_add(uploaded as u64, Ordering::Relaxed);
            }
            if downloaded != 0 {
                counter
                    .downloaded
                    .fetch_add(downloaded as u64, Ordering::Relaxed);
            }
        }
    }
}

struct Group {
    handle: TrafficGroup,
    sample: Sample,
}
#[derive(Default)]
struct Process {
    total: Sample,
    groups: HashMap<u64, (u32, Sample)>,
}
struct Registry {
    total: Sample,
    groups: HashMap<u32, Group>,
    processes: HashMap<ProcessIdentity, Process>,
    next_generation: u64,
    sampled_at: Instant,
}

pub struct Traffic(Mutex<Registry>);
impl Traffic {
    pub fn new(now: Instant) -> Self {
        Self(Mutex::new(Registry {
            total: Sample::default(),
            groups: HashMap::new(),
            processes: HashMap::new(),
            next_generation: 0,
            sampled_at: now,
        }))
    }

    /// Called with the routing state locked, at each committed configuration update.
    pub fn sync_groups(&self, groups: &[ProxyGroup]) -> HashMap<u32, TrafficGroup> {
        let mut registry = self.0.lock().unwrap();
        registry
            .groups
            .retain(|id, _| groups.iter().any(|group| group.id == *id));
        for group in groups {
            if registry.groups.contains_key(&group.id) {
                continue;
            }
            registry.next_generation += 1;
            let sample = Sample::default();
            let handle = TrafficGroup {
                id: group.id,
                generation: registry.next_generation,
                bytes: sample.bytes.clone(),
            };
            registry.groups.insert(group.id, Group { handle, sample });
        }
        registry
            .groups
            .iter()
            .map(|(id, group)| (*id, group.handle.clone()))
            .collect()
    }

    pub fn register(&self, identity: &ProcessIdentity, group: &TrafficGroup) -> TrafficMeter {
        let mut registry = self.0.lock().unwrap();
        let total = registry.total.bytes.clone();
        let process = registry.processes.entry(identity.clone()).or_default();
        let pair = process
            .groups
            .entry(group.generation)
            .or_insert_with(|| (group.id, Sample::default()));
        TrafficMeter {
            counters: [
                total,
                group.bytes.clone(),
                process.total.bytes.clone(),
                pair.1.bytes.clone(),
            ],
        }
    }

    pub fn sample(&self, now: Instant, tree: &ProcessTree) {
        let mut registry = self.0.lock().unwrap();
        let seconds = now.duration_since(registry.sampled_at).as_secs_f64();
        if seconds == 0.0 {
            return;
        }
        registry.sampled_at = now;
        registry.total.sample(seconds);
        let generations: HashMap<_, _> = registry
            .groups
            .iter()
            .map(|(id, group)| (*id, group.handle.generation))
            .collect();
        for group in registry.groups.values_mut() {
            group.sample.sample(seconds);
        }
        registry.processes.retain(|identity, process| {
            if tree.identity(identity.pid) != Some(identity)
                && Arc::strong_count(&process.total.bytes) == 1
            {
                return false;
            }
            process.total.sample(seconds);
            process.groups.retain(|generation, (id, sample)| {
                if generations.get(id) != Some(generation) {
                    return false;
                }
                sample.sample(seconds);
                true
            });
            true
        });
    }

    pub fn total(&self) -> TrafficStats {
        self.0.lock().unwrap().total.snapshot(true)
    }

    pub fn snapshot(&self, running: bool) -> TrafficSnapshot {
        let registry = self.0.lock().unwrap();
        TrafficSnapshot {
            total: registry.total.snapshot(running),
            groups: registry
                .groups
                .iter()
                .map(|(id, group)| GroupTraffic {
                    group_id: *id,
                    stats: group.sample.snapshot(running),
                })
                .collect(),
            processes: registry
                .processes
                .iter()
                .map(|(identity, process)| ProcessTraffic {
                    identity: identity.clone(),
                    total: process.total.snapshot(running),
                    groups: process
                        .groups
                        .iter()
                        .filter(|(generation, (id, _))| {
                            registry
                                .groups
                                .get(id)
                                .is_some_and(|group| group.handle.generation == **generation)
                        })
                        .map(|(_, (id, sample))| GroupTraffic {
                            group_id: *id,
                            stats: sample.snapshot(running),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
