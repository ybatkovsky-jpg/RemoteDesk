//! Rendezvous client for NAT traversal coordination.
//!
//! Phase 4: Implements RegisterPeer, RequestConnection, ICE candidate exchange
//! and NAT hole-punching.

use rd_common::{Error, Result};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::protocol::{self, NetworkMessage};

/// Client for interacting with a RemoteDesk rendezvous/relay server.
pub struct RendezvousClient {
    server_addr: String,
    stream: Mutex<Option<TcpStream>>,
    peer_id: String,
}

impl RendezvousClient {
    pub fn new(server_addr: String, peer_id: String) -> Self {
        Self {
            server_addr,
            stream: Mutex::new(None),
            peer_id,
        }
    }

    pub async fn connect(&self, public_addrs: Vec<String>) -> Result<()> {
        let mut stream = TcpStream::connect(&self.server_addr).await.map_err(|e| {
            Error::Network(format!("Rendezvous connect error: {}", e))
        })?;

        tracing::info!("Connected to rendezvous server at {}", self.server_addr);

        protocol::write_message(
            &mut stream,
            &NetworkMessage::RegisterPeer {
                peer_id: self.peer_id.clone(),
                public_addrs,
            },
        )
        .await?;

        *self.stream.lock().await = Some(stream);
        tracing::info!("Registered peer {} with rendezvous", self.peer_id);
        Ok(())
    }

    pub async fn request_connection(&self, target_peer_id: &str) -> Result<Vec<String>> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::Network("Not connected to rendezvous".into()))?;

        protocol::write_message(
            stream,
            &NetworkMessage::RequestConnection {
                target_peer_id: target_peer_id.to_string(),
            },
        )
        .await?;

        match protocol::read_message(stream).await? {
            Some(NetworkMessage::PeerInfo { peer_id, addresses }) => {
                tracing::info!(
                    "Received peer info for {}: {} addresses",
                    peer_id,
                    addresses.len()
                );
                Ok(addresses)
            }
            other => Err(Error::Network(format!(
                "Expected PeerInfo, got: {:?}",
                other
            ))),
        }
    }

    pub async fn send_candidate(&self, _target_peer_id: &str, candidate: &str) -> Result<()> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::Network("Not connected to rendezvous".into()))?;

        protocol::write_message(
            stream,
            &NetworkMessage::IceCandidate {
                from_peer_id: self.peer_id.clone(),
                candidate: candidate.to_string(),
            },
        )
        .await?;

        Ok(())
    }

    pub async fn recv_candidate(&self) -> Result<(String, String)> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::Network("Not connected to rendezvous".into()))?;

        match protocol::read_message(stream).await? {
            Some(NetworkMessage::IceCandidate {
                from_peer_id,
                candidate,
            }) => Ok((from_peer_id, candidate)),
            other => Err(Error::Network(format!(
                "Expected IceCandidate, got: {:?}",
                other
            ))),
        }
    }

    /// Perform NAT hole punching with a target peer.
    pub async fn punch_hole(
        &self,
        target_peer_id: &str,
        local_udp: &super::udp::UdpTransport,
    ) -> Result<SocketAddr> {
        let peer_addrs = self.request_connection(target_peer_id).await?;
        let _local_addr = local_udp.local_addr()?;

        for addr_str in &peer_addrs {
            if let Ok(peer_addr) = addr_str.parse::<SocketAddr>() {
                tracing::info!("Punching hole to {} at {}", target_peer_id, peer_addr);

                let punch_data = b"PUNCH";
                for _ in 0..5 {
                    local_udp
                        .socket
                        .send_to(punch_data, peer_addr)
                        .await
                        .ok();
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                return Ok(peer_addr);
            }
        }

        Err(Error::Network(format!(
            "No valid addresses for peer {}",
            target_peer_id
        )))
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }
}
