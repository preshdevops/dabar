import { useState, useEffect } from "react";
import Btn from "./Btn.jsx";
import { openInExplorer, getAssetUrl } from "../lib/api.js";

const ASPECT_RATIOS = [
  {
    key: "9:16",
    label: "Vertical Reel (9:16)",
    sub: "Instagram, TikTok, YouTube Shorts",
    icon: "bx-mobile-alt",
  },
  {
    key: "1:1",
    label: "Square Post (1:1)",
    sub: "Facebook, Instagram Feed",
    icon: "bx-square",
  },
  {
    key: "16:9",
    label: "Widescreen (16:9)",
    sub: "YouTube, Church Website",
    icon: "bx-tv",
  },
];

export default function ExportModal({
  clip,
  sermonTitle,
  videoId,
  mediaAssetUrl,
  onClose,
  onConfirmExport,
  isRendering = false,
  exportedPath = null,
  renderError = null,
}) {
  const [aspectRatio, setAspectRatio] = useState("9:16");
  const [customFileName, setCustomFileName] = useState(
    clip?.highlight_title || clip?.title || "sermon_clip"
  );
  const [renderedAssetUrl, setRenderedAssetUrl] = useState(null);

  const durationSec =
    clip?.start !== undefined && clip?.end !== undefined
      ? Math.max(1, Math.round(clip.end - clip.start))
      : 45;


  // When export completes, resolve asset URL for the newly rendered MP4 video
  useEffect(() => {
    if (exportedPath) {
      getAssetUrl(exportedPath).then((url) => {
        if (url) setRenderedAssetUrl(url);
      });
    } else {
      setRenderedAssetUrl(null);
    }
  }, [exportedPath]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md animate-in fade-in duration-200">
      <div className="studio-card-elevated max-w-3xl w-full max-h-[90vh] overflow-y-auto p-6 space-y-5 bg-surface border-border">
        {/* ── Modal Header ───────────────────────────────────────── */}
        <div className="flex items-center justify-between border-b border-border pb-3.5">
          <div className="space-y-0.5">
            <h2 className="font-editorial text-xl font-bold text-primary">
              {exportedPath ? "Clip Ready to Watch & Share" : "Export Video Clip"}
            </h2>
            <p className="text-xs text-secondary">
              {exportedPath
                ? "Your video has been rendered and saved to your computer."
                : `Create a ${durationSec}-second video clip ready for social media.`}
            </p>
          </div>

          <button
            type="button"
            onClick={onClose}
            disabled={isRendering}
            className="w-7 h-7 rounded-md bg-surface-elevated hover:bg-surface-hover text-muted hover:text-primary flex items-center justify-center transition-colors disabled:opacity-50"
            aria-label="Close export dialog"
          >
            <i className="bx bx-x text-xl" />
          </button>
        </div>

        {/* ── Render Error Alert ──────────────────────────────────── */}
        {renderError && (
          <div className="p-3.5 rounded-lg border border-danger/40 bg-danger-muted text-xs space-y-1">
            <div className="flex items-center gap-2 text-danger font-semibold">
              <i className="bx bx-error-circle text-base" />
              <span>Could not export video</span>
            </div>
            <p className="text-secondary text-xs leading-relaxed">
              {renderError}
            </p>
            {renderError.toLowerCase().includes("ffmpeg") && (
              <p className="text-accent text-xs pt-1 font-medium">
                Tip: Go to <strong>Settings</strong> and click <strong>Install Video Tools</strong> to enable video export.
              </p>
            )}
          </div>
        )}

        {/* ── Main Display: Rendered Video Player vs Configuration ──── */}
        {exportedPath && renderedAssetUrl ? (
          /* Rendered Video Success Player */
          <div className="space-y-4">
            <div className="flex flex-col items-center justify-center p-4 rounded-xl bg-black border border-border">
              <video
                src={renderedAssetUrl}
                controls
                autoPlay
                className="max-h-[380px] rounded-lg shadow-lg"
              />
            </div>

            <div className="p-3.5 rounded-lg border border-success/30 bg-success-muted flex items-center justify-between gap-3 text-xs">
              <div className="space-y-0.5 min-w-0">
                <p className="font-semibold text-primary flex items-center gap-1.5">
                  <i className="bx bxs-check-circle text-success text-base" />
                  <span>Saved to: Videos/Dabar</span>
                </p>
                <p className="text-[11px] text-muted truncate max-w-md">
                  {exportedPath}
                </p>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                <Btn
                  size="sm"
                  variant="primary"
                  icon="bx-folder-open"
                  onClick={() => openInExplorer(exportedPath)}
                >
                  Open in Folder
                </Btn>
              </div>
            </div>
          </div>
        ) : (
          /* Configuration & Live Preview Stage */
          <div className="grid grid-cols-1 md:grid-cols-12 gap-6 items-start">
            {/* Visual Frame Preview */}
            <div className="md:col-span-5 flex flex-col items-center justify-center p-4 rounded-xl bg-base border border-border min-h-[300px]">
              <span className="text-xs text-muted mb-3 font-medium">
                Preview Frame ({aspectRatio})
              </span>

              <div
                className={`relative bg-black border border-white/20 rounded-lg overflow-hidden shadow-md flex flex-col justify-between p-3 transition-all duration-300 ${
                  aspectRatio === "9:16"
                    ? "w-36 h-60"
                    : aspectRatio === "1:1"
                    ? "w-48 h-48"
                    : "w-56 h-32"
                }`}
              >
                {/* Header */}
                <div className="relative z-10 flex items-center justify-between text-[9px] text-white/80">
                  <span className="px-1.5 py-0.5 rounded bg-black/60 font-semibold">
                    DABAR
                  </span>
                  <span className="flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse" />
                    REC
                  </span>
                </div>

                {/* Middle Clip */}
                <div className="relative z-10 my-auto text-center px-1 space-y-1">
                  <div className="w-7 h-7 rounded-full bg-accent/20 border border-accent/40 text-accent flex items-center justify-center mx-auto text-xs">
                    <i className="bx bx-play" />
                  </div>
                  <p className="font-editorial text-[10.5px] text-white/90 font-bold line-clamp-2">
                    {clip?.highlight_title || clip?.title || "Preaching Moment"}
                  </p>
                </div>

              </div>

              <span className="text-[11px] text-muted mt-3 font-medium">
                {aspectRatio === "9:16"
                  ? "Full HD Vertical Video"
                  : aspectRatio === "1:1"
                  ? "Square Video Post"
                  : "Horizontal Video"}
              </span>
            </div>

            {/* Render Controls */}
            <div className="md:col-span-7 space-y-4">
              {/* Aspect Ratio Options */}
              <div className="space-y-1.5">
                <label className="text-xs font-semibold text-primary block">
                  Video Shape
                </label>
                <div className="grid grid-cols-3 gap-2">
                  {ASPECT_RATIOS.map((fmt) => (
                    <button
                      key={fmt.key}
                      type="button"
                      onClick={() => setAspectRatio(fmt.key)}
                      className={`p-2.5 rounded-lg border text-left transition-all ${
                        aspectRatio === fmt.key
                          ? "border-accent bg-accent-muted text-accent font-semibold"
                          : "border-border bg-surface-elevated hover:bg-surface-hover text-secondary"
                      }`}
                    >
                      <i
                        className={`bx ${fmt.icon} text-base ${
                          aspectRatio === fmt.key ? "text-accent" : "text-muted"
                        }`}
                      />
                      <p className="text-xs text-primary font-medium mt-0.5">
                        {fmt.label.split(" (")[0]}
                      </p>
                      <p className="text-[10px] text-muted truncate">
                        {fmt.sub}
                      </p>
                    </button>
                  ))}
                </div>
              </div>

              {/* Output Filename */}
              <div className="space-y-1">
                <label className="text-xs font-semibold text-primary block">
                  Clip Title
                </label>
                <input
                  type="text"
                  value={customFileName}
                  onChange={(e) => setCustomFileName(e.target.value)}
                  className="w-full rounded-md bg-surface-elevated border border-border px-3 py-1.5 text-xs text-primary outline-none focus:border-accent"
                />
              </div>

              {/* Active Rendering Progress Indicator */}
              {isRendering && (
                <div className="p-3.5 rounded-lg border border-accent/30 bg-accent-muted/20 space-y-2">
                  <div className="flex items-center justify-between text-xs font-semibold text-accent">
                    <span className="flex items-center gap-2">
                      <i className="bx bx-loader-alt bx-spin text-base" />
                      <span>Creating your video clip…</span>
                    </span>
                  </div>
                  <div className="w-full bg-surface-elevated h-1.5 rounded-full overflow-hidden">
                    <div className="bg-accent h-full w-2/3 rounded-full animate-pulse" />
                  </div>
                  <p className="text-[11px] text-secondary">
                    Trimming audio and formatting vertical video layout…
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {/* ── Modal Footer Actions ────────────────────────────────── */}
        <div className="flex items-center justify-end gap-2.5 pt-3 border-t border-border">
          <button
            type="button"
            onClick={onClose}
            disabled={isRendering}
            className="px-3.5 py-1.5 rounded-md text-xs font-medium text-secondary hover:text-primary transition-colors disabled:opacity-50"
          >
            {exportedPath ? "Done" : "Cancel"}
          </button>

          {!exportedPath && (
            <Btn
              size="md"
              variant="primary"
              icon={isRendering ? "bx-loader-alt bx-spin" : "bx-film"}
              disabled={isRendering}
              onClick={() =>
                onConfirmExport(clip, aspectRatio, "none", customFileName)
              }
            >
              {isRendering ? "Creating Video…" : "Create Video Clip"}
            </Btn>
          )}
        </div>
      </div>
    </div>
  );
}
