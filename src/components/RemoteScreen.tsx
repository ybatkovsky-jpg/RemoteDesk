import { createSignal, onCleanup, onMount } from "solid-js";
import { clientGetFrameRaw, clientGetFrameSize, sendMouseEvent, sendKeyEvent, getHostDisplays, type DisplayInfo } from "../lib/tauri";

interface RemoteScreenProps {
  connected: boolean;
  onDisplaySwitch?: (displayId: number) => void;
}

/**
 * Renders remote desktop frames on a canvas.
 * Polls for new frames and draws them via putImageData.
 * Supports multi-monitor switching.
 */
export default function RemoteScreen(props: RemoteScreenProps) {
  let canvasRef: HTMLCanvasElement | undefined;
  const [frameSize, setFrameSize] = createSignal<{ w: number; h: number } | null>(null);
  const [hostDisplays, setHostDisplays] = createSignal<DisplayInfo[]>([]);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    // Poll for frames while connected
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

        // Get latest frame as raw ArrayBuffer
        const bytes = await clientGetFrameRaw();
        if (bytes && canvasRef) {
          const ctx = canvasRef.getContext("2d");
          if (!ctx) return;

          const size = frameSize();
          if (!size) return;

          const clamped = new Uint8ClampedArray(
            bytes.buffer as unknown as ArrayBuffer,
            bytes.byteOffset,
            bytes.byteLength
          );
          const imgData = new ImageData(clamped, size.w, size.h);

          if (canvasRef.width !== size.w || canvasRef.height !== size.h) {
            canvasRef.width = size.w;
            canvasRef.height = size.h;
          }

          ctx.putImageData(imgData, 0, 0);
        }
      } catch {
        // Silently ignore
      }
    }, 1000 / 30);

    // Poll for host displays
    const displayPoll = setInterval(async () => {
      if (!props.connected) return;
      try {
        const displays = await getHostDisplays();
        if (displays.length > 0) {
          setHostDisplays(displays);
        }
      } catch {
        // Not available yet
      }
    }, 3000);

    onCleanup(() => {
      if (pollTimer) clearInterval(pollTimer);
      clearInterval(displayPoll);
    });
  });

  // Mouse handler
  const handleMouse = (e: MouseEvent) => {
    if (!props.connected) return;
    const rect = canvasRef?.getBoundingClientRect();
    if (!rect) return;

    const size = frameSize();
    if (!size) return;

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
      scancode: 0,
      modifiers:
        (e.ctrlKey ? 2 : 0) |
        (e.altKey ? 4 : 0) |
        (e.shiftKey ? 1 : 0) |
        (e.metaKey ? 8 : 0),
    }).catch(() => {});
  };

  return (
    <div class="remote-screen-container">
      {/* Display selector */}
      {hostDisplays().length > 1 && (
        <div class="display-selector">
          {hostDisplays().map((d) => (
            <button
              class="display-btn"
              onClick={() => props.onDisplaySwitch?.(d.id)}
              title={`Switch to ${d.name} (${d.width}x${d.height})`}
            >
              🖥 {d.name}
            </button>
          ))}
        </div>
      )}
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
