//! Client-side audio playback.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapProd};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use std::sync::{Arc, Mutex};

use super::{AudioError, CHANNELS, SAMPLE_RATE};

/// Plays decoded PCM f32 audio to the system output device.
pub struct AudioPlayer {
    /// Producer side of the ring buffer (receives PCM from decoder).
    producer: Arc<Mutex<HeapProd<f32>>>,
    /// cpal output stream.
    _stream: Option<cpal::Stream>,
    /// Whether playback is active.
    active: Arc<Mutex<bool>>,
}

impl AudioPlayer {
    /// Create a new player and start the output stream.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::Playback("No output device found".into()))?;

        let device_name = device.name().unwrap_or_default();
        tracing::info!("Audio playback device: {}", device_name);

        let supported_config = device
            .default_output_config()
            .map_err(|e| AudioError::Playback(format!("Config error: {}", e)))?;

        let sample_format = supported_config.sample_format();
        let config: cpal::StreamConfig = supported_config.into();

        // Create ring buffer (2 seconds).
        let buffer_size = (SAMPLE_RATE as usize * 2) * CHANNELS as usize;
        let buf = ringbuf::HeapRb::<f32>::new(buffer_size);
        let (prod, cons) = buf.split();

        let producer = Arc::new(Mutex::new(prod));
        let consumer = Arc::new(Mutex::new(cons));
        let active = Arc::new(Mutex::new(true));

        let cons_clone = consumer.clone();
        let active_clone = active.clone();

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                device.build_output_stream_raw(
                    &config,
                    sample_format,
                    move |data: &mut cpal::Data, _: &cpal::OutputCallbackInfo| {
                        let samples = data.as_slice_mut::<f32>().unwrap_or(&mut []);
                        let mut cons = cons_clone.lock().unwrap();
                        let available = cons.occupied_len();
                        let to_write = samples.len().min(available);

                        for i in 0..to_write {
                            if let Some(s) = cons.try_pop() {
                                samples[i] = s;
                            }
                        }
                        for i in to_write..samples.len() {
                            samples[i] = 0.0;
                        }
                    },
                    move |err| {
                        tracing::error!("Audio playback error: {}", err);
                        if let Ok(mut a) = active_clone.lock() {
                            *a = false;
                        }
                    },
                    None,
                )
            }
            _ => {
                return Err(AudioError::Playback(
                    "Only F32 sample format is supported".into(),
                ));
            }
        };

        let stream = stream
            .map_err(|e| AudioError::Playback(format!("Build output stream error: {}", e)))?;

        stream
            .play()
            .map_err(|e| AudioError::Playback(format!("Play error: {}", e)))?;

        tracing::info!("Audio playback started");

        Ok(Self {
            producer,
            _stream: Some(stream),
            active,
        })
    }

    /// Push decoded PCM samples into the playback buffer.
    pub fn push_samples(&self, pcm: &[f32]) -> usize {
        let mut prod = self.producer.lock().unwrap();
        let mut pushed = 0;
        for &sample in pcm {
            if prod.try_push(sample).is_ok() {
                pushed += 1;
            } else {
                break;
            }
        }
        pushed
    }

    /// Check if playback is still active.
    pub fn is_active(&self) -> bool {
        self.active.lock().map(|a| *a).unwrap_or(false)
    }
}
