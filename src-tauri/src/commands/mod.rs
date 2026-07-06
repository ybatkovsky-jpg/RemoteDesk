use crate::state::{AppState, AppStatus};
use rd_common::config::Config;
use rd_common::proto::{DisplayInfo, KeyEvent, MouseEvent};
use network::client::ClientSession;
use network::host::HostSession;
use network::{ChatEntry, FileEntry, FileTransferProgress};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

// ── App info ────────────────────────────────────────────

#[tauri::command]
pub fn get_version() -> String {
    format!("RemoteDesk v{}", env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
pub async fn get_app_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let has_host = state.host.lock().await.is_some();

    let (mode, conn_state, w, h) = if has_host {
        ("host".into(), "listening".into(), 0u32, 0u32)
    } else if let Some(ref client) = *state.client.lock().await {
        let cs = client.state().await;
        let (cw, ch) = client.display_size().await.unwrap_or((0, 0));
        let state_str = match cs {
            network::client::ConnectionState::Disconnected => "disconnected",
            network::client::ConnectionState::Connecting => "connecting",
            network::client::ConnectionState::Connected { .. } => "connected",
            network::client::ConnectionState::Error(_) => "error",
        };
        ("client".into(), state_str.into(), cw, ch)
    } else {
        ("idle".into(), "disconnected".into(), 0u32, 0u32)
    };

    Ok(AppStatus {
        mode,
        host_port: 9000,
        client_addr: String::new(),
        connection_state: conn_state,
        display_width: w,
        display_height: h,
    })
}

// ── Config / Settings ───────────────────────────────────

#[tauri::command]
pub async fn load_config(state: State<'_, AppState>) -> Result<Config, String> {
    let config = Config::load();
    *state.config.lock().await = config.clone();
    Ok(config)
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    config.save()?;
    *state.config.lock().await = config;
    Ok(())
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().await.clone())
}

// ── Password / Auth ─────────────────────────────────────

#[tauri::command]
pub async fn set_host_password(state: State<'_, AppState>, password: String) -> Result<(), String> {
    *state.host_password.lock().await = Some(password);
    // Also persist to config
    let mut config = state.config.lock().await.clone();
    config.security.password = Some(state.host_password.lock().await.clone().unwrap());
    config.save()?;
    Ok(())
}

#[tauri::command]
pub async fn set_client_password(state: State<'_, AppState>, password: String) -> Result<(), String> {
    *state.client_password.lock().await = Some(password);
    Ok(())
}

// ── Displays ────────────────────────────────────────────

#[tauri::command]
pub fn list_displays() -> Result<Vec<DisplayInfo>, String> {
    screen_capture::list_displays().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_host_displays(state: State<'_, AppState>) -> Result<Vec<DisplayInfo>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => Ok(client.host_displays().await),
        None => Err("Not connected".into()),
    }
}

// ── Host commands ───────────────────────────────────────

#[tauri::command]
pub async fn start_host(
    app: AppHandle,
    state: State<'_, AppState>,
    display_id: usize,
    port: u16,
    fps: u32,
) -> Result<(), String> {
    tracing::info!("Starting host on port {} (display {}, {} fps)", port, display_id, fps);

    let mut host = HostSession::new(port);

    // Apply password if set
    let pwd = state.host_password.lock().await.clone();
    if let Some(p) = pwd {
        host.set_password(p);
    }

    let host = Arc::new(tokio::sync::Mutex::new(host));
    *state.host.lock().await = Some(host.clone());

    // Check if relay mode is configured
    let config = state.config.lock().await.clone();
    let peer_id = config.id.clone();
    let relay_addr = config.server.relay_server.clone();
    let relay_addr_for_mode = relay_addr.clone();

    let app_clone = app.clone();
    tokio::spawn(async move {
        let result = if !relay_addr.is_empty() {
            // Relay mode: register with relay server and wait for bridged client.
            tracing::info!("Host using relay server: {}", relay_addr);
            let mut h = host.lock().await;
            h.run_relay(display_id, fps, &relay_addr, &peer_id).await
        } else {
            // Direct mode: bind TCP listener and wait for direct connection.
            let mut h = host.lock().await;
            h.run(display_id, fps).await
        };

        match result {
            Ok(_) => {
                let _ = app_clone.emit("host-status", serde_json::json!({"status": "stopped"}));
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "host-status",
                    serde_json::json!({"status": "error", "message": e.to_string()}),
                );
            }
        }
    });

    let mode = if !relay_addr_for_mode.is_empty() { "relay" } else { "direct" };
    let _ = app.emit(
        "host-status",
        serde_json::json!({"status": "listening", "port": port, "mode": mode}),
    );

    Ok(())
}

#[tauri::command]
pub async fn stop_host(state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("Stopping host");
    if let Some(host) = state.host.lock().await.as_ref() {
        host.lock().await.stop();
    }
    *state.host.lock().await = None;
    Ok(())
}

// ── Client commands ─────────────────────────────────────

#[tauri::command]
pub async fn client_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    addr: String,
) -> Result<(), String> {
    tracing::info!("Connecting to host at {}", addr);

    let client = Arc::new(ClientSession::new(addr.clone()));
    *state.client.lock().await = Some(client.clone());

    let password = state.client_password.lock().await.clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        match client.connect_with_password(password.as_deref()).await {
            Ok(_) => {
                let _ = app_clone.emit(
                    "connection-state",
                    serde_json::json!({"state": "connected"}),
                );
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "connection-state",
                    serde_json::json!({"state": "error", "message": e.to_string()}),
                );
            }
        }
    });

    let _ = app.emit("connection-state", serde_json::json!({"state": "connecting"}));

    Ok(())
}

#[tauri::command]
pub async fn client_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(client) = state.client.lock().await.as_ref() {
        client.stop();
    }
    *state.client.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn client_connect_by_id(
    app: AppHandle,
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<(), String> {
    let config = state.config.lock().await.clone();
    let relay_addr = config.server.relay_server.clone();

    if relay_addr.is_empty() {
        return Err("No relay server configured. Set relay_server in Settings.".into());
    }

    tracing::info!("Connecting to peer {} via relay {}", peer_id, relay_addr);

    let client = Arc::new(ClientSession::new(relay_addr.clone()));
    *state.client.lock().await = Some(client.clone());

    let password = state.client_password.lock().await.clone();
    let app_clone = app.clone();
    let relay = relay_addr.clone();
    let id = peer_id.clone();

    tokio::spawn(async move {
        match client.connect_by_id(&relay, &id, password.as_deref()).await {
            Ok(_) => {
                let _ = app_clone.emit(
                    "connection-state",
                    serde_json::json!({"state": "connected"}),
                );
            }
            Err(e) => {
                let _ = app_clone.emit(
                    "connection-state",
                    serde_json::json!({"state": "error", "message": e.to_string()}),
                );
            }
        }
    });

    let _ = app.emit("connection-state", serde_json::json!({"state": "connecting"}));

    Ok(())
}

#[tauri::command]
pub async fn get_peer_id(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.config.lock().await.id.clone())
}

#[tauri::command]
pub async fn client_get_frame(state: State<'_, AppState>) -> Result<Option<String>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => match client.latest_frame().await {
            Some(frame) => {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);
                Ok(Some(b64))
            }
            None => Ok(None),
        },
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn client_get_frame_raw(state: State<'_, AppState>) -> Result<Option<String>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => match client.latest_frame().await {
            Some(frame) => {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);
                Ok(Some(b64))
            }
            None => Ok(None),
        },
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn client_get_frame_size(state: State<'_, AppState>) -> Result<(u32, u32), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.display_size().await.ok_or("Not connected".into()),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn client_get_state(state: State<'_, AppState>) -> Result<String, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => Ok(format!("{:?}", client.state().await)),
        None => Ok("disconnected".into()),
    }
}

// ── Input commands (Client → Host) ──────────────────────

#[tauri::command]
pub async fn send_key_event(
    state: State<'_, AppState>,
    event: KeyEvent,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.send_key_event(event).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn send_mouse_event(
    state: State<'_, AppState>,
    event: MouseEvent,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.send_mouse_event(event).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

// ── Multi-monitor ───────────────────────────────────────

#[tauri::command]
pub async fn switch_display(
    state: State<'_, AppState>,
    display_id: usize,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.switch_display(display_id).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

// ── Chat ────────────────────────────────────────────────

#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    text: String,
    sender: String,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.send_chat_message(&text, &sender).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn get_chat_history(state: State<'_, AppState>) -> Result<Vec<ChatEntry>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => Ok(client.chat_history().await),
        None => Err("Not connected".into()),
    }
}

// ── File Transfer ───────────────────────────────────────

#[tauri::command]
pub async fn request_file_list(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => {
            client.request_file_list(&path).await.map_err(|e| e.to_string())?;
            // Poll for response (simple approach — wait up to 2 seconds).
            for _ in 0..20 {
                if let Some(listing) = client.file_listing().await {
                    return Ok(listing);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err("Timeout waiting for file listing".into())
        }
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn request_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.request_file(&path).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn get_file_progress(
    state: State<'_, AppState>,
) -> Result<Option<FileTransferProgress>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => Ok(client.file_progress().await),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn cancel_file_transfer(
    state: State<'_, AppState>,
    reason: String,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.cancel_file_transfer(&reason).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
pub async fn send_file_to_host(
    state: State<'_, AppState>,
    path: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let client = match state.client.lock().await.as_ref() {
        Some(c) => c.clone(),
        None => return Err("Not connected".into()),
    };

    let size = data.len() as u64;
    let chunk_size = 65536usize; // 64KB
    let _total_chunks = data.len().div_ceil(chunk_size) as u32;

    client.send_file_offer(&path, size).await.map_err(|e| e.to_string())?;

    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        client
            .send_file_chunk(&path, i as u32, chunk.to_vec())
            .await
            .map_err(|e| e.to_string())?;
    }

    client.send_file_end(&path).await.map_err(|e| e.to_string())?;

    Ok(())
}

// ── Audio ───────────────────────────────────────────────

#[tauri::command]
pub async fn toggle_audio(
    state: State<'_, AppState>,
    enable: bool,
) -> Result<(), String> {
    match state.client.lock().await.as_ref() {
        Some(client) => client.send_audio_control(enable).await.map_err(|e| e.to_string()),
        None => Err("Not connected".into()),
    }
}
