use rd_common::{Error, Result};
use super::{CompressedFrame, CompressionType};

/// Compresses raw BGRA frames.
///
/// Supports multiple codecs selected during construction.
/// - zstd: always available, ~2-5x compression for screen content.
/// - H.264/H.265: requires native library, gated behind features.
pub struct FrameEncoder {
    /// Currently active codec.
    codec: CompressionType,
    /// zstd compression level (1-22, default 3 for speed).
    zstd_level: i32,
    /// Frame sequence counter.
    sequence: u64,
}

impl FrameEncoder {
    /// Create a new encoder with the default codec (zstd).
    pub fn new() -> Self {
        Self {
            codec: CompressionType::Zstd,
            zstd_level: 3,
            sequence: 0,
        }
    }

    /// Select the codec to use for compression.
    pub fn with_codec(mut self, codec: CompressionType) -> Self {
        self.codec = codec;
        self
    }

    /// Set zstd compression level.
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = level.clamp(1, 22);
        self
    }

    /// Returns the active codec.
    pub fn codec(&self) -> CompressionType {
        self.codec
    }

    /// Compress a raw BGRA frame.
    pub fn compress(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<CompressedFrame> {
        let seq = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);

        match self.codec {
            CompressionType::Zstd | CompressionType::Raw => {
                self.compress_zstd(data, width, height, seq)
            }
            CompressionType::H264 => {
                // H.264 requires native OpenH264 library.
                // Fall back to zstd if not available.
                #[cfg(feature = "openh264")]
                {
                    self.compress_h264(data, width, height, seq)
                }
                #[cfg(not(feature = "openh264"))]
                {
                    tracing::warn!("H.264 requested but openh264 feature not enabled — using zstd");
                    self.compress_zstd(data, width, height, seq)
                }
            }
            CompressionType::H265 => {
                #[cfg(feature = "hwcodec")]
                {
                    self.compress_h265(data, width, height, seq)
                }
                #[cfg(not(feature = "hwcodec"))]
                {
                    tracing::warn!("H.265 requested but hwcodec feature not enabled — using zstd");
                    self.compress_zstd(data, width, height, seq)
                }
            }
            CompressionType::VP9 => {
                tracing::warn!("VP9 not yet implemented — using zstd");
                self.compress_zstd(data, width, height, seq)
            }
        }
    }

    /// zstd compression (always available).
    fn compress_zstd(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        seq: u64,
    ) -> Result<CompressedFrame> {
        let actual_codec = if self.codec == CompressionType::Raw {
            // Raw: no compression, just passthrough.
            tracing::debug!(
                "Raw frame {}x{}: {} bytes (passthrough)",
                width, height, data.len()
            );
            return Ok(CompressedFrame {
                data: data.to_vec(),
                width,
                height,
                compression: CompressionType::Raw,
                key_frame: true,
                sequence: seq,
            });
        } else {
            CompressionType::Zstd
        };

        let compressed = zstd::encode_all(data, self.zstd_level).map_err(|e| {
            Error::Codec(format!("Zstd compression failed: {}", e))
        })?;

        tracing::debug!(
            "{} {}x{}: {} -> {} bytes ({:.0}%)",
            actual_codec.name(),
            width,
            height,
            data.len(),
            compressed.len(),
            (compressed.len() as f64 / data.len() as f64) * 100.0
        );

        Ok(CompressedFrame {
            data: compressed,
            width,
            height,
            compression: actual_codec,
            key_frame: true,
            sequence: seq,
        })
    }

    /// Quick quality estimate — returns approximate compression ratio.
    pub fn compression_ratio(&self, original: usize, compressed: usize) -> f64 {
        original as f64 / compressed as f64
    }

    /// H.264 encoding via OpenH264 (stub — library not yet integrated).
    #[cfg(feature = "openh264")]
    fn compress_h264(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        seq: u64,
    ) -> Result<CompressedFrame> {
        // OpenH264 native library is not yet integrated.
        // This stub prevents compile errors when the feature is enabled for negotiation.
        tracing::warn!(
            "H.264 encoding not yet implemented (OpenH264 library not integrated) — falling back to zstd"
        );
        self.compress_zstd(data, width, height, seq)
    }

    /// H.265 encoding via hardware codec (stub — library not yet integrated).
    #[cfg(feature = "hwcodec")]
    fn compress_h265(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        seq: u64,
    ) -> Result<CompressedFrame> {
        // Hardware codec not yet integrated.
        tracing::warn!(
            "H.265 encoding not yet implemented (hwcodec library not integrated) — falling back to zstd"
        );
        self.compress_zstd(data, width, height, seq)
    }
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}
