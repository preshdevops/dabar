use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use dabar_core::Sermon;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health))
        .route("/health/", get(health))
        .route("/health", get(health))
        .route("/api/sermons/", get(list_sermons).post(create_sermon))
        .route("/api/sermons/{id}/", get(get_sermon))
        .route("/api/sermons/{id}/transcript/", get(get_transcript))
        .route("/api/sermons/{id}/transcribe/", post(queue_transcription))
        .route("/api/clips/{id}/download/", get(download_clip_by_id))
        .route("/api/clips/download/", get(download_clip_by_query))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct CreateSermonRequest {
    youtube_url: String,
}

#[derive(Debug, Deserialize)]
struct DownloadClipQuery {
    url: String,
    start: f32,
    end: f32,
}

#[derive(Debug, Serialize)]
struct ApiError {
    detail: String,
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "engine": "dabar-rust-axum"
    }))
}

async fn list_sermons(State(state): State<AppState>) -> impl IntoResponse {
    match state.list_sermons().await {
        Ok(sermons) => Json(sermons).into_response(),
        Err(error) => server_error(error),
    }
}

async fn create_sermon(
    State(state): State<AppState>,
    Json(payload): Json<CreateSermonRequest>,
) -> impl IntoResponse {
    let url = payload.youtube_url.trim().to_string();
    if url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                detail: "youtube_url is required.".to_string(),
            }),
        )
            .into_response();
    }

    let sermon = match state.insert_sermon(Sermon::queued(url.clone())).await {
        Ok(sermon) => sermon,
        Err(error) => return server_error(error),
    };

    // Clone owned variables before moving into the async move tokio::spawn task:
    // 1. `state_clone`: AppState wraps `AnyPool` (cheap pointer copy), needed for DB calls inside the spawned task.
    // 2. `sermon_id`: Uuid is Copy, identifies the sermon across pipeline stage DB writes.
    // 3. `youtube_url`: Owned String, required by yt-dlp to download sermon audio.
    let state_clone = state.clone();
    let sermon_id = sermon.id;
    let youtube_url = sermon.youtube_url.clone();

    tokio::spawn(async move {
        if let Err(err) = run_pipeline(state_clone.clone(), sermon_id, youtube_url).await {
            let err_msg = err.to_string();
            if err_msg.contains("confirm you're not a bot") || err_msg.contains("bot-detection") {
                tracing::error!(
                    "Sermon pipeline failed due to YouTube bot-detection for sermon {sermon_id}: {err:?}. To fix this on Render, export cookies.txt from a logged-in YouTube session using a browser extension ('Get cookies.txt LOCALLY'), upload the file to Render Secret Files (e.g. /etc/secrets/cookies.txt), and set YT_DLP_COOKIES_PATH=/etc/secrets/cookies.txt."
                );
            } else {
                tracing::error!("Sermon pipeline failed for sermon {sermon_id}: {err:?}");
            }
            if let Err(db_err) = state_clone.mark_failed(sermon_id, &err_msg).await {
                tracing::error!(
                    "Failed to record sermon failure in DB for {sermon_id}: {db_err:?}"
                );
            }
        }
    });

    (StatusCode::CREATED, Json(sermon)).into_response()
}

async fn run_pipeline(state: AppState, sermon_id: Uuid, youtube_url: String) -> anyhow::Result<()> {
    let groq_key = std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    let deepgram_key = std::env::var("DEEPGRAM_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());

    let (api_key, backend) = match (groq_key, deepgram_key) {
        (Some(gk), _) => (
            gk.clone(),
            dabar_core::whisper::TranscriptionBackend::Groq { api_key: gk },
        ),
        (None, Some(dk)) => (
            dk.clone(),
            dabar_core::whisper::TranscriptionBackend::Deepgram { api_key: dk },
        ),
        (None, None) => {
            anyhow::bail!("Neither GROQ_API_KEY nor DEEPGRAM_API_KEY environment variable is set")
        }
    };

    tracing::info!("🚀 [Pipeline Start] Processing sermon {sermon_id} from {youtube_url}");

    // Stage 1: Downloading
    tracing::info!("📥 [Stage 1/4: Downloading] Downloading audio from YouTube...");
    state
        .update_status(sermon_id, dabar_core::SermonStatus::Downloading)
        .await?;
    let temp_dir = std::env::temp_dir().join(format!("dabar_{sermon_id}"));
    let downloaded =
        dabar_core::downloader::download_youtube_audio(&youtube_url, &temp_dir).await?;

    if let Some(title) = &downloaded.title {
        let _ = state.update_title(sermon_id, title).await;
    }

    // Stage 2: Transcribing
    tracing::info!("🎙️ [Stage 2/4: Transcribing] Transcribing sermon audio...");
    state
        .update_status(sermon_id, dabar_core::SermonStatus::Transcribing)
        .await?;
    let trans_res =
        dabar_core::whisper::transcribe_audio(&backend, &downloaded.path, None, None).await?;
    let segments = trans_res.segments;

    // Clean up temporary audio file asynchronously
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    if segments.is_empty() {
        anyhow::bail!("Whisper transcription produced no text segments");
    }

    // Stage 3: Highlight Detection (LLM analysis with [HH:MM:SS] prompt formatting & 30-90s validation)
    tracing::info!(
        "🧠 [Stage 3/4: Highlight Detection] Extracting key pastoral moments with LLM..."
    );
    state
        .update_status(sermon_id, dabar_core::SermonStatus::Detecting)
        .await?;
    let analysis = dabar_core::llm::analyze_sermon(Some(&api_key), &segments).await?;
    let highlights = analysis.highlights_report.highlights;

    // Stage 4: Commit Ready State & Save Results to DB
    tracing::info!("💾 [Stage 4/4: Persistence] Saving sermon metadata, manuscript, and highlights to database...");
    state
        .save_sermon_results(
            sermon_id,
            downloaded.title.as_deref(),
            &highlights,
            &segments,
        )
        .await?;

    tracing::info!("🎉 [Pipeline Complete] Sermon {sermon_id} pipeline completed successfully!");
    Ok(())
}

async fn get_sermon(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.get_sermon(id).await {
        Ok(Some(sermon)) => Json(sermon).into_response(),
        Ok(None) => not_found("Sermon not found."),
        Err(error) => server_error(error),
    }
}

async fn get_transcript(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.get_sermon(id).await {
        Ok(Some(sermon)) => Json(sermon.transcript_segments).into_response(),
        Ok(None) => not_found("Sermon not found."),
        Err(error) => server_error(error),
    }
}

async fn queue_transcription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_sermon(id).await {
        Ok(Some(sermon)) => Json(sermon).into_response(),
        Ok(None) => not_found("Sermon not found."),
        Err(error) => server_error(error),
    }
}

async fn download_clip_by_id(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let (highlight, sermon) = match state.get_highlight_with_sermon(id).await {
        Ok(Some(pair)) => pair,
        Ok(None) => return not_found("Clip highlight not found."),
        Err(err) => return server_error(err),
    };

    if highlight.start_time < 0.0 || highlight.end_time <= highlight.start_time {
        return bad_request("Invalid clip duration bounds for highlight.");
    }

    let stream_url = match dabar_core::downloader::resolve_stream_url(&sermon.youtube_url).await {
        Ok(url) => url,
        Err(err) => {
            tracing::error!("Stream resolution failed for clip {id}: {err:?}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    detail: format!("Failed to resolve media stream URL: {err}"),
                }),
            )
                .into_response();
        }
    };

    let temp_path = std::env::temp_dir().join(format!("clip_{id}.mp4"));
    let extract_res = dabar_core::ffmpeg::extract_vertical_clip(
        &stream_url,
        &temp_path,
        highlight.start_time,
        highlight.end_time,
    )
    .await;

    if let Err(err) = extract_res {
        tracing::error!("FFmpeg clip extraction failed for clip {id}: {err:?}");
        let _ = tokio::fs::remove_file(&temp_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                detail: format!("FFmpeg clip rendering failed: {err}"),
            }),
        )
            .into_response();
    }

    let bytes = match tokio::fs::read(&temp_path).await {
        Ok(bytes) => bytes,
        Err(read_err) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return server_error(anyhow::anyhow!("Reading clip file failed: {read_err}"));
        }
    };

    let _ = tokio::fs::remove_file(&temp_path).await;

    let filename = format!("clip-{id}.mp4");
    let mut response = (StatusCode::OK, bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap_or_else(
            |_| HeaderValue::from_static("attachment; filename=\"dabar-clip.mp4\""),
        ),
    );
    response
}

async fn download_clip_by_query(Query(query): Query<DownloadClipQuery>) -> Response {
    if query.url.trim().is_empty() {
        return bad_request("url query parameter is required.");
    }

    if query.start < 0.0 || query.end <= query.start {
        return bad_request("Invalid start/end query parameters: start must be >= 0 and < end.");
    }

    let stream_url = match dabar_core::downloader::resolve_stream_url(&query.url).await {
        Ok(url) => url,
        Err(err) => {
            tracing::error!(
                "Stream resolution failed for query clip {}: {err:?}",
                query.url
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    detail: format!("Failed to resolve media stream URL: {err}"),
                }),
            )
                .into_response();
        }
    };

    let clip_id = Uuid::new_v4();
    let temp_path = std::env::temp_dir().join(format!("clip_{clip_id}.mp4"));
    let extract_res =
        dabar_core::ffmpeg::extract_vertical_clip(&stream_url, &temp_path, query.start, query.end)
            .await;

    if let Err(err) = extract_res {
        tracing::error!(
            "FFmpeg query clip extraction failed for {}: {err:?}",
            query.url
        );
        let _ = tokio::fs::remove_file(&temp_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                detail: format!("FFmpeg clip rendering failed: {err}"),
            }),
        )
            .into_response();
    }

    let bytes = match tokio::fs::read(&temp_path).await {
        Ok(bytes) => bytes,
        Err(read_err) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return server_error(anyhow::anyhow!("Reading clip file failed: {read_err}"));
        }
    };

    let _ = tokio::fs::remove_file(&temp_path).await;

    let filename = format!("dabar-clip-{:.0}s.mp4", query.start.max(0.0));
    let mut response = (StatusCode::OK, bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap_or_else(
            |_| HeaderValue::from_static("attachment; filename=\"dabar-clip.mp4\""),
        ),
    );
    response
}

fn bad_request(detail: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            detail: detail.to_string(),
        }),
    )
        .into_response()
}

fn not_found(detail: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            detail: detail.to_string(),
        }),
    )
        .into_response()
}

fn server_error(error: anyhow::Error) -> Response {
    tracing::error!("{error:?}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            detail: "Dabar could not complete that request.".to_string(),
        }),
    )
        .into_response()
}
