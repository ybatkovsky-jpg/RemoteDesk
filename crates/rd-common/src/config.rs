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
    /// Auto-generates a peer ID if none is set.
    pub fn load() -> Self {
        let path = Self::config_path();
        let mut config = match std::fs::read_to_string(&path) {
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
        };
        // Auto-generate a stable peer ID on first run.
        if config.id.is_empty() {
            config.generate_id();
            // Persist immediately so the ID survives restarts.
            let _ = config.save();
        }
        config
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

    /// Generate a stable peer ID from the machine's MAC address.
    /// Falls back to a random 9-digit number if MAC cannot be obtained.
    /// ID is a numeric string in the range 0..536870911 (like RustDesk).
    pub fn generate_id(&mut self) {
        if !self.id.is_empty() {
            return; // already has an ID
        }
        self.id = Self::auto_id();
    }

    /// Compute a peer ID from MAC address (same algorithm as RustDesk).
    pub fn auto_id() -> String {
        if let Ok(Some(ma)) = mac_address::get_mac_address() {
            let bytes = ma.bytes();
            let mut id: u32 = 0;
            // Skip first 2 bytes (OUI), use remaining for uniqueness
            for b in &bytes[2..] {
                id = (id << 8) | (*b as u32);
            }
            id &= 0x1FFF_FFFF; // 29 bits, like RustDesk
            id.to_string()
        } else {
            // Fallback: random 9-digit number
            use rand::Rng as _;
            let id: u32 = rand::rng().random_range(1_000_000_000u32..2_000_000_000u32);
            id.to_string()
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
