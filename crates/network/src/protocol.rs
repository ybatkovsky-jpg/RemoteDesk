//! Wire protocol: length-delimited bincode messages over TCP.
//!
//! Format: `[u32 LE: payload length][bincode: NetworkMessage]`
//!
//! Phase 2: Replace with protobuf from rd_common::message_proto.

use codec::CompressedFrame;
use rd_common::proto::{KeyEvent, MouseEvent};
use serde::{Deserialize, Serialize};

/// Messages exchanged between host and client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Client → Host: request connection
    Hello { client_version: String },
    /// Host → Client: connection accepted
    Welcome { host_version: String, display_width: u32, display_height: u32 },

    /// Host → Client: compressed video frame
    VideoFrame(CompressedFrame),

    /// Client → Host: keyboard event
    KeyEvent(KeyEvent),
    /// Client → Host: mouse event
    MouseEvent(MouseEvent),

    /// Client → Host: request to change quality/resolution
    UpdateSettings { max_fps: u32, quality: u32 },

    /// Either direction: keep-alive
    Ping,
    /// Either direction: keep-alive response
    Pong,

    /// Either direction: graceful disconnect
    Disconnect,
}

impl NetworkMessage {
    /// Serialize to bytes (bincode)
    pub fn to_bytes(&self) -> rd_common::Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| rd_common::Error::Network(format!("Serialization error: {}", e)))
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> rd_common::Result<Self> {
        bincode::deserialize(data)
            .map_err(|e| rd_common::Error::Network(format!("Deserialization error: {}", e)))
    }
}

/// Read a length-delimited message from a TCP stream
pub async fn read_message(
    reader: &mut (impl tokio::io::AsyncReadExt + Unpin),
) -> rd_common::Result<Option<NetworkMessage>> {
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(rd_common::Error::Network(format!("Read error: {}", e))),
    }

    let len = u32::from_le_bytes(len_buf) as usize;

    // Sanity check
    if len > 100 * 1024 * 1024 {
        // 100 MB max
        return Err(rd_common::Error::Network(format!(
            "Message too large: {} bytes",
            len
        )));
    }

    // Read payload
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(|e| {
        rd_common::Error::Network(format!("Failed to read payload: {}", e))
    })?;

    NetworkMessage::from_bytes(&payload).map(Some)
}

/// Write a length-delimited message to a TCP stream
pub async fn write_message(
    writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
    msg: &NetworkMessage,
) -> rd_common::Result<()> {
    let payload = msg.to_bytes()?;
    let len = payload.len() as u32;

    writer.write_all(&len.to_le_bytes()).await.map_err(|e| {
        rd_common::Error::Network(format!("Write error: {}", e))
    })?;

    writer.write_all(&payload).await.map_err(|e| {
        rd_common::Error::Network(format!("Write error: {}", e))
    })?;

    Ok(())
}
