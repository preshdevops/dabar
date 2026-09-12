import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  getOfflineStatus,
  downloadYtDlp,
  downloadFfmpeg,
} from "../lib/api.js";

export default function Onboarding() {
  const navigate = useNavigate();
  const [activeState, setActiveState] = useState(1); // 1: Audio, 2: Transcript, 3: Clip
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false);

  // Background dependency check & install
  useEffect(() => {
    async function initTools() {
      try {
        const status = await getOfflineStatus();
        if (!status?.yt_dlp_ready) downloadYtDlp().catch(() => {});
        if (!status?.ffmpeg_ready) downloadFfmpeg().catch(() => {});
      } catch {
        // Silently continue
      }
    }
    initTools();
  }, []);

  // Detect system prefers-reduced-motion
  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    setPrefersReducedMotion(mediaQuery.matches);
    if (mediaQuery.matches) {
      setActiveState(2); // Show State 2 (transcript) statically
    }
    const handler = (e) => {
      setPrefersReducedMotion(e.matches);
      if (e.matches) setActiveState(2);
    };
    mediaQuery.addEventListener("change", handler);
    return () => mediaQuery.removeEventListener("change", handler);
  }, []);

  // Self-animating continuous cycle: 5.5 seconds per state, looping forever
  useEffect(() => {
    if (prefersReducedMotion) return;
    const timer = setInterval(() => {
      setActiveState((prev) => (prev % 3) + 1);
    }, 5500);
    return () => clearInterval(timer);
  }, [prefersReducedMotion]);

  function handleStart(destination = "/upload") {
    localStorage.setItem("dabaar_onboarded", "true");
    navigate(destination);
  }

  return (
    <div className="min-h-[100dvh] bg-base text-primary flex flex-col justify-between selection:bg-accent/20 overflow-x-hidden font-sans">
      {/* ── Top Atmospheric Accent Line ──────────────────────────────────── */}
      <div className="h-[2px] w-full bg-gradient-to-r from-transparent via-accent to-transparent opacity-80" />

      {/* ── Top Header ───────────────────────────────────────────────────── */}
      <header className="w-full px-6 sm:px-12 py-5 flex items-center justify-between border-b border-border/60 backdrop-blur-md bg-surface/40 sticky top-0 z-50">
        <div className="flex items-center gap-3.5">
          <div className="w-9 h-9 rounded-xl bg-surface-elevated border border-border flex items-center justify-center font-editorial font-bold text-lg shadow-xs relative">
            <span className="text-accent select-none">ד</span>
            <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-accent ring-2 ring-base" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="font-editorial text-xl font-bold tracking-tight text-primary">
                DABAR
              </span>
              <span className="text-[10px] font-mono uppercase tracking-widest text-accent bg-accent-muted px-2 py-0.5 rounded-md border border-accent-border">
                דָּבָר
              </span>
            </div>
            <p className="text-[11px] text-muted tracking-tight">
              Sermon Media & Clip Studio
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3 sm:gap-4">
          <button
            onClick={() => handleStart("/dashboard")}
            className="text-xs text-secondary hover:text-primary transition-colors flex items-center gap-1.5 py-1.5 px-3 rounded-lg hover:bg-surface-elevated border border-transparent hover:border-border font-medium"
          >
            <span>Skip to Library</span>
            <i className="bx bx-right-arrow-alt text-sm" />
          </button>
        </div>
      </header>

      {/* ── Main Editorial Showcase ──────────────────────────────────────── */}
      <main className="max-w-4xl mx-auto px-6 sm:px-12 py-12 sm:py-16 w-full flex-1 flex flex-col justify-center space-y-20">
        {/* ── Section 1: Hero ──────────────────────────────────────────────── */}
        <div className="text-center max-w-3xl mx-auto space-y-6">
          <h1 className="text-3xl sm:text-5xl lg:text-6xl font-editorial font-bold text-primary tracking-tight leading-[1.12]">
            Turn Sunday sermons into chapters and social video clips.
          </h1>

          <p className="text-secondary text-sm sm:text-base leading-relaxed max-w-2xl mx-auto">
            Dabar automatically converts your sermon recordings into word-for-word transcripts,
            YouTube section timestamps, and short vertical video clips ready for Instagram, TikTok,
            and YouTube Shorts.
          </p>

          {/* Primary CTA Buttons */}
          <div className="pt-2 flex flex-col sm:flex-row items-center justify-center gap-3.5">
            <button
              onClick={() => handleStart("/upload")}
              className="group relative inline-flex items-center gap-3 px-7 py-3 rounded-full bg-accent hover:bg-accent-hover text-accent-fg font-sans text-sm font-semibold transition-all duration-200 shadow-sm active:scale-[0.98]"
            >
              <span>Upload a Sermon</span>
              <div className="w-6 h-6 rounded-full bg-white/20 flex items-center justify-center transition-transform duration-200 group-hover:translate-x-0.5">
                <i className="bx bx-right-arrow-alt text-base text-accent-fg" />
              </div>
            </button>

            <button
              onClick={() => handleStart("/dashboard")}
              className="px-6 py-3 rounded-full bg-surface-elevated hover:bg-surface-hover border border-border text-secondary hover:text-primary text-sm font-medium transition-colors"
            >
              Open Sermon Library
            </button>
          </div>
        </div>

        {/* ── Section 2: ONE Continuous Self-Animating Morphing Element ──────── */}
        <section aria-label="Transformation Demonstration" className="w-full flex flex-col items-center justify-center py-6">
          <div className="w-full max-w-[600px] h-64 relative flex items-center justify-center">
            {/* ── STATE 1: Raw Jagged Audio Waveform ─────────────────────────── */}
            <div
              className={`absolute inset-0 flex items-center justify-center transition-all duration-700 ease-in-out ${
                activeState === 1
                  ? "opacity-100 scale-100 pointer-events-auto"
                  : "opacity-0 scale-95 pointer-events-none"
              }`}
            >
              <div className="w-full max-w-[540px] flex items-center justify-between gap-1 sm:gap-1.5 px-4 h-28">
                {[
                  18, 55, 88, 30, 65, 95, 42, 80, 100, 58, 25, 90, 85, 45, 75, 92, 34,
                  68, 98, 90, 80, 48, 22, 70, 94, 76, 52, 38, 88, 96, 62, 44, 72, 85,
                  28, 58, 92, 78, 48, 20,
                ].map((height, idx) => (
                  <div
                    key={idx}
                    className="w-full rounded-full transition-all duration-300"
                    style={{
                      height: `${height}%`,
                      backgroundColor: "var(--border-strong, #374151)",
                      opacity: idx % 3 === 0 ? 0.9 : idx % 2 === 0 ? 0.6 : 0.4,
                    }}
                  />
                ))}
              </div>
            </div>

            {/* ── STATE 2: Transcribed Text with Moving Accent Highlight ─────── */}
            <div
              className={`absolute inset-0 flex items-center justify-center transition-all duration-700 ease-in-out ${
                activeState === 2
                  ? "opacity-100 scale-100 pointer-events-auto"
                  : "opacity-0 scale-95 pointer-events-none"
              }`}
            >
              <div className="w-full max-w-[540px] px-6 space-y-4 text-left">
                {/* Line 1 */}
                <div className="space-y-1.5">
                  <div className="flex items-center gap-3">
                    <span className="text-[11px] font-mono text-muted select-none">04:12</span>
                    <div className="h-2.5 w-4/5 bg-surface-elevated rounded-full" />
                  </div>
                  <div className="h-2.5 w-full bg-surface-elevated rounded-full ml-11" />
                </div>

                {/* Line 2 with Moving Highlight Block */}
                <div className="space-y-1.5">
                  <div className="flex items-center gap-3">
                    <span className="text-[11px] font-mono text-accent select-none font-semibold">04:28</span>
                    <div className="h-5 flex-1 rounded-md bg-accent-muted border border-accent-border px-2 flex items-center overflow-hidden">
                      <span className="text-xs font-editorial font-medium text-accent truncate">
                        “Those who wait upon the Lord shall renew their strength…”
                      </span>
                    </div>
                  </div>
                  <div className="h-2.5 w-3/4 bg-surface-elevated rounded-full ml-11" />
                </div>

                {/* Line 3 */}
                <div className="space-y-1.5">
                  <div className="flex items-center gap-3">
                    <span className="text-[11px] font-mono text-muted select-none">04:45</span>
                    <div className="h-2.5 w-2/3 bg-surface-elevated rounded-full" />
                  </div>
                </div>
              </div>
            </div>

            {/* ── STATE 3: Clipped Vertical 9:16 Rectangle ───────────────────── */}
            <div
              className={`absolute inset-0 flex items-center justify-center transition-all duration-700 ease-in-out ${
                activeState === 3
                  ? "opacity-100 scale-100 pointer-events-auto"
                  : "opacity-0 scale-95 pointer-events-none"
              }`}
            >
              <div className="w-36 h-56 rounded-xl border border-border bg-surface-elevated p-3 flex flex-col justify-between shadow-sm">
                <div className="space-y-1.5">
                  <div className="h-1 w-8 bg-muted/40 rounded-full" />
                  <div className="h-1.5 w-full bg-border rounded-full" />
                </div>

                <div className="space-y-1 text-center py-2">
                  <div className="h-2 w-full bg-border rounded-full" />
                  <div className="h-2 w-4/5 mx-auto bg-border rounded-full" />
                </div>

                {/* Bottom Caption Line */}
                <div className="space-y-1.5 pt-2 border-t border-border/50">
                  <div className="h-2 w-full bg-accent rounded-full opacity-90" />
                  <div className="h-1.5 w-2/3 bg-accent rounded-full opacity-60" />
                </div>
              </div>
            </div>
          </div>

          {/* ── Three Quiet Plain-Text State Labels Below ────────────────────── */}
          <div className="flex items-center justify-center gap-8 sm:gap-12 pt-8 text-xs font-sans select-none">
            <span
              className={`transition-colors duration-500 ${
                activeState === 1 ? "text-accent font-bold" : "text-muted font-normal"
              }`}
            >
              Clean & Ingest Audio
            </span>
            <span
              className={`transition-colors duration-500 ${
                activeState === 2 ? "text-accent font-bold" : "text-muted font-normal"
              }`}
            >
              Accurate Transcript
            </span>
            <span
              className={`transition-colors duration-500 ${
                activeState === 3 ? "text-accent font-bold" : "text-muted font-normal"
              }`}
            >
              Social Video Clips
            </span>
          </div>
        </section>

        {/* ── Section 3: Bottom Call-to-Action ──────────────────────────────── */}
        <div className="text-center space-y-6 pt-4">
          <div className="max-w-xl mx-auto space-y-2">
            <h2 className="font-editorial text-2xl sm:text-3xl font-bold text-primary">
              Ready to get started?
            </h2>
            <p className="text-xs sm:text-sm text-secondary">
              Upload your recorded sermon file or paste a YouTube link to get your transcript and
              clips in minutes.
            </p>
          </div>

          <div className="flex flex-col sm:flex-row items-center justify-center gap-3">
            <button
              onClick={() => handleStart("/upload")}
              className="group inline-flex items-center gap-3 px-8 py-3 rounded-full bg-accent hover:bg-accent-hover text-accent-fg font-sans text-sm font-semibold transition-all duration-200 shadow-sm active:scale-[0.98]"
            >
              <span>Upload a Sermon</span>
              <div className="w-6 h-6 rounded-full bg-white/20 flex items-center justify-center transition-transform duration-200 group-hover:translate-x-0.5">
                <i className="bx bx-upload text-sm text-accent-fg" />
              </div>
            </button>

            <button
              onClick={() => handleStart("/dashboard")}
              className="px-6 py-3 rounded-full bg-surface-elevated hover:bg-surface-hover border border-border text-secondary hover:text-primary text-sm font-medium transition-colors"
            >
              Open Sermon Library
            </button>
          </div>
        </div>
      </main>

      {/* ── Footer ──────────────────────────────────────────────────────── */}
      <footer className="w-full px-6 sm:px-12 py-6 border-t border-border/60 backdrop-blur-md bg-surface/20">
        <div className="max-w-4xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-3 text-xs text-muted">
          <p>© 2026 DABAR · Sermon Media Studio</p>
          <div className="flex items-center gap-4">
            <span className="font-mono text-[11px] text-accent">v0.2.0</span>
            <span className="text-border">|</span>
            <span>Simple, Fast & Private</span>
          </div>
        </div>
      </footer>
    </div>
  );
}
