use network::client::ClientSession;
use network::host::HostSession;
use rd_common::config::Config;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// Application-wide state managed by Tauri
#[allow(dead_code)]
pub struct AppState {
    /// Host session wrapped for shared access between commands and background task.
    pub host: Mutex<Option<Arc<Mutex<HostSession>>>>,
    /// Client session (when connected to a remote host)
    pub client: Mutex<Option<Arc<ClientSession>>>,
    /// Host shutdown signal.
    pub host_shutdown: Mutex<Option<broadcast::Sender<()>>>,
    /// App handle for emitting events (set during setup)
    pub app_handle: Mutex<Option<tauri::AppHandle>>,
    /// Application configuration (loaded from disk).
    pub config: Mutex<Config>,
    /// Host password (set by user).
    pub host_password: Mutex<Option<String>>,
    /// Client password (for connecting).
    pub client_password: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            host: Mutex::new(None),
            client: Mutex::new(None),
            host_shutdown: Mutex::new(None),
            app_handle: Mutex::new(None),
            config: Mutex::new(Config::default()),
            host_password: Mutex::new(None),
            client_password: Mutex::new(None),
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
