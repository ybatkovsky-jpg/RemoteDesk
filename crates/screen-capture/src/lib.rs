//! Cross-platform screen capture.
//!
//! With `native` feature: uses RustDesk's `scrap` crate (DXGI/ScreenCaptureKit/PipeWire).
//! Without `native`: stub implementation for development.

use rd_common::proto::DisplayInfo;
use rd_common::Result;

/// Represents a captured frame from a display
#[derive(Debug, Clone)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub display_id: usize,
    pub timestamp: u64,
}

/// Screen capturer — real when `native` feature is enabled
#[cfg(feature = "native")]
pub struct Capturer {
    inner: scrap::Capturer,
    display: scrap::Display,
    display_id: usize,
    width: u32,
    height: u32,
}

/// Stub capturer for dev builds
#[cfg(not(feature = "native"))]
pub struct Capturer {
    display_id: usize,
    width: u32,
    height: u32,
}

impl Capturer {
    #[cfg(feature = "native")]
    pub fn new(display_index: usize) -> Result<Self> {
        let displays = scrap::Display::all().map_err(|e| {
            Error::Capture(format!("Failed to enumerate displays: {}", e))
        })?;

        let display = displays.into_iter().nth(display_index).ok_or_else(|| {
            Error::Capture(format!("Display index {} not found", display_index))
        })?;

        let width = display.width() as u32;
        let height = display.height() as u32;

        let inner = scrap::Capturer::new(display.clone()).map_err(|e| {
            Error::Capture(format!("Failed to create capturer: {}", e))
        })?;

        Ok(Self {
            inner,
            display,
            display_id: display_index,
            width,
            height,
        })
    }

    #[cfg(not(feature = "native"))]
    pub fn new(display_index: usize) -> Result<Self> {
        Ok(Self {
            display_id: display_index,
            width: 1920,
            height: 1080,
        })
    }

    #[cfg(feature = "native")]
    pub fn capture_frame(&mut self, timeout_ms: u64) -> Result<Option<Frame>> {
        let timeout = std::time::Duration::from_millis(timeout_ms);

        match self.inner.frame(timeout) {
            Ok(frame) => {
                let data = frame_to_bgra(&frame);
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                Ok(Some(Frame {
                    data,
                    width: self.width,
                    height: self.height,
                    stride: self.width * 4,
                    display_id: self.display_id,
                    timestamp,
                }))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(Error::Capture(format!("Capture error: {}", e))),
        }
    }

    #[cfg(not(feature = "native"))]
    pub fn capture_frame(&mut self, _timeout_ms: u64) -> Result<Option<Frame>> {
        Ok(None) // Stub: no frames
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn display_id(&self) -> usize {
        self.display_id
    }

    #[cfg(feature = "native")]
    pub fn display_name(&self) -> String {
        self.display.name()
    }

    #[cfg(not(feature = "native"))]
    pub fn display_name(&self) -> String {
        "Stub Display".into()
    }
}

#[cfg(feature = "native")]
fn frame_to_bgra(frame: &scrap::Frame<'_>) -> Vec<u8> {
    match frame {
        scrap::Frame::PixelBuffer(pb) => pb.data().to_vec(),
        scrap::Frame::Texture(_) => {
            tracing::warn!("GPU texture frame cannot be read as BGRA without VRAM support");
            vec![]
        }
    }
}

/// List all available displays
#[cfg(feature = "native")]
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    let displays = scrap::Display::all().map_err(|e| {
        Error::Capture(format!("Failed to enumerate displays: {}", e))
    })?;

    Ok(displays
        .into_iter()
        .enumerate()
        .map(|(id, d)| DisplayInfo {
            id,
            name: d.name(),
            width: d.width() as u32,
            height: d.height() as u32,
            is_primary: d.is_primary(),
            dpi: 1.0,
        })
        .collect())
}

#[cfg(not(feature = "native"))]
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    Ok(vec![DisplayInfo {
        id: 0,
        name: "Stub Display".into(),
        width: 1920,
        height: 1080,
        is_primary: true,
        dpi: 1.0,
    }])
}
