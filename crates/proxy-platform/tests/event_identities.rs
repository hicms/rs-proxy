#[path = "../src/events/identities.rs"]
mod identities;

use identities::Identities;
use proxy_core::model::ProcessIdentity;

fn identity(pid: u32, sequence: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        sequence: sequence.to_string(),
    }
}

#[test]
fn exits_return_the_original_system_identity() {
    let mut events = Identities::default();
    let process = identity(7, 100);
    assert!(events.started(55, 10, process.clone()));
    assert!(
        !events.started(55, 12, process.clone()),
        "rundown must not duplicate a start"
    );
    assert_eq!(events.exited(7, 55, 20), Some(process));
    assert_eq!(events.exited(7, 55, 20), None);
    assert!(!events.take_sync_required());
}

#[test]
fn stale_events_cannot_replace_or_exit_a_reused_pid() {
    let mut events = Identities::default();
    let old = identity(7, 100);
    let new = identity(7, 101);
    assert!(events.started(55, 10, old.clone()));
    assert!(events.started(66, 30, new.clone()));
    assert!(!events.started(55, 10, old));
    assert_eq!(events.exited(7, 55, 20), None);
    assert_eq!(events.exited(7, 66, 40), Some(new));
}

#[test]
fn reused_kernel_keys_still_require_a_current_timestamp() {
    let mut events = Identities::default();
    let new = identity(7, 101);
    assert!(events.started(55, 30, new.clone()));
    assert_eq!(events.exited(7, 55, 20), None);
    assert_eq!(events.exited(7, 55, 40), Some(new));
}

#[test]
fn known_and_unknown_exits_prevent_late_or_equal_starts() {
    let mut events = Identities::default();
    let old = identity(7, 100);
    assert!(events.started(55, 10, old.clone()));
    assert_eq!(events.exited(7, 55, 20), Some(old.clone()));
    assert!(!events.started(55, 10, old.clone()));
    assert!(!events.started(55, 20, old));
    assert_eq!(events.exited(8, 66, 20), None);
    assert!(!events.started(66, 19, identity(8, 101)));
    assert!(!events.started(66, 20, identity(8, 101)));
    assert!(events.started(77, 30, identity(8, 102)));
    assert!(
        !events.take_sync_required(),
        "unknown exits must not cause a snapshot storm"
    );
}

#[test]
fn mismatched_exit_does_not_invent_an_identity() {
    let mut events = Identities::default();
    assert!(events.started(55, 10, identity(7, 100)));
    assert_eq!(events.exited(7, 66, 20), None);
    assert!(!events.take_sync_required());
}

#[test]
fn discarded_history_cannot_be_recreated_by_old_events() {
    let mut events = Identities::default();
    let process = identity(7, 100);
    assert!(events.started(55, 10, process.clone()));
    events.forget_history();
    assert!(events.take_sync_required());
    assert!(!events.take_sync_required());
    assert!(!events.started(55, 10, process));
    assert_eq!(events.exited(7, 55, 9), None);
    assert!(events.started(66, 20, identity(7, 101)));
}

#[test]
fn unresolved_events_advance_the_discard_watermark() {
    let mut events = Identities::default();
    assert!(events.started(55, 10, identity(7, 100)));
    events.observe(100);
    events.forget_history();
    assert!(events.take_sync_required());
    assert!(!events.started(66, 50, identity(8, 101)));
    assert!(!events.started(66, 100, identity(8, 101)));
    assert!(events.started(77, 101, identity(9, 102)));
}

#[test]
fn an_earlier_duplicate_start_preserves_the_original_lifetime() {
    let mut events = Identities::default();
    let process = identity(7, 100);
    assert!(events.started(55, 20, process.clone()));
    assert!(!events.started(55, 10, process.clone()));
    assert_eq!(events.exited(7, 55, 15), Some(process.clone()));
    assert!(!events.started(55, 10, process));
}

#[test]
fn capacity_bounds_active_records_and_exit_watermarks() {
    let mut events = Identities::default();
    for pid in 1..=16_384 {
        if pid % 2 == 0 {
            assert!(events.started(
                u64::from(pid),
                u64::from(pid),
                identity(pid, u64::from(pid))
            ));
        } else {
            assert_eq!(events.exited(pid, u64::from(pid), u64::from(pid)), None);
        }
    }
    assert!(!events.started(50_000, 50_000, identity(50_000, 50_000)));
    assert!(events.take_sync_required());
    assert!(!events.take_sync_required());
    assert!(!events.started(2, 2, identity(2, 2)));
    assert_eq!(events.exited(2, 2, 3), None);
    assert!(events.started(50_001, 50_001, identity(50_001, 50_001)));
    assert_eq!(
        events.exited(50_001, 50_001, 50_002),
        Some(identity(50_001, 50_001))
    );
}
