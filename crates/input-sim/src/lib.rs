//! Keyboard and mouse input simulation using RustDesk's `enigo` crate.
//!
//! Provides cross-platform input injection:
//! - **Windows**: `SendInput` API
//! - **macOS**: Core Graphics events (`CGEvent`)
//! - **Linux**: `libxdo` (X11) / `evdev` (Wayland)

use rd_common::proto::{KeyEvent, MouseEvent, MouseEventType};
use rd_common::Result;

/// Input simulator wrapping enigo
pub struct InputSimulator {
    enigo: enigo::Enigo,
}

impl InputSimulator {
    /// Create a new input simulator for the current platform
    pub fn new() -> Self {
        Self {
            enigo: enigo::Enigo::new(),
        }
    }

    /// Simulate a keyboard event
    pub fn simulate_key(&mut self, event: &KeyEvent) -> Result<()> {
        use enigo::{Key, KeyboardControllable};

        // Map common keycodes to enigo Key enum
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
        use enigo::{MouseButton, MouseControllable};

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
        // enigo clipboard support varies by platform
        // Will implement properly in Phase 2
        Ok(String::new())
    }

    /// Set clipboard content
    pub fn set_clipboard(&mut self, _content: &str) -> Result<()> {
        // Will implement in Phase 2
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
