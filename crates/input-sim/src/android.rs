//! Android input simulation via JNI bridge to Java MainService.
//!
//! On Android, keyboard and mouse/touch input is injected by calling
//! Java-side MainService methods through JNI. When the Tauri Android
//! build includes the scrap crate, these functions are provided by
//! scrap::android::ffi. For standalone builds, we declare them as
//! extern "C" and link at runtime.

use rd_common::proto::{KeyEvent, MouseEvent, MouseEventType};
use rd_common::Result;

/// Android input simulator.
/// Uses JNI calls to Java MainService (via scrap FFI when available).
pub struct AndroidInputSimulator;

impl AndroidInputSimulator {
    pub fn new() -> Self {
        AndroidInputSimulator
    }

    pub fn simulate_key(&mut self, event: &KeyEvent) -> Result<()> {
        // Encode key event: [down(1B), keycode(4B LE), scancode(4B LE), modifiers(4B LE)]
        let mut data = Vec::with_capacity(13);
        data.push(if event.down { 1u8 } else { 0u8 });
        data.extend_from_slice(&event.keycode.to_le_bytes());
        data.extend_from_slice(&event.scancode.to_le_bytes());
        data.extend_from_slice(&event.modifiers.to_le_bytes());

        // Call JNI bridge — when scrap is linked, this function is available.
        #[cfg(target_os = "android")]
        {
            // The function is provided by scrap::android::ffi::call_main_service_key_event
            // which is linked via the Android build system.
            extern "C" {
                fn call_main_service_key_event(data: *const u8, len: usize) -> i32;
            }
            unsafe {
                let ret = call_main_service_key_event(data.as_ptr(), data.len());
                if ret != 0 {
                    return Err(rd_common::Error::Input(format!(
                        "Android key event failed: {}",
                        ret
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn simulate_mouse(&mut self, event: &MouseEvent) -> Result<()> {
        #[cfg(target_os = "android")]
        {
            let kind = match event.event_type {
                MouseEventType::Move => "move",
                MouseEventType::ButtonDown => "down",
                MouseEventType::ButtonUp => "up",
                MouseEventType::Wheel => "wheel",
            };

            extern "C" {
                fn call_main_service_pointer_input(
                    kind: *const u8,
                    kind_len: usize,
                    mask: i32,
                    x: i32,
                    y: i32,
                ) -> i32;
            }
            unsafe {
                let kind_bytes = kind.as_bytes();
                let ret = call_main_service_pointer_input(
                    kind_bytes.as_ptr(),
                    kind_bytes.len(),
                    event.buttons as i32,
                    event.x as i32,
                    event.y as i32,
                );
                if ret != 0 {
                    return Err(rd_common::Error::Input(format!(
                        "Android pointer event failed: {}",
                        ret
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn get_clipboard(&mut self) -> Result<String> {
        tracing::warn!("Clipboard get not implemented on Android");
        Ok(String::new())
    }

    pub fn set_clipboard(&mut self, _content: &str) -> Result<()> {
        tracing::warn!("Clipboard set not implemented on Android");
        Ok(())
    }
}
