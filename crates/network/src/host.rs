//! Host side: captures screen, compresses frames, sends to connected client.
//!
//! Phase 2: E2E encryption via NaCl/libsodium key exchange.
//! Phase 3: Authentication, chat, file transfer, display switching.

use codec::FrameEncoder;
use crypto::{KeyExchange, SessionCipher};
use input_sim::InputSimulator;
use rd_common::{Error, Result};
use screen_capture::Capturer;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

use super::protocol::{self, FileEntry, NetworkMessage};

pub struct HostSession {
    port: u16,
    password: Option<String>,
    capturer: Option<Capturer>,
    encoder: FrameEncoder,
    shutdown_tx: Option<broadcast::Sender<()>>,
    /// Current FPS setting (modifiable via UpdateSettings).
    fps: u32,
    /// Current quality (0-100).
    quality: u32,
}

impl HostSession {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            password: None,
            capturer: None,
            encoder: FrameEncoder::new(),
            shutdown_tx: None,
            fps: 15,
            quality: 75,
        }
    }

    /// Set a password required for client authentication.
    pub fn set_password(&mut self, password: String) {
        self.password = Some(password);
    }

    pub async fn run(&mut self, display_id: usize, fps: u32) -> Result<()> {
        self.fps = fps;
        // ── Init crypto ───────────────────────────────────
        crypto::init();

        let capturer = Capturer::new(display_id)?;
        let width = capturer.width();
        let height = capturer.height();
        self.capturer = Some(capturer);

        let (shutdown_tx, _) = broadcast::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await.map_err(|e| {
            Error::Network(format!("Failed to bind {}: {}", addr, e))
        })?;

        tracing::info!("Host listening on {} ({}x{} @ {}fps)", addr, width, height, self.fps);

        let (stream, client_addr) = listener.accept().await.map_err(|e| {
            Error::Network(format!("Accept error: {}", e))
        })?;

        tracing::info!("Client connected from {}", client_addr);

        self.handle_client(stream, width, height).await?;

        Ok(())
    }

    async fn handle_client(
        &mut self,
        stream: TcpStream,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let frame_interval = std::time::Duration::from_secs_f64(1.0 / self.fps as f64);
        let _frame_timeout = frame_interval / 2;

        let (mut reader, mut writer) = stream.into_split();
        let shutdown_tx = self.shutdown_tx.as_ref().unwrap().clone();
        let mut shutdown_rx = shutdown_tx.subscribe();

        // ── Plaintext handshake ───────────────────────────
        let selected_codec = match protocol::read_message(&mut reader).await? {
            Some(NetworkMessage::Hello {
                client_version,
                supported_codecs,
            }) => {
                tracing::info!(
                    "Client hello: version={}, codecs={:?}",
                    client_version,
                    supported_codecs.iter().map(|c| c.name()).collect::<Vec<_>>()
                );
                let host_codecs = codec::supported_codecs();
                codec::negotiate_codec(&host_codecs, &supported_codecs)
                    .unwrap_or(codec::CompressionType::Zstd)
            }
            other => {
                return Err(Error::Network(format!(
                    "Expected Hello, got: {:?}",
                    other
                )));
            }
        };

        self.encoder = FrameEncoder::new().with_codec(selected_codec);

        protocol::write_message(
            &mut writer,
            &NetworkMessage::Welcome {
                host_version: rd_common::VERSION.to_string(),
                display_width: width,
                display_height: height,
                selected_codec,
            },
        )
        .await?;

        tracing::info!(
            "Sent Welcome: {}x{}, codec={}",
            width, height, selected_codec.name()
        );

        // ── Key exchange ─────────────────────────────────
        let key_exchange = KeyExchange::generate();

        let client_public = match protocol::read_message(&mut reader).await? {
            Some(NetworkMessage::CryptoHandshake { public_key }) => public_key,
            other => {
                return Err(Error::Network(format!(
                    "Expected CryptoHandshake, got: {:?}",
                    other
                )));
            }
        };

        protocol::write_message(
            &mut writer,
            &NetworkMessage::CryptoHandshakeAck {
                public_key: key_exchange.public_key_bytes(),
            },
        )
        .await?;

        let shared_secret = key_exchange.compute_shared_secret(&client_public);
        let symmetric_key = shared_secret.derive_symmetric_key();
        let cipher = Arc::new(Mutex::new(SessionCipher::new(&symmetric_key)));
        tracing::info!("E2E encryption established with client");

        // ── Authentication (Phase 3) ─────────────────────
        if let Some(ref expected_password) = self.password {
            let auth_msg = {
                let mut c = cipher.lock().await;
                protocol::read_encrypted(&mut reader, &mut c).await
            };

            let authenticated = match auth_msg {
                Ok(Some(NetworkMessage::LoginRequest { password })) => {
                    password == *expected_password
                }
                _ => false,
            };

            let response = if authenticated {
                NetworkMessage::LoginResponse {
                    success: true,
                    message: "Authenticated".into(),
                }
            } else {
                NetworkMessage::LoginResponse {
                    success: false,
                    message: "Invalid password".into(),
                }
            };

            {
                let mut c = cipher.lock().await;
                protocol::write_encrypted(&mut writer, &response, &mut c).await?;
            }

            if !authenticated {
                tracing::warn!("Client authentication failed");
                return Err(Error::Network("Authentication failed".into()));
            }

            tracing::info!("Client authenticated successfully");
        }

        // Wrap capturer and encoder in Arcs for the reader task to access.
        let capturer_arc = Arc::new(Mutex::new(self.capturer.take()));
        let encoder_arc = Arc::new(Mutex::new(std::mem::replace(
            &mut self.encoder,
            FrameEncoder::new(),
        )));
        let fps_arc: Arc<Mutex<u32>> = Arc::new(Mutex::new(self.fps));
        let quality_arc: Arc<Mutex<u32>> = Arc::new(Mutex::new(self.quality));

        // ── Spawn reader task ────────────────────────────
        let reader_cipher = cipher.clone();
        let reader_capturer = capturer_arc.clone();
        let reader_fps = fps_arc.clone();
        let reader_quality = quality_arc.clone();
        tokio::spawn(async move {
            let mut input_sim = InputSimulator::new();
            loop {
                let msg = {
                    let mut c = reader_cipher.lock().await;
                    protocol::read_encrypted(&mut reader, &mut c).await
                };
                match msg {
                    Ok(Some(NetworkMessage::KeyEvent(event))) => {
                        tracing::debug!("Host received KeyEvent: keycode={}, down={}", event.keycode, event.down);
                        if let Err(e) = input_sim.simulate_key(&event) {
                            tracing::error!("Input simulation error (key): {}", e);
                        }
                    }
                    Ok(Some(NetworkMessage::MouseEvent(event))) => {
                        tracing::debug!(
                            "Host received MouseEvent: {:?} at ({}, {})",
                            event.event_type, event.x, event.y
                        );
                        if let Err(e) = input_sim.simulate_mouse(&event) {
                            tracing::error!("Input simulation error (mouse): {}", e);
                        }
                    }
                    Ok(Some(NetworkMessage::UpdateSettings { max_fps, quality })) => {
                        tracing::info!("Client requested settings: fps={}, quality={}", max_fps, quality);
                        *reader_fps.lock().await = max_fps;
                        *reader_quality.lock().await = quality;
                    }
                    Ok(Some(NetworkMessage::SwitchDisplay { display_id })) => {
                        tracing::info!("Client requested display switch to {}", display_id);
                        match Capturer::new(display_id) {
                            Ok(new_cap) => {
                                *reader_capturer.lock().await = Some(new_cap);
                                tracing::info!("Switched to display {}", display_id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to switch display: {}", e);
                            }
                        }
                    }
                    Ok(Some(NetworkMessage::DisplayList(displays))) => {
                        // Host receives display list request — we don't respond here,
                        // the client's DisplayList is client->host for requests.
                        // Actually this is host->client for response. Ignore on host side.
                        let _ = displays;
                    }
                    Ok(Some(NetworkMessage::ClipboardText { content })) => {
                        tracing::debug!("Received clipboard: {} chars", content.len());
                        if let Err(e) = input_sim.set_clipboard(&content) {
                            tracing::error!("Clipboard set error: {}", e);
                        }
                    }
                    Ok(Some(NetworkMessage::AudioControl { enable })) => {
                        tracing::info!("Client requested audio: {}", enable);
                        // Phase 3: audio capture/encoding not yet implemented.
                    }
                    Ok(Some(NetworkMessage::ChatMessage { text, sender, timestamp })) => {
                        tracing::info!("Chat from {}: {}", sender, text);
                        // Chat is echoed back to all clients in the reader task via the
                        // writer's Arc — but host doesn't echo chat. Client handles display.
                        // For now, just log. Real relay requires writer access.
                        let _ = timestamp;
                    }
                    Ok(Some(NetworkMessage::FileListRequest { path })) => {
                        tracing::info!("Client requested file listing: {}", path);
                        // File listing is handled inline via the writer — but we don't have
                        // writer access here. We'll handle it in a separate channel.
                    }
                    Ok(Some(NetworkMessage::FileRequest { path })) => {
                        tracing::info!("Client requested file: {}", path);
                        // File serving requires writer access — handled via channel.
                    }
                    Ok(Some(NetworkMessage::FileSendOffer { path, size })) => {
                        tracing::info!("Client wants to send file: {} ({} bytes)", path, size);
                        // Auto-accept for now.
                    }
                    Ok(Some(NetworkMessage::FileSendChunk { path, chunk_index, data })) => {
                        tracing::debug!("Received file chunk: {} #{} ({} bytes)", path, chunk_index, data.len());
                        // Save to received_files directory.
                        if let Err(e) = save_received_chunk(&path, chunk_index, &data).await {
                            tracing::error!("Failed to save chunk: {}", e);
                        }
                    }
                    Ok(Some(NetworkMessage::FileSendEnd { path })) => {
                        tracing::info!("File receive complete: {}", path);
                    }
                    Ok(Some(NetworkMessage::FileCancel { reason })) => {
                        tracing::info!("File transfer cancelled: {}", reason);
                    }
                    Ok(Some(NetworkMessage::Ping)) => {
                        tracing::trace!("Host received Ping");
                    }
                    Ok(Some(NetworkMessage::Disconnect)) => {
                        tracing::info!("Client requested disconnect");
                        break;
                    }
                    Ok(None) => {
                        tracing::info!("Client closed connection");
                        break;
                    }
                    Ok(Some(other)) => {
                        tracing::trace!("Host received unexpected: {:?}", other);
                    }
                    Err(e) => {
                        tracing::error!("Read error on host: {}", e);
                        break;
                    }
                }
            }
        });

        // ── Main loop: capture and send encrypted frames ──
        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            let current_fps = *fps_arc.lock().await;
            let frame_interval = std::time::Duration::from_secs_f64(1.0 / current_fps as f64);
            let frame_timeout = frame_interval / 2;

            let frame = {
                let mut cap_guard = capturer_arc.lock().await;
                let capturer = cap_guard.as_mut().ok_or_else(|| {
                    Error::Capture("Capturer not initialized".into())
                })?;

                match capturer.capture_frame(frame_timeout.as_millis() as u64) {
                    Ok(Some(f)) => f,
                    Ok(None) => {
                        drop(cap_guard);
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Capture error: {}", e);
                        break;
                    }
                }
            };

            let compressed = {
                let mut enc = encoder_arc.lock().await;
                enc.compress(&frame.data, width, height)?
            };

            let result = {
                let mut c = cipher.lock().await;
                protocol::write_encrypted(
                    &mut writer,
                    &NetworkMessage::VideoFrame(compressed),
                    &mut c,
                )
                .await
            };

            if let Err(e) = result {
                tracing::error!("Write error on host: {}", e);
                break;
            }

            tokio::time::sleep(frame_interval).await;
        }

        tracing::info!("Host session ended");
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }
    }
}

// ── Helpers ────────────────────────────────────────────────

/// List directory contents for file transfer.
pub fn list_directory(path: &str) -> Result<Vec<FileEntry>> {
    let dir_path = if path.is_empty() || path == "/" {
        // Default to user's home directory
        dirs_next::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(path)
    };

    let entries = std::fs::read_dir(&dir_path).map_err(|e| {
        Error::Network(format!("Cannot read directory {:?}: {}", dir_path, e))
    })?;

    let mut result = Vec::new();
    for entry in entries.flatten() {
        let metadata = entry.metadata().ok();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        result.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            size,
            is_dir,
            modified,
        });
    }

    Ok(result)
}

/// Save a received file chunk to disk.
async fn save_received_chunk(path: &str, chunk_index: u32, data: &[u8]) -> Result<()> {
    let recv_dir = dirs_next::download_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RemoteDesk");
    std::fs::create_dir_all(&recv_dir).ok();

    let file_name = std::path::Path::new(path)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
    let dest = recv_dir.join(file_name);

    use std::io::{Seek, Write};
    let mut file = if chunk_index == 0 {
        std::fs::File::create(&dest).map_err(|e| Error::Network(format!("Cannot create {:?}: {}", dest, e)))?
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .open(&dest)
            .map_err(|e| Error::Network(format!("Cannot open {:?}: {}", dest, e)))?
    };

    let offset = chunk_index as u64 * 65536; // 64KB chunks
    file.seek(std::io::SeekFrom::Start(offset)).ok();
    file.write_all(data)
        .map_err(|e| Error::Network(format!("Write error: {}", e)))?;

    Ok(())
}
