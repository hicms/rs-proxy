mod identities;

use crate::windows::{status, wide};
use anyhow::{Context as _, Result};
use identities::Identities;
use proxy_core::model::{ProcessIdentity, ProcessInfo};
use std::{
    mem::size_of,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
};
use windows_sys::{
    Win32::System::{
        Diagnostics::Etw::*,
        Rpc::{RPC_S_UUID_LOCAL_ONLY, UuidCreate},
    },
    core::GUID,
};

#[derive(Debug)]
pub enum ProcessEvent {
    Started(ProcessInfo),
    Exited(ProcessIdentity),
}

struct CallbackContext {
    sender: SyncSender<ProcessEvent>,
    needs_sync: Arc<AtomicBool>,
    state: Mutex<CallbackState>,
}

#[derive(Default)]
struct CallbackState {
    identities: Identities,
    events_lost: u32,
    buffers_lost: u32,
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    unsafe {
        let record = &*record;
        let context = &*record.UserContext.cast::<CallbackContext>();
        let provider = record.EventHeader.ProviderId;
        if (
            provider.data1,
            provider.data2,
            provider.data3,
            provider.data4,
        ) != (
            ProcessGuid.data1,
            ProcessGuid.data2,
            ProcessGuid.data3,
            ProcessGuid.data4,
        ) || !matches!(
            u32::from(record.EventHeader.EventDescriptor.Opcode),
            EVENT_TRACE_TYPE_START | EVENT_TRACE_TYPE_END | EVENT_TRACE_TYPE_DC_START
        ) {
            return;
        }
        let mut state = context.state.lock().unwrap();
        match decode(record, &mut state.identities) {
            Ok(Some(event)) => {
                if context.sender.try_send(event).is_err() {
                    context.needs_sync.store(true, Ordering::Release);
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "解码进程事件失败");
                context.needs_sync.store(true, Ordering::Release);
            }
        }
        if state.identities.take_sync_required() {
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
fn decode(record: &EVENT_RECORD, identities: &mut Identities) -> Result<Option<ProcessEvent>> {
    // ProcessTrace converts the QPC session clock to FILETIME unless RAW_TIMESTAMP is set.
    let timestamp = u64::try_from(record.EventHeader.TimeStamp).context("无效的进程事件时间")?;
    identities.observe(timestamp);
    let pid = u32::from_ne_bytes(
        property(record, "ProcessId")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("无效的进程 ID 字段"))?,
    );
    let key = property(record, "UniqueProcessKey")?;
    let key = match key.as_slice() {
        [a, b, c, d] => u64::from(u32::from_ne_bytes([*a, *b, *c, *d])),
        [a, b, c, d, e, f, g, h] => u64::from_ne_bytes([*a, *b, *c, *d, *e, *f, *g, *h]),
        _ => anyhow::bail!("无效的进程事件标识字段"),
    };
    if u32::from(record.EventHeader.EventDescriptor.Opcode) == EVENT_TRACE_TYPE_END {
        return Ok(identities
            .exited(pid, key, timestamp)
            .map(ProcessEvent::Exited));
    }
    let process = match crate::process::query_before(pid, Some(timestamp)) {
        Ok(process) => process,
        Err(error) => {
            // Protected or already vanished processes cannot supply a proven system identity.
            tracing::debug!(pid, %error, "进程事件身份不可用");
            return Ok(None);
        }
    };
    Ok(identities
        .started(key, timestamp, process.identity.clone())
        .then_some(ProcessEvent::Started(process)))
}
unsafe extern "system" fn on_buffer(log: *mut EVENT_TRACE_LOGFILEW) -> u32 {
    unsafe {
        let context = &*(*log).Context.cast::<CallbackContext>();
        let header = &(*log).LogfileHeader;
        let events_lost = header.Anonymous2.Anonymous.EventsLost;
        let mut state = context.state.lock().unwrap();
        if events_lost > state.events_lost || header.BuffersLost > state.buffers_lost {
            state.identities.forget_history();
            state.identities.take_sync_required();
            context.needs_sync.store(true, Ordering::Release);
        }
        state.events_lost = events_lost;
        state.buffers_lost = header.BuffersLost;
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
        let mut session_guid = GUID::default();
        let result = unsafe { UuidCreate(&mut session_guid) };
        if result != RPC_S_UUID_LOCAL_ONLY {
            status(result as u32).context("创建进程事件会话标识")?;
        }
        let name = wide(&format!(
            "RSProxy.Process.{}.{:08x}{:04x}{:04x}{:016x}",
            std::process::id(),
            session_guid.data1,
            session_guid.data2,
            session_guid.data3,
            u64::from_be_bytes(session_guid.data4)
        ));
        let bytes = size_of::<EVENT_TRACE_PROPERTIES>() + name.len() * 2;
        let mut storage = vec![0u64; bytes.div_ceil(8)];
        let properties = storage.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
        unsafe {
            (*properties).Wnode.BufferSize = bytes as u32;
            (*properties).Wnode.ClientContext = 1;
            (*properties).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*properties).Wnode.Guid = session_guid;
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
            (*properties).EnableFlags = EVENT_TRACE_FLAG_PROCESS;
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
            state: Mutex::new(CallbackState::default()),
        });
        let mut instance = Self {
            receiver,
            session,
            consumer: PROCESSTRACE_HANDLE { Value: u64::MAX },
            thread: None,
            context,
        };
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
