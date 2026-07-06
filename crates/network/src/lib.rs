//! Network transport and protocol for RemoteDesk.
//!
//! Phase 1: Simple TCP-based frame + input streaming in LAN.
//! Phase 2: P2P with NAT traversal, encryption, relay fallback.

pub mod protocol;
pub mod host;
pub mod client;
pub mod udp;

pub use host::HostSession;
pub use client::{ChatEntry, ClientSession, ConnectionState, FileTransferProgress};
pub use protocol::{FileEntry, NetworkMessage};
pub use udp::UdpTransport;
