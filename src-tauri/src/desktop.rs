use rs_proxy::service::Service;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tokio::sync::Mutex;

pub struct Desktop {
    pub icons: std::sync::Arc<rs_proxy::icons::ProcessIcons>,
    pub service: Mutex<Result<Service, String>>,
    pub close_to_tray: AtomicBool,
    pub exit_ready: AtomicBool,
    exit_pending: AtomicBool,
    _logs: tracing_appender::non_blocking::WorkerGuard,
}
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn request_exit(app: AppHandle) {
    if app
        .state::<Desktop>()
        .exit_pending
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Desktop>();
        let mut guard = state.service.lock().await;
        let result = match guard.as_mut() {
            Ok(service) => service.stop().await,
            Err(_) => Ok(()),
        };
        drop(guard);
        match result {
            Ok(()) => {
                state.exit_ready.store(true, Ordering::Release);
                app.exit(0);
            }
            Err(error) => {
                state.exit_pending.store(false, Ordering::Release);
                let _ = app.emit("service-error", format!("退出未完成：{error:#}"));
                show(&app);
            }
        }
    });
}
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    if let Some(window) = app.get_webview_window("main") {
        window.set_decorations(false)?;
    }
    let data = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("rs-proxy")
        .max_log_files(7)
        .build(data.join("logs"))?;
    let (writer, logs) = tracing_appender::non_blocking(appender);
    let driver = app.path().resource_dir()?.join("windivert");
    #[cfg(debug_assertions)]
    let driver = if driver.join("WinDivert.dll").exists() {
        driver
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/windivert")
    };
    let service = Service::open(&data, driver).map_err(|e| format!("{e:#}"));
    let level = service
        .as_ref()
        .map(|s| s.config().log_level.as_str())
        .unwrap_or("info");
    tracing_subscriber::fmt()
        .with_env_filter(level)
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .ok();
    let close_to_tray = service.as_ref().is_ok_and(|s| s.config().ui.close_to_tray);
    app.manage(Desktop {
        icons: Default::default(),
        service: Mutex::new(service),
        close_to_tray: AtomicBool::new(close_to_tray),
        exit_ready: AtomicBool::new(false),
        exit_pending: AtomicBool::new(false),
        _logs: logs,
    });
    let show_item = MenuItem::with_id(app, "show", "打开 RS Proxy", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().ok_or("缺少应用图标")?.clone())
        .tooltip("RS Proxy")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app),
            "quit" => request_exit(app.clone()),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show(tray.app_handle());
            }
        })
        .build(app)?;
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<Desktop>();
        let mut guard = state.service.lock().await;
        if let Ok(service) = guard.as_mut()
            && let Err(error) = service.start().await
        {
            tracing::error!(%error,"代理引擎启动失败");
        }
    });
    if std::env::args().any(|arg| arg == "--minimized")
        && let Some(window) = app.get_webview_window("main")
    {
        window.hide()?;
    }
    Ok(())
}
