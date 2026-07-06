import { createSignal, onMount } from "solid-js";
import { loadConfig, saveConfig, setHostPassword, setClientPassword, type Config } from "../lib/tauri";

interface SettingsPanelProps {
  onClose: () => void;
}

export default function SettingsPanel(props: SettingsPanelProps) {
  const [config, setConfig] = createSignal<Config | null>(null);
  const [hostPwd, setHostPwd] = createSignal("");
  const [clientPwd, setClientPwd] = createSignal("");
  const [saved, setSaved] = createSignal(false);

  onMount(async () => {
    try {
      const cfg = await loadConfig();
      setConfig(cfg);
      setHostPwd(cfg.security.password || "");
    } catch (e) {
      console.error("Failed to load config:", e);
    }
  });

  const handleSave = async () => {
    try {
      const cfg = config();
      if (!cfg) return;

      // Update video settings from form
      const fps = (document.getElementById("cfg-fps") as HTMLInputElement)?.value;
      const quality = (document.getElementById("cfg-quality") as HTMLInputElement)?.value;
      const bitrate = (document.getElementById("cfg-bitrate") as HTMLInputElement)?.value;
      const codec = (document.getElementById("cfg-codec") as HTMLSelectElement)?.value;
      const rendezvous = (document.getElementById("cfg-rendezvous") as HTMLInputElement)?.value;
      const relay = (document.getElementById("cfg-relay") as HTMLInputElement)?.value;

      cfg.video.max_fps = Number(fps) || 30;
      cfg.video.quality = Number(quality) || 75;
      cfg.video.bitrate = Number(bitrate) || 5000;
      cfg.video.codec = codec || "h264";
      cfg.server.rendezvous_server = rendezvous || "";
      cfg.server.relay_server = relay || "";

      await saveConfig(cfg);
      await setHostPassword(hostPwd());
      await setClientPassword(clientPwd());
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save config:", e);
    }
  };

  const cfg = config();

  return (
    <div class="settings-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="settings-modal">
        <div class="settings-header">
          <h2>⚙️ Settings</h2>
          <button class="close-btn" onClick={props.onClose}>✕</button>
        </div>

        <div class="settings-body">
          {/* Video */}
          <section class="settings-section">
            <h3>📺 Video</h3>
            <label>Max FPS</label>
            <input id="cfg-fps" type="number" value={cfg?.video.max_fps ?? 30} min={1} max={60} />
            <label>Quality (0-100)</label>
            <input id="cfg-quality" type="number" value={cfg?.video.quality ?? 75} min={0} max={100} />
            <label>Bitrate (kbps)</label>
            <input id="cfg-bitrate" type="number" value={cfg?.video.bitrate ?? 5000} min={100} max={50000} />
            <label>Codec</label>
            <select id="cfg-codec">
              <option value="h264" selected={cfg?.video.codec === "h264"}>H.264</option>
              <option value="h265" selected={cfg?.video.codec === "h265"}>H.265 / HEVC</option>
              <option value="vp9" selected={cfg?.video.codec === "vp9"}>VP9</option>
              <option value="zstd" selected={cfg?.video.codec === "zstd"}>Zstd (software)</option>
            </select>
          </section>

          {/* Security */}
          <section class="settings-section">
            <h3>🔒 Security</h3>
            <label>Host Password (leave empty for no auth)</label>
            <input
              type="password"
              value={hostPwd()}
              onChange={(e) => setHostPwd(e.target.value)}
              placeholder="Set a password for host mode"
            />
            <label>Client Default Password</label>
            <input
              type="password"
              value={clientPwd()}
              onChange={(e) => setClientPwd(e.target.value)}
              placeholder="Password to use when connecting"
            />
          </section>

          {/* Server */}
          <section class="settings-section">
            <h3>🌐 Server</h3>
            <label>Rendezvous Server</label>
            <input
              id="cfg-rendezvous"
              type="text"
              value={cfg?.server.rendezvous_server ?? ""}
              placeholder="host:port"
            />
            <label>Relay Server</label>
            <input
              id="cfg-relay"
              type="text"
              value={cfg?.server.relay_server ?? ""}
              placeholder="host:port"
            />
          </section>
        </div>

        <div class="settings-footer">
          {saved() && <span class="saved-badge">✅ Saved!</span>}
          <button class="primary" onClick={handleSave}>💾 Save Settings</button>
        </div>
      </div>
    </div>
  );
}
