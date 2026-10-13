use proxy_core::{
    config::Config,
    model::{ProcessIdentity, ProcessInfo},
    tree::ProcessTree,
};
use proxy_engine::traffic::{Direction, Traffic};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::io::AsyncWrite;

fn identity(sequence: &str) -> ProcessIdentity {
    ProcessIdentity {
        pid: 42,
        sequence: sequence.into(),
    }
}
fn process(identity: ProcessIdentity) -> ProcessInfo {
    ProcessInfo {
        identity,
        parent: None,
        name: "client.exe".into(),
        image_path: String::new(),
        cmdline: String::new(),
    }
}

#[test]
fn rates_follow_elapsed_time_and_idle_processes_are_identity_scoped() {
    let now = Instant::now();
    let traffic = Traffic::new(now);
    let groups = traffic.sync_groups(&Config::default().proxy_groups);
    let mut tree = ProcessTree::default();
    tree.upsert(process(identity("1")));
    let meter = traffic.register(&identity("1"), &groups[&0]);
    meter.record(100, 400);
    traffic.sample(now + Duration::from_secs(2), &tree);
    let snapshot = traffic.snapshot(true);
    assert_eq!(snapshot.total.upload_bytes_per_second, 50.0);
    assert_eq!(snapshot.groups[0].stats.download_bytes_per_second, 200.0);
    assert_eq!(snapshot.processes[0].total.downloaded_bytes, 400.0);
    let stopped = traffic.snapshot(false);
    for stats in [
        &stopped.total,
        &stopped.groups[0].stats,
        &stopped.processes[0].total,
        &stopped.processes[0].groups[0].stats,
    ] {
        assert_eq!(stats.upload_bytes_per_second, 0.0);
        assert_eq!(stats.download_bytes_per_second, 0.0);
        assert_eq!(stats.downloaded_bytes, 400.0);
    }
    tree.upsert(process(identity("2")));
    let reused = traffic.register(&identity("2"), &groups[&0]);
    reused.record(20, 0);
    traffic.sample(now + Duration::from_secs(3), &tree);
    let snapshot = traffic.snapshot(true);
    assert_eq!(
        snapshot
            .processes
            .iter()
            .find(|p| p.identity == identity("1"))
            .unwrap()
            .total
            .upload_bytes_per_second,
        0.0
    );
    assert_eq!(
        snapshot
            .processes
            .iter()
            .find(|p| p.identity == identity("2"))
            .unwrap()
            .total
            .uploaded_bytes,
        20.0
    );
    drop(meter);
    drop(reused);
    traffic.sample(now + Duration::from_secs(4), &tree);
    let snapshot = traffic.snapshot(true);
    assert_eq!(snapshot.processes.len(), 1);
    assert_eq!(snapshot.total.upload_bytes_per_second, 0.0);
    assert_eq!(snapshot.groups[0].stats.uploaded_bytes, 120.0);
    tree.exit(&identity("2"));
    traffic.sample(now + Duration::from_secs(5), &tree);
    assert!(traffic.snapshot(true).processes.is_empty());
    assert_eq!(traffic.total().downloaded_bytes, 400.0);
}

#[test]
fn deleting_and_reusing_a_group_id_does_not_reassign_existing_or_pending_flows() {
    let now = Instant::now();
    let traffic = Traffic::new(now);
    let config = Config::default();
    let old = traffic.sync_groups(&config.proxy_groups);
    let meter = traffic.register(&identity("1"), &old[&0]);
    meter.record(10, 20);
    traffic.sync_groups(&[]);
    let new = traffic.sync_groups(&config.proxy_groups);
    assert_ne!(old[&0].generation(), new[&0].generation());
    meter.record(30, 40);
    let late = traffic.register(&identity("1"), &old[&0]);
    late.record(50, 60);
    let current = traffic.register(&identity("1"), &new[&0]);
    current.record(70, 80);
    traffic.sample(now + Duration::from_secs(1), &ProcessTree::default());
    let snapshot = traffic.snapshot(true);
    assert_eq!(snapshot.total.uploaded_bytes, 160.0);
    assert_eq!(snapshot.processes[0].total.downloaded_bytes, 200.0);
    assert_eq!(snapshot.groups[0].stats.uploaded_bytes, 70.0);
    assert_eq!(snapshot.processes[0].groups.len(), 1);
    assert_eq!(snapshot.processes[0].groups[0].stats.downloaded_bytes, 80.0);
    let late_after_sample = traffic.register(&identity("1"), &old[&0]);
    late_after_sample.record(10, 10);
    assert_eq!(traffic.snapshot(true).processes[0].groups.len(), 1);
    assert_eq!(traffic.snapshot(true).groups[0].stats.uploaded_bytes, 70.0);
}

struct PartialWriter {
    remaining: usize,
    zero: bool,
}
impl AsyncWrite for PartialWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.remaining == 0 {
            return Poll::Ready(if self.zero {
                Ok(0)
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            });
        }
        let written = bytes.len().min(self.remaining).min(3);
        self.remaining -= written;
        Poll::Ready(Ok(written))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn transfer_counts_successful_partial_writes_even_when_the_next_write_fails() {
    let traffic = Traffic::new(Instant::now());
    let groups = traffic.sync_groups(&Config::default().proxy_groups);
    let meter = traffic.register(&identity("1"), &groups[&0]);
    let data = [7; 16];
    let error = meter
        .transfer(
            &data[..],
            PartialWriter {
                remaining: 7,
                zero: false,
            },
            Direction::Upload,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    let error = meter
        .transfer(
            &data[..],
            PartialWriter {
                remaining: 5,
                zero: true,
            },
            Direction::Download,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);
    let mut received = Vec::new();
    meter
        .transfer(&data[..], &mut received, Direction::Download)
        .await
        .unwrap();
    assert_eq!(received, data);
    let snapshot = traffic.snapshot(true);
    assert_eq!(snapshot.total.uploaded_bytes, 7.0);
    assert_eq!(snapshot.groups[0].stats.downloaded_bytes, 21.0);
    assert_eq!(snapshot.processes[0].total.downloaded_bytes, 21.0);
}
