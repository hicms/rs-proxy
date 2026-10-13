#![windows_subsystem = "windows"]
mod commands;
mod desktop;
use std::sync::atomic::Ordering;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            desktop::show(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(desktop::setup)
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::save_config,
            commands::set_engine_running,
            commands::set_process_proxy,
            commands::delete_group,
            commands::test_proxy,
            commands::get_autostart,
            commands::set_autostart,
            commands::import_config,
            commands::export_config,
            commands::quit_app
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window
                    .state::<desktop::Desktop>()
                    .close_to_tray
                    .load(Ordering::Acquire)
                {
                    let _ = window.hide();
                } else {
                    desktop::request_exit(window.app_handle().clone());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("启动 RS Proxy")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !app
                    .state::<desktop::Desktop>()
                    .exit_ready
                    .load(Ordering::Acquire)
            {
                api.prevent_exit();
                desktop::request_exit(app.clone());
            }
        });
}
