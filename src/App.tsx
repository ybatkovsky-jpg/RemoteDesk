import { createSignal, onMount } from "solid-js";
import {
  getVersion,
  listDisplays,
  startHost,
  stopHost,
  clientConnect,
  clientDisconnect,
  onHostStatus,
  onConnectionState,
  type DisplayInfo,
} from "./lib/tauri";
import RemoteScreen from "./components/RemoteScreen";
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

  const addLog = (msg: string) => {
    setLogs((prev) => [...prev.slice(-50), `[${new Date().toLocaleTimeString()}] ${msg}`]);
  };

  onMount(async () => {
    try {
      const v = await getVersion();
      setVersion(v);
      addLog(`RemoteDesk ${v} started`);
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
    try {
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
    try {
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

  const isConnected = () => mode() === "client" && connectionState() === "connected";

  return (
    <div class="app-container">
      <header class="app-header">
        <h1>RemoteDesk</h1>
        <span class="version">{version()}</span>
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
              Status: <strong>{hostStatus()}</strong> | Port: {hostPort()}
            </p>
            <p class="hint">
              Waiting for client connections... Share your IP:{hostPort()} with the
              client.
            </p>
            <button onClick={handleStopHost} class="danger">
              ⏹ Stop Host
            </button>
          </section>
        )}

        {/* Client connected */}
        {mode() === "client" && (
          <div class="client-view">
            <div class="client-toolbar">
              <span>
                Connected to: <strong>{clientAddr()}</strong> — {connectionState()}
              </span>
              <button onClick={handleDisconnect} class="danger">
                Disconnect
              </button>
            </div>
            <RemoteScreen connected={isConnected()} />
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
