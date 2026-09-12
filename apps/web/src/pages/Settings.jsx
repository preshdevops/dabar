import { useEffect, useState } from "react";
import {
  getSettings,
  saveSettings,
  checkDependencies,
  downloadYtDlp,
  downloadFfmpeg,
  getOfflineStatus,
  onDownloadProgress,
} from "../lib/api.js";
import { useTheme } from "../context/ThemeContext.jsx";
import Btn from "../components/Btn.jsx";

export default function Settings() {
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState("general");
  const [settings, setSettings] = useState({
    groq_api_key: "",
    deepgram_api_key: "",
    openai_api_key: "",
    output_dir: "",
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

  const handleSave = async (e) => {
    if (e) e.preventDefault();
    setIsSaving(true);
    try {
      await saveSettings(settings);
      setSavedNotice(true);
      setTimeout(() => setSavedNotice(false), 2500);
    } catch (err) {
      console.error("Failed to save settings:", err);
    } finally {
      setIsSaving(false);
    }
  };

  const handleDownload = async (component) => {
    setDownloadingComponent(component);
    try {
      if (component === "yt-dlp") {
        await downloadYtDlp();
      } else if (component === "ffmpeg") {
        await downloadFfmpeg();
      }
      const updatedDeps = await checkDependencies();
      setDeps(updatedDeps);
    } catch (err) {
      console.error(`Failed to download ${component}:`, err);
    } finally {
      setDownloadingComponent(null);
    }
  };

  return (
    <div className="space-y-6 max-w-4xl mx-auto pb-12">
      {/* Header */}
      <div className="border-b border-border pb-4 flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-primary flex items-center gap-2">
            <i className="bx bx-cog text-accent text-2xl" />
            <span>Settings</span>
          </h1>
          <p className="text-xs text-muted mt-1">
            Configure your AI transcription keys, themes, and media tools.
          </p>
        </div>
        {savedNotice && (
          <div className="flex items-center gap-1.5 text-xs text-emerald-500 bg-emerald-500/10 px-3 py-1.5 rounded-full animate-fade-in font-medium">
            <i className="bx bx-check-circle" />
            <span>Saved successfully</span>
          </div>
        )}
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border gap-2">
        <button
          onClick={() => setActiveTab("general")}
          className={`pb-2.5 px-3 text-xs font-medium border-b-2 transition-all flex items-center gap-1.5 ${
            activeTab === "general"
              ? "border-accent text-accent"
              : "border-transparent text-muted hover:text-primary"
          }`}
        >
          <i className="bx bx-sliders" />
          <span>General & Appearance</span>
        </button>
        <button
          onClick={() => setActiveTab("mode")}
          className={`pb-2.5 px-3 text-xs font-medium border-b-2 transition-all flex items-center gap-1.5 ${
            activeTab === "mode"
              ? "border-accent text-accent"
              : "border-transparent text-muted hover:text-primary"
          }`}
        >
          <i className="bx bx-bolt-circle" />
          <span>AI & Transcription</span>
        </button>
      </div>

      {/* Content Area */}
      <div className="pt-2">
        {/* Tab 1: General & Appearance */}
        {activeTab === "general" && (
          <form onSubmit={handleSave} className="space-y-5 text-xs">
            {/* Theme Selector */}
            <div className="studio-card p-4 space-y-3">
              <label className="font-semibold text-primary block">
                Theme / Appearance
              </label>
              <div className="grid grid-cols-2 gap-3">
                <button
                  type="button"
                  onClick={() => setTheme("dark")}
                  className={`p-3 rounded-lg border flex items-center gap-3 transition-all ${
                    theme === "dark"
                      ? "border-accent bg-accent-muted/20 text-primary"
                      : "border-border bg-surface text-muted hover:text-primary"
                  }`}
                >
                  <i className="bx bx-moon text-lg text-accent" />
                  <div className="text-left">
                    <div className="font-medium text-xs">Sanctuary Dark</div>
                    <div className="text-[10px] text-muted">Rich charcoal with warm amber accents</div>
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() => setTheme("light")}
                  className={`p-3 rounded-lg border flex items-center gap-3 transition-all ${
                    theme === "light"
                      ? "border-accent bg-accent-muted/20 text-primary"
                      : "border-border bg-surface text-muted hover:text-primary"
                  }`}
                >
                  <i className="bx bx-sun text-lg text-amber-500" />
                  <div className="text-left">
                    <div className="font-medium text-xs">Illumination Light</div>
                    <div className="text-[10px] text-muted">Clean parchment paper aesthetics</div>
                  </div>
                </button>
              </div>
            </div>

            {/* Custom Vocabulary Card */}
            <div className="studio-card p-4 space-y-2">
              <label className="font-semibold text-primary block">
                Custom Church Vocabulary & Terms
              </label>
              <p className="text-[11px] text-muted">
                Add ministry names, pastors, theological terms, and Yoruba phrases (comma or newline separated).
              </p>
              <textarea
                rows={3}
                value={settings.custom_vocabulary || ""}
                onChange={(e) =>
                  setSettings({ ...settings, custom_vocabulary: e.target.value })
                }
                placeholder="e.g. Pastor Paul, Oluwaseun, Hallelujah, Ruach Hakodesh, Koinonia"
                className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent resize-none"
              />
            </div>

            {/* Output Directory */}
            <div className="studio-card p-4 space-y-2">
              <label className="font-semibold text-primary block">
                Saved Videos Folder
              </label>
              <input
                type="text"
                value={settings.output_dir || ""}
                onChange={(e) =>
                  setSettings({ ...settings, output_dir: e.target.value })
                }
                placeholder="Videos/Dabar"
                className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary outline-none focus:border-accent"
              />
              <p className="text-[11px] text-muted">
                The local folder where exported reel and sermon video clips are saved.
              </p>
            </div>

            {/* Save Button */}
            <Btn type="submit" variant="primary" disabled={isSaving}>
              <i className={`bx ${isSaving ? "bx-loader-alt bx-spin" : "bx-save"}`} />
              <span>{isSaving ? "Saving…" : "Save Settings"}</span>
            </Btn>
          </form>
        )}

        {/* Tab 2: Transcription Mode & Tools */}
        {activeTab === "mode" && (
          <div className="space-y-5 text-xs">
            {/* Primary Engine Choice: Groq vs Deepgram */}
            <div className="space-y-2">
              <label className="font-semibold text-primary block">
                Primary Transcription Engine
              </label>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {/* Groq Mode */}
                <div
                  onClick={() =>
                    setSettings({ ...settings, transcription_backend: "groq" })
                  }
                  className={`cursor-pointer border rounded-lg p-3.5 transition-all ${
                    settings.transcription_backend === "groq"
                      ? "border-accent bg-accent-muted/20 shadow-xs"
                      : "border-border bg-surface hover:bg-surface-hover"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-primary flex items-center gap-1.5">
                      <i className="bx bx-bolt-circle text-accent text-base" />
                      <span>Groq Whisper (Recommended)</span>
                    </span>
                    {settings.transcription_backend === "groq" && (
                      <i className="bx bxs-check-circle text-accent text-base" />
                    )}
                  </div>
                  <p className="text-[11px] text-muted mt-1 leading-relaxed">
                    Ultra-fast Whisper Large v3 Turbo transcription and GPT-OSS 120B pastoral highlight detection.
                  </p>
                </div>

                {/* Deepgram Mode */}
                <div
                  onClick={() =>
                    setSettings({ ...settings, transcription_backend: "deepgram" })
                  }
                  className={`cursor-pointer border rounded-lg p-3.5 transition-all ${
                    settings.transcription_backend === "deepgram"
                      ? "border-accent bg-accent-muted/20 shadow-xs"
                      : "border-border bg-surface hover:bg-surface-hover"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-primary flex items-center gap-1.5">
                      <i className="bx bx-podcast text-accent text-base" />
                      <span>Deepgram Nova-3 (Fallback / Fast)</span>
                    </span>
                    {settings.transcription_backend === "deepgram" && (
                      <i className="bx bxs-check-circle text-accent text-base" />
                    )}
                  </div>
                  <p className="text-[11px] text-muted mt-1 leading-relaxed">
                    High-accuracy speech-to-text with smart punctuation and natural sentence formatting.
                  </p>
                </div>
              </div>
            </div>

            {/* Cloud AI Runtime & Keys */}
            <div className="studio-card p-4 space-y-4 border-border">
              <div>
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-xs text-primary flex items-center gap-1.5">
                    <i className="bx bx-key text-accent text-base" />
                    <span>API Keys</span>
                  </span>
                  <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-accent-muted text-accent">
                    Saved Privately
                  </span>
                </div>
                <p className="text-[11px] text-muted mt-1 leading-relaxed">
                  Your keys are stored securely on your local device.
                </p>
              </div>

              <div className="space-y-3 pt-1 border-t border-border">
                <div className="space-y-1">
                  <label className="font-semibold text-primary block text-[11px]">
                    Groq API Key (Primary)
                  </label>
                  <input
                    type="password"
                    value={settings.groq_api_key || ""}
                    onChange={(e) =>
                      setSettings({ ...settings, groq_api_key: e.target.value })
                    }
                    placeholder="gsk_..."
                    className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent"
                  />
                  <p className="text-[10px] text-muted">
                    Powers Groq Whisper Large v3 Turbo & GPT-OSS 120B pastoral moment detection.
                  </p>
                </div>

                <div className="space-y-1">
                  <label className="font-semibold text-primary block text-[11px]">
                    Deepgram API Key (Fallback)
                  </label>
                  <input
                    type="password"
                    value={settings.deepgram_api_key || ""}
                    onChange={(e) =>
                      setSettings({ ...settings, deepgram_api_key: e.target.value })
                    }
                    placeholder="Token ..."
                    className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent"
                  />
                  <p className="text-[10px] text-muted">
                    Used for Deepgram Nova-3 transcription. Get your key from https://deepgram.com
                  </p>
                </div>

                <div className="space-y-1">
                  <label className="font-semibold text-primary block text-[11px]">
                    OpenAI API Key (Optional)
                  </label>
                  <input
                    type="password"
                    value={settings.openai_api_key || ""}
                    onChange={(e) =>
                      setSettings({ ...settings, openai_api_key: e.target.value })
                    }
                    placeholder="sk-..."
                    className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent"
                  />
                </div>
              </div>

              <Btn type="button" onClick={handleSave} variant="primary" disabled={isSaving}>
                <i className={`bx ${isSaving ? "bx-loader-alt bx-spin" : "bx-save"}`} />
                <span>{isSaving ? "Saving…" : "Save Settings"}</span>
              </Btn>
            </div>

            {/* Media Tools Readiness Card */}
            <div className="studio-card p-4 space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="font-semibold text-xs text-primary">
                    Local Media Tools
                  </h3>
                  <p className="text-[11px] text-muted">
                    Required media engines installed on your computer.
                  </p>
                </div>
                {offlineStatus &&
                  offlineStatus.ffmpeg_ready &&
                  offlineStatus.yt_dlp_ready && (
                    <span className="text-[11px] font-medium text-emerald-500 flex items-center gap-1">
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
                      <p className="font-semibold text-primary text-[11px]">yt-dlp</p>
                      <p className="text-[10px] text-muted">
                        Extracts sermon audio from YouTube streams.
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {deps?.yt_dlp ? (
                      <span className="px-2 py-0.5 rounded text-[10px] bg-emerald-500/10 text-emerald-500 font-medium">
                        Installed
                      </span>
                    ) : (
                      <Btn
                        size="sm"
                        variant="secondary"
                        disabled={downloadingComponent === "yt-dlp"}
                        onClick={() => handleDownload("yt-dlp")}
                      >
                        {downloadingComponent === "yt-dlp"
                          ? `Downloading ${downloadProgress["yt-dlp"] || 0}%`
                          : "Download"}
                      </Btn>
                    )}
                  </div>
                </div>

                {/* FFmpeg Preprocessor */}
                <div className="flex items-center justify-between p-2.5 rounded-md bg-surface border border-border">
                  <div className="flex items-center gap-2">
                    <i className="bx bx-film text-base text-accent" />
                    <div>
                      <p className="font-semibold text-primary text-[11px]">FFmpeg</p>
                      <p className="text-[10px] text-muted">
                        High-performance audio transcoding and video clip rendering.
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {deps?.ffmpeg ? (
                      <span className="px-2 py-0.5 rounded text-[10px] bg-emerald-500/10 text-emerald-500 font-medium">
                        Installed
                      </span>
                    ) : (
                      <Btn
                        size="sm"
                        variant="secondary"
                        disabled={downloadingComponent === "ffmpeg"}
                        onClick={() => handleDownload("ffmpeg")}
                      >
                        {downloadingComponent === "ffmpeg"
                          ? `Downloading ${downloadProgress["ffmpeg"] || 0}%`
                          : "Download"}
                      </Btn>
                    )}
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
