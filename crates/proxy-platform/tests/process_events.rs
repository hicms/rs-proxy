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
    let mut parent_pid = None;
    let mut received = 0;
    while Instant::now() < deadline {
        if let Ok(event) = events.receiver.recv_timeout(Duration::from_millis(100)) {
            received += 1;
            if let ProcessEvent::Started(process) = event
                && process.identity.pid == pid
            {
                parent_pid = process.parent.as_ref().map(|parent| parent.pid);
                observed = true;
                break;
            }
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let sync_required = events.take_sync_required();
    assert!(
        observed,
        "ETW 未收到子进程启动事件; received={received}, sync_required={sync_required}"
    );
    assert_eq!(parent_pid, Some(std::process::id()));
    assert!(!sync_required);
}
