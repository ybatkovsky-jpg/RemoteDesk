//! Wire protocol: length-delimited bincode messages over TCP.
//!
//! Format: `[u32 LE: payload length][bincode: NetworkMessage]`
//!
//! Phase 2: E2E encryption via NaCl/libsodium.
//! After crypto handshake, all messages are wrapped in `NetworkMessage::Encrypted`.

use codec::CompressedFrame;
use codec::CompressionType;
use rd_common::proto::{KeyEvent, MouseEvent};
use serde::{Deserialize, Serialize};

/// Size of a Curve25519 public key.
pub const PUBLIC_KEY_BYTES: usize = 32;

/// Messages exchanged between host and client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Client → Host: request connection
    Hello {
        client_version: String,
        /// List of codecs the client supports (in preference order).
        supported_codecs: Vec<CompressionType>,
    },
    /// Host → Client: connection accepted
    Welcome {
        host_version: String,
        display_width: u32,
        display_height: u32,
        /// Codec selected by the host for this session.
        selected_codec: CompressionType,
    },

    // ── Crypto handshake (Phase 2) ───────────────────────
    /// Client → Host: ephemeral public key for key exchange
    CryptoHandshake { public_key: [u8; PUBLIC_KEY_BYTES] },
    /// Host → Client: ephemeral public key for key exchange
    CryptoHandshakeAck { public_key: [u8; PUBLIC_KEY_BYTES] },
    /// Either direction: encrypted payload (after handshake)
    /// Contains [nonce (24 bytes) || ciphertext_with_mac]
    Encrypted(Vec<u8>),

    /// Host → Client: compressed video frame
    VideoFrame(CompressedFrame),

    /// Client → Host: keyboard event
    KeyEvent(KeyEvent),
    /// Client → Host: mouse event
    MouseEvent(MouseEvent),

    /// Client → Host: request to change quality/resolution
    UpdateSettings { max_fps: u32, quality: u32 },

    // ── Multi-monitor / display control (Phase 2) ────────
    /// Client → Host: switch to a different display.
    SwitchDisplay { display_id: usize },

    // ── Clipboard (Phase 2) ──────────────────────────────
    /// Either direction: clipboard content.
    ClipboardText { content: String },

    // ── File transfer (Phase 2) ──────────────────────────
    /// Client → Host: request a file from the host.
    FileRequest { path: String },
    /// Host → Client: start of a file transfer.
    FileStart { path: String, size: u64 },
    /// Host → Client: a chunk of file data.
    FileChunk { chunk_index: u32, data: Vec<u8> },
    /// Host → Client: file transfer complete.
    FileEnd { path: String },
    /// Either direction: cancel a transfer.
    FileCancel { reason: String },

    // ── Audio (Phase 2, stub) ────────────────────────────
    /// Host → Client: audio frame (compressed with Opus).
    AudioFrame { data: Vec<u8>, timestamp: u64 },
    /// Client → Host: request audio start/stop.
    AudioControl { enable: bool },

    /// Either direction: keep-alive
    Ping,
    /// Either direction: keep-alive response
    Pong,

    /// Either direction: graceful disconnect
    Disconnect,

    // ── Signaling / NAT traversal (Phase 2) ───────────────
    /// Peer → Server: register this peer with its public addresses.
    RegisterPeer { peer_id: String, public_addrs: Vec<String> },
    /// Client → Server: request to connect to target peer.
    RequestConnection { target_peer_id: String },
    /// Server → Peer: relay an ICE candidate.
    IceCandidate { from_peer_id: String, candidate: String },
    /// Server → Client: connection info for the target peer.
    PeerInfo { peer_id: String, addresses: Vec<String> },
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
    let payload = match read_raw(reader).await? {
        Some(data) => data,
        None => return Ok(None),
    };
    NetworkMessage::from_bytes(&payload).map(Some)
}

/// Write a length-delimited message to a TCP stream
pub async fn write_message(
    writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
    msg: &NetworkMessage,
) -> rd_common::Result<()> {
    let payload = msg.to_bytes()?;
    write_raw(writer, &payload).await
}

/// Write raw bytes with length prefix (used for encrypted payloads).
pub async fn write_raw(
    writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
    data: &[u8],
) -> rd_common::Result<()> {
    let len = data.len() as u32;
    writer.write_all(&len.to_le_bytes()).await.map_err(|e| {
        rd_common::Error::Network(format!("Write error: {}", e))
    })?;
    writer.write_all(data).await.map_err(|e| {
        rd_common::Error::Network(format!("Write error: {}", e))
    })?;
    Ok(())
}

/// Write a message encrypted through the session cipher.
pub async fn write_encrypted(
    writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
    msg: &NetworkMessage,
    cipher: &mut crypto::SessionCipher,
) -> rd_common::Result<()> {
    let plaintext = msg.to_bytes()?;
    let (nonce, ciphertext) = cipher.encrypt(&plaintext);
    // Prepend nonce to ciphertext for the receiver.
    let mut payload = nonce;
    payload.extend_from_slice(&ciphertext);
    write_raw(writer, &payload).await
}

/// Read an encrypted message and decrypt it.
pub async fn read_encrypted(
    reader: &mut (impl tokio::io::AsyncReadExt + Unpin),
    cipher: &mut crypto::SessionCipher,
) -> rd_common::Result<Option<NetworkMessage>> {
    let raw = read_raw(reader).await?;
    let raw = match raw {
        Some(data) => data,
        None => return Ok(None),
    };

    if raw.len() < crypto::NONCE_BYTES + crypto::MAC_BYTES {
        return Err(rd_common::Error::Network("Encrypted message too short".into()));
    }

    let nonce = &raw[..crypto::NONCE_BYTES];
    let ciphertext = &raw[crypto::NONCE_BYTES..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| rd_common::Error::Network(format!("Decryption failed: {}", e)))?;

    NetworkMessage::from_bytes(&plaintext).map(Some)
}

/// Read raw bytes with length prefix.
pub async fn read_raw(
    reader: &mut (impl tokio::io::AsyncReadExt + Unpin),
) -> rd_common::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(rd_common::Error::Network(format!("Read error: {}", e))),
    }

    let len = u32::from_le_bytes(len_buf) as usize;

    if len > 100 * 1024 * 1024 {
        return Err(rd_common::Error::Network(format!(
            "Message too large: {} bytes",
            len
        )));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(|e| {
        rd_common::Error::Network(format!("Failed to read payload: {}", e))
    })?;

    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_roundtrip() {
        let original = NetworkMessage::Hello {
            client_version: "0.1.0".into(),
            supported_codecs: vec![CompressionType::Zstd, CompressionType::H264],
        };
        let bytes = original.to_bytes().expect("serialize");
        let restored = NetworkMessage::from_bytes(&bytes).expect("deserialize");
        match restored {
            NetworkMessage::Hello {
                client_version,
                supported_codecs,
            } => {
                assert_eq!(client_version, "0.1.0");
                assert_eq!(supported_codecs, vec![CompressionType::Zstd, CompressionType::H264]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_video_frame_roundtrip() {
        let frame = codec::CompressedFrame {
            data: vec![1, 2, 3, 4, 5],
            width: 1920,
            height: 1080,
            compression: CompressionType::Zstd,
            key_frame: true,
            sequence: 42,
        };
        let msg = NetworkMessage::VideoFrame(frame);
        let bytes = msg.to_bytes().expect("serialize");
        let restored = NetworkMessage::from_bytes(&bytes).expect("deserialize");
        match restored {
            NetworkMessage::VideoFrame(f) => {
                assert_eq!(f.width, 1920);
                assert_eq!(f.height, 1080);
                assert_eq!(f.sequence, 42);
                assert_eq!(f.compression, CompressionType::Zstd);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_key_event_roundtrip() {
        let event = KeyEvent {
            down: true,
            keycode: 65,  // 'A'
            scancode: 30,
            modifiers: 2, // Ctrl
        };
        let msg = NetworkMessage::KeyEvent(event.clone());
        let bytes = msg.to_bytes().expect("serialize");
        let restored = NetworkMessage::from_bytes(&bytes).expect("deserialize");
        match restored {
            NetworkMessage::KeyEvent(e) => {
                assert!(e.down);
                assert_eq!(e.keycode, 65);
                assert_eq!(e.modifiers, 2);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_all_message_variants_serialize() {
        // Verify every variant serializes/deserializes without panic.
        let messages = vec![
            NetworkMessage::Hello { client_version: "v1".into(), supported_codecs: vec![CompressionType::Zstd] },
            NetworkMessage::Welcome { host_version: "v1".into(), display_width: 1024, display_height: 768, selected_codec: CompressionType::Zstd },
            NetworkMessage::Ping,
            NetworkMessage::Pong,
            NetworkMessage::Disconnect,
            NetworkMessage::Encrypted(vec![0, 1, 2, 3]),
            NetworkMessage::UpdateSettings { max_fps: 30, quality: 80 },
            NetworkMessage::ClipboardText { content: "hello".into() },
            NetworkMessage::FileRequest { path: "/tmp/test.txt".into() },
            NetworkMessage::AudioControl { enable: true },
            NetworkMessage::RegisterPeer { peer_id: "abc".into(), public_addrs: vec!["1.2.3.4:9000".into()] },
        ];

        for msg in messages {
            let bytes = msg.to_bytes().expect("serialize");
            let _restored = NetworkMessage::from_bytes(&bytes).expect("deserialize");
        }
    }
}
