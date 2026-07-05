//! Video frame compression.
//!
//! Phase 1 MVP: zstd compression on raw BGRA frames.
//! Phase 2: H.264 via OpenH264, hardware codecs via hwcodec.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    Zstd = 0,
    H264 = 1,
    H265 = 2,
    VP9 = 3,
    Raw = 4,
}
