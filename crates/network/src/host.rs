//! Host side: captures screen, compresses frames, sends to connected client.
//!
//! Phase 2: E2E encryption via NaCl/libsodium key exchange.

use codec::FrameEncoder;
use crypto::{KeyExchange, SessionCipher};
use input_sim::InputSimulator;
use rd_common::{Error, Result};
use screen_capture::Capturer;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

use super::protocol::{self, NetworkMessage};

pub struct HostSession {
    port: u16,
    capturer: Option<Capturer>,
    encoder: FrameEncoder,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl HostSession {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            capturer: None,
            encoder: FrameEncoder::new(),
            shutdown_tx: None,
        }
    }

    pub async fn run(&mut self, display_id: usize, fps: u32) -> Result<()> {
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

        tracing::info!("Host listening on {} ({}x{} @ {}fps)", addr, width, height, fps);

        let (stream, client_addr) = listener.accept().await.map_err(|e| {
            Error::Network(format!("Accept error: {}", e))
        })?;

        tracing::info!("Client connected from {}", client_addr);

        self.handle_client(stream, width, height, fps).await?;

        Ok(())
    }

    async fn handle_client(
        &mut self,
        stream: TcpStream,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<()> {
        let frame_interval = std::time::Duration::from_secs_f64(1.0 / fps as f64);
        let frame_timeout = frame_interval / 2;

        let (mut reader, mut writer) = stream.into_split();
        let shutdown_tx = self.shutdown_tx.as_ref().unwrap().clone();
        let mut shutdown_rx = shutdown_tx.subscribe();

        // ── Plaintext handshake ───────────────────────────
        // 1. Read Hello (client version, supported codecs).
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
                // Negotiate codec: pick the best one both sides support.
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

        // 2. Set up encoder with negotiated codec.
        self.encoder = FrameEncoder::new().with_codec(selected_codec);

        // 3. Send Welcome (with negotiated codec).
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

        // ── Spawn reader task (receives encrypted input events) ──
        let reader_cipher = cipher.clone();
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
                        tracing::info!(
                            "Client requested settings change: fps={}, quality={}",
                            max_fps, quality
                        );
                    }
                    Ok(Some(NetworkMessage::SwitchDisplay { display_id })) => {
                        tracing::info!("Client requested display switch to {}", display_id);
                        // Phase 2: hot-switch Capturer to new display.
                    }
                    Ok(Some(NetworkMessage::ClipboardText { content })) => {
                        tracing::debug!("Received clipboard: {} chars", content.len());
                        let mut sim = InputSimulator::new();
                        if let Err(e) = sim.set_clipboard(&content) {
                            tracing::error!("Clipboard set error: {}", e);
                        }
                    }
                    Ok(Some(NetworkMessage::AudioControl { enable })) => {
                        tracing::info!("Client requested audio: {}", enable);
                    }
                    Ok(Some(NetworkMessage::FileRequest { path })) => {
                        tracing::info!("Client requested file: {}", path);
                        // Phase 2: validate path and start transfer.
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

            let frame = {
                let capturer = self.capturer.as_mut().ok_or_else(|| {
                    Error::Capture("Capturer not initialized".into())
                })?;

                match capturer.capture_frame(frame_timeout.as_millis() as u64) {
                    Ok(Some(f)) => f,
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Capture error: {}", e);
                        break;
                    }
                }
            };

            let compressed = self.encoder.compress(&frame.data, width, height)?;

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
