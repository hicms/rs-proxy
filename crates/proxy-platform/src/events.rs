use crate::windows::{status, wide};
use anyhow::{Context as _, Result};
use proxy_core::model::{ProcessIdentity, ProcessInfo};
use std::{
    mem::size_of,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
};
use windows_sys::{Win32::System::Diagnostics::Etw::*, core::GUID};

const PROCESS_PROVIDER: GUID = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);

#[derive(Debug)]
pub enum ProcessEvent {
    Started(ProcessInfo),
    Exited(ProcessIdentity),
}

struct CallbackContext {
    sender: SyncSender<ProcessEvent>,
    needs_sync: Arc<AtomicBool>,
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    unsafe {
        let record = &*record;
        let context = &*record.UserContext.cast::<CallbackContext>();
        if !matches!(record.EventHeader.EventDescriptor.Id, 1 | 2) {
            return;
        }
        let event = decode(record);
        if event.is_err() || event.is_ok_and(|event| context.sender.try_send(event).is_err()) {
            context.needs_sync.store(true, Ordering::Release);
        }
    }
}

fn property(record: &EVENT_RECORD, name: &str) -> Result<Vec<u8>> {
    let name = wide(name);
    let descriptor = PROPERTY_DATA_DESCRIPTOR {
        PropertyName: name.as_ptr() as u64,
        ArrayIndex: u32::MAX,
        Reserved: 0,
    };
    let mut size = 0;
    status(unsafe { TdhGetPropertySize(record, 0, ptr::null(), 1, &descriptor, &mut size) })?;
    let mut buffer = vec![0u8; size as usize];
    status(unsafe {
        TdhGetProperty(
            record,
            0,
            ptr::null(),
            1,
            &descriptor,
            size,
            buffer.as_mut_ptr(),
        )
    })?;
    Ok(buffer)
}
fn decode(record: &EVENT_RECORD) -> Result<ProcessEvent> {
    let pid = u32::from_ne_bytes(
        property(record, "ProcessID")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("无效的进程 ID 字段"))?,
    );
    let sequence = u64::from_ne_bytes(
        property(record, "ProcessSequenceNumber")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("无效的进程序号字段"))?,
    );
    let identity = ProcessIdentity {
        pid,
        sequence: sequence.to_string(),
    };
    if record.EventHeader.EventDescriptor.Id == 2 {
        return Ok(ProcessEvent::Exited(identity));
    }
    if let Ok(live) = crate::process::query(pid)
        && live.identity == identity
    {
        return Ok(ProcessEvent::Started(live));
    }
    let parent_pid = u32::from_ne_bytes(
        property(record, "ParentProcessID")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("无效的父进程字段"))?,
    );
    let parent_sequence = u64::from_ne_bytes(
        property(record, "ParentProcessSequenceNumber")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("无效的父进程序号字段"))?,
    );
    let raw_image = property(record, "ImageName")?;
    let image: Vec<u16> = raw_image
        .chunks_exact(2)
        .map(|s| u16::from_ne_bytes([s[0], s[1]]))
        .take_while(|c| *c != 0)
        .collect();
    let image_path = String::from_utf16_lossy(&image);
    let name = image_path
        .rsplit('\\')
        .next()
        .unwrap_or(&image_path)
        .to_owned();
    let parent = (parent_pid != 0 && parent_sequence < sequence).then(|| ProcessIdentity {
        pid: parent_pid,
        sequence: parent_sequence.to_string(),
    });
    Ok(ProcessEvent::Started(ProcessInfo {
        identity,
        parent,
        name,
        image_path,
        cmdline: String::new(),
    }))
}
unsafe extern "system" fn on_buffer(log: *mut EVENT_TRACE_LOGFILEW) -> u32 {
    unsafe {
        if (*log).EventsLost > 0 {
            let context = &*(*log).Context.cast::<CallbackContext>();
            context.needs_sync.store(true, Ordering::Release);
        }
    }
    1
}

pub struct ProcessEvents {
    pub receiver: Receiver<ProcessEvent>,
    session: CONTROLTRACE_HANDLE,
    consumer: PROCESSTRACE_HANDLE,
    thread: Option<JoinHandle<()>>,
    context: Box<CallbackContext>,
}
impl ProcessEvents {
    pub fn start() -> Result<Self> {
        let name = wide(&format!("RSProxy.Process.{}", std::process::id()));
        let bytes = size_of::<EVENT_TRACE_PROPERTIES>() + name.len() * 2;
        let mut storage = vec![0u64; bytes.div_ceil(8)];
        let properties = storage.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
        unsafe {
            (*properties).Wnode.BufferSize = bytes as u32;
            (*properties).Wnode.ClientContext = 1;
            (*properties).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            (*properties).FlushTimer = 1;
            (*properties).LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                storage
                    .as_mut_ptr()
                    .cast::<u8>()
                    .add(size_of::<EVENT_TRACE_PROPERTIES>())
                    .cast::<u16>(),
                name.len(),
            );
        }
        let mut session = CONTROLTRACE_HANDLE::default();
        status(unsafe { StartTraceW(&mut session, name.as_ptr(), properties) })
            .context("启动进程事件会话")?;
        let (sender, receiver) = mpsc::sync_channel(4096);
        let context = Box::new(CallbackContext {
            sender,
            needs_sync: Arc::new(AtomicBool::new(false)),
        });
        let mut instance = Self {
            receiver,
            session,
            consumer: PROCESSTRACE_HANDLE { Value: u64::MAX },
            thread: None,
            context,
        };
        status(unsafe {
            EnableTraceEx2(
                session,
                &PROCESS_PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                5,
                0x10,
                0,
                0,
                ptr::null(),
            )
        })
        .context("订阅系统进程事件")?;
        let mut log = EVENT_TRACE_LOGFILEW {
            LoggerName: name.as_ptr().cast_mut(),
            Context: (&mut *instance.context as *mut CallbackContext).cast(),
            BufferCallback: Some(on_buffer),
            ..Default::default()
        };
        log.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        log.Anonymous2.EventRecordCallback = Some(on_event);
        let consumer = unsafe { OpenTraceW(&mut log) };
        if consumer.Value == u64::MAX {
            return Err(std::io::Error::last_os_error()).context("打开进程事件消费者");
        }
        instance.consumer = consumer;
        let needs_sync = instance.context.needs_sync.clone();
        instance.thread = Some(thread::Builder::new().name("process-events".into()).spawn(
            move || {
                let result = unsafe { ProcessTrace(&consumer, 1, ptr::null(), ptr::null()) };
                if result != 0 && result != 1223 {
                    tracing::error!(result, "进程事件消费结束");
                    needs_sync.store(true, Ordering::Release);
                }
            },
        )?);
        Ok(instance)
    }
    pub fn take_sync_required(&self) -> bool {
        self.context.needs_sync.swap(false, Ordering::AcqRel)
    }
}
impl Drop for ProcessEvents {
    fn drop(&mut self) {
        let mut properties = EVENT_TRACE_PROPERTIES::default();
        properties.Wnode.BufferSize = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        unsafe {
            let result = ControlTraceW(
                self.session,
                ptr::null(),
                &mut properties,
                EVENT_TRACE_CONTROL_STOP,
            );
            // STOP has already succeeded when only the optional properties buffer is short.
            if result != 0 && result != windows_sys::Win32::Foundation::ERROR_MORE_DATA {
                tracing::warn!(result, "停止进程事件会话失败");
            }
            if self.consumer.Value != u64::MAX {
                CloseTrace(self.consumer);
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
