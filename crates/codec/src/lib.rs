//! Video frame compression.
//!
//! Phase 1 MVP: zstd compression on raw BGRA frames.
//! Phase 2: H.264 via OpenH264, hardware codecs via hwcodec.
//! Codec negotiation happens during Hello/Welcome handshake.

mod encoder;
mod decoder;

pub use encoder::FrameEncoder;
pub use decoder::FrameDecoder;

use serde::{Deserialize, Serialize};

/// Compressed frame ready for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedFrame {
    /// Compressed data
    pub data: Vec<u8>,
    /// Original width
    pub width: u32,
    /// Original height
    pub height: u32,
    /// Compression type identifier
    pub compression: CompressionType,
    /// Whether this is a keyframe (always true for zstd)
    pub key_frame: bool,
    /// Sequence number for ordering
    pub sequence: u64,
}

/// Supported compression algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompressionType {
    Zstd = 0,
    H264 = 1,
    H265 = 2,
    VP9 = 3,
    Raw = 4,
}

impl CompressionType {
    /// Human-readable name for logging.
    pub fn name(&self) -> &'static str {
        match self {
            CompressionType::Zstd => "zstd",
            CompressionType::H264 => "H.264",
            CompressionType::H265 => "H.265",
            CompressionType::VP9 => "VP9",
            CompressionType::Raw => "raw",
        }
    }

    /// Returns true if this is a keyframe-only codec (no temporal compression).
    pub fn is_intra_only(&self) -> bool {
        matches!(self, CompressionType::Zstd | CompressionType::Raw)
    }
}

/// Returns the list of codecs supported by this build.
/// Ordered by preference (best first).
pub fn supported_codecs() -> Vec<CompressionType> {
    #[allow(unused_mut)]
    let mut codecs = vec![
        CompressionType::Zstd,  // Always available
        CompressionType::Raw,   // Always available (passthrough)
    ];

    // H.264 via OpenH264 — requires native library.
    #[cfg(feature = "openh264")]
    codecs.insert(0, CompressionType::H264);

    // H.265 via hardware codec — requires hwcodec + native drivers.
    #[cfg(feature = "hwcodec")]
    {
        codecs.insert(0, CompressionType::H265);
    }

    codecs
}

/// Negotiate the best codec from client and host supported sets.
/// Returns the first codec in host_preference that client also supports.
pub fn negotiate_codec(
    host_supported: &[CompressionType],
    client_supported: &[CompressionType],
) -> Option<CompressionType> {
    for host_codec in host_supported {
        if client_supported.contains(host_codec) {
            return Some(*host_codec);
        }
    }
    // Fallback: both sides must support zstd.
    if host_supported.contains(&CompressionType::Zstd)
        && client_supported.contains(&CompressionType::Zstd)
    {
        return Some(CompressionType::Zstd);
    }
    None
}
