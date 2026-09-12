import { useEffect, useState, useMemo, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  listSermons,
  createSermon,
  pickMediaFile,
  deleteSermon,
  onPipelineProgress,
} from "../lib/api.js";
import { cleanSermonTitle } from "../lib/formatters.js";
import Btn from "../components/Btn.jsx";

export default function Dashboard() {
  const [sermons, setSermons] = useState([]);
  const [isLoading, setIsLoading] = useState(true);
  const [filterQuery, setFilterQuery] = useState("");
  const [quickUrl, setQuickUrl] = useState("");
  const [isStartingQuick, setIsStartingQuick] = useState(false);
  const [deletingId, setDeletingId] = useState(null);
  const navigate = useNavigate();

  const fetchSermons = useCallback(async () => {
    try {
      const data = await listSermons();
      if (Array.isArray(data)) {
        setSermons(data);
      }
    } catch (err) {
      console.warn("Could not list sermons:", err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSermons();

    // Listen for live pipeline progress events to update statuses in real-time
    let unlisten = null;
    onPipelineProgress((event) => {
      if (event && event.sermon_id) {
        setSermons((prev) =>
          prev.map((s) => {
            if (s.id === event.sermon_id) {
              const stage = (event.stage || "").toLowerCase();
              let newStatus = s.status;
              if (stage === "ready" || event.is_complete) {
                newStatus = "ready";
              } else if (stage === "cancelled") {
                newStatus = "cancelled";
              } else if (stage === "failed" || event.is_error) {
                newStatus = "failed";
              } else if (stage) {
                newStatus = stage;
              }
              return { ...s, status: newStatus };
            }
            return s;
          })
        );
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (typeof unlisten === "function") unlisten();
    };
  }, [fetchSermons]);

  const totalClips = useMemo(() => {
    return sermons.reduce((acc, s) => {
      const count =
        (s.highlights && s.highlights.length) ||
        (s.chapters && s.chapters.length) ||
        s.clips_count ||
        0;
      return acc + count;
    }, 0);
  }, [sermons]);

  const filtered = useMemo(() => {
    if (!filterQuery.trim()) return sermons;
    const q = filterQuery.toLowerCase();
    return sermons.filter((s) => {
      const title = (s.title || "").toLowerCase();
      const speaker = (s.speaker || "").toLowerCase();
      return title.includes(q) || speaker.includes(q);
    });
  }, [sermons, filterQuery]);

  async function handleQuickSubmit(e) {
    e.preventDefault();
    if (!quickUrl.trim()) return;
    setIsStartingQuick(true);
    try {
      const res = await createSermon(quickUrl.trim());
      if (res?.id) {
        navigate(`/processing/${res.id}`);
      }
    } catch (err) {
      alert("Could not start sermon processing: " + (err.message || err));
    } finally {
      setIsStartingQuick(false);
    }
  }

  async function handlePickLocal() {
    try {
      const path = await pickMediaFile();
      if (path) {
        const res = await createSermon(path);
        if (res?.id) {
          navigate(`/processing/${res.id}`);
        }
      }
    } catch (err) {
      alert("File selection error: " + (err.message || err));
    }
  }

  async function handleDelete(sermonId, sermonTitle) {
    const clean = cleanSermonTitle(sermonTitle || "this sermon");
    if (!window.confirm(`Delete "${clean}" from your library?`)) return;
    setDeletingId(sermonId);
    try {
      await deleteSermon(sermonId);
      setSermons((prev) => prev.filter((s) => s.id !== sermonId));
    } catch (err) {
      alert("Could not delete sermon: " + (err.message || err));
    } finally {
      setDeletingId(null);
    }
  }

  return (
    <div className="space-y-7 animate-in fade-in duration-300">
      {/* ── Header ─────────────────────────────────────────────────── */}
      <section className="flex flex-col md:flex-row md:items-end justify-between gap-4 pt-2">
        <div className="space-y-1">
          <div className="flex items-center gap-2 text-xs text-secondary">
            <span>
              {sermons.length} {sermons.length === 1 ? "sermon" : "sermons"} in library
            </span>
          </div>
          <h1 className="font-editorial text-3xl sm:text-4xl font-bold tracking-tight text-primary">
            Sermon Library
          </h1>
          <p className="text-secondary text-xs sm:text-sm font-normal max-w-xl">
            Import sermons to automatically extract social media video clips, topic chapters, and
            full word-for-word transcripts.
          </p>
        </div>

        <div className="flex items-center gap-2.5 shrink-0">
          <Btn variant="secondary" icon="bx-folder" onClick={handlePickLocal}>
            Import File
          </Btn>
          <Btn variant="primary" icon="bx-plus" onClick={() => navigate("/upload")}>
            New Sermon
          </Btn>
        </div>
      </section>

      {/* ── Quick Ingestion & Summary Overview ───────────────────────── */}
      <section className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch">
        {/* Quick Dispatch Ingest Bar */}
        <div className="lg:col-span-8 studio-card p-5 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-xs text-primary font-semibold">
              <i className="bx bx-link text-accent text-base" />
              <span>Quick Transcribe</span>
            </div>
            <span className="text-[11px] text-muted">YouTube URL or Local File</span>
          </div>

          <form onSubmit={handleQuickSubmit} className="flex flex-col sm:flex-row gap-2">
            <div className="relative flex-1">
              <i className="bx bxl-youtube absolute left-3.5 top-1/2 -translate-y-1/2 text-red-500 text-lg" />
              <input
                type="text"
                value={quickUrl}
                onChange={(e) => setQuickUrl(e.target.value)}
                placeholder="Paste YouTube sermon link or file path…"
                className="w-full rounded-md bg-surface-elevated border border-border pl-10 pr-3 py-2 text-xs text-primary outline-none focus:border-accent transition-colors"
              />
            </div>
            <Btn
              type="submit"
              variant="primary"
              size="md"
              icon={isStartingQuick ? "bx-loader-alt bx-spin" : "bx-zap"}
              disabled={isStartingQuick || !quickUrl.trim()}
            >
              {isStartingQuick ? "Starting…" : "Transcribe"}
            </Btn>
          </form>
        </div>

        {/* Summary Stats */}
        <div className="lg:col-span-4 studio-card p-5 flex flex-col justify-between space-y-2">
          <span className="text-xs text-secondary font-medium">Library Summary</span>

          <div className="grid grid-cols-2 gap-3">
            <div className="p-2.5 rounded-md bg-surface-elevated border border-border">
              <span className="text-xs text-muted block">Sermons</span>
              <span className="text-xl font-bold text-primary font-editorial">
                {sermons.length}
              </span>
            </div>
            <div className="p-2.5 rounded-md bg-surface-elevated border border-border">
              <span className="text-xs text-muted block">Clips Created</span>
              <span className="text-xl font-bold text-accent font-editorial">
                {totalClips}
              </span>
            </div>
          </div>
        </div>
      </section>

      {/* ── Sermons List & Search Filter ────────────────────────────── */}
      <section className="space-y-4 pt-1">
        <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
          <h2 className="font-editorial text-xl font-bold text-primary">
            Sermon Recordings
          </h2>

          <div className="relative w-full sm:w-72">
            <i className="bx bx-search absolute left-3 top-1/2 -translate-y-1/2 text-muted text-sm" />
            <input
              type="text"
              value={filterQuery}
              onChange={(e) => setFilterQuery(e.target.value)}
              placeholder="Search by sermon title or speaker…"
              className="w-full rounded-md bg-surface border border-border pl-9 pr-3 py-1.5 text-xs text-primary placeholder:text-muted outline-none focus:border-accent transition-colors"
            />
          </div>
        </div>

        {/* Sermons Table */}
        {isLoading ? (
          <div className="studio-card py-20 text-center space-y-2.5">
            <i className="bx bx-loader-alt bx-spin text-2xl text-accent" />
            <p className="text-xs text-secondary">Loading sermon library…</p>
          </div>
        ) : filtered.length > 0 ? (
          <div className="studio-card overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs border-collapse">
                <thead>
                  <tr className="border-b border-border bg-surface-elevated/40 text-secondary">
                    <th className="py-3 px-4 font-semibold">Title</th>
                    <th className="py-3 px-4 font-semibold">Speaker</th>
                    <th className="py-3 px-4 font-semibold">Length</th>
                    <th className="py-3 px-4 font-semibold">Status</th>
                    <th className="py-3 px-4 font-semibold text-right">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border/60">
                  {filtered.map((sermon) => {
                    const cleanTitle = cleanSermonTitle(
                      sermon.title || "Untitled Sermon Recording"
                    );
                    const statusStr = (sermon.status || "").toLowerCase();
                    const isReady =
                      statusStr.includes("clip") ||
                      statusStr.includes("ready") ||
                      statusStr.includes("complete");
                    const isCancelled = statusStr.includes("cancel");
                    const isFailed =
                      statusStr.includes("fail") || statusStr.includes("error");
                    const chaptersCount =
                      sermon.highlights?.length ||
                      sermon.chapters?.length ||
                      sermon.clips_count ||
                      0;

                    return (
                      <tr
                        key={sermon.id}
                        className="hover:bg-surface-hover/50 transition-colors group"
                      >
                        {/* Title & Clips count */}
                        <td className="py-3.5 px-4">
                          <div className="space-y-0.5">
                            <button
                              type="button"
                              onClick={() =>
                                navigate(
                                  isReady
                                    ? `/clips/${sermon.id}`
                                    : `/processing/${sermon.id}`
                                )
                              }
                              className="text-left font-editorial text-base font-bold text-primary group-hover:text-accent transition-colors leading-snug"
                            >
                              {cleanTitle}
                            </button>
                            <div className="flex items-center gap-2 text-xs text-muted">
                              {sermon.date && <span>{sermon.date}</span>}
                              {chaptersCount > 0 && (
                                <>
                                  <span>·</span>
                                  <span className="text-accent font-medium">
                                    {chaptersCount} clips
                                  </span>
                                </>
                              )}
                            </div>
                          </div>
                        </td>

                        {/* Speaker */}
                        <td className="py-3.5 px-4 font-medium text-secondary">
                          {sermon.speaker || <span className="text-muted italic">—</span>}
                        </td>

                        {/* Duration */}
                        <td className="py-3.5 px-4 text-secondary">
                          {sermon.duration || "—"}
                        </td>

                        {/* Status Badge */}
                        <td className="py-3.5 px-4">
                          {isReady ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-success-muted text-success text-[11px] font-semibold">
                              <span className="w-1.5 h-1.5 rounded-full bg-success" />
                              Ready
                            </span>
                          ) : isCancelled ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-surface-elevated border border-border text-muted text-[11px] font-semibold">
                              <span className="w-1.5 h-1.5 rounded-full bg-muted" />
                              Cancelled
                            </span>
                          ) : isFailed ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-danger-muted text-danger text-[11px] font-semibold">
                              <span className="w-1.5 h-1.5 rounded-full bg-danger" />
                              Failed
                            </span>
                          ) : (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-accent-muted text-accent text-[11px] font-semibold">
                              <i className="bx bx-loader-alt bx-spin text-xs" />
                              Transcribing
                            </span>
                          )}
                        </td>

                        {/* Actions */}
                        <td className="py-3.5 px-4 text-right">
                          <div className="flex items-center justify-end gap-2">
                            {isReady ? (
                              <>
                                <button
                                  type="button"
                                  onClick={() => navigate(`/transcript/${sermon.id}`)}
                                  className="px-2.5 py-1 rounded bg-surface-elevated hover:bg-surface-hover text-xs font-semibold text-secondary hover:text-primary transition-colors border border-border"
                                >
                                  Transcript
                                </button>
                                <button
                                  type="button"
                                  onClick={() => navigate(`/clips/${sermon.id}`)}
                                  className="px-3 py-1 rounded btn-studio-primary font-semibold text-xs transition-colors flex items-center gap-1"
                                >
                                  <i className="bx bx-film text-xs" />
                                  <span>Clips</span>
                                </button>
                              </>
                            ) : isCancelled || isFailed ? (
                              <>
                                <button
                                  type="button"
                                  onClick={() => navigate(`/processing/${sermon.id}`)}
                                  className="px-2.5 py-1 rounded bg-surface-elevated text-xs text-secondary hover:text-primary transition-colors border border-border"
                                >
                                  Details
                                </button>
                                <button
                                  type="button"
                                  onClick={() => handleDelete(sermon.id, sermon.title)}
                                  disabled={deletingId === sermon.id}
                                  title="Delete from library"
                                  className="p-1.5 rounded hover:bg-danger/10 text-muted hover:text-danger transition-colors text-sm"
                                >
                                  <i
                                    className={`bx ${
                                      deletingId === sermon.id
                                        ? "bx-loader-alt bx-spin"
                                        : "bx-trash"
                                    }`}
                                  />
                                </button>
                              </>
                            ) : (
                              <>
                                <button
                                  type="button"
                                  onClick={() => navigate(`/processing/${sermon.id}`)}
                                  className="px-2.5 py-1 rounded bg-surface-elevated text-xs text-secondary hover:text-primary transition-colors border border-border"
                                >
                                  View Progress
                                </button>
                                <button
                                  type="button"
                                  onClick={() => handleDelete(sermon.id, sermon.title)}
                                  disabled={deletingId === sermon.id}
                                  title="Delete from library"
                                  className="p-1.5 rounded hover:bg-danger/10 text-muted hover:text-danger transition-colors text-sm"
                                >
                                  <i
                                    className={`bx ${
                                      deletingId === sermon.id
                                        ? "bx-loader-alt bx-spin"
                                        : "bx-trash"
                                    }`}
                                  />
                                </button>
                              </>
                            )}
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        ) : (
          <div className="studio-card py-16 text-center space-y-3">
            <div className="w-10 h-10 rounded-full bg-surface-elevated text-accent flex items-center justify-center mx-auto text-xl border border-border">
              <i className="bx bx-film" />
            </div>
            <div className="space-y-1">
              <h3 className="font-editorial text-xl font-bold text-primary">
                No sermon recordings found
              </h3>
              <p className="text-xs text-secondary max-w-sm mx-auto">
                {filterQuery
                  ? "No recordings match your filter search."
                  : "Import an audio or video sermon recording to extract clips and transcripts."}
              </p>
            </div>
            {!filterQuery && (
              <div className="pt-2">
                <Btn variant="primary" icon="bx-plus" onClick={() => navigate("/upload")}>
                  Add First Sermon
                </Btn>
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
