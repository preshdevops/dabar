import { useEffect, useState } from "react";
import {
  getSettings,
  saveSettings,
  checkDependencies,
  downloadYtDlp,
  downloadFfmpeg,
  downloadWhisperModel,
  getOfflineStatus,
  onDownloadProgress,
} from "../lib/api.js";
import { useTheme } from "../context/ThemeContext.jsx";
import Btn from "../components/Btn.jsx";

export default function Settings() {
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState("general");
  const [settings, setSettings] = useState({
    output_dir: "",
    offline_mode: false,
    offline_model: "base", // "tiny" | "base"
    custom_vocabulary: "",
    transcription_backend: "groq",
  });
  const [deps, setDeps] = useState(null);
  const [offlineStatus, setOfflineStatus] = useState(null);
  const [isSaving, setIsSaving] = useState(false);
  const [downloadingComponent, setDownloadingComponent] = useState(null);
  const [downloadProgress, setDownloadProgress] = useState({});
  const [savedNotice, setSavedNotice] = useState(false);

  useEffect(() => {
    getSettings().then((s) => {
      if (s) setSettings(s);
    });
    checkDependencies().then((d) => {
      if (d) setDeps(d);
    });
    getOfflineStatus().then((s) => {
      if (s) setOfflineStatus(s);
    });

    let unlisten = null;
    onDownloadProgress((payload) => {
      if (payload && payload.component) {
        const pct =
          payload.total > 0
            ? Math.round((payload.downloaded / payload.total) * 100)
            : 0;
        setDownloadProgress((prev) => ({
          ...prev,
          [payload.component]: pct,
        }));
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (typeof unlisten === "function") unlisten();
    };
  }, []);

  async function handleSave(e) {
    if (e) e.preventDefault();
    setIsSaving(true);
    try {
      await saveSettings(settings);
      setSavedNotice(true);
      setTimeout(() => setSavedNotice(false), 2500);
    } catch (err) {
      alert("Could not save settings: " + (err.message || err));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDownload(comp) {
    setDownloadingComponent(comp);
    try {
      if (comp === "yt_dlp") {
        await downloadYtDlp();
      } else if (comp === "ffmpeg") {
        await downloadFfmpeg();
      } else if (comp === "whisper_base") {
        await downloadWhisperModel("base");
      } else if (comp === "whisper_tiny") {
        await downloadWhisperModel("tiny");
      } else if (comp === "all") {
        await downloadYtDlp();
        await downloadFfmpeg();
        await downloadWhisperModel(settings.offline_model || "base");
      }
      const updatedDeps = await checkDependencies();
      setDeps(updatedDeps);
      const updatedStatus = await getOfflineStatus();
      setOfflineStatus(updatedStatus);
    } catch (err) {
      alert(`Download failed: ` + (err.message || err));
    } finally {
      setDownloadingComponent(null);
    }
  }

  return (
    <div className="flex flex-col min-h-screen pb-16 space-y-6">
      {/* ── Page Header ───────────────────────────────────────────── */}
      <header className="pt-2">
        <div className="space-y-1">
          <h1 className="font-editorial text-2xl sm:text-3xl font-bold text-primary">Settings</h1>
          <p className="text-secondary text-xs sm:text-sm font-normal">
            Configure transcription mode, church vocabulary terms, and output folders.
          </p>
        </div>
      </header>

      {/* ── Tabs Toolbar ──────────────────────────────────────────── */}
      <div className="flex p-1 bg-surface border border-border rounded-lg text-xs font-semibold max-w-xl">
        {[
          { key: "general", label: "General", icon: "bx-slider" },
          { key: "mode", label: "Transcription Mode", icon: "bx-chip" },
          { key: "vocabulary", label: "Church Vocabulary", icon: "bx-book" },
        ].map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`flex-1 py-1.5 px-3 rounded-md flex items-center justify-center gap-1.5 transition-all ${
              activeTab === tab.key
                ? "bg-accent text-accent-fg shadow-xs font-semibold"
                : "text-secondary hover:text-primary"
            }`}
          >
            <i className={`bx ${tab.icon} text-sm`} />
            <span>{tab.label}</span>
          </button>
        ))}
      </div>

      {/* ── Tab Content ───────────────────────────────────────────── */}
      <div className="flex justify-start py-2">
        <div className="w-full max-w-xl space-y-5">
          {/* Tab 1: General */}
          {activeTab === "general" && (
            <form onSubmit={handleSave} className="space-y-5 text-xs">
              {/* Theme Mode */}
              <div className="space-y-2">
                <label className="font-semibold text-primary block text-xs">
                  Appearance
                </label>
                <div className="grid grid-cols-2 gap-3">
                  <button
                    type="button"
                    onClick={() => setTheme("dark")}
                    className={`p-3 rounded-lg border text-left flex items-center justify-between transition-all ${
                      theme === "dark"
                        ? "border-accent bg-accent-muted shadow-xs"
                        : "border-border bg-surface hover:bg-surface-hover text-secondary"
                    }`}
                  >
                    <div className="flex items-center gap-2.5">
                      <i className="bx bx-moon text-base text-accent" />
                      <div>
                        <p className="font-semibold text-primary">Obsidian Navy</p>
                        <p className="text-[11px] text-muted">Deep midnight and cobalt</p>
                      </div>
                    </div>
                    {theme === "dark" && (
                      <i className="bx bxs-check-circle text-accent text-base" />
                    )}
                  </button>

                  <button
                    type="button"
                    onClick={() => setTheme("light")}
                    className={`p-3 rounded-lg border text-left flex items-center justify-between transition-all ${
                      theme === "light"
                        ? "border-accent bg-accent-muted shadow-xs"
                        : "border-border bg-surface hover:bg-surface-hover text-secondary"
                    }`}
                  >
                    <div className="flex items-center gap-2.5">
                      <i className="bx bx-sun text-base text-accent" />
                      <div>
                        <p className="font-semibold text-primary">Porcelain White</p>
                        <p className="text-[11px] text-muted">Pristine ice and azure</p>
                      </div>
                    </div>
                    {theme === "light" && (
                      <i className="bx bxs-check-circle text-accent text-base" />
                    )}
                  </button>
                </div>
              </div>

              {/* Studio Onboarding Tour */}
              <div className="p-3.5 rounded-lg bg-surface border border-border flex items-center justify-between">
                <div>
                  <p className="font-semibold text-primary">Interactive Studio Tour</p>
                  <p className="text-[11px] text-muted">Revisit the first-time setup and interactive reel sandbox.</p>
                </div>
                <Btn
                  variant="secondary"
                  size="sm"
                  icon="bx-sparkles"
                  onClick={() => {
                    localStorage.removeItem("dabaar_onboarded");
                    window.location.href = "/onboarding";
                  }}
                >
                  Start Tour
                </Btn>
              </div>

              {/* Output Directory */}
              <div className="space-y-1.5">
                <label className="font-semibold text-primary block">
                  Saved Videos Folder
                </label>
                <input
                  type="text"
                  value={settings.output_dir}
                  onChange={(e) =>
                    setSettings({ ...settings, output_dir: e.target.value })
                  }
                  placeholder="Videos/Dabar"
                  className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary outline-none focus:border-accent"
                />
                <p className="text-[11px] text-muted">
                  The folder on your computer where exported video clips are saved.
                </p>
              </div>

              {savedNotice && (
                <div className="p-2.5 rounded-md border border-success/30 bg-success-muted text-success flex items-center gap-2">
                  <i className="bx bxs-check-circle text-base" />
                  <span>Settings saved successfully.</span>
                </div>
              )}

              <Btn type="submit" disabled={isSaving}>
                <i
                  className={`bx ${
                    isSaving ? "bx-loader-alt bx-spin" : "bx-check"
                  } text-sm`}
                />
                <span>{isSaving ? "Saving…" : "Save Settings"}</span>
              </Btn>
            </form>
          )}

          {/* Tab 2: Transcription Mode & Tools */}
          {activeTab === "mode" && (
            <div className="space-y-5 text-xs">
              {/* Primary Engine Choice: Online vs Offline */}
              <div className="space-y-2">
                <label className="font-semibold text-primary block">
                  How should sermons be transcribed?
                </label>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  {/* Online Mode */}
                  <div
                    onClick={() =>
                      setSettings({ ...settings, offline_mode: false })
                    }
                    className={`cursor-pointer border rounded-lg p-3.5 transition-all ${
                      !settings.offline_mode
                        ? "border-accent bg-accent-muted/20 shadow-xs"
                        : "border-border bg-surface hover:bg-surface-hover"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold text-primary flex items-center gap-1.5">
                        <i className="bx bx-cloud text-accent text-base" />
                        <span>Fast Cloud (Recommended)</span>
                      </span>
                      {!settings.offline_mode && (
                        <i className="bx bxs-check-circle text-accent text-base" />
                      )}
                    </div>
                    <p className="text-[11px] text-muted mt-1 leading-relaxed">
                      Transcribes a full 45-minute sermon in ~20 seconds with highest accuracy. Ready out of the box with zero setup.
                    </p>
                  </div>

                  {/* Offline Mode */}
                  <div
                    onClick={() =>
                      setSettings({ ...settings, offline_mode: true })
                    }
                    className={`cursor-pointer border rounded-lg p-3.5 transition-all ${
                      settings.offline_mode
                        ? "border-accent bg-accent-muted/20 shadow-xs"
                        : "border-border bg-surface hover:bg-surface-hover"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold text-primary flex items-center gap-1.5">
                        <i className="bx bx-laptop text-accent text-base" />
                        <span>Private On-Device</span>
                      </span>
                      {settings.offline_mode && (
                        <i className="bx bxs-check-circle text-accent text-base" />
                      )}
                    </div>
                    <p className="text-[11px] text-muted mt-1 leading-relaxed">
                      Runs completely on your computer with zero internet required. All sermon files stay private on your machine.
                    </p>
                  </div>
                </div>
              </div>

              {/* Offline Model Selection (Standard vs Enhanced) */}
              {settings.offline_mode && (
                <div className="studio-card p-4 space-y-3 border-accent/40 animate-in fade-in duration-200">
                  <div>
                    <label className="font-semibold text-primary block text-xs">
                      On-Device Accuracy Level
                    </label>
                    <p className="text-[11px] text-muted">
                      Choose standard speed for laptops or enhanced accuracy for complex church acoustics.
                    </p>
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                    {/* Standard / Tiny */}
                    <div
                      onClick={() =>
                        setSettings({ ...settings, offline_model: "tiny" })
                      }
                      className={`cursor-pointer border rounded-md p-3 transition-all ${
                        settings.offline_model === "tiny"
                          ? "border-accent bg-accent-muted text-accent font-semibold"
                          : "border-border bg-surface hover:bg-surface-hover text-secondary"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-medium text-xs text-primary">
                          Standard (Fastest)
                        </span>
                        {settings.offline_model === "tiny" && (
                          <i className="bx bxs-check-circle text-accent text-sm" />
                        )}
                      </div>
                      <p className="text-[11px] text-muted mt-1">
                        ~75MB download. Recommended for regular laptops and quick turnarounds.
                      </p>
                    </div>

                    {/* Enhanced / Base */}
                    <div
                      onClick={() =>
                        setSettings({ ...settings, offline_model: "base" })
                      }
                      className={`cursor-pointer border rounded-md p-3 transition-all ${
                        settings.offline_model === "base"
                          ? "border-accent bg-accent-muted text-accent font-semibold"
                          : "border-border bg-surface hover:bg-surface-hover text-secondary"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-medium text-xs text-primary">
                          Enhanced (Higher Accuracy)
                        </span>
                        {settings.offline_model === "base" && (
                          <i className="bx bxs-check-circle text-accent text-sm" />
                        )}
                      </div>
                      <p className="text-[11px] text-muted mt-1">
                        ~140MB download. Better recognition of quiet voices and background organ pads.
                      </p>
                    </div>
                  </div>

                  {/* Link Downloader */}
                  <div className="p-3 flex items-center justify-between">
                    <div>
                      <p className="font-medium text-primary">Web Link Downloader</p>
                      <p className="text-[10px] text-muted">
                        Retrieves audio directly from YouTube and Google Drive links
                      </p>
                      {downloadProgress.yt_dlp !== undefined &&
                        downloadingComponent === "yt_dlp" && (
                          <div className="w-36 mt-1.5">
                            <div className="download-bar-track">
                              <div
                                className="download-bar-fill"
                                style={{ width: `${downloadProgress.yt_dlp}%` }}
                              />
                            </div>
                            <span className="text-[9px] font-mono text-muted">
                              {downloadProgress.yt_dlp}%
                            </span>
                          </div>
                        )}
                    </div>
                    <div className="flex items-center gap-2">
                      {deps?.yt_dlp?.found ? (
                        <span className="status-pill ready">
                          <i className="bx bxs-check-circle text-xs" />
                          <span>Installed</span>
                        </span>
                      ) : (
                        <Btn
                          size="sm"
                          variant="secondary"
                          onClick={() => handleDownload("yt_dlp")}
                          disabled={Boolean(downloadingComponent)}
                        >
                          <i className="bx bx-download text-xs" />
                          <span>Install</span>
                        </Btn>
                      )}
                    </div>
                  </div>

                  {/* Speech Model */}
                  <div className="p-3 flex items-center justify-between">
                    <div>
                      <p className="font-medium text-primary">
                        Offline Speech Recognition Engine (~140MB)
                      </p>
                      <p className="text-[10px] text-muted">
                        Local language package for offline speech transcription
                      </p>
                      {downloadProgress.whisper_base !== undefined &&
                        downloadingComponent === "whisper_base" && (
                          <div className="w-36 mt-1.5">
                            <div className="download-bar-track">
                              <div
                                className="download-bar-fill"
                                style={{
                                  width: `${downloadProgress.whisper_base}%`,
                                }}
                              />
                            </div>
                            <span className="text-[9px] font-mono text-muted">
                              {downloadProgress.whisper_base}%
                            </span>
                          </div>
                        )}
                    </div>
                    <div className="flex items-center gap-2">
                      {offlineStatus?.whisper_base_ready ||
                      deps?.whisper_model?.base_available ? (
                        <span className="status-pill ready">
                          <i className="bx bxs-check-circle text-xs" />
                          <span>Installed</span>
                        </span>
                      ) : (
                        <Btn
                          size="sm"
                          variant="secondary"
                          onClick={() => handleDownload("whisper_base")}
                          disabled={Boolean(downloadingComponent)}
                        >
                          <i className="bx bx-download text-xs" />
                          <span>Install</span>
                        </Btn>
                      )}
                    </div>
                  </div>
                </div>
              </div>

              {/* Ollama Local LLM Section */}
              <div className="space-y-3 pt-2">
                <div>
                  <h3 className="font-semibold text-primary">
                    Local AI Highlight Detection (Ollama)
                  </h3>
                  <p className="text-[11px] text-muted mt-0.5">
                    Use a locally-running Ollama model for fully offline pastoral highlight and chapter detection. Requires{" "}
                  <a href="https://ollama.com" target="_blank" rel="noreferrer" className="text-accent underline">ollama.com</a>{" "}
                    installed and running.
                  </p>
                </div>

                <div className="border border-border rounded-md bg-surface p-3 space-y-3">
                  <div className="space-y-1.5">
                    <label className="font-semibold text-primary block">
                      Ollama Server URL
                    </label>
                    <input
                      type="text"
                      value={settings.ollama_url ?? "http://localhost:11434"}
                      onChange={(e) =>
                        setSettings({ ...settings, ollama_url: e.target.value })
                      }
                      placeholder="http://localhost:11434"
                      className="field-input font-mono"
                    />
                  </div>

                  <div className="space-y-1.5">
                    <label className="font-semibold text-primary block">
                      Ollama Model
                    </label>
                    <select
                      value={settings.ollama_model ?? "llama3.2:3b"}
                      onChange={(e) =>
                        setSettings({ ...settings, ollama_model: e.target.value })
                      }
                      className="field-input"
                    >
                      <option value="llama3.2:3b">llama3.2:3b — Fast, low-end friendly (~2GB)</option>
                      <option value="llama3.1:8b">llama3.1:8b — Better accuracy (~5GB)</option>
                      <option value="llama3.3:70b">llama3.3:70b — Highest quality (~40GB)</option>
                      <option value="mistral:7b">mistral:7b — Alternate option (~4GB)</option>
                    </select>
                    <p className="text-[10px] text-muted">
                      Run <code className="bg-surface-hover px-1 rounded font-mono">ollama pull llama3.2:3b</code> to download the model.
                    </p>
                  </div>
                </div>
              </div>

              {savedNotice && (
                <div className="p-2.5 rounded border border-success/30 bg-success-muted text-success flex items-center gap-2">
                  <i className="bx bxs-check-circle text-base" />
                  <span>Settings saved.</span>
                </div>
              )}

              {/* On-Device Components Readiness Card */}
              <div className="studio-card p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="font-semibold text-xs text-primary">
                      On-Device Tools
                    </h3>
                    <p className="text-[11px] text-muted">
                      Status of audio and video processors on your machine.
                    </p>
                  </div>
                  {offlineStatus &&
                    offlineStatus.ffmpeg_ready &&
                    offlineStatus.yt_dlp_ready && (
                      <span className="text-[11px] font-medium text-success flex items-center gap-1">
                        <i className="bx bx-check-circle text-sm" /> Ready
                      </span>
                    )}
                </div>

                <div className="space-y-2 pt-1">
                  {/* YouTube Downloader */}
                  <div className="flex items-center justify-between p-2.5 rounded-md bg-surface border border-border">
                    <div className="flex items-center gap-2">
                      <i className="bx bxl-youtube text-base text-accent" />
                      <div>
                        <p className="font-medium text-primary">YouTube Stream Extractor</p>
                        <p className="text-[10px] text-muted">Downloads audio streams for instant processing</p>
                      </div>
                    </div>
                    {offlineStatus?.yt_dlp_ready ? (
                      <span className="text-success text-[11px] font-medium flex items-center gap-1">
                        <i className="bx bx-check text-sm" /> Ready
                      </span>
                    ) : (
                      <Btn
                        variant="secondary"
                        onClick={() => handleDownload("yt_dlp")}
                        disabled={downloadingComponent === "yt_dlp"}
                      >
                        {downloadingComponent === "yt_dlp" ? "Installing…" : "Install (15MB)"}
                      </Btn>
                    )}
                  </div>

                  {/* Video & Audio Engine */}
                  <div className="flex items-center justify-between p-2.5 rounded-md bg-surface border border-border">
                    <div className="flex items-center gap-2">
                      <i className="bx bx-video-recording text-base text-accent" />
                      <div>
                        <p className="font-medium text-primary">Clip Rendering Engine</p>
                        <p className="text-[10px] text-muted">Renders vertical clips and video waveforms</p>
                      </div>
                    </div>
                    {offlineStatus?.ffmpeg_ready ? (
                      <span className="text-success text-[11px] font-medium flex items-center gap-1">
                        <i className="bx bx-check text-sm" /> Ready
                      </span>
                    ) : (
                      <Btn
                        variant="secondary"
                        onClick={() => handleDownload("ffmpeg")}
                        disabled={downloadingComponent === "ffmpeg"}
                      >
                        {downloadingComponent === "ffmpeg" ? "Installing…" : "Install (40MB)"}
                      </Btn>
                    )}
                  </div>

                  {/* Whisper Speech Model */}
                  <div className="flex items-center justify-between p-2.5 rounded-md bg-surface border border-border">
                    <div className="flex items-center gap-2">
                      <i className="bx bx-microphone text-base text-accent" />
                      <div>
                        <p className="font-medium text-primary">
                          Whisper Model ({settings.offline_model === "tiny" ? "Standard" : "Enhanced"})
                        </p>
                        <p className="text-[10px] text-muted">Offline speech recognition model</p>
                      </div>
                    </div>
                    {(settings.offline_model === "tiny" && offlineStatus?.whisper_tiny_ready) ||
                    (settings.offline_model === "base" && offlineStatus?.whisper_base_ready) ? (
                      <span className="text-success text-[11px] font-medium flex items-center gap-1">
                        <i className="bx bx-check text-sm" /> Ready
                      </span>
                    ) : (
                      <Btn
                        variant="secondary"
                        onClick={() =>
                          handleDownload(
                            settings.offline_model === "tiny" ? "whisper_tiny" : "whisper_base"
                          )
                        }
                        disabled={downloadingComponent?.startsWith("whisper")}
                      >
                        {downloadingComponent?.startsWith("whisper")
                          ? "Downloading…"
                          : `Download (${settings.offline_model === "tiny" ? "75MB" : "140MB"})`}
                      </Btn>
                    )}
                  </div>
                </div>
              </div>

              {/* Save Mode Button */}
              <div className="pt-2">
                <Btn onClick={handleSave} disabled={isSaving}>
                  <i
                    className={`bx ${
                      isSaving ? "bx-loader-alt bx-spin" : "bx-check"
                    } text-sm`}
                  />
                  <span>{isSaving ? "Saving…" : "Save Mode"}</span>
                </Btn>
              </div>
            </div>
          )}

          {/* Tab 3: Church Vocabulary */}
          {activeTab === "vocabulary" && (
            <form onSubmit={handleSave} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="font-semibold text-primary block">
                  Church Names, Ministers & Vocabulary
                </label>
                <p className="text-[11px] text-muted">
                  Comma-separated names of pastors, church departments, or biblical terms to ensure 100% spelling accuracy.
                </p>
              </div>

              <textarea
                rows={5}
                value={settings.custom_vocabulary}
                onChange={(e) =>
                  setSettings({ ...settings, custom_vocabulary: e.target.value })
                }
                placeholder="e.g. Pastor Paul Adefarasin, Apostle Joshua Selman, Dunamis, Koinonia, Shiloh, Covenant"
                className="w-full rounded-md bg-surface border border-border p-3 text-xs text-primary outline-none focus:border-accent resize-none leading-relaxed font-sans"
              />

              <div className="p-3 rounded-md bg-surface border border-border space-y-1 text-muted">
                <p className="font-medium text-secondary">
                  Included Nigerian Christian Preaching Defaults:
                </p>
                <p className="text-[11px]">
                  Hallelujah, Amen, Jehovah, Apostle, Pastor, Anointing, Prophetic, Deliverance, Yoruba interjections (Ese O, Amin).
                </p>
              </div>

              {savedNotice && (
                <div className="p-2.5 rounded-md border border-success/30 bg-success-muted text-success flex items-center gap-2">
                  <i className="bx bxs-check-circle text-base" />
                  <span>Vocabulary saved.</span>
                </div>
              )}

              <Btn type="submit" disabled={isSaving}>
                <i
                  className={`bx ${
                    isSaving ? "bx-loader-alt bx-spin" : "bx-check"
                  } text-sm`}
                />
                <span>{isSaving ? "Saving…" : "Save Vocabulary"}</span>
              </Btn>
            </form>
          )}
        </div>
      </div>
    </div>
  );
}
