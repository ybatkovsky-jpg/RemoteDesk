use serde::{Deserialize, Serialize};

/// Main configuration for the RemoteDesk application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Unique peer ID
    pub id: String,
    /// Server connection settings
    pub server: ServerConfig,
    /// Video capture settings
    pub video: VideoConfig,
    /// Security settings
    pub security: SecurityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            id: String::new(),
            server: ServerConfig::default(),
            video: VideoConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Rendezvous server address (host:port)
    pub rendezvous_server: String,
    /// Relay server address (host:port)
    pub relay_server: String,
    /// API server for token-based auth
    pub api_server: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            rendezvous_server: String::new(),
            relay_server: String::new(),
            api_server: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    /// Maximum frames per second
    pub max_fps: u32,
    /// Preferred codec (h264, h265, vp9)
    pub codec: String,
    /// Quality preset (0-100)
    pub quality: u32,
    /// Bitrate in kbps
    pub bitrate: u32,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            max_fps: 30,
            codec: "h264".to_string(),
            quality: 75,
            bitrate: 5000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Password for peer authentication
    pub password: Option<String>,
    /// Key pair for NaCl encryption
    pub key_pair: Option<KeyPair>,
    /// Allowed permissions
    pub permissions: Permissions,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            password: None,
            key_pair: None,
            permissions: Permissions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub keyboard: bool,
    pub mouse: bool,
    pub clipboard: bool,
    pub file_transfer: bool,
    pub audio: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            keyboard: true,
            mouse: true,
            clipboard: true,
            file_transfer: true,
            audio: false,
        }
    }
}
