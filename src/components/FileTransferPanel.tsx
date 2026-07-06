import { createSignal, onMount } from "solid-js";
import {
  requestFileList,
  requestFile,
  getFileProgress,
  cancelFileTransfer,
  sendFileToHost,
  type FileEntry,
  type FileTransferProgress,
} from "../lib/tauri";

interface FileTransferPanelProps {
  connected: boolean;
}

export default function FileTransferPanel(props: FileTransferPanelProps) {
  const [currentPath, setCurrentPath] = createSignal("");
  const [entries, setEntries] = createSignal<FileEntry[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [progress, setProgress] = createSignal<FileTransferProgress | null>(null);
  const [selectedFiles, setSelectedFiles] = createSignal<File[]>([]);
  const [uploading, setUploading] = createSignal(false);
  let progressTimer: ReturnType<typeof setInterval> | null = null;

  const browsePath = async (path: string) => {
    if (!props.connected) return;
    setLoading(true);
    try {
      const result = await requestFileList(path);
      setEntries(result);
      setCurrentPath(path);
    } catch (e) {
      console.error("Browse error:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async (entry: FileEntry) => {
    if (entry.is_dir) {
      await browsePath(entry.path);
      return;
    }
    try {
      await requestFile(entry.path);
      // Poll for progress
      if (progressTimer) clearInterval(progressTimer);
      progressTimer = setInterval(async () => {
        try {
          const prog = await getFileProgress();
          setProgress(prog);
          if (prog?.done || prog?.error) {
            if (progressTimer) clearInterval(progressTimer);
          }
        } catch {
          // Not yet
        }
      }, 500);
    } catch (e) {
      console.error("Download error:", e);
    }
  };

  const handleCancel = async () => {
    try {
      await cancelFileTransfer("User cancelled");
      setProgress(null);
    } catch (e) {
      console.error("Cancel error:", e);
    }
  };

  const handleUpload = async () => {
    const files = selectedFiles();
    if (files.length === 0) return;
    setUploading(true);
    try {
      for (const file of files) {
        const buf = await file.arrayBuffer();
        const data = Array.from(new Uint8Array(buf));
        await sendFileToHost(file.name, data);
      }
    } catch (e) {
      console.error("Upload error:", e);
    } finally {
      setUploading(false);
      setSelectedFiles([]);
    }
  };

  const goUp = () => {
    if (!currentPath()) return;
    const parent = currentPath().split(/[\\/]/).slice(0, -1).join("/") || "/";
    browsePath(parent);
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
    return `${(bytes / 1073741824).toFixed(1)} GB`;
  };

  return (
    <div class="file-transfer-panel">
      <div class="ft-header">
        <span>📁 File Transfer</span>
        <button class="ft-browse-btn" onClick={() => browsePath("/")} disabled={!props.connected}>
          Browse Host
        </button>
      </div>

      {/* Directory listing */}
      {loading() && <div class="ft-loading">Loading...</div>}

      {entries().length > 0 && (
        <div class="ft-path-bar">
          <button onClick={goUp} disabled={!currentPath()}>⬆ Up</button>
          <span class="ft-path">{currentPath() || "/"}</span>
        </div>
      )}

      <div class="ft-entries">
        {entries().map((entry) => (
          <div class="ft-entry" onClick={() => handleDownload(entry)}>
            <span class="ft-entry-icon">{entry.is_dir ? "📁" : "📄"}</span>
            <span class="ft-entry-name">{entry.name}</span>
            <span class="ft-entry-size">{entry.is_dir ? "" : formatSize(entry.size)}</span>
          </div>
        ))}
        {entries().length === 0 && !loading() && (
          <div class="ft-empty">Click "Browse Host" to list files</div>
        )}
      </div>

      {/* Transfer progress */}
      {progress() && (
        <div class="ft-progress">
          <div class="ft-progress-info">
            <span>{progress()!.path.split(/[\\/]/).pop()}</span>
            <span>
              {formatSize(progress()!.received_bytes)} / {formatSize(progress()!.total_size)}
            </span>
            {progress()!.done && <span class="ft-done">✅ Done</span>}
            {progress()!.error && <span class="ft-error">❌ {progress()!.error}</span>}
          </div>
          {!progress()!.done && (
            <div class="ft-progress-bar">
              <div
                class="ft-progress-fill"
                style={{
                  width: `${(progress()!.received_bytes / Math.max(1, progress()!.total_size)) * 100}%`,
                }}
              />
            </div>
          )}
          {!progress()!.done && (
            <button class="danger" onClick={handleCancel}>Cancel</button>
          )}
        </div>
      )}

      {/* Upload section */}
      <div class="ft-upload">
        <label class="ft-upload-label">
          📤 Send files to host:
          <input
            type="file"
            multiple
            onChange={(e) => setSelectedFiles(Array.from(e.target.files || []))}
            disabled={!props.connected}
          />
        </label>
        {selectedFiles().length > 0 && (
          <div>
            <span>{selectedFiles().length} file(s) selected</span>
            <button onClick={handleUpload} disabled={uploading()}>
              {uploading() ? "Uploading..." : "📤 Upload"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
