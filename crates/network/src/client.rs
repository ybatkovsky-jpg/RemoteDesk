//! Client side: connects to host, receives and decompresses frames.

use codec::FrameDecoder;
use rd_common::{Error, Result};
use rd_common::proto::{KeyEvent, MouseEvent};
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
        }
    }

    pub async fn connect(&self) -> Result<()> {
        {
            let mut state = self.state.lock().await;
            *state = ConnectionState::Connecting;
        }

        let mut stream = TcpStream::connect(&self.host_addr).await.map_err(|e| {
            Error::Network(format!("Failed to connect to {}: {}", self.host_addr, e))
        })?;

        tracing::info!("Connected to host at {}", self.host_addr);

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let (mut reader, mut writer) = stream.split();

        protocol::write_message(
            &mut writer,
            &NetworkMessage::Hello {
                client_version: rd_common::VERSION.to_string(),
            },
        )
        .await?;

        match protocol::read_message(&mut reader).await? {
            Some(NetworkMessage::Welcome {
                display_width,
                display_height,
                ..
            }) => {
                let mut state = self.state.lock().await;
                *state = ConnectionState::Connected {
                    width: display_width,
                    height: display_height,
                };
                *self.display_size.lock().await = Some((display_width, display_height));
                tracing::info!("Welcome: {}x{}", display_width, display_height);
            }
            other => {
                return Err(Error::Network(format!(
                    "Expected Welcome, got: {:?}",
                    other
                )));
            }
        }

        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match protocol::read_message(&mut reader).await {
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
                    protocol::write_message(&mut writer, &NetworkMessage::Pong).await?;
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

    pub async fn send_key_event(&self, event: KeyEvent) -> Result<()> {
        tracing::debug!("Key event: {:?}", event);
        Ok(())
    }

    pub async fn send_mouse_event(&self, event: MouseEvent) -> Result<()> {
        tracing::debug!("Mouse event: {:?}", event);
        Ok(())
    }

    pub fn stop(&self) {
        tracing::info!("Client stop requested");
    }
}
