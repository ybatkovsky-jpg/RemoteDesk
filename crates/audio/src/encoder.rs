//! Opus encoder: PCM f32 → compressed Opus frames.

use magnum_opus::{Channels, Encoder as OpusEncoderInner};
use super::{AudioError, CHANNELS, MAX_FRAME_BYTES, SAMPLES_PER_FRAME, SAMPLE_RATE};

/// Encodes PCM f32 audio frames to Opus compressed packets.
pub struct AudioEncoder {
    encoder: OpusEncoderInner,
}

impl AudioEncoder {
    /// Create a new Opus encoder (48kHz stereo, 10ms frames).
    pub fn new() -> Result<Self, AudioError> {
        let channels = Channels::Stereo;

        let encoder = OpusEncoderInner::new(SAMPLE_RATE, channels, magnum_opus::Application::Audio)
            .map_err(|e| AudioError::Codec(format!("Opus encoder init: {}", e)))?;

        Ok(Self { encoder })
    }

    /// Encode a PCM f32 frame to Opus bytes.
    /// Input: interleaved stereo samples (left, right, left, right, ...).
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>, AudioError> {
        let expected_len = SAMPLES_PER_FRAME * CHANNELS as usize;
        if pcm.len() != expected_len {
            return Err(AudioError::Codec(format!(
                "Expected {} samples, got {}",
                expected_len,
                pcm.len()
            )));
        }

        let mut output = vec![0u8; MAX_FRAME_BYTES];
        let bytes_written = self
            .encoder
            .encode_float(pcm, &mut output)
            .map_err(|e| AudioError::Codec(format!("Opus encode error: {}", e)))?;

        output.truncate(bytes_written);
        Ok(output)
    }
}
