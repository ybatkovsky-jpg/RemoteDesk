import { createSignal, onCleanup, onMount } from "solid-js";
import { clientGetFrameRaw, clientGetFrameSize, sendMouseEvent, sendKeyEvent } from "../lib/tauri";

interface RemoteScreenProps {
  connected: boolean;
}

/**
 * Renders remote desktop frames on a canvas.
 * Polls for new frames and draws them via putImageData.
 */
export default function RemoteScreen(props: RemoteScreenProps) {
  let canvasRef: HTMLCanvasElement | undefined;
  const [frameSize, setFrameSize] = createSignal<{ w: number; h: number } | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  // Poll for frames while connected
  onMount(() => {
    pollTimer = setInterval(async () => {
      if (!props.connected) return;

      try {
        // Get frame size first
        if (!frameSize()) {
          const size = await clientGetFrameSize();
          if (size) {
            setFrameSize({ w: size[0], h: size[1] });
          }
        }

        // Get latest frame as raw ArrayBuffer (no base64 overhead)
        const bytes = await clientGetFrameRaw();
        if (bytes && canvasRef) {
          const ctx = canvasRef.getContext("2d");
          if (!ctx) return;

          const size = frameSize();
          if (!size) return;

          // Create ImageData from raw BGRA buffer (Uint8Array from Tauri)
          const clamped = new Uint8ClampedArray(
            bytes.buffer as unknown as ArrayBuffer,
            bytes.byteOffset,
            bytes.byteLength
          );
          const imgData = new ImageData(clamped, size.w, size.h);

          // Resize canvas if needed
          if (canvasRef.width !== size.w || canvasRef.height !== size.h) {
            canvasRef.width = size.w;
            canvasRef.height = size.h;
          }

          ctx.putImageData(imgData, 0, 0);
        }
      } catch (e) {
        // Silently ignore — frame not ready yet
      }
    }, 1000 / 30); // Poll at 30 FPS

    onCleanup(() => {
      if (pollTimer) clearInterval(pollTimer);
    });
  });

  // Mouse handler: send mouse events to host
  const handleMouse = (e: MouseEvent) => {
    if (!props.connected) return;
    const rect = canvasRef?.getBoundingClientRect();
    if (!rect) return;

    const size = frameSize();
    if (!size) return;

    // Scale coordinates to remote display size
    const x = ((e.clientX - rect.left) / rect.width) * size.w;
    const y = ((e.clientY - rect.top) / rect.height) * size.h;

    let eventType: "Move" | "ButtonDown" | "ButtonUp" | "Wheel" = "Move";
    if (e.type === "mousedown") eventType = "ButtonDown";
    else if (e.type === "mouseup") eventType = "ButtonUp";
    else if (e.type === "wheel") eventType = "Wheel";

    sendMouseEvent({
      event_type: eventType,
      x,
      y,
      buttons: e.buttons,
      wheel_delta: (e as WheelEvent).deltaY || 0,
    }).catch(() => {});
  };

  // Keyboard handler
  const handleKey = (e: KeyboardEvent) => {
    if (!props.connected) return;

    sendKeyEvent({
      down: e.type === "keydown",
      keycode: e.keyCode || 0,
      scancode: 0, // Simplified
      modifiers:
        (e.ctrlKey ? 2 : 0) |
        (e.altKey ? 4 : 0) |
        (e.shiftKey ? 1 : 0) |
        (e.metaKey ? 8 : 0),
    }).catch(() => {});
  };

  return (
    <div class="remote-screen-container">
      {!props.connected && (
        <div class="remote-placeholder">
          <p>Connect to a remote host to see the screen</p>
        </div>
      )}
      <canvas
        ref={canvasRef}
        class="remote-canvas"
        classList={{ hidden: !props.connected }}
        onMouseMove={handleMouse}
        onMouseDown={handleMouse}
        onMouseUp={handleMouse}
        onWheel={handleMouse}
        onKeyDown={handleKey}
        onKeyUp={handleKey}
        tabIndex={0}
      />
    </div>
  );
}
