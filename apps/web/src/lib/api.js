/**
 * Dabar Tauri Desktop IPC API Client
 *
 * Calls native Rust commands via Tauri IPC (`invoke`) and listens for real-time
 * pipeline progress events (`listen`).
 */

// Check if running inside Tauri runtime
export const isTauri = typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__ || window.__TAURI__);

async function getTauriCore() {
  if (!isTauri) return null;
  try {
    return await import("@tauri-apps/api/core");
  } catch (e) {
    console.warn("Tauri core import failed:", e);
    return null;
  }
}

async function getTauriEvent() {
  if (!isTauri) return null;
  try {
    return await import("@tauri-apps/api/event");
  } catch (e) {
    console.warn("Tauri event import failed:", e);
    return null;
  }
}

async function getTauriDialog() {
  if (!isTauri) return null;
  try {
    return await import("@tauri-apps/plugin-dialog");
  } catch (e) {
    console.warn("Tauri dialog import failed:", e);
    return null;
  }
}

/**
 * List all sermons in local SQLite database.
 */
export async function listSermons() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("list_sermons");
  }
  // Browser fallback for UI preview
  const local = localStorage.getItem("dabar_sermons");
  return local ? JSON.parse(local) : [];
}

/**
 * Get a single sermon by ID with its highlights and transcript segments.
 */
export async function getSermon(id) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("get_sermon", { id });
  }
  const local = localStorage.getItem("dabar_sermons");
  if (local) {
    const list = JSON.parse(local);
    return list.find((s) => s.id === id) || null;
  }
  return null;
}

/**
 * Delete a sermon by ID from local SQLite database.
 */
export async function deleteSermon(id) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("delete_sermon", { id });
  }
  const local = localStorage.getItem("dabar_sermons");
  if (local) {
    const list = JSON.parse(local).filter((s) => s.id !== id);
    localStorage.setItem("dabar_sermons", JSON.stringify(list));
  }
  return true;
}

/**
 * Convert a local file path into a webview asset URL for HTML5 video/audio playback.
 */
export async function getAssetUrl(filePath) {
  if (!filePath) return null;
  const core = await getTauriCore();
  if (core && typeof core.convertFileSrc === "function") {
    return core.convertFileSrc(filePath);
  }
  return filePath;
}

/**
 * Start the pipeline for a YouTube URL or local audio/video file.
 * Returns the created sermon ID immediately.
 */
export async function createSermon(source) {
  const core = await getTauriCore();
  if (core) {
    const sermonId = await core.invoke("start_pipeline", { source });
    return { id: sermonId };
  }
  // Browser mock
  const mockId = "mock-" + Date.now();
  return { id: mockId };
}

/**
 * Cancel an in-flight sermon processing pipeline.
 */
export async function cancelPipeline(sermonId) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("cancel_pipeline", { sermonId });
  }
  return true;
}

/**
 * Render a clip to disk and return the output file path.
 */
export async function renderClip(
  sermonId,
  clipId,
  startTime,
  endTime,
  clipTitle,
  aspectRatio = "9:16",
  captionStyle = "amber"
) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("render_clip", {
      sermonId,
      highlightId: typeof clipId === "string" ? clipId : null,
      clipId: typeof clipId === "string" ? clipId : null,
      startTime: typeof startTime === "number" ? Number(startTime) : null,
      endTime: typeof endTime === "number" ? Number(endTime) : null,
      clipTitle: clipTitle || null,
      aspectRatio: aspectRatio || "9:16",
      captionStyle: captionStyle || "amber",
    });
  }
  return "C:\\Users\\User\\Videos\\Dabar\\dabar_clip.mp4";
}

/**
 * Render an arbitrary time range clip to disk and return the output file path.
 */
export async function renderClipRange(
  sermonId,
  startTime,
  endTime,
  clipTitle,
  aspectRatio = "9:16",
  captionStyle = "amber"
) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("render_clip_range", {
      sermonId,
      startTime: Number(startTime),
      endTime: Number(endTime),
      clipTitle: clipTitle || null,
      aspectRatio: aspectRatio || "9:16",
      captionStyle: captionStyle || "amber",
    });
  }
  return "C:\\Users\\Mock\\Videos\\Dabar\\mock_range_clip.mp4";
}

/**
 * Re-run highlight detection on an already transcribed sermon.
 */
export async function retryHighlights(sermonId) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("retry_highlights", { sermonId });
  }
  return [];
}

/**
 * Open native OS file dialog to select audio or video file(s).
 * Uses native async Tauri command to prevent any window freezing.
 */
export async function pickMediaFile() {
  const core = await getTauriCore();
  if (core) {
    try {
      const selected = await core.invoke("pick_media_file");
      if (selected) return selected;
      if (selected === null) return null; // user cancelled
    } catch (e) {
      console.warn("Native pick_media_file command error:", e);
    }
  }

  const dialog = await getTauriDialog();
  if (dialog && typeof dialog.open === "function") {
    try {
      const res = await dialog.open({
        multiple: false,
        filters: [
          {
            name: "Audio & Video",
            extensions: ["mp4", "mov", "webm", "mkv", "mp3", "wav", "m4a", "ogg", "opus", "aac", "flac"],
          },
        ],
      });
      if (res) return typeof res === "string" ? res : res[0];
      if (res === null) return null;
    } catch (err) {
      console.warn("plugin dialog.open error:", err);
    }
  }

  // Universal HTML5 file picker fallback for browser preview
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "video/*,audio/*,.mp4,.mov,.mkv,.mp3,.wav,.m4a";
    input.style.display = "none";
    input.onchange = (e) => {
      const file = e.target.files?.[0];
      if (file) {
        resolve(file.name || "selected_sermon.mp4");
      } else {
        resolve(null);
      }
      if (input.parentNode) input.parentNode.removeChild(input);
    };
    input.oncancel = () => {
      resolve(null);
      if (input.parentNode) input.parentNode.removeChild(input);
    };
    document.body.appendChild(input);
    input.click();
  });
}

/**
 * Open a folder or file in the OS file explorer.
 */
export async function openInExplorer(path) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("open_in_explorer", { path });
  }
  console.log("Mock openInExplorer:", path);
}

/**
 * Get user settings.
 */
export async function getSettings() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("get_settings");
  }
  const local = localStorage.getItem("dabar_settings");
  return local
    ? JSON.parse(local)
    : {
        groq_api_key: "",
        output_dir: "Videos/Dabar",
        offline_mode: false,
        offline_model: "base",
        custom_vocabulary: "",
        transcription_backend: "groq",
      };
}

/**
 * Save user settings.
 */
export async function saveSettings(settings) {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("save_settings", { settings });
  }
  localStorage.setItem("dabar_settings", JSON.stringify(settings));
}

/**
 * Check external tool statuses (ffmpeg, yt-dlp, whisper models).
 */
export async function checkDependencies() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("check_dependencies");
  }
  return {
    ffmpeg: { found: true, path: "ffmpeg", version: "mock ffmpeg" },
    yt_dlp: { found: true, path: "yt-dlp", version: "mock yt-dlp" },
    whisper_model: { base_available: true, tiny_available: false, base_path: null, tiny_path: null },
  };
}

/**
 * Download yt-dlp binary on-demand.
 */
export async function downloadYtDlp() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("download_yt_dlp");
  }
}

/**
 * Get hardware info.
 */
export async function getHardwareInfo() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("get_hardware_info");
  }
  return { ram_gb: 16, is_low_end: false, recommended_ffmpeg_preset: "veryfast" };
}

/**
 * Subscribe to real-time pipeline progress events.
 * Returns an unlisten function.
 */
export async function onPipelineProgress(callback) {
  const tauriEvent = await getTauriEvent();
  if (tauriEvent) {
    return await tauriEvent.listen("pipeline-progress", (event) => {
      callback(event.payload);
    });
  }
  return () => {};
}

/**
 * Subscribe to download progress events (FFmpeg, Whisper model downloads).
 * Payload: { component: string, downloaded: number, total: number }
 * Returns an unlisten function.
 */
export async function onDownloadProgress(callback) {
  const tauriEvent = await getTauriEvent();
  if (tauriEvent) {
    return await tauriEvent.listen("download-progress", (event) => {
      callback(event.payload);
    });
  }
  return () => {};
}

/**
 * Download FFmpeg binary to the app data directory.
 * Progress reported via onDownloadProgress events.
 */
export async function downloadFfmpeg() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("download_ffmpeg");
  }
  throw new Error("Tauri runtime not available");
}

/**
 * Get offline tool readiness status — which components are already downloaded.
 * Returns { ffmpeg_ready, yt_dlp_ready }
 */
export async function getOfflineStatus() {
  const core = await getTauriCore();
  if (core) {
    return await core.invoke("get_offline_status");
  }
  // Browser fallback
  return {
    ffmpeg_ready: false,
    yt_dlp_ready: false,
  };
}
