//! Keyboard and mouse input simulation using RustDesk's `enigo` crate.
//!
//! Provides cross-platform input injection:
//! - **Windows**: `SendInput` API
//! - **macOS**: Core Graphics events (`CGEvent`)
//! - **Linux**: `libxdo` (X11) / `evdev` (Wayland)
//! - **Android**: JNI bridge via scrap (`call_main_service_pointer_input`, `call_main_service_key_event`)

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::AndroidInputSimulator as InputSimulator;

#[cfg(not(target_os = "android"))]
use rd_common::proto::{KeyEvent, MouseEvent, MouseEventType};
#[cfg(not(target_os = "android"))]
use rd_common::Result;

/// Input simulator wrapping enigo (or Android JNI bridge)
#[cfg(not(target_os = "android"))]
pub struct InputSimulator {
    enigo: enigo::Enigo,
}

#[cfg(not(target_os = "android"))]
impl InputSimulator {
    /// Create a new input simulator for the current platform
    pub fn new() -> Self {
        Self {
            enigo: enigo::Enigo::new(),
        }
    }

    /// Simulate a keyboard event
    pub fn simulate_key(&mut self, event: &KeyEvent) -> Result<()> {
        use enigo::KeyboardControllable;

        let key = keycode_to_enigo(event.keycode);

        if event.down {
            self.enigo.key_down(key).map_err(|e| {
                rd_common::Error::Input(format!("Key down error: {:?}", e))
            })?;
        } else {
            self.enigo.key_up(key);
        }
        Ok(())
    }

    /// Simulate a mouse event
    pub fn simulate_mouse(&mut self, event: &MouseEvent) -> Result<()> {
        use enigo::MouseControllable;

        match event.event_type {
            MouseEventType::Move => {
                self.enigo.mouse_move_to(event.x as i32, event.y as i32);
            }
            MouseEventType::ButtonDown => {
                let button = mouse_button_from_code(event.buttons);
                self.enigo.mouse_down(button).map_err(|e| {
                    rd_common::Error::Input(format!("Mouse down error: {:?}", e))
                })?;
            }
            MouseEventType::ButtonUp => {
                let button = mouse_button_from_code(event.buttons);
                self.enigo.mouse_up(button);
            }
            MouseEventType::Wheel => {
                self.enigo.mouse_scroll_y(event.wheel_delta);
            }
        }
        Ok(())
    }

    /// Get clipboard content
    pub fn get_clipboard(&mut self) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            clipboard_win::get_clipboard_text()
        }
        #[cfg(not(target_os = "windows"))]
        {
            tracing::warn!("Clipboard get not implemented on this platform");
            Ok(String::new())
        }
    }

    /// Set clipboard content
    pub fn set_clipboard(&mut self, content: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            clipboard_win::set_clipboard_text(content)
        }
        #[cfg(not(target_os = "windows"))]
        {
            tracing::warn!("Clipboard set not implemented on this platform");
            Ok(())
        }
    }
}

/// Windows clipboard using raw Win32 FFI (avoids windows crate version conflicts).
#[cfg(target_os = "windows")]
mod clipboard_win {
    use rd_common::Result;

    // Raw Win32 FFI declarations.
    extern "system" {
        fn OpenClipboard(hWndNewOwner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(uFormat: u32) -> isize;
        fn SetClipboardData(uFormat: u32, hMem: isize) -> isize;
        fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> isize;
        fn GlobalLock(hMem: isize) -> *mut u8;
        fn GlobalUnlock(hMem: isize) -> i32;
    }

    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;

    pub fn get_clipboard_text() -> Result<String> {
        unsafe {
            if OpenClipboard(0) == 0 {
                return Err(rd_common::Error::Input("Failed to open clipboard".into()));
            }

            let result = {
                let handle = GetClipboardData(CF_UNICODETEXT);
                if handle != 0 {
                    let ptr = GlobalLock(handle);
                    if !ptr.is_null() {
                        let len = (0..).take_while(|&i| *((ptr as *const u16).add(i)) != 0).count();
                        let slice = std::slice::from_raw_parts(ptr as *const u16, len);
                        let text = String::from_utf16_lossy(slice);
                        GlobalUnlock(handle);
                        Ok(text)
                    } else {
                        Err(rd_common::Error::Input("Failed to lock clipboard".into()))
                    }
                } else {
                    Ok(String::new())
                }
            };

            CloseClipboard();
            result
        }
    }

    pub fn set_clipboard_text(text: &str) -> Result<()> {
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            if OpenClipboard(0) == 0 {
                return Err(rd_common::Error::Input("Failed to open clipboard for set".into()));
            }

            EmptyClipboard();

            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            if mem == 0 {
                CloseClipboard();
                return Err(rd_common::Error::Input("GlobalAlloc failed".into()));
            }

            let ptr = GlobalLock(mem);
            if ptr.is_null() {
                CloseClipboard();
                return Err(rd_common::Error::Input("GlobalLock failed".into()));
            }

            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            GlobalUnlock(mem);

            SetClipboardData(CF_UNICODETEXT, mem);
            CloseClipboard();
        }

        Ok(())
    }
}

/// Map a keycode to an enigo Key
fn keycode_to_enigo(code: u32) -> enigo::Key {
    use enigo::Key;
    match code {
        // Letters
        0x41..=0x5A => Key::Layout(char::from_u32(code).unwrap_or('?')),
        // Numbers
        0x30..=0x39 => Key::Layout(char::from_u32(code).unwrap_or('?')),
        // Function keys (simplified mapping)
        0x70 => Key::F1,
        0x71 => Key::F2,
        0x72 => Key::F3,
        0x73 => Key::F4,
        0x74 => Key::F5,
        0x75 => Key::F6,
        0x76 => Key::F7,
        0x77 => Key::F8,
        0x78 => Key::F9,
        0x79 => Key::F10,
        0x7A => Key::F11,
        0x7B => Key::F12,
        // Navigation
        0x25 => Key::LeftArrow,
        0x26 => Key::UpArrow,
        0x27 => Key::RightArrow,
        0x28 => Key::DownArrow,
        // Special keys
        0x0D => Key::Return,
        0x1B => Key::Escape,
        0x09 => Key::Tab,
        0x20 => Key::Space,
        0x08 => Key::Backspace,
        0x2E => Key::Delete,
        0x24 => Key::Home,
        0x23 => Key::End,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        // Modifiers
        0x10 => Key::Shift,
        0x11 => Key::Control,
        0x12 => Key::Alt,
        0x5B => Key::Meta,
        // Default
        _ => Key::Raw(code as u16),
    }
}

/// Map a button code to enigo MouseButton
fn mouse_button_from_code(buttons: u32) -> enigo::MouseButton {
    match buttons {
        1 => enigo::MouseButton::Left,
        2 => enigo::MouseButton::Right,
        4 => enigo::MouseButton::Middle,
        _ => enigo::MouseButton::Left,
    }
}
