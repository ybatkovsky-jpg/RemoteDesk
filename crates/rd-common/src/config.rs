use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

impl Config {
    /// Compute the config directory for the current platform.
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RemoteDesk")
    }

    /// Full path to config.toml.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load config from the standard location. Returns default if no config exists.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                toml::from_str(&contents).unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse config at {:?}: {}. Using defaults.", path, e);
                    Config::default()
                })
            }
            Err(_) => {
                tracing::info!("No config found at {:?}, using defaults.", path);
                Config::default()
            }
        }
    }

    /// Save config to the standard location. Creates directories if needed.
    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create config dir: {}", e))?;

        let path = Self::config_path();
        let contents = toml::to_string_pretty(self).map_err(|e| format!("TOML serialize error: {}", e))?;

        std::fs::write(&path, contents).map_err(|e| format!("Cannot write config: {}", e))?;

        tracing::info!("Config saved to {:?}", path);
        Ok(())
    }

    /// Generate a new random peer ID.
    pub fn generate_id(&mut self) {
        use rand::Rng;
        let id: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();
        self.id = format!("rd-{}", id);
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
