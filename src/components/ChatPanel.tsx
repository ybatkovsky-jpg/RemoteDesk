import { createSignal, onCleanup, onMount } from "solid-js";
import { sendChatMessage, getChatHistory, type ChatEntry } from "../lib/tauri";

interface ChatPanelProps {
  connected: boolean;
}

export default function ChatPanel(props: ChatPanelProps) {
  const [messages, setMessages] = createSignal<ChatEntry[]>([]);
  const [input, setInput] = createSignal("");
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let scrollRef: HTMLDivElement | undefined;

  onMount(() => {
    pollTimer = setInterval(async () => {
      if (!props.connected) return;
      try {
        const history = await getChatHistory();
        if (history.length > 0) {
          setMessages(history);
          // Auto-scroll to bottom
          if (scrollRef) {
            scrollRef.scrollTop = scrollRef.scrollHeight;
          }
        }
      } catch {
        // Not connected yet
      }
    }, 1000);
  });

  onCleanup(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  const handleSend = async () => {
    const text = input().trim();
    if (!text) return;
    try {
      await sendChatMessage(text, "Me");
      setMessages((prev) => [
        ...prev,
        { text, sender: "Me", timestamp: Date.now() / 1000 },
      ]);
      setInput("");
    } catch (e) {
      console.error("Failed to send chat:", e);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const formatTime = (ts: number) => {
    return new Date(ts * 1000).toLocaleTimeString();
  };

  return (
    <div class="chat-panel">
      <div class="chat-header">
        <span>💬 Chat</span>
      </div>
      <div class="chat-messages" ref={scrollRef}>
        {messages().length === 0 && (
          <div class="chat-empty">No messages yet</div>
        )}
        {messages().map((msg) => (
          <div
            class={`chat-message ${msg.sender === "Me" ? "chat-message-mine" : ""}`}
          >
            <div class="chat-meta">
              <span class="chat-sender">{msg.sender}</span>
              <span class="chat-time">{formatTime(msg.timestamp)}</span>
            </div>
            <div class="chat-text">{msg.text}</div>
          </div>
        ))}
      </div>
      <div class="chat-input-row">
        <input
          type="text"
          class="chat-input"
          value={input()}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message..."
          disabled={!props.connected}
        />
        <button
          class="chat-send-btn"
          onClick={handleSend}
          disabled={!props.connected || !input().trim()}
        >
          ▶
        </button>
      </div>
    </div>
  );
}
