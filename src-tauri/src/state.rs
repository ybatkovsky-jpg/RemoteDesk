use network::client::ClientSession;
use network::host::HostSession;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

/// Application-wide state managed by Tauri
#[allow(dead_code)]
pub struct AppState {
    /// Host session wrapped for shared access between commands and background task.
    pub host: Mutex<Option<Arc<Mutex<HostSession>>>>,
    /// Client session (when connected to a remote host)
    pub client: Mutex<Option<Arc<ClientSession>>>,
    /// Host shutdown signal (deprecated — host now uses its own internal shutdown).
    pub host_shutdown: Mutex<Option<broadcast::Sender<()>>>,
    /// App handle for emitting events (set during setup)
    pub app_handle: Mutex<Option<tauri::AppHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            host: Mutex::new(None),
            client: Mutex::new(None),
            host_shutdown: Mutex::new(None),
            app_handle: Mutex::new(None),
        }
    }
}

/// Serializable state for frontend
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStatus {
    pub mode: String,       // "idle", "host", "client"
    pub host_port: u16,
    pub client_addr: String,
    pub connection_state: String,
    pub display_width: u32,
    pub display_height: u32,
}
