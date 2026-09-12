# Dabar (דָּבָר) 🎙️✨

> **"Gospel. Tech. Precious."** — Faith and technology together, intentional and warm, never sterile or generic.

**Dabar** is a local-first, low-end-PC-friendly desktop application (Tauri v2 + React + Rust) that turns sermon audio and video into vertical short-form clips, structured transcripts, and written archives. Built specifically for church media volunteers, not marketers.

---

## 🕯️ Signature Interaction: *Illumination, not scanning*

The interface expresses a single core metaphor: **a sermon coming to light as it is confirmed.**

- **Unconfirmed / Low-Confidence Text**: Renders in a quiet, dim warm grey (`#8A7F73`).
- **Confirmed / Reviewed Text**: Brightens to warm cream (`#1C1815` in paper mode / `#F5EFE6` in dark mode) with a soft amber ambient glow.
- **Key Teaching Moments**: AI-flagged pastoral moments receive a warm amber wash (`#D4913A` at ~8-12% opacity) pooling behind the paragraph where it matters most.
- **Volunteer-Friendly Copy**: Technical notation is translated to human terms (e.g. visual phone icons with `"Vertical · For Reels, TikTok & Shorts"`, and `"32 moments worth sharing"`).

---

## 🏗️ Architecture & Stack

```text
dabar/
├── apps/
│   ├── desktop/src-tauri/      # Tauri v2 Native Shell & Background Pipeline (Rust + SQLite)
│   │   ├── src/
│   │   │   ├── db.rs           # Embedded SQLite persistence & migrations
│   │   │   ├── deps.rs         # Hardware profiling (sysinfo) & dependency resolver
│   │   │   ├── pipeline.rs     # Async background pipeline with live event broadcast
│   │   │   └── lib.rs          # Tauri IPC commands
│   │   └── tauri.conf.json
│   │
│   └── web/                    # React 19 Frontend (Vite + TailwindCSS)
│       └── src/
│           ├── components/     # ManuscriptView, ExportModal, ClipCard, SermonCard
│           ├── pages/          # ClipReview, Dashboard, Upload, Processing, Settings
│           └── lib/api.js      # Tauri IPC client (`invoke` & `listen`)
│
└── packages/
    └── core/                   # Core Processing Engine
        └── src/
            ├── structuring.rs  # Rule-based paragraphs, sections & scripture reference parser
            ├── whisper.rs      # Hybrid Whisper engine (Cloud Groq + Local Offline)
            ├── llm.rs          # Pastoral sermon highlight scoring & reason extraction
            ├── ffmpeg.rs       # 1080x1920 blur-pad vertical slice & bitrate caps (<75MB)
            └── downloader.rs   # Resilient audio/video stream downloader
```

---

## ✨ Key Features

1. **Dual-Mode Intake**:
   - **Local Media File Picker**: MP4, MOV, MKV, MP3, WAV, M4A, OGG, Opus.
   - **YouTube Sermon Links**: Automated stream extraction with Android VR/TV client bypass.
2. **Manuscript Reading & Correction Experience**:
   - Single-column manuscript layout with quiet left-margin timestamps (`12:45`).
   - **Keyboard Navigation**: `Space` (play/pause), `↑ / ↓` (jump sentence/paragraph), `Enter` (edit/confirm).
   - **Scripture Reference Verification**: Detected Bible verses (e.g. `📖 John 3:16`) are shown with canonical text alongside for volunteer verification.
   - **Custom Church Vocabulary**: Case-insensitive autocorrection for church leaders, Hebrew/Greek terms, and ministry names.
3. **Pastoral Highlight Detection**:
   - GPT-OSS 120B evaluates theological depth, testimonies, scripture exposition, and calls to faith rather than generic social media hooks.
   - Generates sermon-specific *"Why it matters"* explanations for every clip.
4. **4-Step Export Studio**:
   - **Live Reflow Preview**: Live video playback with caption preview.
   - **Visual Format Cards**: Vertical (9:16), Square (1:1), Widescreen (16:9).
   - **Caption Presets**: *Warm Ember*, *Clean Serif*, and *Classic Subtitle*.
   - **Direct Disk Render**: Encoded to local `Videos/Dabar/` directory with one-click *"Show in Folder"*.
5. **Low-End PC Optimization**:
   - Hardware RAM detection dynamically tunes FFmpeg presets (`ultrafast` on $\le$4GB RAM machines vs `veryfast` on standard PCs).
   - Strict bitrate limits (`-crf 22 -maxrate 6000k`) guarantee clips remain under 75 MB.

---

## 🎨 Type System & Color Discipline

- **Display & Headlines**: `Fraunces` — warm editorial serif.
- **Body & Manuscript**: `Source Serif 4` — designed for 30+ minutes of comfortable reading.
- **Single Accent Color**: Warm Amber (`#D4913A`) is the **only** accent color across the interface.

---

## 🚀 Getting Started

### Prerequisites
- **Rust**: 1.75+ (`rustc`, `cargo`)
- **Node.js**: 18+ (`npm`)
- **FFmpeg** on system PATH
- A **Groq API Key** ([console.groq.com](https://console.groq.com))

---

### Running the Desktop App

```powershell
# 1. Install frontend dependencies
cd apps/web
npm install

# 2. Run the desktop app with live reload (launches React dev server + Tauri window):
npm run tauri dev
```

Alternatively, run in two separate terminals:
```powershell
# Terminal 1 (Frontend):
cd apps/web
npm run dev

# Terminal 2 (Backend):
cd apps/desktop/src-tauri
cargo run
```

---

## 🧪 Testing

```powershell
# Run all Rust core engine unit tests:
cargo test -p dabar-core

# Verify frontend production bundle:
cd apps/web && npm run build
```
