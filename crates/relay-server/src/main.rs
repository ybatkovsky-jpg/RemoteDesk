//! Relay Server for RemoteDesk.
//!
//! A lightweight TCP relay that pairs two peers by a shared session token
//! and proxies encrypted traffic between them. Used as a fallback when
//! direct P2P (NAT hole punching) fails.
//!
//! Protocol:
//! 1. Peer connects with "RELAY <token>\n"
//! 2. Server buffers the connection until a second peer connects with the same token
//! 3. Server bridges the two TCP streams (bidirectional copy)
//!
//! Usage:
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
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("relay_server=info")),
        )
        .init();

    let bind_addr = std::env::var("RELAY_BIND")
        .unwrap_or_else(|_| "0.0.0.0:21117".to_string());

    tracing::info!("RemoteDesk Relay Server starting on {}", bind_addr);

    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!("Relay server listening on {}", bind_addr);

    // Map of session_token -> waiting peer connection.
    let pending: Arc<Mutex<HashMap<String, TcpStream>>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::info!("New connection from {}", addr);

        let pending = pending.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, pending).await {
                tracing::error!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    pending: Arc<Mutex<HashMap<String, TcpStream>>>,
) -> anyhow::Result<()> {
    // Read the RELAY command line.
    let mut buf = [0u8; 256];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let line = String::from_utf8_lossy(&buf[..n]);
    let line = line.trim();

    if !line.starts_with("RELAY ") {
        tracing::warn!("Invalid handshake: {}", line);
        stream.write_all(b"ERROR invalid handshake\n").await?;
        return Ok(());
    }

    let token = line[6..].trim().to_string();
    if token.is_empty() {
        stream.write_all(b"ERROR empty token\n").await?;
        return Ok(());
    }

    tracing::info!("Relay request with token: {}", token);

    // Check if another peer is waiting with this token.
    let mut pending_map = pending.lock().await;
    if let Some(other_stream) = pending_map.remove(&token) {
        drop(pending_map);
        tracing::info!("Pairing two peers with token: {}", token);

        // Notify both peers that relay is starting.
        let _ = stream.write_all(b"RELAY_OK paired\n").await;
        // Bridge the two connections.
        bridge_connections(stream, other_stream).await;
    } else {
        // Store this connection and wait for the other peer.
        stream.write_all(b"RELAY_OK waiting\n").await?;
        pending_map.insert(token.clone(), stream);
        tracing::info!("Peer waiting for partner (token: {})", token);
    }

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
                Ok(0) => break, // EOF
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

    tracing::info!("Relay bridge ended");
}
