mod db;
mod deps;
mod pipeline;

use crate::db::Db;
use crate::pipeline::PipelineSource;
use dabar_core::Sermon;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

// ── App state ─────────────────────────────────────────────────────────────────

/// Shared application state injected into all Tauri commands.
pub struct AppState {
    pub db: Db,
    pub app_data_dir: PathBuf,
    pub active_tasks: Arc<Mutex<HashMap<Uuid, AbortHandle>>>,
}

// ── IPC Commands ──────────────────────────────────────────────────────────────

/// List all sermons in the local database.
#[tauri::command]
async fn list_sermons(state: State<'_, AppState>) -> Result<Vec<Sermon>, String> {
    state.db.list_sermons().await.map_err(|e| e.to_string())
}

/// Get a single sermon by ID, including highlights and transcript segments.
#[tauri::command]
async fn get_sermon(id: String, state: State<'_, AppState>) -> Result<Option<Sermon>, String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state.db.get_sermon(id).await.map_err(|e| e.to_string())
}

/// Delete a sermon from the local database and clean up any active tasks.
#[tauri::command]
async fn delete_sermon(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    if let Some(handle) = state.active_tasks.lock().await.remove(&id) {
        handle.abort();
    }
    state.db.delete_sermon(id).await.map_err(|e| e.to_string())
}

/// Start the processing pipeline for a YouTube URL or local file path.
/// Returns the new sermon ID immediately; pipeline runs in the background.
#[tauri::command]
async fn start_pipeline(
    source: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let source = source.trim().to_string();
    if source.is_empty() {
        return Err("Source URL or file path is required.".to_string());
    }

    let pipeline_source = PipelineSource::from_str(&source);
    let stored_str = pipeline_source.as_stored_str();

    // Determine a preliminary title
    let title = match &pipeline_source {
        PipelineSource::YouTube(_) => "Processing…".to_string(),
        PipelineSource::GoogleDrive(_) => "Processing…".to_string(),
        PipelineSource::LocalFile(p) => p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Local Sermon")
            .to_string(),
    };

    // Create sermon record immediately so the UI can navigate to processing page
    let sermon = dabar_core::Sermon::queued(stored_str);
    let sermon_id = sermon.id;
    // Override title
    let mut sermon = sermon;
    sermon.title = title;

    state.db.insert_sermon(&sermon).await.map_err(|e| e.to_string())?;

    // Read settings for API keys and transcription backend choice
    let groq_api_key = state
        .db
        .get_setting("groq_api_key")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("GROQ_API_KEY").ok())
        .unwrap_or_default();

    let offline_mode = state
        .db
        .get_setting("offline_mode")
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        == "true";

    let transcription_backend = if offline_mode || groq_api_key.trim().is_empty() {
        let model_path = {
            let model_name = state
                .db
                .get_setting("offline_model")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "tiny".to_string());
            let filename = format!("ggml-{model_name}.bin");
            state.app_data_dir.join("whisper-models").join(filename)
        };
        dabar_core::whisper::TranscriptionBackend::Local { model_path }
    } else {
        dabar_core::whisper::TranscriptionBackend::Groq {
            api_key: groq_api_key.clone(),
        }
    };

    // Read Ollama settings for offline LLM highlight detection
    let ollama_url = state
        .db
        .get_setting("ollama_url")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let ollama_model = state
        .db
        .get_setting("ollama_model")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "llama3.2:3b".to_string());

    // Spawn pipeline in background — never blocks the UI
    let db_clone = state.db.clone();
    let app_clone = app.clone();
    let app_data_dir_clone = state.app_data_dir.clone();
    let active_tasks_clone = state.active_tasks.clone();
    let task = tokio::spawn(async move {
        let result = pipeline::run_pipeline(
            app_clone.clone(),
            db_clone.clone(),
            sermon_id,
            pipeline_source,
            groq_api_key,
            transcription_backend,
            app_data_dir_clone,
            ollama_url,
            ollama_model,
            offline_mode,
        )
        .await;

        active_tasks_clone.lock().await.remove(&sermon_id);

        if let Err(err) = result {
            pipeline::handle_pipeline_failure(&app_clone, &db_clone, sermon_id, err).await;
        }
    });

    state
        .active_tasks
        .lock()
        .await
        .insert(sermon_id, task.abort_handle());

    Ok(sermon_id.to_string())
}

/// Cancel an in-flight sermon processing pipeline.
#[tauri::command]
async fn cancel_pipeline(
    app: AppHandle,
    sermon_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = Uuid::parse_str(&sermon_id).map_err(|e| e.to_string())?;

    // Abort active Tokio background task if currently executing
    if let Some(handle) = state.active_tasks.lock().await.remove(&id) {
        handle.abort();
        tracing::info!("Aborted pipeline background task for sermon {}", id);
    }

    // Update DB status to Cancelled
    state
        .db
        .update_status(id, dabar_core::SermonStatus::Cancelled)
        .await
        .map_err(|e| e.to_string())?;

    // Emit cancellation event to UI
    let _ = app.emit(
        "pipeline-progress",
        pipeline::PipelineEvent {
            sermon_id: sermon_id.clone(),
            stage: "cancelled".to_string(),
            progress: 0,
            detail: "Sermon processing was cancelled.".to_string(),
            is_error: true,
            is_complete: false,
        },
    );

    Ok(())
}

/// Retry highlight detection for a sermon that has already been transcribed.
#[tauri::command]
async fn retry_highlights(
    app: AppHandle,
    sermon_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<dabar_core::Highlight>, String> {
    let sermon_id = Uuid::parse_str(&sermon_id).map_err(|e| e.to_string())?;
    let api_key = state
        .db
        .get_setting("groq_api_key")
        .await
        .ok()
        .flatten()
        .or_else(|| std::env::var("GROQ_API_KEY").ok())
        .unwrap_or_default();

    pipeline::retry_highlights_pipeline(app, state.db.clone(), sermon_id, api_key)
        .await
        .map_err(|e| e.to_string())
}

/// Render a specific highlight clip or timestamp range to disk and return the output file path.
#[tauri::command]
async fn render_clip(
    sermon_id: String,
    highlight_id: Option<String>,
    clip_id: Option<String>,
    start_time: Option<f32>,
    end_time: Option<f32>,
    clip_title: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sermon_uuid = Uuid::parse_str(&sermon_id).map_err(|e| format!("Invalid sermon ID: {e}"))?;

    let sermon = state
        .db
        .get_sermon(sermon_uuid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Sermon not found: {sermon_id}"))?;

    let output_dir = state
        .db
        .get_setting("output_dir")
        .await
        .ok()
        .flatten()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::video_dir()
                .unwrap_or_else(|| state.app_data_dir.clone())
                .join("Dabar")
        });

    let effective_id_str = highlight_id.or(clip_id);
    let maybe_uuid = effective_id_str.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    // 1. If we have a valid UUID that matches a sermon highlight:
    if let Some(h_id) = maybe_uuid {
        if sermon.highlights.iter().any(|h| h.id == h_id) {
            let output_path = pipeline::render_clip_to_disk(&sermon, h_id, &output_dir)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(output_path.to_string_lossy().to_string());
        }
        // If matches a chapter ID:
        if let Some(ch) = sermon.chapters.iter().find(|c| c.id == h_id) {
            let output_path = pipeline::render_clip_range_to_disk(
                &sermon,
                ch.start_time,
                ch.end_time,
                Some(&ch.title),
                &output_dir,
            )
            .await
            .map_err(|e| e.to_string())?;
            return Ok(output_path.to_string_lossy().to_string());
        }
    }

    // 2. If start_time and end_time were provided:
    if let (Some(start), Some(end)) = (start_time, end_time) {
        let output_path = pipeline::render_clip_range_to_disk(
            &sermon,
            start,
            end,
            clip_title.as_deref(),
            &output_dir,
        )
        .await
        .map_err(|e| e.to_string())?;
        return Ok(output_path.to_string_lossy().to_string());
    }

    // 3. If there is at least one highlight in the sermon:
    if let Some(first_hl) = sermon.highlights.first() {
        let output_path = pipeline::render_clip_to_disk(&sermon, first_hl.id, &output_dir)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(output_path.to_string_lossy().to_string());
    }

    Err("No valid clip timestamp range or highlight found to render".to_string())
}

#[tauri::command]
async fn render_clip_range(
    sermon_id: String,
    start_time: f32,
    end_time: f32,
    clip_title: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sermon_id = Uuid::parse_str(&sermon_id).map_err(|e| e.to_string())?;

    let sermon = state
        .db
        .get_sermon(sermon_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Sermon not found: {sermon_id}"))?;

    let output_dir = state
        .db
        .get_setting("output_dir")
        .await
        .ok()
        .flatten()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::video_dir()
                .unwrap_or_else(|| state.app_data_dir.clone())
                .join("Dabar")
        });

    let output_path = pipeline::render_clip_range_to_disk(
        &sermon,
        start_time,
        end_time,
        clip_title.as_deref(),
        &output_dir,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(output_path.to_string_lossy().to_string())
}

/// Get all user settings as a serializable map.
#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let groq_api_key = state.db.get_setting("groq_api_key").await.ok().flatten().unwrap_or_default();
    let output_dir = state
        .db
        .get_setting("output_dir")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| {
            dirs::video_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .join("Dabar")
                .to_string_lossy()
                .to_string()
        });
    let offline_mode = state.db.get_setting("offline_mode").await.ok().flatten().unwrap_or_default() == "true";
    let offline_model = state.db.get_setting("offline_model").await.ok().flatten().unwrap_or_else(|| "base".to_string());
    let custom_vocab = state.db.get_setting("custom_vocabulary").await.ok().flatten().unwrap_or_default();
    let transcription_backend = state
        .db
        .get_setting("transcription_backend")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "groq".to_string());
    let ollama_url = state.db.get_setting("ollama_url").await.ok().flatten()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let ollama_model = state.db.get_setting("ollama_model").await.ok().flatten()
        .unwrap_or_else(|| "llama3.2:3b".to_string());

    Ok(AppSettings {
        groq_api_key,
        output_dir,
        offline_mode,
        offline_model,
        custom_vocabulary: custom_vocab,
        transcription_backend,
        ollama_url,
        ollama_model,
    })
}

/// Save user settings to the local database.
#[tauri::command]
async fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    state.db.set_setting("groq_api_key", &settings.groq_api_key).await.map_err(|e| e.to_string())?;
    state.db.set_setting("output_dir", &settings.output_dir).await.map_err(|e| e.to_string())?;
    state.db.set_setting("offline_mode", if settings.offline_mode { "true" } else { "false" }).await.map_err(|e| e.to_string())?;
    state.db.set_setting("offline_model", &settings.offline_model).await.map_err(|e| e.to_string())?;
    state.db.set_setting("custom_vocabulary", &settings.custom_vocabulary).await.map_err(|e| e.to_string())?;
    state.db.set_setting("transcription_backend", &settings.transcription_backend).await.map_err(|e| e.to_string())?;
    state.db.set_setting("ollama_url", &settings.ollama_url).await.map_err(|e| e.to_string())?;
    state.db.set_setting("ollama_model", &settings.ollama_model).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Check status of all required external dependencies.
#[tauri::command]
async fn check_dependencies(state: State<'_, AppState>) -> Result<deps::DepsStatus, String> {
    Ok(deps::check_all(&state.app_data_dir).await)
}

/// Open a native file picker to select a sermon audio or video file.
/// Spawns a dedicated OS thread outside of Tokio's worker pool so Windows Common Item Dialog
/// has clean Single-Threaded Apartment (STA) COM state, preventing "Not Responding" shell hangs.
#[tauri::command]
async fn pick_media_file() -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::Builder::new()
        .name("dabar-file-picker".into())
        .spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Select Sermon Recording")
                .add_filter(
                    "Audio & Video Files",
                    &[
                        "mp4", "mov", "webm", "mkv", "mp3", "wav", "m4a", "ogg", "opus", "aac", "flac",
                    ],
                )
                .pick_file();

            let _ = tx.send(file.map(|p| p.to_string_lossy().to_string()));
        })
        .map_err(|e| e.to_string())?;

    rx.await.map_err(|e| e.to_string())
}

/// Download yt-dlp binary to the app data directory.
#[tauri::command]
async fn download_yt_dlp(state: State<'_, AppState>) -> Result<String, String> {
    let path = deps::download_yt_dlp(&state.app_data_dir)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Download static FFmpeg binary to the app data directory.
/// Emits download-progress events: { component: "ffmpeg", downloaded: u64, total: u64 }
#[tauri::command]
async fn download_ffmpeg(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let app_data_dir = state.app_data_dir.clone();
    let app_clone = app.clone();
    let path = deps::download_ffmpeg(&app_data_dir, move |downloaded, total| {
        let _ = app_clone.emit(
            "download-progress",
            serde_json::json!({
                "component": "ffmpeg",
                "downloaded": downloaded,
                "total": total,
            }),
        );
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Download the Whisper GGML model (base or tiny) to the app data directory.
/// Emits download-progress events: { component: "whisper_<model>", downloaded: u64, total: u64 }
#[tauri::command]
async fn download_whisper_model(
    model: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let app_data_dir = state.app_data_dir.clone();
    let app_clone = app.clone();
    let model_label = model.clone();
    let path = deps::download_whisper_model(&app_data_dir, &model, move |downloaded, total| {
        let _ = app_clone.emit(
            "download-progress",
            serde_json::json!({
                "component": format!("whisper_{model_label}"),
                "downloaded": downloaded,
                "total": total,
            }),
        );
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Return offline readiness status for the Settings screen.
#[tauri::command]
async fn get_offline_status(state: State<'_, AppState>) -> Result<deps::OfflineStatus, String> {
    Ok(deps::get_offline_status(&state.app_data_dir).await)
}

/// Open a file or folder in the OS file explorer.
#[tauri::command]
async fn open_in_explorer(path: String, _app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tokio::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        tokio::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Get hardware information for adaptive optimisation.
#[tauri::command]
async fn get_hardware_info() -> HardwareInfo {
    HardwareInfo {
        ram_gb: deps::available_ram_gb(),
        is_low_end: deps::is_low_end(),
        recommended_ffmpeg_preset: deps::recommended_ffmpeg_preset().to_string(),
    }
}

// ── Data transfer types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub groq_api_key: String,
    pub output_dir: String,
    pub offline_mode: bool,
    pub offline_model: String,   // "tiny" | "base"
    pub custom_vocabulary: String,
    #[serde(default = "default_backend_name")]
    pub transcription_backend: String,
    /// Ollama server URL for local LLM highlight detection (e.g. "http://localhost:11434")
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    /// Ollama model name for highlight detection (e.g. "llama3.2:3b")
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
}

fn default_backend_name() -> String {
    "groq".to_string()
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2:3b".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub ram_gb: u64,
    pub is_low_end: bool,
    pub recommended_ffmpeg_preset: String,
}

// ── App entry point ───────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Initialise tracing
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "dabar_tauri=info,dabar_core=info".into()),
                )
                .init();

            // Automatically load .env environment variables from workspace/current dir
            dotenvy::dotenv().ok();
            if let Ok(cwd) = std::env::current_dir() {
                for ancestor in cwd.ancestors() {
                    let env_path = ancestor.join(".env");
                    if env_path.exists() {
                        let _ = dotenvy::from_path(&env_path);
                        break;
                    }
                }
            }

            // Resolve the app data directory for this user
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");
            std::fs::create_dir_all(&app_data_dir)
                .expect("Could not create app data directory");

            // Set environment paths so dabar-core can find binaries in app data
            let bin_dir = app_data_dir.join("bin");
            if bin_dir.exists() {
                let current_path = std::env::var("PATH").unwrap_or_default();
                let sep = if cfg!(windows) { ";" } else { ":" };
                std::env::set_var(
                    "PATH",
                    format!("{}{sep}{current_path}", bin_dir.display()),
                );
            }

            // Connect to SQLite (blocking here is fine — it's setup, not a command)
            let db_path = format!(
                "sqlite://{}?mode=rwc",
                app_data_dir.join("dabar.sqlite3").display()
            );

            let db = tauri::async_runtime::block_on(async {
                Db::connect(&db_path)
                    .await
                    .expect("Failed to connect to local database")
            });

            let state = AppState {
                db,
                app_data_dir,
                active_tasks: Arc::new(Mutex::new(HashMap::new())),
            };
            app.manage(state);

            tracing::info!("Dabar desktop app initialised");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sermons,
            get_sermon,
            delete_sermon,
            start_pipeline,
            cancel_pipeline,
            render_clip,
            render_clip_range,
            get_settings,
            save_settings,
            check_dependencies,
            pick_media_file,
            download_yt_dlp,
            download_ffmpeg,
            download_whisper_model,
            get_offline_status,
            open_in_explorer,
            get_hardware_info,
            retry_highlights,
        ])
        .run(tauri::generate_context!())
        .expect("Error running Dabar desktop app");
}
