use rd_common::{Error, Result};
use super::CompressedFrame;

/// Decompresses frames received from the network.
pub struct FrameDecoder {
    /// Decompression buffer (reused to avoid allocations)
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    /// Decompress a frame back to raw BGRA
    pub fn decompress(&mut self, frame: &CompressedFrame) -> Result<Vec<u8>> {
        match frame.compression {
            super::CompressionType::Zstd => {
                let decompressed = zstd::decode_all(&frame.data[..]).map_err(|e| {
                    Error::Codec(format!("Zstd decompression failed: {}", e))
                })?;

                // Verify size matches expected
                let expected = (frame.width * frame.height * 4) as usize;
                if decompressed.len() != expected {
                    tracing::warn!(
                        "Decompressed size mismatch: got {}, expected {}",
                        decompressed.len(),
                        expected
                    );
                }

                Ok(decompressed)
            }
            super::CompressionType::Raw => {
                Ok(frame.data.clone())
            }
            other => Err(Error::Codec(format!(
                "Unsupported compression: {:?}",
                other
            ))),
        }
    }

    /// Get a reference to the internal buffer (after decompress)
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}
