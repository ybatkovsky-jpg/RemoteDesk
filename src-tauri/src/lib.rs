mod commands;
mod state;

use state::AppState;

/// Application entry point called from main.rs
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Note: logging is handled by tauri-plugin-log, not manual tracing_subscriber init

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .manage(AppState::new())
    .invoke_handler(tauri::generate_handler![
        commands::get_version,
        commands::get_app_status,
        commands::load_config,
        commands::save_config,
        commands::get_config,
        commands::set_host_password,
        commands::set_client_password,
        commands::list_displays,
        commands::get_host_displays,
        commands::start_host,
        commands::stop_host,
        commands::client_connect,
        commands::client_connect_by_id,
        commands::client_disconnect,
        commands::client_get_frame,
        commands::client_get_frame_raw,
        commands::client_get_frame_size,
        commands::client_get_state,
        commands::get_peer_id,
        commands::send_key_event,
        commands::send_mouse_event,
        commands::switch_display,
        commands::send_chat_message,
        commands::get_chat_history,
        commands::request_file_list,
        commands::request_file,
        commands::get_file_progress,
        commands::cancel_file_transfer,
        commands::send_file_to_host,
        commands::toggle_audio,
    ])
        .setup(|_app| {
            tracing::info!("RemoteDesk setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running RemoteDesk");
}
