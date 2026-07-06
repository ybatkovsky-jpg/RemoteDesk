import { createSignal, onMount, Show } from "solid-js";
import {
  getVersion,
  listDisplays,
  startHost,
  stopHost,
  clientConnect,
  clientDisconnect,
  onHostStatus,
  onConnectionState,
  setHostPassword,
  setClientPassword,
  switchDisplay,
  toggleAudio,
  loadConfig,
  type DisplayInfo,
} from "./lib/tauri";
import RemoteScreen from "./components/RemoteScreen";
import SettingsPanel from "./components/SettingsPanel";
import ChatPanel from "./components/ChatPanel";
import FileTransferPanel from "./components/FileTransferPanel";
import AuthDialog from "./components/AuthDialog";
import "./App.css";

type Mode = "idle" | "host" | "client";

function App() {
  const [version, setVersion] = createSignal("");
  const [mode, setMode] = createSignal<Mode>("idle");
  const [displays, setDisplays] = createSignal<DisplayInfo[]>([]);
  const [selectedDisplay, setSelectedDisplay] = createSignal(0);
  const [hostPort, setHostPort] = createSignal(9000);
  const [hostFps, setHostFps] = createSignal(15);
  const [hostStatus, setHostStatus] = createSignal("stopped");
  const [clientAddr, setClientAddr] = createSignal("127.0.0.1:9000");
  const [connectionState, setConnectionState] = createSignal("disconnected");
  const [logs, setLogs] = createSignal<string[]>([]);
  const [showSettings, setShowSettings] = createSignal(false);
  const [showAuth, setShowAuth] = createSignal<"host" | "client" | null>(null);
  const [showSidepanels, setShowSidepanels] = createSignal(true);
  const [audioEnabled, setAudioEnabled] = createSignal(false);

  const addLog = (msg: string) => {
    setLogs((prev) => [...prev.slice(-100), `[${new Date().toLocaleTimeString()}] ${msg}`]);
  };

  onMount(async () => {
    try {
      const v = await getVersion();
      setVersion(v);
      addLog(`RemoteDesk ${v} started`);

      // Load saved config
      const cfg = await loadConfig();
      if (cfg.security.password) {
        addLog("Config loaded with password protection");
      }
    } catch (e) {
      addLog(`Error: ${e}`);
    }

    onHostStatus((data) => {
      setHostStatus(data.status);
      addLog(`Host: ${data.status}${data.message ? ` (${data.message})` : ""}`);
    });

    onConnectionState((data) => {
      setConnectionState(data.state);
      addLog(`Connection: ${data.state}${data.message ? ` (${data.message})` : ""}`);
    });
  });

  const refreshDisplays = async () => {
    try {
      const d = await listDisplays();
      setDisplays(d);
      addLog(`${d.length} display(s) found`);
    } catch (e) {
      addLog(`Error: ${e}`);
    }
  };

  const handleStartHost = async () => {
    addLog("Opening host auth...");
    setShowAuth("host");
  };

  const doStartHost = async (password: string) => {
    setShowAuth(null);
    try {
      if (password) {
        await setHostPassword(password);
        addLog("Host password set");
      }
      setMode("host");
      await startHost(selectedDisplay(), hostPort(), hostFps());
      addLog(`Host started on port ${hostPort()}`);
    } catch (e) {
      addLog(`Host error: ${e}`);
      setMode("idle");
    }
  };

  const handleStopHost = async () => {
    try {
      await stopHost();
      setMode("idle");
      setHostStatus("stopped");
      addLog("Host stopped");
    } catch (e) {
      addLog(`Stop error: ${e}`);
    }
  };

  const handleConnect = async () => {
    addLog("Opening client auth...");
    setShowAuth("client");
  };

  const doConnect = async (password: string) => {
    setShowAuth(null);
    try {
      if (password) {
        await setClientPassword(password);
      }
      setMode("client");
      await clientConnect(clientAddr());
      addLog(`Connecting to ${clientAddr()}...`);
    } catch (e) {
      addLog(`Connect error: ${e}`);
      setMode("idle");
    }
  };

  const handleDisconnect = async () => {
    try {
      await clientDisconnect();
      setMode("idle");
      setConnectionState("disconnected");
      addLog("Disconnected");
    } catch (e) {
      addLog(`Disconnect error: ${e}`);
    }
  };

  const handleSwitchDisplay = async (displayId: number) => {
    try {
      await switchDisplay(displayId);
      addLog(`Switched to display ${displayId}`);
    } catch (e) {
      addLog(`Switch display error: ${e}`);
    }
  };

  const handleToggleAudio = async () => {
    try {
      const newState = !audioEnabled();
      await toggleAudio(newState);
      setAudioEnabled(newState);
      addLog(`Audio ${newState ? "enabled" : "disabled"}`);
    } catch (e) {
      addLog(`Audio toggle error: ${e}`);
    }
  };

  const isConnected = () => mode() === "client" && connectionState() === "connected";

  return (
    <div class="app-container">
      {/* Auth dialog */}
      <Show when={showAuth()}>
        <AuthDialog
          mode={showAuth()!}
          onConfirm={(pwd) => {
            if (showAuth() === "host") doStartHost(pwd);
            else doConnect(pwd);
          }}
          onCancel={() => {
            setShowAuth(null);
            if (showAuth() === "client") setMode("idle");
          }}
        />
      </Show>

      {/* Settings modal */}
      <Show when={showSettings()}>
        <SettingsPanel onClose={() => setShowSettings(false)} />
      </Show>

      <header class="app-header">
        <h1>RemoteDesk</h1>
        <div class="header-right">
          <span class="version">{version()}</span>
          <button class="icon-btn" onClick={() => setShowSettings(true)} title="Settings">
            ⚙️
          </button>
        </div>
      </header>

      <main class="main-content">
        {/* Mode selector */}
        {mode() === "idle" && (
          <div class="mode-selector">
            {/* Host Panel */}
            <section class="card">
              <h2>🎥 Host Mode</h2>
              <p class="hint">Share your screen with remote clients</p>

              <label>Display:</label>
              <select
                value={selectedDisplay()}
                onChange={(e) => setSelectedDisplay(Number(e.target.value))}
              >
                {displays().length === 0 && <option value={0}>No displays found</option>}
                {displays().map((d) => (
                  <option value={d.id}>
                    {d.name} ({d.width}x{d.height})
                  </option>
                ))}
              </select>

              <label>Port:</label>
              <input
                type="number"
                value={hostPort()}
                onChange={(e) => setHostPort(Number(e.target.value))}
                min={1024}
                max={65535}
              />

              <label>Max FPS:</label>
              <input
                type="number"
                value={hostFps()}
                onChange={(e) => setHostFps(Number(e.target.value))}
                min={1}
                max={60}
              />

              <button onClick={refreshDisplays}>🔄 Refresh Displays</button>
              <button onClick={handleStartHost} class="primary">
                ▶ Start Host
              </button>
            </section>

            {/* Client Panel */}
            <section class="card">
              <h2>🖥️ Client Mode</h2>
              <p class="hint">Connect to a remote host</p>

              <label>Host Address (IP:port):</label>
              <input
                type="text"
                value={clientAddr()}
                onChange={(e) => setClientAddr(e.target.value)}
                placeholder="127.0.0.1:9000"
              />

              <button onClick={handleConnect} class="primary">
                🔗 Connect
              </button>
            </section>
          </div>
        )}

        {/* Host running */}
        {mode() === "host" && (
          <section class="card full-width">
            <h2>🎥 Host Running</h2>
            <p>
              Status: <strong>{hostStatus()}</strong> | Port: {hostPort()} | Display: {selectedDisplay()}
            </p>
            <p class="hint">
              Waiting for client connections... Share your IP:{hostPort()} with the client.
            </p>
            <button onClick={handleStopHost} class="danger">
              ⏹ Stop Host
            </button>
          </section>
        )}

        {/* Client connected */}
        {mode() === "client" && (
          <div class="client-workspace">
            <div class="client-main">
              <div class="client-toolbar">
                <span>
                  Connected to: <strong>{clientAddr()}</strong> — {connectionState()}
                </span>
                <div class="toolbar-actions">
                  <button
                    class={`icon-btn ${audioEnabled() ? "active" : ""}`}
                    onClick={handleToggleAudio}
                    title="Toggle Audio"
                  >
                    {audioEnabled() ? "🔊" : "🔇"}
                  </button>
                  <button
                    class="icon-btn"
                    onClick={() => setShowSidepanels(!showSidepanels())}
                    title="Toggle panels"
                  >
                    {showSidepanels() ? "📋" : "📋"}
                  </button>
                  <button onClick={handleDisconnect} class="danger">
                    Disconnect
                  </button>
                </div>
              </div>
              <RemoteScreen
                connected={isConnected()}
                onDisplaySwitch={handleSwitchDisplay}
              />
            </div>

            {/* Side panels */}
            {showSidepanels() && (
              <div class="client-sidepanels">
                <ChatPanel connected={isConnected()} />
                <FileTransferPanel connected={isConnected()} />
              </div>
            )}
          </div>
        )}

        {/* Log panel */}
        <section class="card log-panel">
          <h2>📋 Log</h2>
          <div class="log-entries">
            {logs().map((entry) => (
              <div class="log-entry">{entry}</div>
            ))}
          </div>
        </section>
      </main>
    </div>
  );
}

export default App;
