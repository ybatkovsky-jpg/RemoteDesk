import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── Types ──────────────────────────────────────────────

export interface DisplayInfo {
  id: number;
  name: string;
  width: number;
  height: number;
  is_primary: boolean;
  dpi: number;
}

export interface KeyEvent {
  down: boolean;
  keycode: number;
  scancode: number;
  modifiers: number;
}

export interface MouseEvent {
  event_type: "Move" | "ButtonDown" | "ButtonUp" | "Wheel";
  x: number;
  y: number;
  buttons: number;
  wheel_delta: number;
}

export interface AppStatus {
  mode: string;
  host_port: number;
  client_addr: string;
  connection_state: string;
  display_width: number;
  display_height: number;
}

// ── Commands ───────────────────────────────────────────

export async function getVersion(): Promise<string> {
  return invoke("get_version");
}

export async function getAppStatus(): Promise<AppStatus> {
  return invoke("get_app_status");
}

export async function listDisplays(): Promise<DisplayInfo[]> {
  return invoke("list_displays");
}

export async function startHost(
  displayId: number,
  port: number,
  fps: number
): Promise<void> {
  return invoke("start_host", { displayId, port, fps });
}

export async function stopHost(): Promise<void> {
  return invoke("stop_host");
}

export async function clientConnect(addr: string): Promise<void> {
  return invoke("client_connect", { addr });
}

export async function clientDisconnect(): Promise<void> {
  return invoke("client_disconnect");
}

export async function clientGetFrame(): Promise<string | null> {
  return invoke("client_get_frame");
}

export async function clientGetFrameSize(): Promise<[number, number]> {
  return invoke("client_get_frame_size");
}

export async function clientGetState(): Promise<string> {
  return invoke("client_get_state");
}

export async function sendKeyEvent(event: KeyEvent): Promise<void> {
  return invoke("send_key_event", { event });
}

export async function sendMouseEvent(event: MouseEvent): Promise<void> {
  return invoke("send_mouse_event", { event });
}

// ── Events ─────────────────────────────────────────────

export function onHostStatus(
  callback: (data: { status: string; port?: number; message?: string }) => void
): Promise<UnlistenFn> {
  return listen<{ status: string; port?: number; message?: string }>(
    "host-status",
    (event) => callback(event.payload)
  );
}

export function onConnectionState(
  callback: (data: { state: string; message?: string }) => void
): Promise<UnlistenFn> {
  return listen<{ state: string; message?: string }>(
    "connection-state",
    (event) => callback(event.payload)
  );
}
