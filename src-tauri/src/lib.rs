mod commands;
mod state;

use state::AppState;

/// Application entry point called from main.rs
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("remote_desk=debug"))
                .add_directive("tauri=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting RemoteDesk v{}", rd_common::VERSION);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::get_app_status,
            commands::list_displays,
            commands::start_host,
            commands::stop_host,
            commands::client_connect,
            commands::client_disconnect,
            commands::client_get_frame,
            commands::client_get_frame_size,
            commands::client_get_state,
            commands::send_key_event,
            commands::send_mouse_event,
        ])
        .setup(|app| {
            tracing::info!("RemoteDesk setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running RemoteDesk");
}
