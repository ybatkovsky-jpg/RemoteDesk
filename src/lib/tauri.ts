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

// ── Config types ───────────────────────────────────────

export interface ServerConfig {
  rendezvous_server: string;
  relay_server: string;
  api_server: string | null;
}

export interface KeyPair {
  public_key: number[];
  secret_key: number[];
}

export interface Permissions {
  keyboard: boolean;
  mouse: boolean;
  clipboard: boolean;
  file_transfer: boolean;
  audio: boolean;
}

export interface SecurityConfig {
  password: string | null;
  key_pair: KeyPair | null;
  permissions: Permissions;
}

export interface VideoConfig {
  max_fps: number;
  codec: string;
  quality: number;
  bitrate: number;
}

export interface Config {
  id: string;
  server: ServerConfig;
  video: VideoConfig;
  security: SecurityConfig;
}

// ── Chat types ─────────────────────────────────────────

export interface ChatEntry {
  text: string;
  sender: string;
  timestamp: number;
}

// ── File transfer types ────────────────────────────────

export interface FileEntry {
  name: string;
  path: string;
  size: number;
  is_dir: boolean;
  modified: number;
}

export interface FileTransferProgress {
  path: string;
  total_size: number;
  received_bytes: number;
  done: boolean;
  error: string | null;
}

// ── Commands ───────────────────────────────────────────

export async function getVersion(): Promise<string> {
  return invoke("get_version");
}

export async function getAppStatus(): Promise<AppStatus> {
  return invoke("get_app_status");
}

// Config
export async function loadConfig(): Promise<Config> {
  return invoke("load_config");
}

export async function saveConfig(config: Config): Promise<void> {
  return invoke("save_config", { config });
}

export async function getConfig(): Promise<Config> {
  return invoke("get_config");
}

// Auth
export async function setHostPassword(password: string): Promise<void> {
  return invoke("set_host_password", { password });
}

export async function setClientPassword(password: string): Promise<void> {
  return invoke("set_client_password", { password });
}

// Displays
export async function listDisplays(): Promise<DisplayInfo[]> {
  return invoke("list_displays");
}

export async function getHostDisplays(): Promise<DisplayInfo[]> {
  return invoke("get_host_displays");
}

export async function switchDisplay(displayId: number): Promise<void> {
  return invoke("switch_display", { displayId });
}

// Host
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

// Client
export async function clientConnect(addr: string): Promise<void> {
  return invoke("client_connect", { addr });
}

export async function clientDisconnect(): Promise<void> {
  return invoke("client_disconnect");
}

export async function clientConnectById(peerId: string): Promise<void> {
  return invoke("client_connect_by_id", { peerId });
}

export async function getPeerId(): Promise<string> {
  return invoke("get_peer_id");
}

export async function clientGetFrame(): Promise<string | null> {
  return invoke("client_get_frame");
}

export async function clientGetFrameRaw(): Promise<string | null> {
  return invoke("client_get_frame_raw");
}

export async function clientGetFrameSize(): Promise<[number, number]> {
  return invoke("client_get_frame_size");
}

export async function clientGetState(): Promise<string> {
  return invoke("client_get_state");
}

// Input
export async function sendKeyEvent(event: KeyEvent): Promise<void> {
  return invoke("send_key_event", { event });
}

export async function sendMouseEvent(event: MouseEvent): Promise<void> {
  return invoke("send_mouse_event", { event });
}

// Chat
export async function sendChatMessage(text: string, sender: string): Promise<void> {
  return invoke("send_chat_message", { text, sender });
}

export async function getChatHistory(): Promise<ChatEntry[]> {
  return invoke("get_chat_history");
}

// File Transfer
export async function requestFileList(path: string): Promise<FileEntry[]> {
  return invoke("request_file_list", { path });
}

export async function requestFile(path: string): Promise<void> {
  return invoke("request_file", { path });
}

export async function getFileProgress(): Promise<FileTransferProgress | null> {
  return invoke("get_file_progress");
}

export async function cancelFileTransfer(reason: string): Promise<void> {
  return invoke("cancel_file_transfer", { reason });
}

export async function sendFileToHost(path: string, data: number[]): Promise<void> {
  return invoke("send_file_to_host", { path, data });
}

// Audio
export async function toggleAudio(enable: boolean): Promise<void> {
  return invoke("toggle_audio", { enable });
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
