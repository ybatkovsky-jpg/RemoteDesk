//! Client side: connects to host, receives and decompresses frames.
//!
//! Phase 2: E2E encryption via NaCl/libsodium key exchange.
//! Phase 3: Authentication, chat, file transfer, display switching, audio.
//! Phase 4: Audio decoding + playback.

use audio::{AudioDecoder, AudioPlayer};
use codec::FrameDecoder;
use crypto::{KeyExchange, SessionCipher};
use rd_common::{Error, Result};
use rd_common::proto::{KeyEvent, MouseEvent};
use std::sync::Mutex as StdMutex;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};

use super::protocol::{self, FileEntry, NetworkMessage};

#[derive(Debug, Clone)]
pub struct ReceivedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { width: u32, height: u32 },
    Error(String),
}

/// Chat message stored for UI polling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatEntry {
    pub text: String,
    pub sender: String,
    pub timestamp: u64,
}

/// File transfer progress info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileTransferProgress {
    pub path: String,
    pub total_size: u64,
    pub received_bytes: u64,
    pub done: bool,
    pub error: Option<String>,
}

pub struct ClientSession {
    host_addr: String,
    latest_frame: Mutex<Option<ReceivedFrame>>,
    frame_tx: broadcast::Sender<()>,
    state: Mutex<ConnectionState>,
    shutdown_tx: StdMutex<Option<broadcast::Sender<()>>>,
    display_size: Mutex<Option<(u32, u32)>>,
    /// Writer half of the TCP stream, shared so input commands can send events.
    writer: Mutex<Option<OwnedWriteHalf>>,
    /// Session cipher for encrypted communication (set after key exchange).
    cipher: Mutex<Option<SessionCipher>>,
    /// Chat history for UI polling.
    chat_history: Mutex<Vec<ChatEntry>>,
    /// Latest file listing result.
    file_listing: Mutex<Option<Vec<FileEntry>>>,
    /// Current file transfer progress.
    file_progress: Mutex<Option<FileTransferProgress>>,
    /// Received file data buffer (in-memory for simplicity).
    file_buffer: Mutex<Vec<u8>>,
    /// Available displays on the host.
    host_displays: Mutex<Vec<rd_common::proto::DisplayInfo>>,
    /// Audio player (for received audio frames).
    audio_player: Mutex<Option<AudioPlayer>>,
    /// Audio decoder (Opus → PCM).
    audio_decoder: Mutex<Option<AudioDecoder>>,
}

// SAFETY: cpal Stream is !Send due to NotSendSyncAcrossAllPlatforms
// (primarily for macOS CoreAudio). On Windows/Linux the stream is
// thread-safe and we access it only through the message loop.
unsafe impl Send for ClientSession {}
unsafe impl Sync for ClientSession {}

impl ClientSession {
    pub fn new(host_addr: String) -> Self {
        let (frame_tx, _) = broadcast::channel(16);
        Self {
            host_addr,
            latest_frame: Mutex::new(None),
            frame_tx,
            state: Mutex::new(ConnectionState::Disconnected),
            shutdown_tx: StdMutex::new(None),
            display_size: Mutex::new(None),
            writer: Mutex::new(None),
            cipher: Mutex::new(None),
            chat_history: Mutex::new(Vec::new()),
            file_listing: Mutex::new(None),
            file_progress: Mutex::new(None),
            file_buffer: Mutex::new(Vec::new()),
            host_displays: Mutex::new(Vec::new()),
            audio_player: Mutex::new(None),
            audio_decoder: Mutex::new(None),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        self.connect_with_password(None).await
    }

    /// Connect to a host via relay server using its peer ID.
    /// The relay server bridges the TCP connection to the target host.
    pub async fn connect_by_id(
        &self,
        relay_addr: &str,
        peer_id: &str,
        password: Option<&str>,
    ) -> Result<()> {
        crypto::init();

        {
            let mut state = self.state.lock().await;
            *state = ConnectionState::Connecting;
        }

        // Connect to relay server.
        tracing::info!("Client connecting to relay at {} for peer {}", relay_addr, peer_id);
        let mut stream = TcpStream::connect(relay_addr).await.map_err(|e| {
            Error::Network(format!("Failed to connect to relay {}: {}", relay_addr, e))
        })?;

        // Send CONNECT command.
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(format!("CONNECT {}\n", peer_id).as_bytes())
            .await
            .map_err(|e| Error::Network(format!("Relay CONNECT write error: {}", e)))?;

        // Read response.
        let mut buf = [0u8; 256];
        use tokio::io::AsyncReadExt;
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| Error::Network(format!("Relay CONNECT read error: {}", e)))?;
        let response = String::from_utf8_lossy(&buf[..n]).trim().to_string();

        tracing::info!("Relay CONNECT response: {}", response);

        if response == "CONNECT_OK paired" || response == "CONNECT_OK waiting" {
            // Connected! The stream is now bridged to the host.
            // Run the normal handshake + encrypted loop over this stream.
            self.run_protocol(stream, password).await?;
        } else {
            return Err(Error::Network(format!(
                "Relay connection failed: {}",
                response
            )));
        }

        Ok(())
    }

    /// Connect and optionally authenticate with password (direct TCP).
    pub async fn connect_with_password(&self, password: Option<&str>) -> Result<()> {
        // ── Init crypto ───────────────────────────────────
        crypto::init();

        {
            let mut state = self.state.lock().await;
            *state = ConnectionState::Connecting;
        }

        let stream = TcpStream::connect(&self.host_addr).await.map_err(|e| {
            Error::Network(format!("Failed to connect to {}: {}", self.host_addr, e))
        })?;

        tracing::info!("Connected to host at {}", self.host_addr);

        self.run_protocol(stream, password).await
    }

    /// Internal: run the RemoteDesk protocol over an established TCP stream.
    /// Used by both direct TCP and relay-bridged connections.
    async fn run_protocol(&self, stream: TcpStream, password: Option<&str>) -> Result<()> {

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        *self.shutdown_tx.lock().unwrap() = Some(shutdown_tx);

        let (mut reader, mut writer) = stream.into_split();

        // ── Plaintext handshake ───────────────────────────
        protocol::write_message(
            &mut writer,
            &NetworkMessage::Hello {
                client_version: rd_common::VERSION.to_string(),
                supported_codecs: codec::supported_codecs(),
            },
        )
        .await?;

        match protocol::read_message(&mut reader).await? {
            Some(NetworkMessage::Welcome {
                display_width,
                display_height,
                selected_codec,
                ..
            }) => {
                tracing::info!(
                    "Welcome: {}x{}, codec={}",
                    display_width, display_height,
                    selected_codec.name()
                );
                *self.display_size.lock().await = Some((display_width, display_height));
            }
            other => {
                return Err(Error::Network(format!(
                    "Expected Welcome, got: {:?}",
                    other
                )));
            }
        }

        // ── Key exchange ─────────────────────────────────
        let key_exchange = KeyExchange::generate();
        protocol::write_message(
            &mut writer,
            &NetworkMessage::CryptoHandshake {
                public_key: key_exchange.public_key_bytes(),
            },
        )
        .await?;

        let host_public = match protocol::read_message(&mut reader).await? {
            Some(NetworkMessage::CryptoHandshakeAck { public_key }) => public_key,
            other => {
                return Err(Error::Network(format!(
                    "Expected CryptoHandshakeAck, got: {:?}",
                    other
                )));
            }
        };

        let shared_secret = key_exchange.compute_shared_secret(&host_public);
        let symmetric_key = shared_secret.derive_symmetric_key();
        let mut cipher = SessionCipher::new(&symmetric_key);
        tracing::info!("E2E encryption established");

        // ── Authentication (Phase 3) ─────────────────────
        // Always send login attempt if password is provided or not — host decides.
        {
            let login_pwd = password.unwrap_or("").to_string();
            protocol::write_encrypted(
                &mut writer,
                &NetworkMessage::LoginRequest { password: login_pwd },
                &mut cipher,
            )
            .await?;

            // Read response
            match protocol::read_encrypted(&mut reader, &mut cipher).await? {
                Some(NetworkMessage::LoginResponse { success, message }) => {
                    if !success {
                        return Err(Error::Network(format!("Authentication failed: {}", message)));
                    }
                    tracing::info!("Authenticated: {}", message);
                }
                other => {
                    return Err(Error::Network(format!(
                        "Expected LoginResponse, got: {:?}",
                        other
                    )));
                }
            }
        }

        // Publish writer and cipher for input forwarding.
        let cipher_send = cipher.clone();
        *self.cipher.lock().await = Some(cipher_send);
        *self.writer.lock().await = Some(writer);

        // Update state to connected.
        let display_size = *self.display_size.lock().await;
        if let Some((w, h)) = display_size {
            let mut state = self.state.lock().await;
            *state = ConnectionState::Connected { width: w, height: h };
        }

        // ── Encrypted message loop ────────────────────────
        let mut recv_cipher = cipher;
        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match protocol::read_encrypted(&mut reader, &mut recv_cipher).await {
                Ok(Some(NetworkMessage::VideoFrame(compressed))) => {
                    let mut decoder = FrameDecoder::new();
                    match decoder.decompress(&compressed) {
                        Ok(bgra_data) => {
                            let frame = ReceivedFrame {
                                data: bgra_data,
                                width: compressed.width,
                                height: compressed.height,
                            };
                            *self.latest_frame.lock().await = Some(frame);
                            let _ = self.frame_tx.send(());
                        }
                        Err(e) => {
                            tracing::error!("Decompression error: {}", e);
                        }
                    }
                }
                Ok(Some(NetworkMessage::ChatMessage { text, sender, timestamp })) => {
                    tracing::info!("Chat: {}: {}", sender, text);
                    let mut history = self.chat_history.lock().await;
                    history.push(ChatEntry { text, sender, timestamp });
                    if history.len() > 200 {
                        history.remove(0);
                    }
                }
                Ok(Some(NetworkMessage::FileListResponse { path: _, entries })) => {
                    tracing::info!("File listing received: {} entries", entries.len());
                    *self.file_listing.lock().await = Some(entries);
                }
                Ok(Some(NetworkMessage::FileStart { path, size })) => {
                    tracing::info!("File transfer started: {} ({} bytes)", path, size);
                    *self.file_progress.lock().await = Some(FileTransferProgress {
                        path: path.clone(),
                        total_size: size,
                        received_bytes: 0,
                        done: false,
                        error: None,
                    });
                    *self.file_buffer.lock().await = Vec::new();
                }
                Ok(Some(NetworkMessage::FileChunk { chunk_index: _, data })) => {
                    {
                        let mut buf = self.file_buffer.lock().await;
                        buf.extend_from_slice(&data);
                    }
                    if let Some(ref mut prog) = *self.file_progress.lock().await {
                        prog.received_bytes += data.len() as u64;
                    }
                }
                Ok(Some(NetworkMessage::FileEnd { path })) => {
                    tracing::info!("File transfer complete: {}", path);
                    // Save received file to downloads.
                    if let Some(ref prog) = *self.file_progress.lock().await {
                        let buf = self.file_buffer.lock().await.clone();
                        if let Err(e) = save_file(&prog.path, &buf).await {
                            tracing::error!("Failed to save file: {}", e);
                            let mut p = self.file_progress.lock().await;
                            if let Some(ref mut pp) = *p {
                                pp.error = Some(e.to_string());
                            }
                        }
                    }
                    if let Some(ref mut prog) = *self.file_progress.lock().await {
                        prog.done = true;
                    }
                }
                Ok(Some(NetworkMessage::FileSendOffer { path, size })) => {
                    tracing::info!("Host wants to send file: {} ({} bytes)", path, size);
                    // Auto-accept for now — send accept.
                    let _ = self.send_encrypted(&NetworkMessage::FileSendAccept { path }).await;
                    let _ = size;
                }
                Ok(Some(NetworkMessage::DisplayList(displays))) => {
                    tracing::info!("Received display list: {} displays", displays.len());
                    *self.host_displays.lock().await = displays;
                }
                Ok(Some(NetworkMessage::AudioFrame { data, timestamp: _ })) => {
                    // Decode and play audio.
                    let mut decoder_guard = self.audio_decoder.lock().await;
                    if decoder_guard.is_none() {
                        match AudioDecoder::new() {
                            Ok(d) => *decoder_guard = Some(d),
                            Err(e) => {
                                tracing::error!("Audio decoder init: {}", e);
                                continue;
                            }
                        }
                    }
                    if let Some(ref mut decoder) = *decoder_guard {
                        match decoder.decode(&data, audio::SAMPLES_PER_FRAME) {
                            Ok(pcm) => {
                                let mut player_guard = self.audio_player.lock().await;
                                if player_guard.is_none() {
                                    match AudioPlayer::new() {
                                        Ok(p) => *player_guard = Some(p),
                                        Err(e) => {
                                            tracing::error!("Audio player init: {}", e);
                                            continue;
                                        }
                                    }
                                }
                                if let Some(ref player) = *player_guard {
                                    player.push_samples(&pcm);
                                }
                            }
                            Err(e) => {
                                tracing::error!("Audio decode error: {}", e);
                            }
                        }
                    }
                }
                Ok(Some(NetworkMessage::Ping)) => {
                    let _ = self.send_encrypted(&NetworkMessage::Pong).await;
                }
                Ok(Some(NetworkMessage::Disconnect)) => {
                    tracing::info!("Host requested disconnect");
                    break;
                }
                Ok(None) => {
                    tracing::info!("Host closed connection");
                    break;
                }
                Ok(Some(other)) => {
                    tracing::trace!("Client received: {:?}", other);
                }
                Err(e) => {
                    tracing::error!("Read error on client: {}", e);
                    *self.state.lock().await = ConnectionState::Error(e.to_string());
                    break;
                }
            }
        }

        // Clear writer and cipher on disconnect.
        *self.writer.lock().await = None;
        *self.cipher.lock().await = None;
        *self.state.lock().await = ConnectionState::Disconnected;
        Ok(())
    }

    // ── Public accessors ──────────────────────────────────

    pub async fn latest_frame(&self) -> Option<ReceivedFrame> {
        self.latest_frame.lock().await.clone()
    }

    pub fn frame_receiver(&self) -> broadcast::Receiver<()> {
        self.frame_tx.subscribe()
    }

    pub async fn state(&self) -> ConnectionState {
        self.state.lock().await.clone()
    }

    pub async fn display_size(&self) -> Option<(u32, u32)> {
        *self.display_size.lock().await
    }

    pub async fn chat_history(&self) -> Vec<ChatEntry> {
        self.chat_history.lock().await.clone()
    }

    pub async fn file_listing(&self) -> Option<Vec<FileEntry>> {
        self.file_listing.lock().await.clone()
    }

    pub async fn file_progress(&self) -> Option<FileTransferProgress> {
        self.file_progress.lock().await.clone()
    }

    pub async fn host_displays(&self) -> Vec<rd_common::proto::DisplayInfo> {
        self.host_displays.lock().await.clone()
    }

    // ── Commands to send to host ──────────────────────────

    /// Send a key event to the remote host (encrypted).
    pub async fn send_key_event(&self, event: KeyEvent) -> Result<()> {
        tracing::debug!("Client sending KeyEvent: keycode={}, down={}", event.keycode, event.down);
        self.send_encrypted(&NetworkMessage::KeyEvent(event)).await
    }

    /// Send a mouse event to the remote host (encrypted).
    pub async fn send_mouse_event(&self, event: MouseEvent) -> Result<()> {
        tracing::debug!("Client sending MouseEvent: {:?} at ({}, {})", event.event_type, event.x, event.y);
        self.send_encrypted(&NetworkMessage::MouseEvent(event)).await
    }

    /// Send a chat message.
    pub async fn send_chat_message(&self, text: &str, sender: &str) -> Result<()> {
        self.send_encrypted(&NetworkMessage::ChatMessage {
            text: text.to_string(),
            sender: sender.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
        .await
    }

    /// Switch the host to a different display.
    pub async fn switch_display(&self, display_id: usize) -> Result<()> {
        self.send_encrypted(&NetworkMessage::SwitchDisplay { display_id }).await
    }

    /// Request a directory listing from the host.
    pub async fn request_file_list(&self, path: &str) -> Result<()> {
        *self.file_listing.lock().await = None;
        self.send_encrypted(&NetworkMessage::FileListRequest {
            path: path.to_string(),
        })
        .await
    }

    /// Request a file from the host.
    pub async fn request_file(&self, path: &str) -> Result<()> {
        *self.file_progress.lock().await = None;
        *self.file_buffer.lock().await = Vec::new();
        self.send_encrypted(&NetworkMessage::FileRequest {
            path: path.to_string(),
        })
        .await
    }

    /// Cancel an ongoing file transfer.
    pub async fn cancel_file_transfer(&self, reason: &str) -> Result<()> {
        self.send_encrypted(&NetworkMessage::FileCancel {
            reason: reason.to_string(),
        })
        .await
    }

    /// Toggle audio on/off.
    pub async fn send_audio_control(&self, enable: bool) -> Result<()> {
        self.send_encrypted(&NetworkMessage::AudioControl { enable }).await
    }

    /// Request display list from host.
    pub async fn request_display_list(&self) -> Result<()> {
        // Client sends a SwitchDisplay-like request; host responds with DisplayList.
        // For now, just request display 0 which triggers the host to send its list.
        self.send_encrypted(&NetworkMessage::SwitchDisplay { display_id: 0 }).await
    }

    /// Send a file to the host.
    pub async fn send_file_offer(&self, path: &str, size: u64) -> Result<()> {
        self.send_encrypted(&NetworkMessage::FileSendOffer {
            path: path.to_string(),
            size,
        })
        .await
    }

    /// Send a file chunk to the host.
    pub async fn send_file_chunk(&self, path: &str, chunk_index: u32, data: Vec<u8>) -> Result<()> {
        self.send_encrypted(&NetworkMessage::FileSendChunk {
            path: path.to_string(),
            chunk_index,
            data,
        })
        .await
    }

    /// Signal end of file send.
    pub async fn send_file_end(&self, path: &str) -> Result<()> {
        self.send_encrypted(&NetworkMessage::FileSendEnd {
            path: path.to_string(),
        })
        .await
    }

    // ── Helper ────────────────────────────────────────────

    /// Encrypt and send a message through the shared writer.
    async fn send_encrypted(&self, msg: &NetworkMessage) -> Result<()> {
        let mut writer_guard = self.writer.lock().await;
        let mut cipher_guard = self.cipher.lock().await;
        match (writer_guard.as_mut(), cipher_guard.as_mut()) {
            (Some(w), Some(c)) => protocol::write_encrypted(w, msg, c).await,
            _ => Err(Error::Network("Cannot send: not connected or encryption not established".into())),
        }
    }

    pub fn stop(&self) {
        tracing::info!("Client stop requested");
        // Send shutdown signal to break the message loop
        if let Ok(mut guard) = self.shutdown_tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }
}

// ── Helpers ────────────────────────────────────────────────

/// Save received file data to the downloads directory.
async fn save_file(path: &str, data: &[u8]) -> Result<()> {
    let recv_dir = dirs_next::download_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RemoteDesk");
    std::fs::create_dir_all(&recv_dir).ok();

    let file_name = std::path::Path::new(path)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
    let dest = recv_dir.join(file_name);

    std::fs::write(&dest, data)
        .map_err(|e| Error::Network(format!("Cannot write {:?}: {}", dest, e)))?;

    tracing::info!("File saved to {:?}", dest);
    Ok(())
}
