use crate::state::{AppState, AppStatus};
use rd_common::proto::{DisplayInfo, KeyEvent, MouseEvent};
use network::client::ClientSession;
use network::host::HostSession;
use screen_capture;
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
    let has_client = state.client.lock().await.is_some();

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

// ── Displays ────────────────────────────────────────────

#[tauri::command]
pub fn list_displays() -> Result<Vec<DisplayInfo>, String> {
    screen_capture::list_displays().map_err(|e| e.to_string())
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

    let host = Arc::new(tokio::sync::Mutex::new(HostSession::new(port)));

    // Store handle so stop_host can access it.
    *state.host.lock().await = Some(host.clone());

    // Spawn the host in background — clone Arc into the task.
    let app_clone = app.clone();
    tokio::spawn(async move {
        let result = {
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

    let _ = app.emit(
        "host-status",
        serde_json::json!({"status": "listening", "port": port}),
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

    // Store for later use
    *state.client.lock().await = Some(client.clone());

    // Spawn connection in background
    let app_clone = app.clone();
    tokio::spawn(async move {
        match client.connect().await {
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

/// Return raw BGRA bytes — frontend receives as ArrayBuffer (no base64 overhead).
#[tauri::command]
pub async fn client_get_frame_raw(state: State<'_, AppState>) -> Result<Option<Vec<u8>>, String> {
    match state.client.lock().await.as_ref() {
        Some(client) => match client.latest_frame().await {
            Some(frame) => Ok(Some(frame.data)),
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
