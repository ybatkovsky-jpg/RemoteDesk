//! Opus decoder: compressed Opus frames → PCM f32.

use magnum_opus::{Channels, Decoder as OpusDecoderInner};
use super::{AudioError, CHANNELS, SAMPLE_RATE};

/// Decodes Opus compressed packets back to PCM f32.
pub struct AudioDecoder {
    decoder: OpusDecoderInner,
}

impl AudioDecoder {
    /// Create a new Opus decoder (48kHz stereo).
    pub fn new() -> Result<Self, AudioError> {
        let channels = Channels::Stereo;

        let decoder = OpusDecoderInner::new(SAMPLE_RATE, channels)
            .map_err(|e| AudioError::Codec(format!("Opus decoder init: {}", e)))?;

        Ok(Self { decoder })
    }

    /// Decode an Opus packet to PCM f32 samples.
    /// Returns interleaved stereo samples.
    pub fn decode(&mut self, opus_data: &[u8], max_frame_size: usize) -> Result<Vec<f32>, AudioError> {
        let mut pcm = vec![0.0f32; max_frame_size * CHANNELS as usize];

        let samples_decoded = self
            .decoder
            .decode_float(opus_data, &mut pcm, false)
            .map_err(|e| AudioError::Codec(format!("Opus decode error: {}", e)))?;

        pcm.truncate(samples_decoded * CHANNELS as usize);
        Ok(pcm)
    }
}
