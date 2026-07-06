//! Audio subsystem for RemoteDesk.
//!
//! Provides:
//! - Capture: system audio output (WASAPI loopback on Windows, default input on others)
//! - Encoding: PCM f32 → Opus (48kHz stereo, 10ms frames)
//! - Decoding: Opus → PCM f32
//! - Playback: PCM f32 → system audio output
//!
//! Architecture:
//! ```
//! Host: AudioCapturer → OpusEncoder → NetworkMessage::AudioFrame → network
//! Client: network → OpusDecoder → AudioPlayer → speakers
//! ```

pub mod capture;
pub mod encoder;
pub mod decoder;
pub mod playback;

pub use capture::AudioCapturer;
pub use decoder::AudioDecoder;
pub use encoder::AudioEncoder;
pub use playback::AudioPlayer;

// ── Safety impls for cpal types ──────────────────────────
// cpal Streams contain `*mut ()` via NotSendSyncAcrossAllPlatforms,
// which exists primarily for macOS CoreAudio safety. On Windows/Linux,
// the streams are safe to move between threads.
unsafe impl Send for AudioCapturer {}
unsafe impl Sync for AudioCapturer {}
unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

// ── Constants ─────────────────────────────────────────────

/// Default sample rate for audio streaming.
pub const SAMPLE_RATE: u32 = 48000;
/// Default number of channels (stereo).
pub const CHANNELS: u16 = 2;
/// Frame duration in milliseconds (standard Opus frame size).
pub const FRAME_MS: u32 = 10;
/// Number of samples per channel per frame.
pub const SAMPLES_PER_FRAME: usize = (SAMPLE_RATE as usize * FRAME_MS as usize) / 1000;
/// Maximum encoded Opus frame size in bytes.
pub const MAX_FRAME_BYTES: usize = 4096;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Capture error: {0}")]
    Capture(String),
    #[error("Playback error: {0}")]
    Playback(String),
    #[error("Codec error: {0}")]
    Codec(String),
    #[error("Buffer error: {0}")]
    Buffer(String),
}
