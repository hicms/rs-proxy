use proxy_platform::{
    events::{ProcessEvent, ProcessEvents},
    is_elevated, process,
};
use std::{
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct TestChild(Child);
impl TestChild {
    fn spawn() -> Self {
        let ping = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/ping.exe");
        Self(
            Command::new(ping)
                .args(["-n", "30", "127.0.0.1"])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    fn stop(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}
impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn assert_lifecycle(events: &ProcessEvents, child: &mut TestChild) {
    let expected = process::query(child.0.id()).unwrap();
    let parent = process::query(std::process::id()).unwrap().identity;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = None;
    let mut received = 0;
    while Instant::now() < deadline {
        if let Ok(event) = events.receiver.recv_timeout(Duration::from_millis(100)) {
            received += 1;
            if let ProcessEvent::Started(process) = event
                && process.identity.pid == expected.identity.pid
            {
                started = Some(process);
                break;
            }
        }
    }
    child.stop();
    let sync_required = events.take_sync_required();
    let started = started.unwrap_or_else(|| {
        panic!("ETW 未收到子进程启动事件; received={received}, sync_required={sync_required}")
    });
    assert_eq!(started.identity, expected.identity);
    assert_eq!(started.parent, Some(parent));
    assert_eq!(started.image_path, expected.image_path);
    assert!(!sync_required);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut exited = None;
    while Instant::now() < deadline {
        if let Ok(ProcessEvent::Exited(identity)) =
            events.receiver.recv_timeout(Duration::from_millis(100))
            && identity.pid == expected.identity.pid
        {
            exited = Some(identity);
            break;
        }
    }
    assert_eq!(
        exited,
        Some(expected.identity),
        "ETW 退出必须匹配启动时的系统身份"
    );
    assert!(!events.take_sync_required());
}

#[test]
fn kernel_events_observe_child_lifecycles_after_repeated_subscriptions_without_polling() {
    assert!(is_elevated(), "此 Windows 集成测试需要管理员权限");
    for _ in 0..3 {
        let events = ProcessEvents::start().unwrap();
        let mut child = TestChild::spawn();
        assert_lifecycle(&events, &mut child);
    }
}

#[test]
fn rundown_tracks_the_exit_of_a_process_running_before_subscription() {
    assert!(is_elevated(), "此 Windows 集成测试需要管理员权限");
    let mut child = TestChild::spawn();
    let events = ProcessEvents::start().unwrap();
    assert_lifecycle(&events, &mut child);
}
