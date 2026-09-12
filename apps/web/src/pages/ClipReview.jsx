import { useEffect, useMemo, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  listSermons,
  getSermon,
  renderClip,
  renderClipRange,
  retryHighlights,
  openInExplorer,
  getAssetUrl,
  checkDependencies,
} from "../lib/api.js";
import ClipCard from "../components/ClipCard.jsx";
import ExportModal from "../components/ExportModal.jsx";
import ManuscriptView from "../components/ManuscriptView.jsx";
import Btn from "../components/Btn.jsx";

function formatSeconds(secs) {
  if (!secs && secs !== 0) return "0:00";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function extractVideoId(url) {
  if (!url) return null;
  const match = url.match(/(?:youtu\.be\/|youtube\.com\/(?:watch\?v=|embed\/|shorts\/))([a-zA-Z0-9_-]{11})/);
  return match ? match[1] : null;
}

export default function ClipReview() {
  const { sermonId } = useParams();
  const navigate = useNavigate();
  const [currentSermonId, setCurrentSermonId] = useState(sermonId || "");
  const [activeTab, setActiveTab] = useState("clips"); // "clips" | "transcript"
  const [segments, setSegments] = useState([]);
  const [highlights, setHighlights] = useState([]);
  const [sermonUrl, setSermonUrl] = useState("");
  const [mediaAssetUrl, setMediaAssetUrl] = useState(null);
  const [isVideoFile, setIsVideoFile] = useState(false);
  const [sermonTitle, setSermonTitle] = useState("");
  const [sermonDuration, setSermonDuration] = useState("");
  const [scriptureRefs, setScriptureRefs] = useState([]);
  const [isLoading, setIsLoading] = useState(true);
  const [activeSegment, setActiveSegment] = useState(null);
  const [exportModalClip, setExportModalClip] = useState(null);
  const [renderingClipId, setRenderingClipId] = useState(null);
  const [renderError, setRenderError] = useState(null);
  const [exportedNotice, setExportedNotice] = useState(null);
  const [searchTerm, setSearchTerm] = useState("");
  const [playbackTime, setPlaybackTime] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [deps, setDeps] = useState(null);
  const [isRetryingHighlights, setIsRetryingHighlights] = useState(false);
  const [customRange, setCustomRange] = useState({ start: 0, end: 60, title: "Custom Moment" });
  const [showCustomRangeModal, setShowCustomRangeModal] = useState(false);
  const [clipRange, setClipRange] = useState({ start: null, end: null });

  useEffect(() => {
    let mounted = true;
    setIsLoading(true);

    checkDependencies().then((d) => {
      if (mounted && d) setDeps(d);
    });

    async function loadData() {
      try {
        let targetId = sermonId;

        if (!targetId) {
          const sermons = await listSermons();
          if (sermons && sermons.length > 0) {
            targetId = sermons[0].id;
          }
        }

        if (!targetId) {
          if (mounted) setIsLoading(false);
          return;
        }

        setCurrentSermonId(targetId);
        const sermon = await getSermon(targetId);
        if (!mounted || !sermon) {
          if (mounted) setIsLoading(false);
          return;
        }

        let displayTitle = sermon.title || "Sunday Sermon";
        if (displayTitle.match(/\.(mp4|mp3|wav|m4a|mov|mkv)$/i)) {
          displayTitle = displayTitle.replace(/\.(mp4|mp3|wav|m4a|mov|mkv)$/i, "").replace(/[-_]/g, " ");
        }
        setSermonTitle(displayTitle);
        setSermonUrl(sermon.youtube_url || "");
        setScriptureRefs(sermon.scripture_references || []);

        const sourcePath = sermon.audio_path || sermon.youtube_url || "";
        const isVideo = Boolean(
          sourcePath.match(/\.(mp4|mov|mkv|webm|avi)$/i) || extractVideoId(sermon.youtube_url)
        );
        setIsVideoFile(isVideo);

        if (sourcePath && !sourcePath.startsWith("http://") && !sourcePath.startsWith("https://")) {
          getAssetUrl(sourcePath).then((assetUrl) => {
            if (mounted) setMediaAssetUrl(assetUrl);
          });
        } else if (sourcePath.startsWith("http://") || sourcePath.startsWith("https://")) {
          setMediaAssetUrl(sourcePath);
        }

        const hlList = Array.isArray(sermon.highlights) ? sermon.highlights : [];
        const rawSegs = Array.isArray(sermon.transcript_segments) ? sermon.transcript_segments : [];

        const structuredHls = hlList.map((hl) => ({
          id: hl.id,
          start: hl.start_time,
          end: hl.end_time,
          score: hl.score,
          title: hl.title,
          highlight_title: hl.title,
          why: hl.reason || hl.suggested_hook_text || "Key preaching moment",
          text: hl.reason || hl.title,
          duration: `${formatSeconds(hl.start_time)} – ${formatSeconds(hl.end_time)}`,
          is_highlight: true,
        }));

        const transcriptItems = rawSegs.map((seg, idx) => {
          const matchingHl = hlList.find(
            (hl) => seg.start >= hl.start_time - 0.5 && seg.end <= hl.end_time + 0.5
          );
          return {
            id: matchingHl ? matchingHl.id : `seg-${idx}`,
            start: seg.start,
            end: seg.end,
            text: seg.text,
            is_highlight: Boolean(matchingHl),
            highlight_title: matchingHl ? matchingHl.title : null,
            highlight_reason: matchingHl ? matchingHl.reason : null,
          };
        });

        setHighlights(structuredHls);
        setSegments(transcriptItems);
        if (transcriptItems.length > 0) {
          const lastSeg = transcriptItems[transcriptItems.length - 1];
          setSermonDuration(formatSeconds(lastSeg.end));
        }
      } catch (err) {
        console.warn("Failed to load sermon studio data:", err);
      } finally {
        if (mounted) setIsLoading(false);
      }
    }

    loadData();
    return () => { mounted = false; };
  }, [sermonId]);

  const videoId = extractVideoId(sermonUrl);

  const topMoment = useMemo(() => {
    if (highlights.length === 0) return null;
    return highlights[0];
  }, [highlights]);

  const otherMoments = useMemo(() => {
    if (highlights.length <= 1) return [];
    return highlights.slice(1);
  }, [highlights]);

  const filteredSegments = useMemo(() => {
    if (!searchTerm.trim()) return segments;
    const q = searchTerm.toLowerCase();
    return segments.filter((s) => s.text.toLowerCase().includes(q));
  }, [segments, searchTerm]);

  function handleUpdateSegmentText(idx, newText) {
    setSegments((prev) => {
      const updated = [...prev];
      if (updated[idx]) {
        updated[idx] = { ...updated[idx], text: newText };
      }
      return updated;
    });
  }

  const totalDurationSecs = useMemo(() => {
    if (segments.length > 0) {
      const last = segments[segments.length - 1];
      if (last?.end) return Math.max(1, last.end);
    }
    if (sermonDuration && sermonDuration.includes(":")) {
      const parts = sermonDuration.split(":").map(Number);
      if (parts.length === 2) return parts[0] * 60 + parts[1];
      if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
    }
    return 2700;
  }, [segments, sermonDuration]);

  function handleNudgeClipBoundary(clip, boundary, delta) {
    if (!clip) return;
    setHighlights((prev) =>
      prev.map((item) => {
        if (item.id !== clip.id) return item;
        let newStart = item.start;
        let newEnd = item.end;
        if (boundary === "start") {
          newStart = Math.max(0, item.start + delta);
          if (newStart >= newEnd) newStart = Math.max(0, newEnd - 1);
        } else if (boundary === "end") {
          newEnd = Math.max(newStart + 1, item.end + delta);
        }
        const updated = {
          ...item,
          start: newStart,
          end: newEnd,
          duration: `${formatSeconds(newStart)} – ${formatSeconds(newEnd)}`,
        };
        if (activeSegment?.id === item.id) {
          setActiveSegment(updated);
        }
        if (exportModalClip?.id === item.id) {
          setExportModalClip(updated);
        }
        return updated;
      })
    );
  }

  function handleSetRangeStart(time) {
    setClipRange((prev) => {
      const newStart = Math.max(0, time);
      const newEnd = prev.end !== null && prev.end <= newStart ? null : prev.end;
      return { start: newStart, end: newEnd };
    });
  }

  function handleSetRangeEnd(time) {
    setClipRange((prev) => {
      const newEnd = Math.max(0, time);
      const newStart = prev.start !== null ? prev.start : 0;
      if (newEnd > newStart) {
        setCustomRange({
          start: newStart,
          end: newEnd,
          title: `${sermonTitle || "Sermon"} (${formatSeconds(newStart)} - ${formatSeconds(newEnd)})`,
        });
      }
      return { start: newStart, end: newEnd };
    });
  }

  async function handleConfirmExport(clip, format, captionStyle, fileName) {
    if (!currentSermonId || !clip) return;
    const clipKey = clip.id || `clip-${clip.start}`;
    setRenderingClipId(clipKey);
    setRenderError(null);

    try {
      const outputPath = await renderClip(
        currentSermonId,
        clip.id,
        clip.start,
        clip.end,
        fileName || clip.highlight_title || clip.title,
        format,
        captionStyle
      );

      setExportedNotice({
        title: clip.highlight_title || clip.title || fileName || "Clip",
        path: outputPath,
      });
    } catch (err) {
      const msg = err?.message || String(err);
      setRenderError(msg);
    } finally {
      setRenderingClipId(null);
    }
  }

  async function handleConfirmCustomRangeExport() {
    if (!currentSermonId) return;
    setRenderingClipId("custom-range");
    setRenderError(null);
    try {
      const outputPath = await renderClipRange(
        currentSermonId,
        customRange.start,
        customRange.end,
        customRange.title,
        customRange.aspectRatio || "9:16",
        customRange.captionStyle || "amber"
      );
      setExportedNotice({
        title: customRange.title || "Custom Clip",
        path: outputPath,
      });
      setShowCustomRangeModal(false);
    } catch (err) {
      const msg = err?.message || String(err);
      setRenderError(msg);
    } finally {
      setRenderingClipId(null);
    }
  }

  async function handleRetryMoments() {
    if (!currentSermonId) return;
    setIsRetryingHighlights(true);
    try {
      const newHls = await retryHighlights(currentSermonId);
      if (newHls && newHls.length > 0) {
        const formatted = newHls.map((hl) => ({
          id: hl.id,
          start: hl.start_time,
          end: hl.end_time,
          score: hl.score,
          title: hl.title,
          highlight_title: hl.title,
          why: hl.reason || hl.suggested_hook_text || "Preaching moment",
          text: hl.reason || hl.title,
          duration: `${formatSeconds(hl.start_time)} – ${formatSeconds(hl.end_time)}`,
          is_highlight: true,
        }));
        setHighlights(formatted);
      }
    } catch (err) {
      alert("Could not generate moments: " + (err.message || err));
    } finally {
      setIsRetryingHighlights(false);
    }
  }

  function handleSeek(time) {
    setPlaybackTime(time);
    setIsPlaying(true);
  }

  function handleTogglePlay() {
    setIsPlaying((prev) => !prev);
  }

  return (
    <div className="space-y-6 pb-32 animate-in fade-in duration-300">
      {/* ── Header ─────────────────────────────────────────────────── */}
      <section className="flex flex-col md:flex-row md:items-end justify-between gap-4 pt-2">
        <div className="space-y-1">
          <div className="flex items-center gap-2 text-xs text-secondary">
            {sermonDuration && <span>{sermonDuration}</span>}
            {scriptureRefs.length > 0 && (
              <>
                <span>·</span>
                <span className="text-accent font-medium">{scriptureRefs.join(", ")}</span>
              </>
            )}
          </div>

          <h1 className="font-editorial text-2xl sm:text-3xl font-bold tracking-tight text-primary leading-tight">
            {sermonTitle || "Sermon Clips & Transcript"}
          </h1>
          <p className="text-secondary text-xs sm:text-sm font-normal">
            {highlights.length} key moments found · Ready to share on social media.
          </p>
        </div>

        <div className="flex items-center gap-2 shrink-0 flex-wrap">
          <Btn variant="secondary" icon="bx-time-five" onClick={() => setShowCustomRangeModal(true)}>
            Custom Clip
          </Btn>
          <Btn variant="secondary" icon="bx-arrow-back" onClick={() => navigate("/dashboard")}>
            Library
          </Btn>
          <Btn variant="primary" icon="bx-plus" onClick={() => navigate("/upload")}>
            New Sermon
          </Btn>
        </div>
      </section>

      {/* ── Dependency Alert Banner if FFmpeg is Missing ───────────── */}
      {deps && !deps?.ffmpeg?.found && (
        <div className="p-3.5 rounded-lg border border-accent/40 bg-accent-muted/20 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-xs">
          <div className="flex items-center gap-2.5">
            <i className="bx bx-info-circle text-accent text-lg" />
            <div>
              <p className="font-semibold text-primary">Video Tools Required for Export</p>
              <p className="text-xs text-muted">Install the video engine in settings to export finished video clips.</p>
            </div>
          </div>
          <Btn size="xs" variant="primary" icon="bx-download" onClick={() => navigate("/settings")}>
            Install in Settings
          </Btn>
        </div>
      )}

      {/* ── Export Error Notification Banner ────────────────────────── */}
      {renderError && !exportModalClip && (
        <div className="studio-card p-4 border-danger/40 bg-danger-muted flex items-center justify-between gap-4 text-xs">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-full bg-danger text-white flex items-center justify-center text-lg shrink-0">
              <i className="bx bx-error" />
            </div>
            <div>
              <p className="font-semibold text-primary">Video export failed</p>
              <p className="text-xs text-secondary leading-relaxed max-w-xl">{renderError}</p>
            </div>
          </div>
          <button onClick={() => setRenderError(null)} className="text-muted hover:text-primary p-1">
            <i className="bx bx-x text-xl" />
          </button>
        </div>
      )}

      {/* ── Export Success Notification ─────────────────────────────── */}
      {exportedNotice && (
        <div className="studio-card p-4 border-success/40 bg-success-muted flex items-center justify-between gap-4 text-xs">
          <div className="flex items-center gap-3 min-w-0">
            <div className="w-8 h-8 rounded-full bg-success text-white flex items-center justify-center text-lg shrink-0">
              <i className="bx bxs-check-circle" />
            </div>
            <div className="min-w-0">
              <p className="font-semibold text-primary">
                "{exportedNotice.title}" is ready
              </p>
              <p className="text-[11px] text-muted truncate max-w-md">
                {exportedNotice.path}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Btn
              size="xs"
              variant="primary"
              icon="bx-folder-open"
              onClick={() => openInExplorer(exportedNotice.path)}
            >
              Open in Folder
            </Btn>
            <button
              onClick={() => setExportedNotice(null)}
              className="text-muted hover:text-primary p-1"
            >
              <i className="bx bx-x text-xl" />
            </button>
          </div>
        </div>
      )}

      {/* ── Mode Switcher Tabs ────────────────────────────────────────── */}
      <div className="flex items-center justify-start">
        <div className="flex p-1 rounded-lg bg-surface border border-border text-xs font-semibold">
          <button
            type="button"
            onClick={() => setActiveTab("clips")}
            className={`px-4 py-1.5 rounded-md transition-all flex items-center gap-1.5 ${
              activeTab === "clips"
                ? "bg-accent text-accent-fg shadow-xs"
                : "text-secondary hover:text-primary hover:bg-surface-hover"
            }`}
          >
            <i className="bx bx-film text-sm" />
            <span>Sermon Clips ({highlights.length})</span>
          </button>

          <button
            type="button"
            onClick={() => setActiveTab("transcript")}
            className={`px-4 py-1.5 rounded-md transition-all flex items-center gap-1.5 ${
              activeTab === "transcript"
                ? "bg-accent text-accent-fg shadow-xs"
                : "text-secondary hover:text-primary hover:bg-surface-hover"
            }`}
          >
            <i className="bx bx-book-open text-sm" />
            <span>Full Transcript</span>
          </button>
        </div>
      </div>

      {/* ── Studio Content ────────────────────────────────────────────── */}
      {isLoading ? (
        <div className="studio-card py-20 text-center space-y-2.5">
          <i className="bx bx-loader-alt bx-spin text-3xl text-accent" />
          <p className="font-editorial text-lg text-secondary">
            Loading sermon clips…
          </p>
        </div>
      ) : activeTab === "clips" ? (
        <div className="space-y-6">
          {/* Empty Moments Fallback */}
          {highlights.length === 0 && (
            <div className="studio-card p-8 text-center space-y-4 max-w-lg mx-auto">
              <div className="w-12 h-12 rounded-full bg-surface-elevated border border-border text-accent flex items-center justify-center mx-auto text-2xl">
                <i className="bx bx-film" />
              </div>
              <div className="space-y-1">
                <h3 className="font-editorial text-lg font-bold text-primary">No Moments Extracted Yet</h3>
                <p className="text-xs text-secondary max-w-md mx-auto leading-relaxed">
                  You can analyze this sermon to automatically find teaching moments, or create custom clips from the transcript.
                </p>
              </div>
              <div className="flex items-center justify-center gap-2 pt-2 flex-wrap">
                <Btn
                  size="sm"
                  variant="primary"
                  icon={isRetryingHighlights ? "bx-loader-alt bx-spin" : "bx-brain"}
                  disabled={isRetryingHighlights}
                  onClick={handleRetryMoments}
                >
                  {isRetryingHighlights ? "Finding Moments…" : "Find Key Moments"}
                </Btn>
                <Btn
                  size="sm"
                  variant="secondary"
                  icon="bx-time"
                  onClick={() => setShowCustomRangeModal(true)}
                >
                  Cut Custom Clip
                </Btn>
              </div>
            </div>
          )}

          {/* Featured Top Moment */}
          {topMoment && (
            <section className="space-y-2">
              <ClipCard
                clip={topMoment}
                featured={true}
                onPreview={(c) => setActiveSegment(c)}
                onExport={(c) => setExportModalClip(c)}
                onNudge={handleNudgeClipBoundary}
                isExporting={renderingClipId === topMoment.id}
              />
            </section>
          )}

          {/* Secondary Moments Grid */}
          {otherMoments.length > 0 && (
            <section className="space-y-3">
              <h2 className="font-editorial text-xl font-bold text-primary">
                More Preaching Moments ({otherMoments.length})
              </h2>

              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                {otherMoments.map((moment) => (
                  <ClipCard
                    key={moment.id}
                    clip={moment}
                    onPreview={(c) => setActiveSegment(c)}
                    onExport={(c) => setExportModalClip(c)}
                    onNudge={handleNudgeClipBoundary}
                    isExporting={renderingClipId === moment.id}
                  />
                ))}
              </div>
            </section>
          )}

          {/* Active Moment In-Page Player */}
          {activeSegment && (
            <div className="studio-card p-5 space-y-4 border-accent/40 animate-in fade-in duration-200">
              <div className="flex items-center justify-between border-b border-border pb-3">
                <div className="flex items-center gap-2.5 min-w-0">
                  <h3 className="font-editorial text-base font-bold text-primary truncate max-w-md">
                    {activeSegment.highlight_title || activeSegment.title || "Selected Moment"}
                  </h3>
                  <span className="text-xs text-muted">
                    ({formatSeconds(activeSegment.start)} – {formatSeconds(activeSegment.end)})
                  </span>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  <Btn
                    size="sm"
                    variant="primary"
                    icon="bx-film"
                    onClick={() => setExportModalClip(activeSegment)}
                  >
                    Export Video
                  </Btn>
                  <button
                    onClick={() => setActiveSegment(null)}
                    className="w-7 h-7 rounded-md bg-surface-elevated text-muted hover:text-primary flex items-center justify-center transition-colors"
                    aria-label="Close preview"
                  >
                    <i className="bx bx-x text-xl" />
                  </button>
                </div>
              </div>

              {videoId ? (
                /* YouTube Video Player */
                <div className="aspect-video w-full rounded-lg overflow-hidden border border-border bg-black">
                  <iframe
                    src={`https://www.youtube.com/embed/${videoId}?start=${Math.floor(activeSegment.start)}&end=${Math.ceil(activeSegment.end)}&autoplay=1&rel=0`}
                    title="Clip preview player"
                    className="h-full w-full"
                    allow="autoplay; encrypted-media"
                    allowFullScreen
                  />
                </div>
              ) : mediaAssetUrl && isVideoFile ? (
                /* Local Video Player */
                <div className="w-full rounded-lg overflow-hidden border border-border bg-black p-4 space-y-3">
                  <video
                    controls
                    autoPlay
                    src={mediaAssetUrl}
                    className="w-full max-h-80 rounded-md mx-auto bg-black"
                    onLoadedMetadata={(e) => {
                      e.target.currentTime = activeSegment.start || 0;
                    }}
                    onTimeUpdate={(e) => {
                      if (activeSegment.end && e.target.currentTime >= activeSegment.end) {
                        e.target.pause();
                        e.target.currentTime = activeSegment.start || 0;
                      }
                    }}
                  />
                  <div className="flex items-center justify-between text-xs text-secondary font-editorial italic px-1">
                    <p>"{activeSegment.why || activeSegment.text || "Preaching moment"}"</p>
                    <span className="text-accent font-semibold shrink-0">
                      Duration: {Math.round((activeSegment.end || 0) - (activeSegment.start || 0))}s
                    </span>
                  </div>
                </div>
              ) : (
                /* Audio-only Preaching Quote Card */
                <div className="p-6 rounded-lg bg-base border border-border space-y-4 text-center">
                  <div className="w-10 h-10 rounded-full bg-accent/20 text-accent flex items-center justify-center mx-auto text-xl">
                    <i className="bx bx-volume-full" />
                  </div>
                  <div className="max-w-md mx-auto space-y-1">
                    <p className="font-editorial text-base font-bold text-primary">
                      "{activeSegment.why || activeSegment.text || activeSegment.highlight_title}"
                    </p>
                    <p className="text-xs text-muted">
                      Audio Moment · {Math.round((activeSegment.end || 0) - (activeSegment.start || 0))}s duration
                    </p>
                  </div>
                  {mediaAssetUrl && (
                    <audio
                      controls
                      autoPlay
                      src={mediaAssetUrl}
                      className="w-full max-w-sm mx-auto h-8"
                      onLoadedMetadata={(e) => {
                        e.target.currentTime = activeSegment.start || 0;
                      }}
                    />
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      ) : (
        /* ── Transcript View ───────────────────────────────────────── */
        <div className="space-y-4">
          <div className="relative w-full max-w-md">
            <i className="bx bx-search absolute left-3 top-1/2 -translate-y-1/2 text-muted text-sm" />
            <input
              type="text"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              placeholder="Search sermon words, Scripture citations, or topics…"
              className="w-full rounded-md bg-surface border border-border pl-9 pr-3 py-1.5 text-xs text-primary placeholder:text-muted outline-none focus:border-accent transition-colors"
            />
          </div>

          <div className="studio-card p-4 sm:p-8">
            <ManuscriptView
              segments={filteredSegments}
              currentTime={playbackTime}
              isPlaying={isPlaying}
              clipRange={clipRange}
              selectionMode={true}
              onSetRangeStart={handleSetRangeStart}
              onSetRangeEnd={handleSetRangeEnd}
              onSeek={handleSeek}
              onTogglePlay={handleTogglePlay}
              onUpdateSegmentText={handleUpdateSegmentText}
            />
          </div>
        </div>
      )}

      {/* ── Fixed Bottom Audio Scrubber Dock ─────────────────────────── */}
      <div className="fixed bottom-4 left-1/2 -translate-x-1/2 z-40 bg-surface/95 backdrop-blur-md border border-border shadow-md rounded-lg px-4 py-2.5 flex items-center gap-4 text-xs font-sans">
        <button
          type="button"
          onClick={handleTogglePlay}
          className="w-8 h-8 rounded-md btn-studio-primary flex items-center justify-center transition-all active:scale-95 shrink-0"
          aria-label={isPlaying ? "Pause audio" : "Play audio"}
        >
          <i className={`bx ${isPlaying ? "bx-pause" : "bx-play"} text-xl`} />
        </button>

        <div className="flex items-center gap-2 font-semibold text-xs text-primary">
          <div className="audio-equalizer mr-0.5">
            <span className="audio-equalizer-bar" />
            <span className="audio-equalizer-bar" />
            <span className="audio-equalizer-bar" />
          </div>
          <span>{formatSeconds(playbackTime)}</span>
          <span className="text-muted">/</span>
          <span className="text-muted">{sermonDuration || "45:00"}</span>
        </div>

        <div
          className="w-32 sm:w-48 h-1.5 bg-surface-elevated rounded-full overflow-hidden cursor-pointer border border-border"
          onClick={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const ratio = clickX / rect.width;
            handleSeek(ratio * totalDurationSecs);
          }}
        >
          <div
            className="h-full bg-accent rounded-full transition-all"
            style={{ width: `${Math.min(100, (playbackTime / totalDurationSecs) * 100)}%` }}
          />
        </div>

        <span className="text-[11px] text-muted hidden sm:inline">
          <kbd className="px-1.5 py-0.5 rounded bg-surface-elevated border border-border text-[9px] text-primary">Space</kbd> Play
        </span>
      </div>

      {/* ── Custom Range Clip Modal ───────────────────────────────────── */}
      {showCustomRangeModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md animate-in fade-in duration-200">
          <div className="studio-card-elevated max-w-md w-full p-6 space-y-4 bg-surface border-border">
            <div className="flex items-center justify-between border-b border-border pb-3">
              <h3 className="font-editorial text-lg font-bold text-primary">Cut Custom Clip</h3>
              <button
                type="button"
                onClick={() => setShowCustomRangeModal(false)}
                className="text-muted hover:text-primary"
              >
                <i className="bx bx-x text-xl" />
              </button>
            </div>

            <div className="space-y-3 text-xs">
              <div className="space-y-1">
                <label className="font-semibold text-primary block">Clip Title</label>
                <input
                  type="text"
                  value={customRange.title}
                  onChange={(e) => setCustomRange({ ...customRange, title: e.target.value })}
                  className="w-full rounded-md bg-surface-elevated border border-border px-3 py-1.5 text-xs text-primary outline-none focus:border-accent"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <label className="font-semibold text-primary block">Start Time (seconds)</label>
                  <input
                    type="number"
                    min="0"
                    step="1"
                    value={customRange.start}
                    onChange={(e) => setCustomRange({ ...customRange, start: parseFloat(e.target.value) || 0 })}
                    className="w-full rounded-md bg-surface-elevated border border-border px-3 py-1.5 text-xs text-primary outline-none focus:border-accent"
                  />
                  <span className="text-[10px] text-muted">{formatSeconds(customRange.start)}</span>
                </div>

                <div className="space-y-1">
                  <label className="font-semibold text-primary block">End Time (seconds)</label>
                  <input
                    type="number"
                    min="1"
                    step="1"
                    value={customRange.end}
                    onChange={(e) => setCustomRange({ ...customRange, end: parseFloat(e.target.value) || 0 })}
                    className="w-full rounded-md bg-surface-elevated border border-border px-3 py-1.5 text-xs text-primary outline-none focus:border-accent"
                  />
                  <span className="text-[10px] text-muted">{formatSeconds(customRange.end)}</span>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end gap-2 pt-2">
              <Btn size="sm" variant="secondary" onClick={() => setShowCustomRangeModal(false)}>
                Cancel
              </Btn>
              <Btn
                size="sm"
                variant="primary"
                icon={renderingClipId === "custom-range" ? "bx-loader-alt bx-spin" : "bx-film"}
                disabled={renderingClipId === "custom-range"}
                onClick={handleConfirmCustomRangeExport}
              >
                {renderingClipId === "custom-range" ? "Creating…" : "Create Video Clip"}
              </Btn>
            </div>
          </div>
        </div>
      )}

      {/* ── Export Video Modal ────────────────────────────────────────── */}
      {exportModalClip && (
        <ExportModal
          clip={exportModalClip}
          sermonTitle={sermonTitle}
          videoId={videoId}
          mediaAssetUrl={mediaAssetUrl}
          onClose={() => setExportModalClip(null)}
          onConfirmExport={handleConfirmExport}
          isRendering={Boolean(renderingClipId)}
          exportedPath={exportedNotice?.path}
          renderError={renderError}
        />
      )}
    </div>
  );
}
