//! Protobuf message definitions for the RemoteDesk protocol.
//!
//! These will be generated from `.proto` files adapted from RustDesk.
//! For Phase 0, we define stub types that will be replaced by proper
//! protobuf-generated code once we integrate the RustDesk protocol definitions.

use serde::{Deserialize, Serialize};

/// Placeholder for protocol message types.
/// Will be replaced by generated protobuf code from `vendor/rustdesk/libs/hbb_common/protos/`.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFrame {
    pub display_id: usize,
    pub timestamp: u64,
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub key_frame: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub down: bool,
    pub keycode: u32,
    pub scancode: u32,
    pub modifiers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseEvent {
    pub event_type: MouseEventType,
    pub x: f64,
    pub y: f64,
    pub buttons: u32,
    pub wheel_delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseEventType {
    Move,
    ButtonDown,
    ButtonUp,
    Wheel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardData {
    pub content: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub dpi: f64,
}
