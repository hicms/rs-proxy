use proxy_platform::{
    events::{ProcessEvent, ProcessEvents},
    is_elevated,
};
use std::{
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn kernel_events_observe_a_new_child_without_polling() {
    assert!(is_elevated(), "此 Windows 集成测试需要管理员权限");
    let events = ProcessEvents::start().unwrap();
    let mut child = Command::new("ping.exe")
        .args(["-n", "30", "127.0.0.1"])
        .creation_flags(0x08000000)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed = false;
    while Instant::now() < deadline {
        if matches!(events.receiver.recv_timeout(Duration::from_millis(100)), Ok(ProcessEvent::Started(process)) if process.identity.pid == pid && process.parent.as_ref().is_some_and(|parent| parent.pid == std::process::id()))
        {
            observed = true;
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(observed, "ETW 未收到子进程启动事件");
    assert!(!events.take_sync_required());
}
