# Dabar Architecture & Pipeline Design

Dabar is a sermon audio processing and illumination app with a **Rust / Tauri v2 desktop core** and a **React frontend**.

```
┌────────────────────────────────────────────────────────┐
│                   React Desktop UI                     │
│  (Chapters Studio · Manuscript Reader · Instant Search)│
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (Commands + Events)
┌───────────────────────────▼────────────────────────────┐
│                  Dabar Tauri Core                      │
│   (Local SQLite · HTML5 Asset Stream · Audio Storage)  │
└───────────────────────────┬────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
┌───────────────────────┐       ┌───────────────────────┐
│ Primary Pipeline      │       │ Optional / Legacy     │
│ Groq Whisper          │       │ AssemblyAI            │
│ (whisper-large-v3-    │       │ Auto-Chapters         │
│  turbo) + Groq LLM    │       │ (or Offline GGML)     │
│ Highlight Detection   │       │                       │
└───────────────────────┘       └───────────────────────┘
```

---

## 1. Primary Pipeline: Groq Whisper + Deepgram Nova-3 + GPT-OSS 120B

Dabar defaults to **Groq Whisper (`whisper-large-v3-turbo`)** with **Deepgram Nova-3** as automatic fallback, combined with **GPT-OSS 120B pastoral moment detection (`openai/gpt-oss-120b`)**.

### Why Groq & Deepgram Nova-3:
- **Speech Recognition Accuracy**: Whisper Large v3 Turbo and Deepgram Nova-3 exhibit superior phonetic recognition and context handling on Nigerian-accented English, West African cadences, and bilingual code-switching with Yoruba (e.g. liturgical interjections, songs, cultural expressions).
- **Speed & Wall-Clock Efficiency**: Parallel chunked cloud transcription completes in ~5–10 seconds for a full 60–90 minute sermon, eliminating server-side queueing and polling bottlenecks.
- **Accurate Pastoral Highlights**: LLM highlight detection analyzes timestamped transcript segments using GPT-OSS 120B on Groq to identify high-impact, standalone 30–90 second teaching moments with verified theological depth and duration bounds.

### Step-by-Step Execution:
1. **Audio Ingestion**:
   - Downloads source audio from YouTube or Google Drive (via `yt-dlp`), or ingests local audio/video files (`mp3`, `wav`, `m4a`, `mp4`, `mov`, `mkv`).
   - Copies audio to persistent local storage: `%APPDATA%\com.dabar.app\audio\{sermon_id}.mp3`.
   - Emits live download progress to the frontend over Tauri IPC.

2. **FFmpeg Preprocessing**:
   - Audio is transcoded to 16kHz mono 32kbps MP3 (`preprocess_audio_for_whisper`).
   - Slashes payload size to ~14.4MB per hour of audio, guaranteeing compatibility with Groq's 25MB request limit and Deepgram.

3. **Transcription (`whisper-large-v3-turbo` / Deepgram `nova-3`)**:
   - For long sermons (or files exceeding size limits), audio is sliced into overlapping chunks and transcribed concurrently using `tokio::task::JoinSet`.
   - Overlap trimming (`stitch_transcript_chunks`) reconstructs seamless sentence-level `TranscriptSegment`s with absolute timeline alignment.

4. **Highlight Detection (`openai/gpt-oss-120b`)**:
   - Formats timestamped transcript segments into an inline prompt: `[HH:MM:SS] Segment text...`.
   - Prompts the LLM under pastoral editorial guidelines for 30–90 second clips.
   - Validates timestamps, ensures duration bounds (25s–120s with 90s clamping), and produces structured `Highlight` records.

5. **Persistence**:
   - Saves sermon metadata, highlights, and transcript segments to the local SQLite database in an atomic transaction.
   - Cleans up temporary files and emits completion event.

---

## 2. Optional / Legacy Pipeline: AssemblyAI Auto-Chapters

For users preferring automated topic-based auto-chapters over LLM highlight extraction, AssemblyAI remains available as an opt-in alternative in Settings:
- Uploads compressed audio to AssemblyAI (`https://api.assemblyai.com/v2/upload`).
- Submits auto-chapters transcription job and polls until complete.
- Transforms response words into `TranscriptSegment`s and topic blocks into `Chapter` structs.

---

## 3. Data Model

### Highlight (`packages/core/src/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    pub id: Uuid,
    pub title: String,
    pub start_time: f32, // in seconds
    pub end_time: f32,   // in seconds
    pub score: f32,
    pub reason: String,
    pub suggested_hook_text: String,
}
```

### Chapter (`packages/core/src/models.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: Uuid,
    pub title: String,
    pub summary: String,
    pub start_time: f32, // in seconds
    pub end_time: f32,   // in seconds
}
```

### SQLite Schema (`sermons`, `highlights`, `chapters`, `transcript_segments`)
```sql
CREATE TABLE IF NOT EXISTS sermons (
    id                TEXT PRIMARY KEY,
    title             TEXT NOT NULL,
    source_url        TEXT NOT NULL,
    source_type       TEXT NOT NULL DEFAULT 'youtube',
    status            TEXT NOT NULL DEFAULT 'queued',
    created_at        TEXT NOT NULL,
    error_message     TEXT,
    audio_path        TEXT,
    highlight_status  TEXT,
    highlight_error   TEXT,
    total_candidates  INTEGER,
    passed_candidates INTEGER
);

CREATE TABLE IF NOT EXISTS highlights (
    id                  TEXT PRIMARY KEY,
    sermon_id           TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    start_time          REAL NOT NULL,
    end_time            REAL NOT NULL,
    score               REAL NOT NULL,
    reason              TEXT NOT NULL DEFAULT '',
    suggested_hook_text TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS chapters (
    id          TEXT PRIMARY KEY,
    sermon_id   TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',
    start_time  REAL NOT NULL,
    end_time    REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id         TEXT PRIMARY KEY,
    sermon_id  TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
    start_time REAL NOT NULL,
    end_time   REAL NOT NULL,
    text       TEXT NOT NULL,
    ordinal    INTEGER NOT NULL
);
```

---

## 4. Real Audio Playback & Synchronization

- **Asset Protocol**: Local audio files are streamed directly to the frontend webview via Tauri's custom asset protocol (`convertFileSrc(audio_path)`).
- **Synchronized Seeking**:
  - Clicking any Highlight or Chapter card seeks the audio player to `start_time` and plays.
  - Clicking any manuscript timestamp seeks audio to that exact second.
  - URL timestamp parameters (`/transcript/:id?t=120`) jump directly to that chapter or timestamp.

---

## 5. Optimized Clip Rendering Engine

- **High-Performance Background Blur**: Vertical video clip export (`extract_vertical_clip` in `ffmpeg.rs`) uses `boxblur=luma_radius=20:luma_power=2` instead of CPU-heavy `gblur`. This provides smooth background aesthetics at a fraction of the CPU rendering time on low-power devices and integrated GPUs.
