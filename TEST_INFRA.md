# Dabar Test Infrastructure (TEST_INFRA)

## 1. Overview & Testing Philosophy

Dabar is a sermon processing and illumination desktop application built with a Tauri (Rust) backend, a core processing engine (`dabar-core`), and a React 19 / Vite frontend (`apps/web`).

The test infrastructure is designed as an **opaque-box, requirement-driven automated test harness**. Tests assert against observable contracts, file artifacts, and UI behaviors rather than brittle internal implementation details.

### Core Principles
1. **Opaque-Box Verification**: Exercise functionality via defined interface contracts (Tauri IPC commands, CLI invocations, media stream properties via `ffprobe`, SQLite state, and UI component actions).
2. **Deterministic Media Probing**: Every rendered video clip artifact is inspected via `ffprobe` to verify video codec (`h264`), audio codec (`aac`), exact aspect ratios (9:16, 1:1, 16:9), even pixel dimensions (`width % 2 == 0` and `height % 2 == 0`), and playback validity.
3. **Multi-Tiered Progression**: Test suites are structured into four strict tiers, progressing from isolated feature contracts to complex cross-feature combinations and realistic end-to-end sermon processing workflows.
4. **Single-Command Invocation**: The entire suite across all four tiers executes via a single command (`node tests/e2e/runner.js` or `npm test`) with zero external test runner daemon overhead.

---

## 2. Test Architecture: 4-Tier Hierarchy

The automated test suite is organized into four hierarchical tiers:

```
tests/e2e/
├── runner.js                      # Central test orchestrator & reporter
├── fixtures/                      # Synthetic media generators & mock fixtures
│   ├── media_generator.js         # Generates test video/audio with SMPTE color bars
│   └── mock_data.js               # Mock sermon transcripts, highlights & LLM payloads
├── tier1_features/                # Tier 1: Feature Coverage
│   ├── test_video_rendering.js    # F1-F4: YouTube vs local, aspect ratios, blur padding
│   ├── test_subtitles_engine.js   # F5, F21: ASS subtitle styles (Warm Amber, Kinetic, etc.)
│   ├── test_tool_discovery.js     # F6: Discovery of ffmpeg, ffprobe, yt-dlp across paths
│   ├── test_llm_pipeline.js       # F7-F9: Reasoning budget, fence stripping, timeouts
│   ├── test_chunking_concurrency.js# F10-F12: Audio chunk triggers, stream copy, Tokio tasks
│   ├── test_sqlite_batch.js       # F13: QueryBuilder batch transactions for transcripts
│   ├── test_clip_ergonomics.js    # F14-F17: Arbitrary durations, in/out, ±1s nudging, bounds
│   └── test_export_modal.js       # F18-F20, F22: Modal lifecycle, HTML5 player, Explorer reveal
├── tier2_boundaries/              # Tier 2: Boundary & Corner Cases
│   ├── test_durations.js          # 10s minimum, 300s+ (5+ min), clamp violations
│   ├── test_odd_dimensions.js     # Odd input dimensions (e.g. 1919x1079) forcing even bounds
│   ├── test_subtitle_escaping.js  # Special characters, quotes, colons, percent, backslashes
│   └── test_resilience.js         # Missing tools, malformed LLM responses, empty transcripts
├── tier3_combinations/            # Tier 3: Cross-Feature Combinations
│   └── test_matrix.js             # Pairwise source x aspect x subtitle x duration x nudging
└── tier4_scenarios/               # Tier 4: Real-World Application Scenarios
    ├── test_sunday_service.js     # Full Sunday sermon: ingestion -> batch -> nudge -> export
    ├── test_bible_study.js        # Midweek study: local video -> 1:1 square blur -> burn-in
    └── test_youtube_clipping.js   # Live YouTube stream section download -> 16:9 widescreen
```

---

## 3. Tier Specifications

### Tier 1: Feature Coverage
Validates all 22 functional features from `PROJECT.md § Feature Inventory` in isolation:
- **F1 & F2**: Real video footage rendering (YouTube section download & local video files) without falling back to audio waveform cards.
- **F3**: Multi-aspect ratio filtergraphs:
  - `9:16`: 1080x1920 (divisible by 2)
  - `1:1`: 1080x1080 (divisible by 2)
  - `16:9`: 1920x1080 (divisible by 2)
- **F4**: Background blur padding (`boxblur=5:2` or Gaussian) for non-native aspect ratios.
- **F5 & F21**: Burnt-in subtitles matching UI presets (Warm Amber `#D4913A`, Kinetic, Sacred Editorial).
- **F6**: External tool discovery across `PATH`, `AppData/Local/com.dabar.app/bin`, and `bin/`.
- **F7**: LLM reasoning token budget (`max_tokens >= 3500`) for Groq reasoning models (`openai/gpt-oss-120b`).
- **F8**: Markdown code fence stripping (extracting valid JSON from ````json ... ```` fences).
- **F9**: LLM 30-second per-model timeout and `o3-mini` temperature incompatibility fix.
- **F10 & F11**: Desktop chunk triggers (20MB / 15m) and fast stream copy (`-c:a copy`) extraction.
- **F12**: Concurrent Tokio transcription pipeline (semaphore limit 4).
- **F13**: Batched SQLite transaction insertion via `sqlx::QueryBuilder`.
- **F14**: Arbitrary clip duration selection (10s to 5+ minutes) without rigid 30s clamps.
- **F15**: Interactive In / Out timestamp marking.
- **F16**: Boundary nudging controls (±1s) with immediate timeline updates.
- **F17**: Backend honors passed `start_time` and `end_time` for nudged highlights.
- **F18**: Video Export Modal lifecycle (remains open on render completion).
- **F19**: In-app HTML5 `<video>` preview playback.
- **F20**: Windows file reveal executes `explorer /select,"<path>"`.
- **F22**: Transcript custom range wired to `<ExportModal>`.

### Tier 2: Boundary & Corner Cases
Stresses edge conditions and adversarial inputs:
- **Duration Extremes**: Exact 10.0s lower bound, 300.0s (5-minute) long-form segments.
- **Clamp Rejections**: Inverted intervals (`end <= start`), negative start times (`start < 0`), sub-minimum durations (`< 10s`).
- **Odd Pixel Bounds**: Video sources with odd dimensions (e.g., 721x1281, 1919x1079) must produce strictly even dimensions (`force_divisible_by=2`).
- **Subtitle Escaping**: Subtitle text containing single quotes (`'`), double quotes (`"`), colons (`:`), percent (`%`), backslashes (`\`), curly braces (`{}`), emojis, and multiline text.
- **Resilience**: Missing external binaries return helpful error messages; malformed LLM responses fall back cleanly; empty or single-character transcripts handled without panics.

### Tier 3: Cross-Feature Combinations
Validates pairwise interaction matrix:
| Combination | Source Type | Aspect Ratio | Subtitle Style | Duration | Nudged? |
|---|---|---|---|---|---|
| C1 | YouTube | 9:16 Vertical | Warm Amber | 15.0s | Yes (±1s) |
| C2 | YouTube | 1:1 Square | Kinetic | 300.0s (5m) | No |
| C3 | Local MP4 | 16:9 Widescreen | Sacred Editorial| 47.0s | Yes (±1s) |
| C4 | Local MP4 | 9:16 Vertical | Warm Amber | 60.0s | No |
| C5 | Audio Only | 9:16 Vertical | None (Waveform) | 20.0s | N/A |
| C6 | Audio Only | 1:1 Square | Warm Amber | 30.0s | N/A |

### Tier 4: Real-World Application Scenarios
Simulates complete real-world pastoral workflows:
1. **Sunday Service Workflow**: Long-form sermon audio ingestion -> concurrent chunking -> LLM highlight extraction -> SQLite batch persistence -> manuscript reading with In/Out marking -> ±1s boundary nudging -> 9:16 vertical render with Warm Amber burnt-in subtitles -> HTML5 preview in modal -> Explorer file highlight.
2. **Midweek Bible Study Workflow**: Local camera video ingestion -> 1:1 square clip selection (5 minutes / 300s) -> background blur padding -> ASS caption burning -> MP4 integrity verification.
3. **YouTube Instant Clipping Workflow**: On-demand section download from YouTube -> 16:9 widescreen export -> verification of actual video stream (no waveform fallback card).

---

## 4. Invocation & Running Tests

### Single-Command Execution (All Tiers)
Run the entire automated test suite:
```bash
node tests/e2e/runner.js
```
Or via npm:
```bash
npm test
```

### Running Specific Tiers
To isolate testing during milestone implementation:
```bash
# Tier 1: Feature Coverage only
node tests/e2e/runner.js --tier 1

# Tier 2: Boundary & Corner Cases only
node tests/e2e/runner.js --tier 2

# Tier 3: Cross-Feature Combinations only
node tests/e2e/runner.js --tier 3

# Tier 4: Real-World Scenarios only
node tests/e2e/runner.js --tier 4
```

### Filtering Specific Tests
Run tests matching a substring:
```bash
node tests/e2e/runner.js --filter "aspect_ratio"
node tests/e2e/runner.js --filter "nudging"
```

### Environment Variables
| Variable | Description | Default |
|---|---|---|
| `DABAR_TEST_KEEP_ARTIFACTS` | Keep generated video/audio files in `tmp_test_artifacts/` for manual inspection | `false` |
| `DABAR_TEST_VERBOSE` | Output full FFmpeg and IPC process logs | `false` |
| `FFMPEG_PATH` | Explicit path to `ffmpeg.exe` override | Discovered automatically |
| `FFPROBE_PATH` | Explicit path to `ffprobe.exe` override | Discovered automatically |
| `YT_DLP_PATH` | Explicit path to `yt-dlp.exe` override | Discovered automatically |

---

## 5. Coverage & Quality Gates

1. **Pass Criteria**: All executed test assertions must pass (`exitCode === 0`).
2. **Integrity Check**:
   - Every generated MP4 must contain valid `h264` video and `aac` audio tracks (verified via `ffprobe`).
   - `width % 2 === 0` and `height % 2 === 0` for all rendered files.
   - Non-zero file size and duration within ±0.5s of requested bounds.
3. **Defect Escalation**: When a test catches an implementation defect, the runner logs the exact assertion, input parameters, and observed vs expected outputs for escalation to the implementing agent.
