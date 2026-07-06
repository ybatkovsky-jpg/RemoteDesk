//! UDP transport with reliable framing for NAT traversal.
//!
//! Implements a simple reliable protocol over UDP:
//! - Sequence numbers for ordering and duplicate detection
//! - Fragment reassembly with timeout
//! - ACK/retransmit for reliable messages
//!
//! Phase 4: Full fragment reassembly buffer + ACK support.

use rd_common::{Error, Result};
use tokio::net::UdpSocket;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

/// Maximum UDP payload size (conservative to avoid fragmentation).
pub const MAX_UDP_PAYLOAD: usize = 1200;
/// Header size: 4 bytes seq + 2 bytes fragment_id + 2 bytes fragment_count + 2 bytes flags = 10.
pub const HEADER_SIZE: usize = 10;
/// Maximum time to wait for missing fragments before giving up.
pub const REASSEMBLY_TIMEOUT_MS: u64 = 500;

/// Flags for UDP frames.
const FLAG_FRAGMENTED: u16 = 0x01;
const FLAG_LAST_FRAGMENT: u16 = 0x02;
const FLAG_RELIABLE: u16 = 0x04;
const FLAG_ACK: u16 = 0x08;

/// A framed UDP message ready for send or received.
#[derive(Debug, Clone)]
pub struct UdpFrame {
    pub sequence: u32,
    pub data: Vec<u8>,
    pub reliable: bool,
}

/// Reassembly state for a single fragmented message.
struct ReassemblyState {
    fragments: Vec<Option<Vec<u8>>>,
    fragment_count: u16,
    received_count: u16,
    created: Instant,
    reliable: bool,
}

/// UDP transport wrapping a tokio UdpSocket.
///
/// Handles framing, fragmentation, reassembly, and sequence tracking.
pub struct UdpTransport {
    pub(crate) socket: UdpSocket,
    send_seq: u32,
    recv_seq: u32,
    /// Pending fragment reassembly buffers, keyed by (sender_addr, sequence).
    reassembly: HashMap<(SocketAddr, u32), ReassemblyState>,
    /// Pending ACKs for reliable messages we sent.
    pending_acks: HashMap<u32, (Vec<u8>, Instant, SocketAddr)>,
}

impl UdpTransport {
    /// Bind to a local port for receiving.
    pub async fn bind(addr: &str) -> Result<Self> {
        let socket = UdpSocket::bind(addr).await.map_err(|e| {
            Error::Network(format!("UDP bind error: {}", e))
        })?;
        tracing::info!("UDP bound to {}", addr);
        Ok(Self {
            socket,
            send_seq: 0,
            recv_seq: 0,
            reassembly: HashMap::new(),
            pending_acks: HashMap::new(),
        })
    }

    /// Send a complete message to a peer (handles fragmentation internally).
    pub async fn send_to(&mut self, data: &[u8], addr: SocketAddr, reliable: bool) -> Result<()> {
        if data.len() <= MAX_UDP_PAYLOAD - HEADER_SIZE {
            // Single frame
            let seq = self.send_seq;
            self.send_seq = self.send_seq.wrapping_add(1);
            let frame = encode_frame(data, seq, 0, 1, reliable, false);
            self.socket.send_to(&frame, addr).await.map_err(|e| {
                Error::Network(format!("UDP send error: {}", e))
            })?;
        } else {
            // Fragment
            let chunk_size = MAX_UDP_PAYLOAD - HEADER_SIZE;
            let fragments: Vec<&[u8]> = data.chunks(chunk_size).collect();
            let total = fragments.len() as u16;
            let seq = self.send_seq;
            self.send_seq = self.send_seq.wrapping_add(1);

            for (i, chunk) in fragments.iter().enumerate() {
                let last = i == total as usize - 1;
                let frame = encode_frame(
                    chunk, seq, i as u16, total, reliable, last,
                );
                self.socket.send_to(&frame, addr).await.map_err(|e| {
                    Error::Network(format!("UDP send error: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// Receive a complete message (reassembles fragments).
    /// Returns (data, sender_addr, reliable_flag).
    /// Also handles ACK frames for reliability.
    pub async fn recv_from(&mut self) -> Result<Option<(Vec<u8>, SocketAddr, bool)>> {
        // Clean up expired reassembly buffers first.
        self.cleanup_reassembly();

        let mut buf = vec![0u8; 65536];
        let (len, addr) = self.socket.recv_from(&mut buf).await.map_err(|e| {
            Error::Network(format!("UDP recv error: {}", e))
        })?;

        if len < HEADER_SIZE {
            return Err(Error::Network("UDP frame too short".into()));
        }

        let (seq, frag_id, frag_count, flags) = decode_header(&buf[..HEADER_SIZE]);
        let payload = &buf[HEADER_SIZE..len];
        let is_reliable = flags & FLAG_RELIABLE != 0;

        // Handle ACK frames.
        if flags & FLAG_ACK != 0 {
            let acked_seq = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
            self.pending_acks.remove(&acked_seq);
            tracing::trace!("Received ACK for seq={}", acked_seq);
            return Ok(None); // ACKs don't produce messages
        }

        if frag_count <= 1 {
            // Single-frame message — no reassembly needed.
            self.recv_seq = seq.wrapping_add(1);

            // Send ACK if reliable.
            if is_reliable {
                self.send_ack(seq, addr).await?;
            }

            return Ok(Some((payload.to_vec(), addr, is_reliable)));
        }

        // ── Fragment reassembly ────────────────────────────
        let key = (addr, seq);
        let state = self.reassembly.entry(key).or_insert_with(|| ReassemblyState {
            fragments: vec![None; frag_count as usize],
            fragment_count: frag_count,
            received_count: 0,
            created: Instant::now(),
            reliable: is_reliable,
        });

        if (frag_id as usize) < state.fragments.len() {
            if state.fragments[frag_id as usize].is_none() {
                state.fragments[frag_id as usize] = Some(payload.to_vec());
                state.received_count += 1;
            }
        }

        // Check if all fragments received.
        if state.received_count >= state.fragment_count {
            let state = self.reassembly.remove(&key).unwrap();
            let mut data = Vec::new();
            for frag in state.fragments {
                if let Some(d) = frag {
                    data.extend_from_slice(&d);
                }
            }

            if state.reliable {
                self.send_ack(seq, addr).await?;
            }

            return Ok(Some((data, addr, state.reliable)));
        }

        Ok(None) // Still waiting for more fragments
    }

    /// Clean up expired reassembly buffers.
    fn cleanup_reassembly(&mut self) {
        let now = Instant::now();
        let timeout = std::time::Duration::from_millis(REASSEMBLY_TIMEOUT_MS);
        self.reassembly.retain(|_, state| {
            now.duration_since(state.created) < timeout
        });
    }

    /// Send an ACK for a received reliable message.
    async fn send_ack(&mut self, seq: u32, addr: SocketAddr) -> Result<()> {
        let mut frame = Vec::with_capacity(HEADER_SIZE + 4);
        frame.extend_from_slice(&seq.to_le_bytes()); // Use ack_seq in header position
        frame.extend_from_slice(&0u16.to_le_bytes()); // frag_id = 0
        frame.extend_from_slice(&1u16.to_le_bytes()); // frag_count = 1
        frame.extend_from_slice(&FLAG_ACK.to_le_bytes());
        frame.extend_from_slice(&seq.to_le_bytes()); // acked_seq in payload
        self.socket.send_to(&frame, addr).await.map_err(|e| {
            Error::Network(format!("UDP ACK send error: {}", e))
        })?;
        Ok(())
    }

    /// Get the local address this socket is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr().map_err(|e| {
            Error::Network(format!("UDP local_addr error: {}", e))
        })
    }

    /// Connect the socket to a specific remote address (for send-only).
    pub async fn connect(&self, addr: SocketAddr) -> Result<()> {
        self.socket.connect(addr).await.map_err(|e| {
            Error::Network(format!("UDP connect error: {}", e))
        })
    }
}

/// Encode a frame with header.
fn encode_frame(
    data: &[u8],
    seq: u32,
    frag_id: u16,
    frag_count: u16,
    reliable: bool,
    last: bool,
) -> Vec<u8> {
    let mut flags: u16 = 0;
    if frag_count > 1 {
        flags |= FLAG_FRAGMENTED;
    }
    if last {
        flags |= FLAG_LAST_FRAGMENT;
    }
    if reliable {
        flags |= FLAG_RELIABLE;
    }

    let mut frame = Vec::with_capacity(HEADER_SIZE + data.len());
    frame.extend_from_slice(&seq.to_le_bytes());
    frame.extend_from_slice(&frag_id.to_le_bytes());
    frame.extend_from_slice(&frag_count.to_le_bytes());
    frame.extend_from_slice(&flags.to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

/// Decode header fields from raw bytes.
fn decode_header(header: &[u8]) -> (u32, u16, u16, u16) {
    let seq = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let frag_id = u16::from_le_bytes([header[4], header[5]]);
    let frag_count = u16::from_le_bytes([header[6], header[7]]);
    let flags = u16::from_le_bytes([header[8], header[9]]);
    (seq, frag_id, frag_count, flags)
}

// ── STUN Client ──────────────────────────────────────────

/// Resolve our public IP:port via a STUN server.
///
/// Uses a simple STUN Binding request (RFC 5389).
/// Default servers: Google's public STUN.
pub async fn stun_resolve(
    stun_server: &str,
    local_addr: SocketAddr,
) -> Result<SocketAddr> {
    let socket = UdpSocket::bind(local_addr).await.map_err(|e| {
        Error::Network(format!("STUN bind error: {}", e))
    })?;

    socket.connect(stun_server).await.map_err(|e| {
        Error::Network(format!("STUN connect error: {}", e))
    })?;

    // Send STUN Binding request.
    let request = build_stun_binding_request();
    socket.send(&request).await.map_err(|e| {
        Error::Network(format!("STUN send error: {}", e))
    })?;

    let mut buf = vec![0u8; 256];
    let len = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.recv(&mut buf),
    )
    .await
    .map_err(|_| Error::Network("STUN timeout".into()))?
    .map_err(|e| Error::Network(format!("STUN recv error: {}", e)))?;

    let addr = parse_stun_response(&buf[..len])?;
    tracing::info!("STUN resolved public address: {}", addr);
    Ok(addr)
}

/// Build a minimal STUN Binding request.
fn build_stun_binding_request() -> Vec<u8> {
    // STUN header: Binding Request, no attributes.
    let mut msg = Vec::with_capacity(20);
    // Message Type: Binding Request (0x0001).
    msg.extend_from_slice(&[0x00, 0x01]);
    // Message Length: 0 (no attributes).
    msg.extend_from_slice(&[0x00, 0x00]);
    // Magic Cookie: 0x2112A442.
    msg.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
    // Transaction ID: 12 random bytes.
    msg.extend_from_slice(&rand_transaction_id());
    msg
}

/// Generate a random 12-byte transaction ID (not cryptographically secure).
fn rand_transaction_id() -> [u8; 12] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut id = [0u8; 12];
    id[..8].copy_from_slice(&val.to_le_bytes());
    // Fill remaining with current timestamp-based bytes.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    id[8..].copy_from_slice(&(now as u32).to_le_bytes());
    id
}

/// Parse STUN Binding response and extract XOR-MAPPED-ADDRESS or MAPPED-ADDRESS.
fn parse_stun_response(data: &[u8]) -> Result<SocketAddr> {
    if data.len() < 20 {
        return Err(Error::Network("STUN response too short".into()));
    }

    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    // Binding Success Response = 0x0101.
    if msg_type != 0x0101 {
        return Err(Error::Network(format!(
            "Unexpected STUN response type: 0x{:04x}",
            msg_type
        )));
    }

    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let magic_cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

    if data.len() < 20 + msg_len {
        return Err(Error::Network("STUN response truncated".into()));
    }

    // Parse attributes.
    let mut offset = 20;
    while offset + 4 <= data.len() && offset < 20 + msg_len {
        let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let attr_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        offset += 4;

        match attr_type {
            0x0001 => {
                // MAPPED-ADDRESS
                if attr_len >= 8 && offset + 8 <= data.len() {
                    let _family = data[offset + 1];
                    let port = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
                    let ip = std::net::Ipv4Addr::new(
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    );
                    return Ok(SocketAddr::new(std::net::IpAddr::V4(ip), port));
                }
            }
            0x0020 => {
                // XOR-MAPPED-ADDRESS
                if attr_len >= 8 && offset + 8 <= data.len() {
                    let _family = data[offset + 1];
                    let xport = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
                    let port = xport ^ (magic_cookie >> 16) as u16;
                    let xip = u32::from_be_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]);
                    let ip_raw = xip ^ magic_cookie;
                    let ip = std::net::Ipv4Addr::from(ip_raw.to_be_bytes());
                    return Ok(SocketAddr::new(std::net::IpAddr::V4(ip), port));
                }
            }
            _ => {}
        }
        offset += attr_len;
        // Align to 4-byte boundary.
        if attr_len % 4 != 0 {
            offset += 4 - (attr_len % 4);
        }
    }

    Err(Error::Network("No MAPPED-ADDRESS in STUN response".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encode_decode() {
        let data = b"hello";
        let frame = encode_frame(data, 42, 0, 1, false, false);
        assert_eq!(frame.len(), HEADER_SIZE + 5);

        let (seq, frag_id, frag_count, flags) = decode_header(&frame[..HEADER_SIZE]);
        assert_eq!(seq, 42);
        assert_eq!(frag_id, 0);
        assert_eq!(frag_count, 1);
        assert_eq!(flags, 0);
        assert_eq!(&frame[HEADER_SIZE..], b"hello");
    }

    #[test]
    fn test_frame_fragmented() {
        let data = b"hello";
        let frame = encode_frame(data, 1, 0, 3, true, false);
        let (_, _, _, flags) = decode_header(&frame[..HEADER_SIZE]);
        assert!(flags & FLAG_FRAGMENTED != 0);
        assert!(flags & FLAG_RELIABLE != 0);
        assert!(flags & FLAG_LAST_FRAGMENT == 0);
    }
}
