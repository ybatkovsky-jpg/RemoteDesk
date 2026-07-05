use rd_common::{Error, Result};
use super::{CompressedFrame, CompressionType};

/// Compresses raw BGRA frames using zstd.
///
/// In Phase 2, this will be replaced with H.264 hardware/software encoding.
pub struct FrameEncoder {
    /// zstd compression level (1-22, default 3 for speed)
    level: i32,
    /// Frame sequence counter
    sequence: u64,
}

impl FrameEncoder {
    pub fn new() -> Self {
        Self {
            level: 3, // Fast compression, decent ratio (~2-5x for screen content)
            sequence: 0,
        }
    }

    /// Set zstd compression level
    pub fn with_level(mut self, level: i32) -> Self {
        self.level = level.clamp(1, 22);
        self
    }

    /// Compress a raw BGRA frame
    pub fn compress(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<CompressedFrame> {
        let compressed = zstd::encode_all(data, self.level).map_err(|e| {
            Error::Codec(format!("Zstd compression failed: {}", e))
        })?;

        let seq = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);

        tracing::debug!(
            "Compressed {}x{}: {} -> {} bytes ({:.0}%)",
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
            compression: CompressionType::Zstd,
            key_frame: true, // zstd frames are always independent
            sequence: seq,
        })
    }

    /// Quick quality estimate — returns approximate compression ratio
    pub fn compression_ratio(&self, original: usize, compressed: usize) -> f64 {
        original as f64 / compressed as f64
    }
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}
