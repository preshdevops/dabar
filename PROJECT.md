# Project: Dabar — Sermon Processing Desktop App

## Architecture
Dabar is a desktop application built with a Tauri (Rust) backend and a React/TypeScript frontend.
- **Frontend (`apps/web`)**: React 19, Tailwind CSS, Lucide icons, Vite. Manages UI views (`Transcript`, `ClipReview`, `Clips`, `Settings`), interactive manuscript reading, clip selection, boundary nudging, and the video export modal.
- **Desktop Host (`apps/desktop/src-tauri`)**: Tauri 2 application. Handles IPC commands (`render_clip`, `render_clip_range`, `open_in_explorer`, `check_dependencies`), pipeline orchestration (`pipeline.rs`), SQLite persistence (`db.rs`), and external binary management (`deps.rs`).
- **Core Engine (`packages/core`)**: Rust crate containing:
  - `downloader.rs`: `yt-dlp` invocation for audio extraction and on-demand video section downloading.
  - `ffmpeg.rs`: Audio preprocessing, stream probing (`ffprobe`), multi-aspect filtergraph generation (9:16, 1:1, 16:9), and ASS subtitle burning.
  - `whisper.rs`: Groq/Deepgram speech-to-text with chunking and concurrency control.
  - `llm.rs`: Sermon analysis, chaptering, and highlight detection across Groq, OpenAI, and offline heuristics.
  - `models.rs`: Core domain types (`Sermon`, `Highlight`, `TranscriptSegment`, `Chapter`).
- **Data Flow**:
  1. Audio Ingestion: YouTube audio (or local file) -> MP3 in app data.
  2. Transcription: Audio chunking -> Groq/Deepgram -> Stitched segments.
  3. Analysis: LLM highlight & chapter detection -> Validated JSON.
  4. Persistence: Batched SQLite transaction.
  5. Clipping & Export: On-demand video section download (for YouTube) or local video file -> Aspect ratio filtergraph + ASS subtitle burn -> H.264 MP4 export -> In-app video preview + File Explorer highlight.

---

## Feature Inventory
Every feature identified during the Survey and from ORIGINAL_REQUEST is inventoried below with its assigned milestone.

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | YouTube Video Clip Rendering | Render real video footage for YouTube sermons via on-demand section download, not audio waveforms | M1 | Survey / R1 |
| 2 | Local Video Clip Rendering | Render real video footage for uploaded local video files | M1 | Survey / R1 |
| 3 | Multi-Aspect Ratio Filtergraphs | Support 9:16, 1:1, and 16:9 with even pixel bounds (`force_divisible_by=2`) | M1 | Survey / R1 |
| 4 | Background Blur Padding | High-quality Gaussian/boxblur background padding for 9:16 and 1:1 aspect ratios | M1 | Survey / R1 |
| 5 | ASS Subtitle Engine & Styles | Timed ASS subtitle burning matching UI presets (Warm Amber, Kinetic, Sacred Editorial) | M1 | Survey / R1 |
| 6 | Unified External Tool Discovery | Reliable discovery for `ffmpeg`, `ffprobe`, `yt-dlp` across PATH, AppData, and repo `bin/` | M1 | Survey / R1 |
| 7 | LLM Reasoning Token Budget | Increase `max_tokens` to 3500+ for Groq reasoning models (`openai/gpt-oss-120b/20b`) | M2 | Survey / R2 |
| 8 | Markdown Code Fence Stripping | Parse JSON from LLMs that return ````json ... ```` fences | M2 | Survey / R2 |
| 9 | LLM Cascading Timeout Fix | Lower per-model timeout to 30s and fix `o3-mini` temperature parameter incompatibility | M2 | Survey / R2 |
| 10 | Desktop Audio Chunk Triggers | Raise chunk triggers to 20MB / 15m to eliminate unnecessary fragmentation | M2 | Survey / R2 |
| 11 | Fast Stream Copy Chunking | Use `-c:a copy` for extracting chunks from already-preprocessed MP3 (<50ms) | M2 | Survey / R2 |
| 12 | Pipelined Concurrent Extraction | Concurrently pipeline chunk extraction and transcription in Tokio tasks (semaphore 4) | M2 | Survey / R2 |
| 13 | Batched SQLite Persistence | Replace 1,000+ row-by-row queries with `sqlx::QueryBuilder` batch transactions | M2 | Survey / R2 |
| 14 | Arbitrary Duration Selection | Relax 30s clamps to allow 10s up to 5+ minute clip selections | M3 | Survey / R3 |
| 15 | Interactive In/Out Marking | Mark In and Out points directly from Transcript and Clip Review views | M3 | Survey / R3 |
| 16 | Boundary Nudging (±1s) | Add ±1s nudge buttons on Clip Cards and Transcript range bar with immediate visual timeline updates | M3 | Survey / R3 |
| 17 | Backend Nudged Bounds Honor | Ensure `render_clip` honors passed `start_time` and `end_time` rather than un-nudged DB highlights | M3 | Survey / R3 |
| 18 | Video Export Modal Lifecycle | Prevent premature dismissal upon render completion; keep modal open for preview | M4 | Survey / R4 |
| 19 | In-App Video Playback Preview | Embed HTML5 `<video>` preview player inside modal using Tauri asset protocol | M4 | Survey / R4 |
| 20 | Windows "Show in Folder" | Fix `open_in_explorer` to run `explorer /select,"<path>"` to highlight the exported MP4 | M4 | Survey / R4 |
| 21 | Warm Amber Theme Polish | Align caption presets and export feedback with Dabar's Warm Amber design aesthetic | M4 | Survey / R4 |
| 22 | Transcript ExportModal Wiring | Connect Transcript custom range export to `<ExportModal>` for unified UX | M4 | Survey / R4 |
| 23 | E2E Test Suite | Automated opaque-box test runner covering Tiers 1-4 | E2E | Spec / Dual Track |
| 24 | Final Milestone Acceptance | Pass 100% E2E tests + Tier 5 Adversarial Coverage Hardening | Final | Spec / Dual Track |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Video Clip Rendering & Tool Discovery | Real video clips (YouTube on-demand download & local files), aspect ratios (9:16, 1:1, 16:9) with even bounds & background blur, ASS subtitle engine, unified binary discovery (`ffmpeg`, `ffprobe`, `yt-dlp`). | None | IN_PROGRESS |
| M2 | Pipeline Concurrency & Persistence | Groq reasoning token fix, markdown fence stripping, 30s timeout, desktop chunk triggers (20MB/15m), fast `-c:a copy` chunking, concurrent transcription (semaphore 4), `sqlx::QueryBuilder` batch insertion. | None | PLANNED |
| M3 | Flexible Clip Selection & Nudging UX | Remove rigid duration clamps (allow 10s to 5+ min), interactive in/out marking, ±1s nudge controls on ClipCards & Transcript, update backend `render_clip` to honor nudged bounds. | None | PLANNED |
| M4 | Export Modal, Preview & File Explorer | Prevent export modal premature dismissal, in-app HTML5 video preview player, Windows `explorer /select,"<file_path>"`, Warm Amber caption presets, wire Transcript export to ExportModal. | M1, M3 | PLANNED |
| E2E | E2E Testing Track | Independent requirement-driven test suite (Tiers 1-4), test runner, and `TEST_READY.md`. | None | IN_PROGRESS |
| Final | Final E2E Pass & Coverage Hardening | Phase 1: 100% E2E test pass (Tiers 1-4). Phase 2: Tier 5 adversarial coverage hardening. | M1, M2, M3, M4, E2E | PLANNED |

---

## Interface Contracts

### 1. Tauri IPC: `render_clip` and `render_clip_range`
```rust
// In apps/desktop/src-tauri/src/lib.rs
#[tauri::command]
async fn render_clip(
    sermon_id: String,
    highlight_id: Option<String>,
    clip_id: Option<String>,
    start_time: Option<f64>,
    end_time: Option<f64>,
    clip_title: Option<String>,
    aspect_ratio: Option<String>,
    caption_style: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String>;

#[tauri::command]
async fn render_clip_range(
    sermon_id: String,
    start_time: f64,
    end_time: f64,
    clip_title: Option<String>,
    aspect_ratio: Option<String>,
    caption_style: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String>;
```

### 2. Frontend API: `renderClip` and `renderClipRange`
```typescript
// In apps/web/src/lib/api.js
export async function renderClip(
  sermonId: string,
  clipId: string | null,
  startTime: number | null,
  endTime: number | null,
  clipTitle: string | null,
  aspectRatio: string = "9:16",
  captionStyle: string = "amber"
): Promise<string>;

export async function renderClipRange(
  sermonId: string,
  startTime: number,
  endTime: number,
  clipTitle: string | null,
  aspectRatio: string = "9:16",
  captionStyle: string = "amber"
): Promise<string>;
```

### 3. Core FFmpeg Extract Clip Contract
```rust
// In packages/core/src/ffmpeg.rs
pub async fn extract_clip(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
    aspect_ratio: &str,
    caption_segments: Option<&[TranscriptSegment]>,
    caption_style: Option<&str>,
) -> Result<()>;
```

### 4. Windows File Reveal Contract
```rust
// In apps/desktop/src-tauri/src/lib.rs
#[tauri::command]
async fn open_in_explorer(path: String, _app: AppHandle) -> Result<(), String> {
    // If path is a file on Windows: explorer.exe /select,"<path>"
}
```

---

## Code Layout
- `packages/core/src/`:
  - `ffmpeg.rs`: Filtergraph generation, ASS subtitle generator, stream probing.
  - `downloader.rs`: `yt-dlp` section downloader and stream resolution.
  - `llm.rs`: Groq/OpenAI cloud highlight detection, prompt formatting, markdown fence stripping.
  - `whisper.rs`: Audio chunking constants, concurrent transcription orchestration.
  - `models.rs`: Core domain models.
  - `deps.rs` (or shared discovery in `packages/core`): Unified `ffmpeg`, `ffprobe`, `yt-dlp` lookup.
- `apps/desktop/src-tauri/src/`:
  - `lib.rs`: Tauri IPC commands (`render_clip`, `render_clip_range`, `open_in_explorer`, `check_dependencies`).
  - `pipeline.rs`: Audio download, transcription, highlight detection, and clip rendering orchestration.
  - `db.rs`: SQLite database queries and `sqlx::QueryBuilder` batch transaction methods.
  - `deps.rs`: External tool status checking and on-demand installer.
- `apps/web/src/`:
  - `pages/Transcript.jsx`: Transcript manuscript view, selection mode, timestamp marking, nudge controls, and `<ExportModal>` integration.
  - `pages/ClipReview.jsx`: Clip review, active player, ±1s card nudging, export modal lifecycle management.
  - `components/ClipCard.jsx`: Clip card UI with ±1s start/end nudge buttons.
  - `components/ManuscriptView.jsx`: Interactive transcript paragraphs with Mark In / Mark Out controls.
  - `components/ExportModal.jsx`: Aspect ratio and Warm Amber caption style selection, progress state, in-app HTML5 video preview player, and "Show in Folder" button.
  - `lib/api.js`: IPC interface bindings.
