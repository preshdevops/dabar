import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { createSermon, pickMediaFile } from "../lib/api.js";
import Btn from "../components/Btn.jsx";

export default function Upload() {
  const [sourceType, setSourceType] = useState("file"); // "file" | "youtube" | "gdrive"
  const [urlInput, setUrlInput] = useState("");
  const [selectedFilePath, setSelectedFilePath] = useState(null);
  const [manualPath, setManualPath] = useState("");
  const [showManualPath, setShowManualPath] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);
  const navigate = useNavigate();

  // Listen for Tauri native file drag-drop if available
  useEffect(() => {
    let unlisten = null;
    async function setupTauriDrop() {
      try {
        const event = await import("@tauri-apps/api/event");
        if (event && event.listen) {
          unlisten = await event.listen("tauri://drag-drop", (e) => {
            if (e.payload?.paths?.length > 0) {
              setSelectedFilePath(e.payload.paths[0]);
              setErrorMessage("");
            }
          });
        }
      } catch {
        // Not in Tauri or plugin not active
      }
    }
    setupTauriDrop();
    return () => {
      if (typeof unlisten === "function") unlisten();
    };
  }, []);

  async function handleBrowseFile() {
    setErrorMessage("");
    try {
      const path = await pickMediaFile();
      if (path) {
        setSelectedFilePath(path);
      }
    } catch (err) {
      setErrorMessage("Could not open file picker: " + (err.message || err));
    }
  }

  function handleDragOver(e) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  }

  function handleDragLeave(e) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  }

  function handleDrop(e) {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);

    if (e.dataTransfer?.files?.length > 0) {
      const file = e.dataTransfer.files[0];
      const path = file.path || file.name;
      setSelectedFilePath(path);
      setErrorMessage("");
    }
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setErrorMessage("");

    let source = "";
    if (sourceType === "file") {
      const finalPath = selectedFilePath || manualPath.trim();
      if (!finalPath) {
        setErrorMessage("Please choose or drag a recording file from your computer.");
        return;
      }
      source = finalPath;
    } else if (sourceType === "youtube") {
      const trimmed = urlInput.trim();
      if (!trimmed) {
        setErrorMessage("Please enter a YouTube link.");
        return;
      }
      if (!trimmed.includes("youtube.com/") && !trimmed.includes("youtu.be/")) {
        setErrorMessage("Please enter a valid YouTube video link.");
        return;
      }
      source = trimmed;
    } else if (sourceType === "gdrive") {
      const trimmed = urlInput.trim();
      if (!trimmed) {
        setErrorMessage("Please enter a Google Drive link.");
        return;
      }
      if (!trimmed.includes("drive.google.com/")) {
        setErrorMessage("Please enter a valid Google Drive share link.");
        return;
      }
      source = trimmed;
    }

    setIsProcessing(true);
    try {
      const result = await createSermon(source);
      navigate(`/processing/${result.id}`);
    } catch (err) {
      setErrorMessage(
        err.message || "Could not start processing. Please check the file or link and try again."
      );
    } finally {
      setIsProcessing(false);
    }
  }

  const fileName = selectedFilePath ? selectedFilePath.split(/[/\\]/).pop() : null;

  return (
    <div className="flex flex-col min-h-screen space-y-6">
      <header className="pt-2">
        <div className="space-y-1">
          <h1 className="font-editorial text-2xl sm:text-3xl font-bold text-primary">
            Import Sermon
          </h1>
          <p className="text-secondary text-xs sm:text-sm font-normal">
            Select a recording from your computer or paste a link to transcribe and create clips.
          </p>
        </div>
      </header>

      <div className="flex justify-center py-4">
        <div className="w-full max-w-xl space-y-5">
          {/* Source Type Selector */}
          <div className="flex p-1 bg-surface border border-border rounded-lg text-xs font-semibold">
            <button
              type="button"
              onClick={() => {
                setSourceType("file");
                setErrorMessage("");
              }}
              className={`flex-1 py-1.5 px-3 rounded-md flex items-center justify-center gap-1.5 transition-all ${
                sourceType === "file"
                  ? "bg-accent text-accent-fg shadow-xs"
                  : "text-secondary hover:text-primary"
              }`}
            >
              <i className="bx bx-file text-sm" />
              <span>Local File</span>
            </button>

            <button
              type="button"
              onClick={() => {
                setSourceType("youtube");
                setErrorMessage("");
              }}
              className={`flex-1 py-1.5 px-3 rounded-md flex items-center justify-center gap-1.5 transition-all ${
                sourceType === "youtube"
                  ? "bg-accent text-accent-fg shadow-xs"
                  : "text-secondary hover:text-primary"
              }`}
            >
              <i className="bx bxl-youtube text-base text-red-500" />
              <span>YouTube Video</span>
            </button>

            <button
              type="button"
              onClick={() => {
                setSourceType("gdrive");
                setErrorMessage("");
              }}
              className={`flex-1 py-1.5 px-3 rounded-md flex items-center justify-center gap-1.5 transition-all ${
                sourceType === "gdrive"
                  ? "bg-accent text-accent-fg shadow-xs"
                  : "text-secondary hover:text-primary"
              }`}
            >
              <i className="bx bxs-cloud text-base text-sky-400" />
              <span>Google Drive</span>
            </button>
          </div>

          {/* Ingestion Form */}
          <form onSubmit={handleSubmit} className="space-y-4">
            {sourceType === "file" && (
              <div className="space-y-2.5">
                <div
                  onClick={handleBrowseFile}
                  onDragOver={handleDragOver}
                  onDragLeave={handleDragLeave}
                  onDrop={handleDrop}
                  className={`cursor-pointer border-2 border-dashed p-8 rounded-xl text-center transition-all flex flex-col items-center justify-center gap-2.5 select-none ${
                    isDragging
                      ? "border-accent bg-accent-muted scale-[1.01]"
                      : "border-border hover:border-border-strong bg-surface"
                  }`}
                >
                  <div className="w-10 h-10 rounded-lg bg-surface-elevated text-accent flex items-center justify-center text-xl border border-border">
                    <i className={`bx ${selectedFilePath ? "bx-check text-success" : isDragging ? "bx-down-arrow-alt" : "bx-upload"}`} />
                  </div>
                  <div className="space-y-0.5">
                    <p className="font-editorial text-base font-bold text-primary">
                      {fileName || (isDragging ? "Drop sermon file here" : "Click to browse or drop sermon recording")}
                    </p>
                    <p className="text-xs text-secondary">
                      Supports MP4, MOV, MKV, MP3, M4A, WAV formats
                    </p>
                  </div>
                  {selectedFilePath && (
                    <span className="text-xs text-accent font-mono truncate max-w-md mt-1 block px-2.5 py-1 rounded bg-surface-elevated border border-border">
                      {selectedFilePath}
                    </span>
                  )}
                </div>

                {/* Path helper */}
                <div className="flex items-center justify-between text-xs px-1">
                  <button
                    type="button"
                    onClick={() => setShowManualPath(!showManualPath)}
                    className="text-secondary hover:text-accent underline underline-offset-2 font-mono text-[11px]"
                  >
                    {showManualPath ? "Hide path input" : "Or type / paste absolute file path"}
                  </button>
                  {selectedFilePath && (
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedFilePath(null);
                        setManualPath("");
                      }}
                      className="text-danger hover:underline text-[11px]"
                    >
                      Clear selection
                    </button>
                  )}
                </div>

                {showManualPath && (
                  <div className="space-y-1 pt-1">
                    <input
                      type="text"
                      value={manualPath}
                      onChange={(e) => setManualPath(e.target.value)}
                      placeholder="C:\Users\Pastor\Videos\Sunday_Message.mp4"
                      className="w-full rounded-md bg-surface border border-border px-3 py-2 text-xs font-mono text-primary outline-none focus:border-accent"
                    />
                  </div>
                )}
              </div>
            )}

            {sourceType === "youtube" && (
              <div className="space-y-2.5 p-5 rounded-xl border border-border bg-surface">
                <div className="space-y-1">
                  <label className="text-xs font-semibold text-primary block">
                    YouTube Sermon Video Link
                  </label>
                  <div className="relative">
                    <i className="bx bxl-youtube absolute left-3 top-1/2 -translate-y-1/2 text-red-500 text-lg" />
                    <input
                      type="text"
                      value={urlInput}
                      onChange={(e) => setUrlInput(e.target.value)}
                      placeholder="https://www.youtube.com/watch?v=..."
                      className="w-full rounded-md bg-surface-elevated border border-border pl-10 pr-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent"
                      autoFocus
                    />
                  </div>
                </div>
                <p className="text-[11px] text-muted">
                  Dabar streams and transcribes directly using Groq Whisper without storing huge video caches.
                </p>
              </div>
            )}

            {sourceType === "gdrive" && (
              <div className="space-y-2.5 p-5 rounded-xl border border-border bg-surface">
                <div className="space-y-1">
                  <label className="text-xs font-semibold text-primary block">
                    Google Drive Shareable Link
                  </label>
                  <div className="relative">
                    <i className="bx bxs-cloud absolute left-3 top-1/2 -translate-y-1/2 text-sky-400 text-lg" />
                    <input
                      type="text"
                      value={urlInput}
                      onChange={(e) => setUrlInput(e.target.value)}
                      placeholder="https://drive.google.com/file/d/.../view?usp=sharing"
                      className="w-full rounded-md bg-surface-elevated border border-border pl-10 pr-3 py-2 text-xs text-primary font-mono outline-none focus:border-accent"
                      autoFocus
                    />
                  </div>
                </div>
                <p className="text-[11px] text-muted">
                  Ensure the Google Drive link has <strong className="text-primary">"Anyone with the link can view"</strong> permissions.
                </p>
              </div>
            )}

            {errorMessage && (
              <div className="p-2.5 rounded-md border border-danger/30 bg-danger-muted text-xs text-danger flex items-center gap-2">
                <i className="bx bx-error-circle text-base shrink-0" />
                <span>{errorMessage}</span>
              </div>
            )}

            <div className="pt-2">
              <Btn
                type="submit"
                variant="primary"
                size="lg"
                className="w-full"
                disabled={isProcessing}
              >
                {isProcessing ? (
                  <>
                    <i className="bx bx-loader-alt bx-spin text-base" />
                    <span>Processing sermon…</span>
                  </>
                ) : (
                  <>
                    <i className="bx bx-zap text-base" />
                    <span>Transcribe & Create Clips</span>
                  </>
                )}
              </Btn>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
