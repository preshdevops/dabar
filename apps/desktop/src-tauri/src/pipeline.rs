use crate::db::Db;
use anyhow::Result;
use dabar_core::{Sermon, SermonStatus};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// Progress event emitted to the frontend during pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEvent {
    pub sermon_id: String,
    pub stage: String,
    pub progress: u8, // 0-100 within current stage
    pub detail: String,
    pub is_error: bool,
    pub is_complete: bool,
}

/// Source input for the pipeline — YouTube URL, Google Drive share link, or local file path.
#[derive(Debug, Clone)]
pub enum PipelineSource {
    YouTube(String),
    GoogleDrive(String),
    LocalFile(PathBuf),
}

impl PipelineSource {
    pub fn from_str(source: &str) -> Self {
        let trimmed = source.trim();
        if trimmed.contains("youtube.com/") || trimmed.contains("youtu.be/") {
            PipelineSource::YouTube(trimmed.to_string())
        } else if dabar_core::downloader::is_gdrive_url(trimmed) {
            PipelineSource::GoogleDrive(trimmed.to_string())
        } else {
            PipelineSource::LocalFile(PathBuf::from(trimmed))
        }
    }

    /// The URL or path string to store in the database.
    pub fn as_stored_str(&self) -> String {
        match self {
            PipelineSource::YouTube(url) => url.clone(),
            PipelineSource::GoogleDrive(url) => url.clone(),
            PipelineSource::LocalFile(path) => path.to_string_lossy().to_string(),
        }
    }
}

/// Emit a progress event to the frontend and log to terminal.
fn emit(app: &AppHandle, sermon_id: Uuid, stage: &str, progress: u8, detail: &str) {
    tracing::info!("📊 [{stage} - {progress}%] {detail} (sermon: {sermon_id})");
    let _ = app.emit(
        "pipeline-progress",
        PipelineEvent {
            sermon_id: sermon_id.to_string(),
            stage: stage.to_string(),
            progress,
            detail: detail.to_string(),
            is_error: false,
            is_complete: false,
        },
    );
}

fn emit_complete(app: &AppHandle, sermon_id: Uuid) {
    tracing::info!("🎉 [ready - 100%] Pipeline complete! Clips and manuscript are ready. (sermon: {sermon_id})");
    let _ = app.emit(
        "pipeline-progress",
        PipelineEvent {
            sermon_id: sermon_id.to_string(),
            stage: "ready".to_string(),
            progress: 100,
            detail: "Your sermon has been processed. Clips and manuscript are ready.".to_string(),
            is_error: false,
            is_complete: true,
        },
    );
}

fn emit_error(app: &AppHandle, sermon_id: Uuid, error: &str) {
    tracing::error!("❌ [failed] Sermon processing failed: {error} (sermon: {sermon_id})");
    let _ = app.emit(
        "pipeline-progress",
        PipelineEvent {
            sermon_id: sermon_id.to_string(),
            stage: "failed".to_string(),
            progress: 0,
            detail: error.to_string(),
            is_error: true,
            is_complete: false,
        },
    );
}

/// The main sermon processing pipeline.
///
/// Runs entirely in a background Tokio task (called via `tokio::spawn`).
/// Emits granular progress events to the frontend via Tauri's event system.
/// Writes pipeline checkpoints to SQLite after each major stage for crash recovery.
pub async fn run_pipeline(
    app: AppHandle,
    db: Db,
    sermon_id: Uuid,
    source: PipelineSource,
    api_key: String,
    transcription_backend: dabar_core::whisper::TranscriptionBackend,
    app_data_dir: PathBuf,
    _ollama_url: String,
    _ollama_model: String,
    offline_mode: bool,
) -> Result<()> {
    tracing::info!("🚀 [Pipeline Start] Processing sermon ID: {sermon_id}");
    let temp_dir = std::env::temp_dir().join(format!("dabar_{sermon_id}"));
    tokio::fs::create_dir_all(&temp_dir).await?;

    let audio_storage_dir = app_data_dir.join("audio");
    let _ = tokio::fs::create_dir_all(&audio_storage_dir).await;

    // ── Stage 1: Download / Locate audio ─────────────────────────────────────

    db.update_status(sermon_id, SermonStatus::Downloading)
        .await?;
    emit(&app, sermon_id, "downloading", 5, "Locating sermon audio…");

    let audio_path: PathBuf = match &source {
        PipelineSource::YouTube(url) => {
            emit(
                &app,
                sermon_id,
                "downloading",
                10,
                "Downloading audio from YouTube…",
            );
            let downloaded = dabar_core::downloader::download_youtube_audio(url, &temp_dir).await?;
            if let Some(title) = &downloaded.title {
                let _ = db.update_title(sermon_id, title).await;
            }

            // Persist audio in app storage for local playback and export (try atomic move first, fallback to copy)
            let persistent_path = audio_storage_dir.join(format!("{sermon_id}.mp3"));
            let final_path = if tokio::fs::rename(&downloaded.path, &persistent_path)
                .await
                .is_ok()
            {
                persistent_path
            } else if let Ok(_) = tokio::fs::copy(&downloaded.path, &persistent_path).await {
                let _ = tokio::fs::remove_file(&downloaded.path).await;
                persistent_path
            } else {
                tracing::warn!(
                    "Could not move or copy audio to persistent storage, using temp path"
                );
                downloaded.path
            };

            db.save_checkpoint(sermon_id, "downloading", &final_path.to_string_lossy())
                .await?;
            emit(
                &app,
                sermon_id,
                "downloading",
                100,
                "Audio downloaded successfully.",
            );
            final_path
        }
        PipelineSource::GoogleDrive(url) => {
            emit(
                &app,
                sermon_id,
                "downloading",
                10,
                "Downloading audio from Google Drive…",
            );
            let downloaded = dabar_core::downloader::download_gdrive_audio(url, &temp_dir).await?;
            if let Some(title) = &downloaded.title {
                let _ = db.update_title(sermon_id, title).await;
            }

            let persistent_path = audio_storage_dir.join(format!("{sermon_id}.mp3"));
            let final_path = if tokio::fs::rename(&downloaded.path, &persistent_path)
                .await
                .is_ok()
            {
                persistent_path
            } else if let Ok(_) = tokio::fs::copy(&downloaded.path, &persistent_path).await {
                let _ = tokio::fs::remove_file(&downloaded.path).await;
                persistent_path
            } else {
                tracing::warn!(
                    "Could not move or copy audio to persistent storage, using temp path"
                );
                downloaded.path
            };

            db.save_checkpoint(sermon_id, "downloading", &final_path.to_string_lossy())
                .await?;
            emit(
                &app,
                sermon_id,
                "downloading",
                100,
                "Audio downloaded from Google Drive.",
            );
            final_path
        }
        PipelineSource::LocalFile(path) => {
            // Validate the file exists and is readable
            if !path.exists() {
                anyhow::bail!("Local file not found: {}", path.display());
            }
            // Derive title from filename
            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Sermon");
            let _ = db.update_title(sermon_id, title).await;
            db.save_checkpoint(sermon_id, "downloading", &path.to_string_lossy())
                .await?;
            emit(&app, sermon_id, "downloading", 100, "Local file located.");
            path.clone()
        }
    };

    // ── Stage 2: Transcription & Chapter Segmentation ───────────────────────

    db.update_status(sermon_id, SermonStatus::Transcribing)
        .await?;
    emit(
        &app,
        sermon_id,
        "transcribing",
        5,
        "Preparing audio for transcription…",
    );

    let custom_vocab = db
        .get_setting("custom_vocabulary")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let transcription_res = dabar_core::whisper::transcribe_audio(
        &transcription_backend,
        &audio_path,
        if custom_vocab.trim().is_empty() {
            None
        } else {
            Some(custom_vocab.trim())
        },
        Some(Box::new({
            let app_clone = app.clone();
            let is_deepgram = matches!(
                transcription_backend,
                dabar_core::whisper::TranscriptionBackend::Deepgram { .. }
            );
            move |progress_pct: f32| {
                let pct = (progress_pct * 100.0) as u8;
                let detail = if pct < 20 {
                    "Preprocessing sermon audio (16kHz mono)…"
                } else if pct < 95 {
                    if is_deepgram {
                        "Transcribing sermon audio via Deepgram Nova-3…"
                    } else {
                        "Transcribing sermon audio via Groq Whisper…"
                    }
                } else {
                    "Stitching & aligning transcript segments…"
                };
                emit(&app_clone, sermon_id, "transcribing", pct.min(95), detail);
            }
        })),
    )
    .await?;

    // Clean up temporary work directory if applicable
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    let mut segments = transcription_res.segments;
    if !custom_vocab.trim().is_empty() {
        dabar_core::structuring::apply_custom_vocabulary(&mut segments, &custom_vocab);
    }

    if segments.is_empty() {
        anyhow::bail!("Transcription produced no text. The audio may be silent or unsupported.");
    }

    db.save_checkpoint(sermon_id, "transcribing", &audio_path.to_string_lossy())
        .await?;
    emit(
        &app,
        sermon_id,
        "transcribing",
        100,
        "Transcription complete.",
    );

    // ── Stage 3: Highlight & Chapter Analysis ─────────────────────────────────

    db.update_status(sermon_id, SermonStatus::Detecting).await?;
    emit(
        &app,
        sermon_id,
        "detecting",
        10,
        "Finding sermon chapters and key moments…",
    );

    let env_groq = std::env::var("GROQ_API_KEY").ok();
    let api_key_opt = if !api_key.trim().is_empty() {
        Some(api_key.trim())
    } else {
        env_groq.as_deref().filter(|k| !k.trim().is_empty())
    };

    let (
        highlights,
        chapters,
        highlight_status,
        highlight_error,
        total_candidates,
        passed_candidates,
    ) = {
        if offline_mode || api_key_opt.is_none() {
            emit(
                &app,
                sermon_id,
                "detecting",
                25,
                "Analyzing sermon audio volume & dynamic loudness peaks…",
            );
            let audio_profile = dabar_core::ffmpeg::detect_audio_loudness_peaks(&audio_path).await.ok();
            if let Some(prof) = &audio_profile {
                tracing::info!(
                    "Extracted audio loudness profile for sermon {sermon_id}: baseline {:.1} LUFS, {} peaks",
                    prof.integrated_loudness,
                    prof.peaks.len()
                );
            }
            emit(
                &app,
                sermon_id,
                "detecting",
                60,
                "Detecting scripture citations, key points & highlight clusters…",
            );
            let analysis = dabar_core::llm::analyze_sermon_offline_heuristics_with_audio(
                &segments,
                audio_profile.as_ref().map(|p| p.peaks.as_slice()),
            );
            let total_passed = analysis.highlights_report.total_passed;
            let chapter_count = analysis.chapters.len();
            emit(
                &app,
                sermon_id,
                "detecting",
                100,
                &format!(
                    "Generated {} topic chapters and {} key moments (offline heuristics).",
                    chapter_count, total_passed
                ),
            );
            (
                analysis.highlights_report.highlights,
                analysis.chapters,
                Some("heuristic".to_string()),
                Some("Generated via offline sermon heuristics and audio loudness analysis".to_string()),
                Some(analysis.highlights_report.total_proposed as u32),
                Some(total_passed as u32),
            )
        } else {
            match dabar_core::llm::analyze_sermon(api_key_opt, &segments).await {
                Ok(analysis) => {
                    let total_passed = analysis.highlights_report.total_passed;
                    let chapter_count = analysis.chapters.len();
                    emit(
                        &app,
                        sermon_id,
                        "detecting",
                        100,
                        &format!(
                            "Generated {} topic chapters and {} key moments.",
                            chapter_count, total_passed
                        ),
                    );
                    let status_str = match analysis.highlights_report.status {
                        dabar_core::llm::HighlightDetectionStatus::Success => "success",
                        dabar_core::llm::HighlightDetectionStatus::Heuristic => "heuristic",
                        dabar_core::llm::HighlightDetectionStatus::NoCandidatesProposed => {
                            "no_candidates"
                        }
                        dabar_core::llm::HighlightDetectionStatus::AllCandidatesFiltered => {
                            "all_filtered"
                        }
                        dabar_core::llm::HighlightDetectionStatus::Failed => "failed",
                    };
                    (
                        analysis.highlights_report.highlights,
                        analysis.chapters,
                        Some(status_str.to_string()),
                        analysis.highlights_report.error_message,
                        Some(analysis.highlights_report.total_proposed as u32),
                        Some(total_passed as u32),
                    )
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    tracing::warn!("Analysis fallback on sermon {sermon_id}: {err_msg}");
                    emit(
                        &app,
                        sermon_id,
                        "detecting",
                        50,
                        "Falling back to offline sermon heuristics & audio peak analysis…",
                    );
                    let audio_profile = dabar_core::ffmpeg::detect_audio_loudness_peaks(&audio_path).await.ok();
                    let fallback_analysis = dabar_core::llm::analyze_sermon_offline_heuristics_with_audio(
                        &segments,
                        audio_profile.as_ref().map(|p| p.peaks.as_slice()),
                    );
                    let total_passed = fallback_analysis.highlights_report.total_passed;
                    let chapter_count = fallback_analysis.chapters.len();
                    emit(
                        &app,
                        sermon_id,
                        "detecting",
                        100,
                        &format!(
                            "Generated {} chapters and {} moments.",
                            chapter_count, total_passed
                        ),
                    );
                    (
                        fallback_analysis.highlights_report.highlights,
                        fallback_analysis.chapters,
                        Some("heuristic".to_string()),
                        Some("Cloud AI unavailable, used sermon heuristics and audio loudness".to_string()),
                        Some(fallback_analysis.highlights_report.total_proposed as u32),
                        Some(total_passed as u32),
                    )
                }
            }
        }
    };

    // ── Stage 4: Save results ─────────────────────────────────────────────────

    let stored_title = match &source {
        PipelineSource::YouTube(_) | PipelineSource::GoogleDrive(_) => None,
        PipelineSource::LocalFile(p) => p
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()),
    };

    db.save_sermon_results(
        sermon_id,
        stored_title.as_deref(),
        Some(&audio_path.to_string_lossy()),
        &highlights,
        &chapters,
        &segments,
        highlight_status.as_deref(),
        highlight_error.as_deref(),
        total_candidates,
        passed_candidates,
    )
    .await?;

    db.delete_checkpoint(sermon_id).await?;

    tracing::info!("Pipeline completed successfully for sermon {sermon_id}");
    emit_complete(&app, sermon_id);
    Ok(())
}

/// Saves sermon results when highlight detection is skipped (no API key / no Ollama configured).
#[allow(dead_code)]
async fn save_no_highlights(
    app: &AppHandle,
    db: &Db,
    sermon_id: Uuid,
    source: &PipelineSource,
    audio_path: &Path,
    segments: &[dabar_core::TranscriptSegment],
    status: &str,
    message: &str,
) -> Result<()> {
    let stored_title = match source {
        PipelineSource::YouTube(_) | PipelineSource::GoogleDrive(_) => None,
        PipelineSource::LocalFile(p) => p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()),
    };

    db.save_sermon_results(
        sermon_id,
        stored_title.as_deref(),
        Some(&audio_path.to_string_lossy()),
        &[],
        &[],
        segments,
        Some(status),
        Some(message),
        None,
        Some(0),
    )
    .await?;

    db.delete_checkpoint(sermon_id).await?;
    tracing::info!("Pipeline completed (no highlights) for sermon {sermon_id}: {message}");
    emit_complete(app, sermon_id);
    Ok(())
}

/// Re-runs highlight detection on an already transcribed sermon.
pub async fn retry_highlights_pipeline(
    app: AppHandle,
    db: Db,
    sermon_id: Uuid,
    api_key: String,
) -> Result<Vec<dabar_core::Highlight>> {
    let sermon = db
        .get_sermon(sermon_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Sermon not found: {sermon_id}"))?;

    if sermon.transcript_segments.is_empty() {
        anyhow::bail!("This sermon has no transcript segments to analyze.");
    }

    emit(
        &app,
        sermon_id,
        "detecting",
        10,
        "Finding key sermon moments…",
    );

    let env_key = std::env::var("GROQ_API_KEY").ok();
    let api_key_opt = if !api_key.trim().is_empty() {
        Some(api_key.trim())
    } else {
        env_key.as_deref().filter(|k| !k.trim().is_empty())
    };

    let analysis =
        dabar_core::llm::analyze_sermon(api_key_opt, &sermon.transcript_segments).await?;
    let report = analysis.highlights_report;

    let status_str = match report.status {
        dabar_core::llm::HighlightDetectionStatus::Success => "success",
        dabar_core::llm::HighlightDetectionStatus::Heuristic => "heuristic",
        dabar_core::llm::HighlightDetectionStatus::NoCandidatesProposed => "no_candidates",
        dabar_core::llm::HighlightDetectionStatus::AllCandidatesFiltered => "all_filtered",
        dabar_core::llm::HighlightDetectionStatus::Failed => "failed",
    };

    db.update_highlights(
        sermon_id,
        &report.highlights,
        status_str,
        report.error_message.as_deref(),
        Some(report.total_proposed as u32),
        Some(report.total_passed as u32),
    )
    .await?;

    emit(
        &app,
        sermon_id,
        "detecting",
        100,
        &format!("Identified {} key moments.", report.total_passed),
    );
    Ok(report.highlights)
}

/// Handles a failed pipeline run: marks the sermon as failed in the DB,
/// emits an error event to the frontend, and logs the error.
pub async fn handle_pipeline_failure(
    app: &AppHandle,
    db: &Db,
    sermon_id: Uuid,
    err: anyhow::Error,
) {
    let msg = err.to_string();
    tracing::error!("Pipeline failed for sermon {sermon_id}: {err:?}");
    if let Err(db_err) = db.mark_failed(sermon_id, &msg).await {
        tracing::error!("Failed to record pipeline failure in DB: {db_err:?}");
    }
    emit_error(app, sermon_id, &msg);
}

/// Render a vertical video clip to disk (replaces HTTP streaming download).
/// Returns the path of the output file.
pub async fn render_clip_to_disk(
    sermon: &Sermon,
    highlight_id: Uuid,
    output_dir: &Path,
    aspect_ratio: Option<&str>,
    caption_style: Option<&str>,
) -> Result<PathBuf> {
    let highlight = sermon
        .highlights
        .iter()
        .find(|h| h.id == highlight_id)
        .ok_or_else(|| anyhow::anyhow!("Highlight {highlight_id} not found in sermon"))?;

    tokio::fs::create_dir_all(output_dir).await?;

    let raw_safe: String = highlight
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let safe_title = if raw_safe.trim_matches('_').is_empty() {
        format!(
            "clip_{:.0}s_{:.0}s",
            highlight.start_time, highlight.end_time
        )
    } else {
        raw_safe
    };
    let output_path = output_dir.join(format!("dabar_{safe_title}.mp4"));

    let ratio = aspect_ratio.unwrap_or("9:16");
    let style = caption_style.unwrap_or("amber");

    let is_youtube = sermon.youtube_url.starts_with("http")
        && (sermon.youtube_url.contains("youtube.com") || sermon.youtube_url.contains("youtu.be"));

    let temp_video_dir = output_dir.join(".temp_clips");

    if is_youtube {
        // Download ONLY the requested video section directly using yt-dlp
        let section_video_path = dabar_core::downloader::download_youtube_video_section(
            &sermon.youtube_url,
            highlight.start_time,
            highlight.end_time,
            &temp_video_dir,
        )
        .await?;

        let duration = highlight.end_time - highlight.start_time;

        let res = dabar_core::ffmpeg::extract_clip_with_timeline_offset(
            &section_video_path.to_string_lossy(),
            &output_path,
            0.0,
            duration,
            highlight.start_time,
            ratio,
            Some(&sermon.transcript_segments),
            Some(style),
        )
        .await;

        // NOTE: section_video_path is intentionally NOT deleted — it is keyed by
        // video_id+start+end in .temp_clips, so re-exporting the same clip with
        // a different caption style or aspect ratio will skip the yt-dlp download.
        res?;
    } else {
        // Local source (uploaded video or audio file)
        let input_source = if let Some(audio_p) = &sermon.audio_path {
            if Path::new(audio_p).exists() {
                audio_p.clone()
            } else {
                sermon.youtube_url.clone()
            }
        } else {
            sermon.youtube_url.clone()
        };

        dabar_core::ffmpeg::extract_clip(
            &input_source,
            &output_path,
            highlight.start_time,
            highlight.end_time,
            ratio,
            Some(&sermon.transcript_segments),
            Some(style),
        )
        .await?;
    }

    Ok(output_path)
}

/// Render an arbitrary time range clip to disk.
pub async fn render_clip_range_to_disk(
    sermon: &Sermon,
    start_time: f32,
    end_time: f32,
    clip_title: Option<&str>,
    output_dir: &Path,
    aspect_ratio: Option<&str>,
    caption_style: Option<&str>,
) -> Result<PathBuf> {
    if end_time <= start_time {
        anyhow::bail!("Invalid clip range: end time ({end_time:.2}s) must be greater than start time ({start_time:.2}s)");
    }

    tokio::fs::create_dir_all(output_dir).await?;

    let default_title = format!("clip_{:.0}s_{:.0}s", start_time, end_time);
    let raw_title = clip_title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(&default_title);
    let raw_safe: String = raw_title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let safe_title = if raw_safe.trim_matches('_').is_empty() {
        default_title
    } else {
        raw_safe
    };
    let output_path = output_dir.join(format!("dabar_{safe_title}.mp4"));

    let ratio = aspect_ratio.unwrap_or("9:16");
    let style = caption_style.unwrap_or("amber");

    let is_youtube = sermon.youtube_url.starts_with("http")
        && (sermon.youtube_url.contains("youtube.com") || sermon.youtube_url.contains("youtu.be"));

    let temp_video_dir = output_dir.join(".temp_clips");

    if is_youtube {
        let section_video_path = dabar_core::downloader::download_youtube_video_section(
            &sermon.youtube_url,
            start_time,
            end_time,
            &temp_video_dir,
        )
        .await?;

        let duration = end_time - start_time;

        let res = dabar_core::ffmpeg::extract_clip_with_timeline_offset(
            &section_video_path.to_string_lossy(),
            &output_path,
            0.0,
            duration,
            start_time,
            ratio,
            Some(&sermon.transcript_segments),
            Some(style),
        )
        .await;

        // section_video_path kept on disk — re-exports of same range skip yt-dlp
        res?;
    } else {
        let input_source = if let Some(audio_p) = &sermon.audio_path {
            if Path::new(audio_p).exists() {
                audio_p.clone()
            } else {
                sermon.youtube_url.clone()
            }
        } else {
            sermon.youtube_url.clone()
        };

        dabar_core::ffmpeg::extract_clip(
            &input_source,
            &output_path,
            start_time,
            end_time,
            ratio,
            Some(&sermon.transcript_segments),
            Some(style),
        )
        .await?;
    }

    Ok(output_path)
}
