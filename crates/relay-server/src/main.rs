//! RemoteDesk Rendezvous + Relay Server
//!
//! A single lightweight TCP server that handles both peer discovery
//! (rendezvous) and traffic relay. Run this on a public VPS.
//!
//! ## Protocol
//!
//! ### REGISTER — host announces itself
//! ```
//! Peer → Server:  "REGISTER <peer_id>\n"
//! Server → Peer:  "REGISTER_OK waiting\n"
//! [host waits here — server holds the connection open]
//! [when client connects:]
//! Server → Peer:  "REGISTER_OK paired\n"
//! [bridge established — raw bytes flow in both directions]
//! ```
//!
//! ### CONNECT — client wants to reach a host
//! ```
//! Peer → Server:  "CONNECT <peer_id>\n"
//! Server → Peer:  "CONNECT_OK paired\n"   [if host is waiting]
//! Server → Peer:  "CONNECT_ERR not found\n" [if no such host]
//! [bridge established]
//! ```
//!
//! ### RELAY — token-based relay (legacy, still supported)
//! ```
//! Peer → Server:  "RELAY <token>\n"
//! Server → Peer:  "RELAY_OK waiting\n"
//! [waits for second peer with same token]
//! ```
//!
//! ## Usage
//! ```bash
//! relay-server --bind 0.0.0.0:21117
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind_addr = std::env::var("RELAY_BIND")
        .unwrap_or_else(|_| "0.0.0.0:21117".to_string());

    tracing::info!("RemoteDesk Rendezvous+Relay Server starting on {}", bind_addr);

    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server listening on {}", bind_addr);

    // Map of peer_id → waiting host connection.
    let registered: Arc<Mutex<HashMap<String, TcpStream>>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::info!("Connection from {}", addr);

        let registered = registered.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, registered).await {
                tracing::error!("Error handling {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    addr: std::net::SocketAddr,
    registered: Arc<Mutex<HashMap<String, TcpStream>>>,
) -> anyhow::Result<()> {
    // Read the command line (up to newline or 256 bytes).
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let line = String::from_utf8_lossy(&buf[..n]);
    let line = line.trim().to_string();

    // ── REGISTER (host announces itself) ──────────────────
    if line.starts_with("REGISTER ") {
        let peer_id = line["REGISTER ".len()..].trim().to_string();
        if peer_id.is_empty() {
            let _ = stream.write_all(b"REGISTER_ERR empty id\n").await;
            return Ok(());
        }

        tracing::info!("[{}] REGISTER peer_id={}", addr, peer_id);

        // Check if someone is already trying to connect to this peer.
        let mut map = registered.lock().await;
        if let Some(client_stream) = map.remove(&format!("CONNECTING_{}", peer_id)) {
            drop(map);
            tracing::info!("[{}] Client already waiting for peer_id={}, bridging", addr, peer_id);
            let _ = stream.write_all(b"REGISTER_OK paired\n").await;
            bridge_connections(stream, client_stream).await;
        } else {
            // Store host connection and wait.
            let _ = stream.write_all(b"REGISTER_OK waiting\n").await;
            map.insert(peer_id.clone(), stream);
            drop(map);
            tracing::info!("[{}] Host {} registered and waiting", addr, peer_id);
            // Note: the TcpStream is stored in the map.
            // The connection stays open. If the host disconnects, the stream
            // becomes invalid but we don't have a cleanup mechanism yet.
            // TODO: periodic cleanup of dead connections.
        }
        return Ok(());
    }

    // ── CONNECT (client wants to reach a host) ─────────────
    if line.starts_with("CONNECT ") {
        let peer_id = line["CONNECT ".len()..].trim().to_string();
        if peer_id.is_empty() {
            let _ = stream.write_all(b"CONNECT_ERR empty id\n").await;
            return Ok(());
        }

        tracing::info!("[{}] CONNECT peer_id={}", addr, peer_id);

        let mut map = registered.lock().await;
        if let Some(host_stream) = map.remove(&peer_id) {
            drop(map);
            tracing::info!("[{}] Found host for peer_id={}, bridging", addr, peer_id);
            let _ = stream.write_all(b"CONNECT_OK paired\n").await;
            bridge_connections(host_stream, stream).await;
        } else {
            // Host not registered yet — store client as waiting.
            // When host REGISTERs, it will find this connection.
            let _ = stream.write_all(b"CONNECT_OK waiting\n").await;
            map.insert(format!("CONNECTING_{}", peer_id), stream);
            drop(map);
            tracing::info!("[{}] Client waiting for peer_id={}", addr, peer_id);
        }
        return Ok(());
    }

    // ── RELAY (legacy token-based relay) ───────────────────
    if line.starts_with("RELAY ") {
        let token = line["RELAY ".len()..].trim().to_string();
        if token.is_empty() {
            let _ = stream.write_all(b"RELAY_ERR empty token\n").await;
            return Ok(());
        }

        tracing::info!("[{}] RELAY token={}", addr, token);

        let mut map = registered.lock().await;
        if let Some(other) = map.remove(&token) {
            drop(map);
            tracing::info!("[{}] Pairing with existing peer, token={}", addr, token);
            let _ = stream.write_all(b"RELAY_OK paired\n").await;
            bridge_connections(stream, other).await;
        } else {
            let _ = stream.write_all(b"RELAY_OK waiting\n").await?;
            let token_clone = token.clone();
            map.insert(token, stream);
            drop(map);
            tracing::info!("[{}] Waiting for partner, token={}", addr, token_clone);
        }
        return Ok(());
    }

    // ── Unknown command ───────────────────────────────────
    tracing::warn!("[{}] Unknown command: {}", addr, line);
    let _ = stream.write_all(b"ERROR unknown command\n").await;
    Ok(())
}

/// Bidirectional copy between two TCP streams.
async fn bridge_connections(a: TcpStream, b: TcpStream) {
    let (mut a_read, mut a_write) = a.into_split();
    let (mut b_read, mut b_write) = b.into_split();

    let a_to_b = tokio::spawn(async move {
        let mut buf = [0u8; 65536];
        loop {
            match a_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if b_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let b_to_a = tokio::spawn(async move {
        let mut buf = [0u8; 65536];
        loop {
            match b_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if a_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Wait for either direction to finish.
    tokio::select! {
        _ = a_to_b => {}
        _ = b_to_a => {}
    }

    tracing::info!("Bridge closed");
}
