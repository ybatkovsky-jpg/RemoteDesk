//! Host side: captures screen, compresses frames, sends to connected client.

use codec::FrameEncoder;
use rd_common::{Error, Result};
use screen_capture::Capturer;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

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

        protocol::write_message(
            &mut writer,
            &NetworkMessage::Welcome {
                host_version: rd_common::VERSION.to_string(),
                display_width: width,
                display_height: height,
            },
        )
        .await?;

        tokio::spawn(async move {
            loop {
                match protocol::read_message(&mut reader).await {
                    Ok(Some(msg)) => {
                        if matches!(msg, NetworkMessage::Disconnect) {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Read error on host: {}", e);
                        break;
                    }
                }
            }
        });

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

            if let Err(e) =
                protocol::write_message(&mut writer, &NetworkMessage::VideoFrame(compressed))
                    .await
            {
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
