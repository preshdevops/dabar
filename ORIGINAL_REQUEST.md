# Original User Request

## 2026-09-12T18:47:52Z

Comprehensive repair and optimization of Dabar, a sermon processing desktop app. Fix video clip rendering across all aspect ratios, eliminate pipeline latency and timeout bottlenecks, and enable flexible arbitrary timestamp selection.

Working directory: c:\Users\aremu\Desktop\projects\dabar
Integrity mode: development

## Requirements

### R1. Video Clip Rendering Repair
The video export system must successfully render real video clips (not audio waveforms or blank outputs) for both uploaded local video files and YouTube sermon URLs. The export engine must support 9:16 (vertical), 1:1 (square), and 16:9 (widescreen) aspect ratios with even pixel dimensions, correct background blur padding where necessary, and burnt-in subtitles/captions matching selected styling without causing FFmpeg or player crashes.

### R2. End-to-End Pipeline Performance Optimization
The sermon processing pipeline must complete reliably without cascading timeouts or unnecessary serialization. Cloud highlight detection must use valid provider model identifiers that succeed on the initial attempt. Audio chunk extraction and transcription must maximize concurrent execution where supported, and database batch persistence must replace row-by-row insertion bottlenecks.

### R3. Arbitrary and Flexible Clip Selection UX
Users must be able to select, preview, and export any arbitrary section of a sermon (from short 10–30s snippets to spontaneous 5+ minute segments) directly from the user interface. Both the Transcript manuscript view and the Clip Review view must support interactive in/out timestamp marking and fine-grained boundary nudging (±1s adjustments).

### R4. Interface Cohesion and Export Feedback
The video export modal must remain responsive during rendering, display accurate progress feedback, prevent premature dismissal, and immediately offer in-app video playback and quick access to the exported file upon completion, staying faithful to Dabar's Warm Amber aesthetic.

## Acceptance Criteria

### Video Clip Rendering
- [ ] Exporting a clip from a YouTube sermon produces an MP4 containing the actual video footage, not an audio-only waveform card.
- [ ] Exporting with 9:16, 1:1, and 16:9 aspect ratios produces playable MP4 files with exact matching aspect dimensions and even pixel bounds.
- [ ] Caption styling selected in the export modal is rendered legibly onto the exported video clip.
- [ ] External tool discovery resolves `ffmpeg`, `ffprobe`, and `yt-dlp` reliably regardless of whether they are located in system PATH or the local application data directory.

### Processing Pipeline Speed
- [ ] Highlight detection calls succeed on the primary model attempt without triggering 90-second timeout cascades or fallback errors.
- [ ] Chunk transcription processes concurrently, eliminating serial preprocessing and extraction bottlenecks.
- [ ] Saving sermon results with hundreds of transcript segments executes as batched transactions.

### Clip Selection & Ergonomics
- [ ] The user can define custom start and end timestamps of any length (e.g. 15s, 47s, 300s) on both the Transcript and Clip Review screens without rigid duration clamps.
- [ ] Clip cards provide nudge buttons to expand or trim boundaries by ±1 second with immediate timeline updates.
- [ ] Export modal remains open upon render completion, playing the generated video and providing a functional "Show in Folder" button.
