//! Cross-platform screen capture.
//!
//! With `native` feature on Windows: uses GDI (`BitBlt` + `GetDIBits`) for capture.
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

// ── Windows native (GDI) ────────────────────────────────────

#[cfg(all(target_os = "windows", feature = "native"))]
mod win {
    use super::*;
    use rd_common::Error;
    use std::mem;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        HGDIOBJ, RGBQUAD, SRCCOPY,
        GET_DEVICE_CAPS_INDEX, GetDeviceCaps,
    };

    pub struct Capturer {
        display_id: usize,
        display_name: String,
        width: u32,
        height: u32,
    }

    impl Capturer {
        pub fn new(display_index: usize) -> Result<Self> {
            let displays = list_displays_impl()?;
            let info = displays
                .into_iter()
                .nth(display_index)
                .ok_or_else(|| Error::Capture(format!("Display index {} not found", display_index)))?;

            Ok(Self {
                display_id: display_index,
                display_name: info.name,
                width: info.width,
                height: info.height,
            })
        }

        pub fn capture_frame(&mut self, _timeout_ms: u64) -> Result<Option<Frame>> {
            unsafe {
                let hwnd = HWND(std::ptr::null_mut());
                let hdc_screen = GetDC(hwnd);
                if hdc_screen.0.is_null() {
                    return Err(Error::Capture("GetDC failed".into()));
                }

                let hdc_mem = CreateCompatibleDC(hdc_screen);
                if hdc_mem.0.is_null() {
                    ReleaseDC(hwnd, hdc_screen);
                    return Err(Error::Capture("CreateCompatibleDC failed".into()));
                }

                let hbmp = CreateCompatibleBitmap(hdc_screen, self.width as i32, self.height as i32);
                if hbmp.0.is_null() {
                    let _ = DeleteDC(hdc_mem);
                    let _ = ReleaseDC(hwnd, hdc_screen);
                    return Err(Error::Capture("CreateCompatibleBitmap failed".into()));
                }

                // Select bitmap into memory DC and copy from screen
                let old_bmp = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
                let _ = BitBlt(
                    hdc_mem,
                    0, 0,
                    self.width as i32,
                    self.height as i32,
                    hdc_screen,
                    0, 0,
                    SRCCOPY,
                );

                // Get the bitmap bits as BGRA
                let mut bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: self.width as i32,
                        biHeight: -(self.height as i32), // negative = top-down
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: 0, // BI_RGB
                        biSizeImage: self.width * self.height * 4,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [RGBQUAD::default(); 1],
                };

                let buf_size = (self.width * self.height * 4) as usize;
                let mut buf: Vec<u8> = vec![0u8; buf_size];

                GetDIBits(
                    hdc_mem,
                    hbmp,
                    0,
                    self.height,
                    Some(buf.as_mut_ptr() as *mut _),
                    &mut bmi,
                    DIB_RGB_COLORS,
                );

                // BGRA → RGBA: swap R and B channels
                for pixel in buf.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }

                // Cleanup
                SelectObject(hdc_mem, old_bmp);
                let _ = DeleteObject(HGDIOBJ(hbmp.0));
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(hwnd, hdc_screen);

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                Ok(Some(Frame {
                    data: buf,
                    width: self.width,
                    height: self.height,
                    stride: self.width * 4,
                    display_id: self.display_id,
                    timestamp,
                }))
            }
        }

        pub fn width(&self) -> u32 { self.width }
        pub fn height(&self) -> u32 { self.height }
        pub fn display_id(&self) -> usize { self.display_id }
        pub fn display_name(&self) -> String { self.display_name.clone() }
    }

    pub fn list_displays_impl() -> Result<Vec<DisplayInfo>> {
        unsafe {
            let hwnd = HWND(std::ptr::null_mut());
            let dc = GetDC(hwnd);
            let w = GetDeviceCaps(dc, GET_DEVICE_CAPS_INDEX(118 /* HORZRES */)) as u32;
            let h = GetDeviceCaps(dc, GET_DEVICE_CAPS_INDEX(117 /* VERTRES */)) as u32;
            ReleaseDC(hwnd, dc);

            Ok(vec![DisplayInfo {
                id: 0,
                name: "Primary Display".into(),
                width: w,
                height: h,
                is_primary: true,
                dpi: 1.0,
            }])
        }
    }
}

// ── Stub (non-native) ──────────────────────────────────────

#[cfg(not(feature = "native"))]
mod stub {
    use super::*;

    pub struct Capturer {
        display_id: usize,
        width: u32,
        height: u32,
    }

    impl Capturer {
        pub fn new(display_index: usize) -> Result<Self> {
            Ok(Self { display_id: display_index, width: 1920, height: 1080 })
        }

        pub fn capture_frame(&mut self, _timeout_ms: u64) -> Result<Option<Frame>> {
            Ok(None)
        }

        pub fn width(&self) -> u32 { self.width }
        pub fn height(&self) -> u32 { self.height }
        pub fn display_id(&self) -> usize { self.display_id }
        pub fn display_name(&self) -> String { "Stub Display".into() }
    }

    pub fn list_displays_impl() -> Result<Vec<DisplayInfo>> {
        Ok(vec![DisplayInfo {
            id: 0, name: "Stub Display".into(),
            width: 1920, height: 1080,
            is_primary: true, dpi: 1.0,
        }])
    }
}

// ── Public re-exports ──────────────────────────────────────

#[cfg(feature = "native")]
pub use win::Capturer;

#[cfg(not(feature = "native"))]
pub use stub::Capturer;

/// List all available displays
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    #[cfg(feature = "native")]
    { win::list_displays_impl() }
    #[cfg(not(feature = "native"))]
    { stub::list_displays_impl() }
}
