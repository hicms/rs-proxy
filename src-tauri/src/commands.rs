use crate::desktop::Desktop;
use proxy_core::{
    config::{Config, ProxyGroup},
    model::{AutostartState, ProcessIdentity, ProxyTestResult, Snapshot},
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

fn message(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}
#[tauri::command]
pub async fn get_snapshot(state: State<'_, Desktop>) -> Result<Snapshot, String> {
    state
        .service
        .lock()
        .await
        .as_ref()
        .map_err(Clone::clone)?
        .snapshot()
        .map_err(message)
}
#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, Desktop>,
    config: Config,
) -> Result<Config, String> {
    let saved = state
        .service
        .lock()
        .await
        .as_mut()
        .map_err(|e| e.clone())?
        .save(config)
        .await
        .map_err(message)?;
    state
        .close_to_tray
        .store(saved.ui.close_to_tray, std::sync::atomic::Ordering::Release);
    let _ = app.emit("config-changed", &saved);
    Ok(saved)
}
#[tauri::command]
pub async fn set_engine_running(state: State<'_, Desktop>, running: bool) -> Result<(), String> {
    let mut guard = state.service.lock().await;
    let service = guard.as_mut().map_err(|e| e.clone())?;
    if running {
        service.start().await
    } else {
        service.stop().await
    }
    .map_err(message)
}
#[tauri::command]
pub async fn set_process_proxy(
    state: State<'_, Desktop>,
    identity: ProcessIdentity,
    group_id: Option<u32>,
) -> Result<(), String> {
    state
        .service
        .lock()
        .await
        .as_ref()
        .map_err(Clone::clone)?
        .set_process_proxy(&identity, group_id)
        .map_err(message)
}
#[tauri::command]
pub async fn delete_group(
    state: State<'_, Desktop>,
    from: u32,
    to: u32,
    revision: u32,
) -> Result<Config, String> {
    state
        .service
        .lock()
        .await
        .as_mut()
        .map_err(|e| e.clone())?
        .delete_group(from, to, revision)
        .await
        .map_err(message)
}
#[tauri::command]
pub async fn test_proxy(group: ProxyGroup) -> Result<ProxyTestResult, String> {
    proxy_engine::socks::test(&group).await.map_err(message)
}
#[tauri::command]
pub async fn get_autostart() -> Result<AutostartState, String> {
    tokio::task::spawn_blocking(proxy_platform::autostart::query)
        .await
        .map_err(message)?
        .map_err(message)
}
#[tauri::command]
pub async fn set_autostart(enabled: bool, start_minimized: bool) -> Result<AutostartState, String> {
    tokio::task::spawn_blocking(move || {
        proxy_platform::autostart::set(&std::env::current_exe()?, enabled, start_minimized)
    })
    .await
    .map_err(message)?
    .map_err(message)
}
#[tauri::command]
pub async fn import_config(
    app: AppHandle,
    state: State<'_, Desktop>,
    revision: u32,
) -> Result<Option<Config>, String> {
    let picker = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
    })
    .await
    .map_err(message)?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(message)?;
    let bytes = tokio::fs::read(path).await.map_err(message)?;
    let mut config: Config = serde_json::from_slice(&bytes).map_err(message)?;
    config.revision = revision;
    save_config(app, state, config).await.map(Some)
}
#[tauri::command]
pub async fn export_config(app: AppHandle, state: State<'_, Desktop>) -> Result<bool, String> {
    let bytes = serde_json::to_vec_pretty(
        state
            .service
            .lock()
            .await
            .as_ref()
            .map_err(Clone::clone)?
            .config(),
    )
    .map_err(message)?;
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name("rs-proxy.json")
            .blocking_save_file()
    })
    .await
    .map_err(message)?;
    let Some(path) = picked else {
        return Ok(false);
    };
    let path = path.into_path().map_err(message)?;
    tokio::task::spawn_blocking(move || proxy_core::store::atomic_write(&path, &bytes))
        .await
        .map_err(message)?
        .map_err(message)?;
    Ok(true)
}
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    crate::desktop::request_exit(app);
}
