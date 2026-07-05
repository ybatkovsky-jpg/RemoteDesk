//! Client side: connects to host, receives and decompresses frames.
//!
//! Phase 2: E2E encryption via NaCl/libsodium key exchange.

use codec::FrameDecoder;
use crypto::{KeyExchange, SessionCipher};
use rd_common::{Error, Result};
use rd_common::proto::{KeyEvent, MouseEvent};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};

use super::protocol::{self, NetworkMessage};

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

pub struct ClientSession {
    host_addr: String,
    latest_frame: Mutex<Option<ReceivedFrame>>,
    frame_tx: broadcast::Sender<()>,
    state: Mutex<ConnectionState>,
    shutdown_tx: Mutex<Option<broadcast::Sender<()>>>,
    display_size: Mutex<Option<(u32, u32)>>,
    /// Writer half of the TCP stream, shared so input commands can send events.
    writer: Mutex<Option<OwnedWriteHalf>>,
    /// Session cipher for encrypted communication (set after key exchange).
    cipher: Mutex<Option<SessionCipher>>,
}

impl ClientSession {
    pub fn new(host_addr: String) -> Self {
        let (frame_tx, _) = broadcast::channel(16);
        Self {
            host_addr,
            latest_frame: Mutex::new(None),
            frame_tx,
            state: Mutex::new(ConnectionState::Disconnected),
            shutdown_tx: Mutex::new(None),
            display_size: Mutex::new(None),
            writer: Mutex::new(None),
            cipher: Mutex::new(None),
        }
    }

    pub async fn connect(&self) -> Result<()> {
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

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

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
        let cipher = SessionCipher::new(&symmetric_key);
        tracing::info!("E2E encryption established");

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
                Ok(Some(NetworkMessage::Ping)) => {
                    if let Some(ref mut w) = *self.writer.lock().await {
                        if let Some(ref mut c) = *self.cipher.lock().await {
                            let _ = protocol::write_encrypted(w, &NetworkMessage::Pong, c).await;
                        }
                    }
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

    /// Send a key event to the remote host (encrypted).
    pub async fn send_key_event(&self, event: KeyEvent) -> Result<()> {
        tracing::debug!("Client sending KeyEvent: keycode={}, down={}", event.keycode, event.down);
        let msg = NetworkMessage::KeyEvent(event);
        self.send_encrypted(&msg).await
    }

    /// Send a mouse event to the remote host (encrypted).
    pub async fn send_mouse_event(&self, event: MouseEvent) -> Result<()> {
        tracing::debug!("Client sending MouseEvent: {:?} at ({}, {})", event.event_type, event.x, event.y);
        let msg = NetworkMessage::MouseEvent(event);
        self.send_encrypted(&msg).await
    }

    /// Helper: encrypt and send a message through the shared writer.
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
    }
}
