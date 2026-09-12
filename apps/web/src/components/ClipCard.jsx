import Btn from "./Btn.jsx";

function formatClipTime(seconds) {
  if (typeof seconds !== "number" || isNaN(seconds)) return "00:00";
  const safe = Math.max(0, seconds);
  const m = Math.floor(safe / 60);
  const s = Math.floor(safe % 60);
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function friendlyDuration(durationStr) {
  if (!durationStr) return "Clip";
  if (durationStr.includes("–") || durationStr.includes("-")) {
    const parts = durationStr.split(/[-–]/).map((s) => s.trim());
    if (parts.length === 2) {
      const [startM, startS] = parts[0].split(":").map(Number);
      const [endM, endS] = parts[1].split(":").map(Number);
      if (!isNaN(startM) && !isNaN(startS) && !isNaN(endM) && !isNaN(endS)) {
        const totalSecs = endM * 60 + endS - (startM * 60 + startS);
        if (totalSecs < 60) return `${totalSecs}s`;
        const m = Math.floor(totalSecs / 60);
        const s = totalSecs % 60;
        return s > 0 ? `${m}m ${s}s` : `${m}m`;
      }
    }
  }
  return durationStr;
}

export default function ClipCard({
  clip,
  onPreview,
  onExport,
  onNudge,
  isExporting,
  featured = false,
}) {
  const durationLabel = friendlyDuration(clip.duration);
  const canNudge = Boolean(onNudge && typeof clip.start === "number" && typeof clip.end === "number");

  const boundaryNudges = canNudge ? (
    <div className="grid grid-cols-2 gap-2 text-[11px] font-mono">
      {[
        { key: "start", label: "Start", value: clip.start },
        { key: "end", label: "End", value: clip.end },
      ].map((boundary) => (
        <div
          key={boundary.key}
          className="rounded-md border border-border bg-surface-elevated px-2 py-1.5 space-y-1"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-muted">{boundary.label}</span>
            <span className="font-semibold text-primary">
              {formatClipTime(boundary.value)}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-1">
            <button
              type="button"
              onClick={() => onNudge(clip, boundary.key, -1)}
              className="h-6 rounded border border-border bg-surface hover:bg-surface-hover text-primary transition-colors"
              aria-label={`Move ${boundary.label.toLowerCase()} earlier by 1 second`}
              title={`Move ${boundary.label.toLowerCase()} earlier by 1 second`}
            >
              -1s
            </button>
            <button
              type="button"
              onClick={() => onNudge(clip, boundary.key, 1)}
              className="h-6 rounded border border-border bg-surface hover:bg-surface-hover text-primary transition-colors"
              aria-label={`Move ${boundary.label.toLowerCase()} later by 1 second`}
              title={`Move ${boundary.label.toLowerCase()} later by 1 second`}
            >
              +1s
            </button>
          </div>
        </div>
      ))}
    </div>
  ) : null;

  if (featured) {
    return (
      <div className="studio-card-elevated p-5 sm:p-6 space-y-4 border-accent/30 bg-surface">
        {/* Header Meta */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <span className="scripture-badge text-[10.5px]">
              <i className="bx bx-star text-xs" />
              Featured Clip
            </span>
            <span className="meta-chip text-[11px]">
              9:16 Vertical Master
            </span>
          </div>

          <div className="flex items-center gap-1.5 font-mono text-xs text-secondary bg-surface-elevated px-2.5 py-1 rounded-md border border-border">
            <i className="bx bx-time-five text-accent text-sm" />
            <span className="font-semibold text-primary">{durationLabel}</span>
            <span className="text-muted">({clip.duration})</span>
          </div>
        </div>

        {/* Title */}
        <h3 className="font-editorial text-2xl sm:text-3xl font-bold text-primary leading-tight">
          {clip.title}
        </h3>

        {/* Quote Excerpt */}
        {clip.why && (
          <div className="p-3.5 rounded-lg bg-surface-elevated/70 border border-border/70 space-y-1.5">
            <div className="flex items-center gap-1.5 text-[10.5px] uppercase font-mono text-accent font-semibold tracking-wider">
              <i className="bx bxs-quote-left text-xs" />
              <span>Transcript Excerpt</span>
            </div>
            <p className="font-editorial text-base text-primary/90 italic leading-relaxed">
              "{clip.why}"
            </p>
          </div>
        )}

        {boundaryNudges}

        {/* Footer CTAs */}
        <div className="flex flex-wrap items-center justify-between gap-3 pt-3 border-t border-border">
          <div className="flex items-center gap-2 text-xs text-muted font-mono">
            <span className="w-2 h-2 rounded-full bg-accent animate-pulse" />
            <span>Ready for 9:16 vertical render</span>
          </div>

          <div className="flex items-center gap-2.5">
            {onPreview && (
              <Btn
                size="md"
                variant="secondary"
                icon="bx-play"
                onClick={() => onPreview(clip)}
              >
                Preview Clip
              </Btn>
            )}
            {onExport && (
              <Btn
                size="md"
                variant="primary"
                icon={isExporting ? "bx-loader-alt bx-spin" : "bx-film"}
                onClick={() => onExport(clip)}
                disabled={isExporting}
              >
                {isExporting ? "Rendering…" : "Export Video"}
              </Btn>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="studio-card p-4 sm:p-5 flex flex-col justify-between h-full space-y-3.5 group hover:border-border-strong transition-all duration-200">
      <div className="space-y-2.5">
        {/* Top Tag & Duration */}
        <div className="flex items-center justify-between text-xs">
          <span className="meta-chip text-[10.5px]">
            9:16 Reel
          </span>
          <span className="font-mono text-xs text-muted font-medium">
            {durationLabel}
          </span>
        </div>

        {/* Title */}
        <h4 className="font-editorial text-base font-bold text-primary group-hover:text-accent transition-colors duration-200 line-clamp-2 leading-snug">
          {clip.title}
        </h4>

        {/* Excerpt */}
        {clip.why && (
          <p className="font-editorial text-xs text-secondary italic line-clamp-3 leading-relaxed border-l-2 border-accent/40 pl-2.5">
            "{clip.why}"
          </p>
        )}

        {boundaryNudges}
      </div>

      {/* Action Buttons */}
      <div className="pt-2.5 border-t border-border flex items-center gap-2">
        {onPreview && (
          <button
            type="button"
            onClick={() => onPreview(clip)}
            className="flex-1 py-1.5 px-3 rounded-md bg-surface-elevated hover:bg-surface-hover text-xs font-semibold text-primary border border-border transition-all flex items-center justify-center gap-1.5 active:scale-95"
          >
            <i className="bx bx-play text-sm text-accent" />
            <span>Preview</span>
          </button>
        )}
        {onExport && (
          <button
            type="button"
            onClick={() => onExport(clip)}
            disabled={isExporting}
            className="flex-1 py-1.5 px-3 rounded-md btn-studio-primary text-xs font-bold transition-all flex items-center justify-center gap-1.5 active:scale-95 disabled:opacity-50"
          >
            <i
              className={`bx ${
                isExporting ? "bx-loader-alt bx-spin" : "bx-film"
              } text-sm`}
            />
            <span>{isExporting ? "Rendering…" : "Export"}</span>
          </button>
        )}
      </div>
    </div>
  );
}
