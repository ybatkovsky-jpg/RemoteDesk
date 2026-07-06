//! Host-side audio capture from system audio output.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapCons};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use std::sync::{Arc, Mutex};

use super::{AudioError, CHANNELS, SAMPLES_PER_FRAME, SAMPLE_RATE};

/// Captures audio from the system and feeds frames into a ring buffer.
pub struct AudioCapturer {
    /// Consumer side of the ring buffer.
    consumer: Arc<Mutex<HeapCons<f32>>>,
    /// cpal stream handle (kept alive to continue capturing).
    _stream: Option<cpal::Stream>,
    /// Whether capture is active.
    active: Arc<Mutex<bool>>,
}

impl AudioCapturer {
    /// Create a new capturer and start capturing.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();

        let device = if cfg!(target_os = "windows") {
            host.output_devices()
                .map_err(|e| AudioError::Capture(format!("No output devices: {}", e)))?
                .find(|d| {
                    d.name()
                        .map(|n| n.contains("loopback") || n.contains("Loopback"))
                        .unwrap_or(false)
                })
                .or_else(|| host.default_output_device())
        } else {
            host.default_input_device()
        };

        let device = device.ok_or_else(|| {
            AudioError::Capture("No suitable audio device found".into())
        })?;

        let device_name = device.name().unwrap_or_default();
        tracing::info!("Audio capture device: {}", device_name);

        let supported_config = if cfg!(target_os = "windows") {
            device
                .default_output_config()
                .map_err(|e| AudioError::Capture(format!("Config error: {}", e)))?
        } else {
            device
                .default_input_config()
                .map_err(|e| AudioError::Capture(format!("Config error: {}", e)))?
        };

        let sample_format = supported_config.sample_format();
        let config: cpal::StreamConfig = supported_config.into();

        // Create ring buffer (2 seconds of audio).
        let buffer_size = (SAMPLE_RATE as usize * 2) * CHANNELS as usize;
        let buf = ringbuf::HeapRb::<f32>::new(buffer_size);
        let (prod, cons) = buf.split();

        let producer = Arc::new(Mutex::new(prod));
        let consumer = Arc::new(Mutex::new(cons));
        let active = Arc::new(Mutex::new(true));

        let prod_clone = producer.clone();
        let active_clone = active.clone();
        let channels = config.channels as usize;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                device.build_input_stream_raw(
                    &config,
                    sample_format,
                    move |data: &cpal::Data, _: &cpal::InputCallbackInfo| {
                        let samples = data.as_slice::<f32>().unwrap_or(&[]);
                        let mut prod = prod_clone.lock().unwrap();
                        for chunk in samples.chunks(channels) {
                            match chunk.len() {
                                0 => {}
                                1 => {
                                    let _ = prod.try_push(chunk[0]);
                                    let _ = prod.try_push(chunk[0]);
                                }
                                _ => {
                                    let _ = prod.try_push(*chunk.first().unwrap_or(&0.0));
                                    let _ = prod.try_push(*chunk.get(1).unwrap_or(&0.0));
                                }
                            }
                        }
                    },
                    move |err| {
                        tracing::error!("Audio capture error: {}", err);
                        if let Ok(mut a) = active_clone.lock() {
                            *a = false;
                        }
                    },
                    None,
                )
            }
            _ => {
                tracing::warn!(
                    "Audio capture format {:?} not F32 — audio will be silent",
                    sample_format
                );
                return Err(AudioError::Capture(
                    "Only F32 sample format is supported".into(),
                ));
            }
        };

        let stream = stream
            .map_err(|e| AudioError::Capture(format!("Build input stream error: {}", e)))?;

        stream
            .play()
            .map_err(|e| AudioError::Capture(format!("Play error: {}", e)))?;

        tracing::info!("Audio capture started");

        Ok(Self {
            consumer,
            _stream: Some(stream),
            active,
        })
    }

    /// Read one frame worth of PCM samples from the ring buffer.
    pub fn read_frame(&mut self) -> Option<Vec<f32>> {
        let expected = SAMPLES_PER_FRAME * CHANNELS as usize;
        let mut cons = self.consumer.lock().ok()?;
        if cons.occupied_len() < expected {
            return None;
        }

        let mut frame = Vec::with_capacity(expected);
        for _ in 0..expected {
            match cons.try_pop() {
                Some(s) => frame.push(s),
                None => break,
            }
        }

        if frame.len() >= expected {
            Some(frame)
        } else {
            None
        }
    }

    /// Check if capture is still active.
    pub fn is_active(&self) -> bool {
        self.active.lock().map(|a| *a).unwrap_or(false)
    }
}
