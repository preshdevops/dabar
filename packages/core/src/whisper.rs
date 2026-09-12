use crate::ffmpeg;
use crate::models::{Chapter, TranscriptSegment};
use anyhow::{Context, Result};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::Path;

const GROQ_TRANSCRIPTIONS_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const DEEPGRAM_TRANSCRIPTIONS_URL: &str = "https://api.deepgram.com/v1/listen";
const WHISPER_MODEL: &str = "whisper-large-v3-turbo";

/// Selects the cloud transcription engine.
///
/// - `Groq`: fast speech-to-text via Groq Whisper Large v3 Turbo (primary).
/// - `Deepgram`: speech-to-text via Deepgram Nova-3 (fallback).
#[derive(Debug, Clone)]
pub enum TranscriptionBackend {
    Groq { api_key: String },
    Deepgram { api_key: String },
}

/// The unified output of the transcription stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub segments: Vec<TranscriptSegment>,
    pub chapters: Vec<Chapter>,
}

const CHUNK_TRIGGER_BYTES: u64 = 3 * 1024 * 1024; // 3 MB (guarantees requests stay strictly under serverless 4.5MB limits)
const CHUNK_TRIGGER_DURATION_SECS: f32 = 420.0; // 7 minutes
const CHUNK_DURATION_SECS: f32 = 360.0; // 6 minutes (~2.1 MB per chunk)
const CHUNK_OVERLAP_SECS: f32 = 4.0;

#[derive(Debug, Deserialize)]
struct GroqTranscription {
    segments: Option<Vec<GroqSegment>>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqSegment {
    start: f32,
    end: f32,
    text: String,
}

// ── Deepgram Nova-3 Data Structures ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: Option<DeepgramResults>,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Option<Vec<DeepgramChannel>>,
    utterances: Option<Vec<DeepgramUtterance>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramUtterance {
    start: f32,
    end: f32,
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Option<Vec<DeepgramAlternative>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: Option<String>,
    paragraphs: Option<DeepgramParagraphsWrapper>,
    words: Option<Vec<DeepgramWord>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramParagraphsWrapper {
    paragraphs: Option<Vec<DeepgramParagraph>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramParagraph {
    sentences: Option<Vec<DeepgramSentence>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramSentence {
    text: String,
    start: f32,
    end: f32,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    word: String,
    start: f32,
    end: f32,
}

pub fn build_whisper_prompt(custom_vocab: Option<&str>) -> String {
    let mut prompt = "Sermon transcript in Nigerian English, Christian preaching, Bible exposition, scripture readings, Yoruba interjections (Hallelujah, Amen, Pastor, Apostle, Jesus Christ, Holy Spirit, Jehovah, Lord God, Bible).".to_string();
    if let Some(vocab) = custom_vocab {
        let clean = vocab.trim();
        if !clean.is_empty() {
            prompt.push_str(" Church Vocabulary: ");
            prompt.push_str(clean);
        }
    }
    prompt
}

pub async fn transcribe_audio(
    backend: &TranscriptionBackend,
    raw_audio_path: &Path,
    custom_vocab: Option<&str>,
    progress_callback: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<TranscriptionResult> {
    match backend {
        TranscriptionBackend::Groq { api_key } => {
            let deepgram_env = std::env::var("DEEPGRAM_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty());
            match transcribe_audio_groq(api_key, raw_audio_path, custom_vocab, progress_callback)
                .await
            {
                Ok(segments) => Ok(TranscriptionResult {
                    segments,
                    chapters: Vec::new(),
                }),
                Err(err) => {
                    if let Some(dk) = deepgram_env {
                        tracing::warn!("Groq Whisper returned error ({err}). Falling back to Deepgram Nova-3...");
                        let segments =
                            transcribe_audio_deepgram(&dk, raw_audio_path, custom_vocab, None)
                                .await?;
                        Ok(TranscriptionResult {
                            segments,
                            chapters: Vec::new(),
                        })
                    } else {
                        Err(err)
                    }
                }
            }
        }
        TranscriptionBackend::Deepgram { api_key } => {
            let segments =
                transcribe_audio_deepgram(api_key, raw_audio_path, custom_vocab, progress_callback)
                    .await?;
            Ok(TranscriptionResult {
                segments,
                chapters: Vec::new(),
            })
        }
    }
}

// ── Cloud Groq Whisper Implementation ───────────────────────────────────────

pub async fn transcribe_audio_groq(
    api_key: &str,
    raw_audio_path: &Path,
    custom_vocab: Option<&str>,
    progress_callback: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<TranscriptSegment>> {
    let temp_dir = std::env::temp_dir().join(format!("dabar_whisper_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .with_context(|| format!("creating temp directory {}", temp_dir.display()))?;

    if let Some(ref cb) = progress_callback {
        cb(0.05);
    }

    let result = transcribe_audio_internal(
        GROQ_TRANSCRIPTIONS_URL,
        "Groq Whisper",
        api_key,
        raw_audio_path,
        &temp_dir,
        custom_vocab,
        progress_callback,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    result
}

// ── Cloud Deepgram Nova-3 Implementation ────────────────────────────────────

pub async fn transcribe_audio_deepgram(
    api_key: &str,
    raw_audio_path: &Path,
    custom_vocab: Option<&str>,
    progress_callback: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<TranscriptSegment>> {
    let temp_dir = std::env::temp_dir().join(format!("dabar_deepgram_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .with_context(|| format!("creating temp directory {}", temp_dir.display()))?;
    let preprocessed_path = temp_dir.join("preprocessed_mono16k.mp3");

    tracing::info!("🔄 [Deepgram Nova-3] Preprocessing audio (mono 16kHz MP3)...");
    ffmpeg::preprocess_audio_for_whisper(raw_audio_path, &preprocessed_path).await?;

    if let Some(ref cb) = progress_callback {
        cb(0.2);
    }

    let bytes = tokio::fs::read(&preprocessed_path).await?;
    let duration = ffmpeg::get_media_duration(&preprocessed_path)
        .await
        .unwrap_or(0.0);
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("building reqwest client for Deepgram")?;

    let auth_header = if api_key.starts_with("Token ") || api_key.starts_with("Bearer ") {
        api_key.to_string()
    } else {
        format!("Token {api_key}")
    };

    let mut query_params: Vec<(&str, String)> = vec![
        ("model", "nova-3".to_string()),
        ("smart_format", "true".to_string()),
        ("punctuate", "true".to_string()),
        ("utterances", "true".to_string()),
        ("paragraphs", "true".to_string()),
    ];

    if let Some(vocab) = custom_vocab {
        for word in vocab.split(&[',', ';', '\n'][..]) {
            let w = word.trim();
            if !w.is_empty() {
                query_params.push(("keywords", w.to_string()));
            }
        }
    }

    tracing::info!(
        "🎙️ [Deepgram Nova-3] Submitting audio ({:.2} MB, duration: {:.1}s) to Deepgram Nova-3 API...",
        (bytes.len() as f64) / (1024.0 * 1024.0),
        duration
    );

    let resp = client
        .post(DEEPGRAM_TRANSCRIPTIONS_URL)
        .header("Authorization", auth_header)
        .header("Content-Type", "audio/mp3")
        .query(&query_params)
        .body(bytes)
        .send()
        .await
        .context("sending audio request to Deepgram API")?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        let snippet = if err_body.len() > 300 {
            &err_body[..300]
        } else {
            &err_body
        };
        anyhow::bail!("Deepgram API error ({status}): {snippet}");
    }

    let parsed: DeepgramResponse = resp
        .json()
        .await
        .context("parsing Deepgram response JSON")?;
    let segments = parse_deepgram_response(parsed, duration);

    if segments.is_empty() {
        anyhow::bail!("Deepgram Nova-3 transcription produced no text segments");
    }

    if let Some(ref cb) = progress_callback {
        cb(1.0);
    }

    tracing::info!(
        "✅ [Deepgram Nova-3] Transcription completed with {} segments.",
        segments.len()
    );
    Ok(segments)
}

fn parse_deepgram_response(
    parsed: DeepgramResponse,
    fallback_duration: f32,
) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();

    if let Some(results) = parsed.results {
        // 1. Prefer utterances (natural pauses with precise timestamps)
        if let Some(utterances) = results.utterances {
            for u in utterances {
                let text = u.transcript.trim().to_string();
                if !text.is_empty() {
                    segments.push(TranscriptSegment {
                        start: u.start,
                        end: u.end,
                        text,
                    });
                }
            }
            if !segments.is_empty() {
                return segments;
            }
        }

        // 2. Paragraph sentences
        if let Some(channels) = results.channels {
            if let Some(alt) = channels
                .first()
                .and_then(|c| c.alternatives.as_ref())
                .and_then(|a| a.first())
            {
                if let Some(pw) = &alt.paragraphs {
                    if let Some(paragraphs) = &pw.paragraphs {
                        for p in paragraphs {
                            if let Some(sentences) = &p.sentences {
                                for s in sentences {
                                    let text = s.text.trim().to_string();
                                    if !text.is_empty() {
                                        segments.push(TranscriptSegment {
                                            start: s.start,
                                            end: s.end,
                                            text,
                                        });
                                    }
                                }
                            }
                        }
                        if !segments.is_empty() {
                            return segments;
                        }
                    }
                }

                // 3. Words chunked into ~10 words
                if let Some(words) = &alt.words {
                    let mut current_words = Vec::new();
                    let mut seg_start = 0.0;
                    for (i, w) in words.iter().enumerate() {
                        if current_words.is_empty() {
                            seg_start = w.start;
                        }
                        current_words.push(w.word.as_str());
                        if current_words.len() >= 10 || i == words.len() - 1 {
                            segments.push(TranscriptSegment {
                                start: seg_start,
                                end: w.end,
                                text: current_words.join(" "),
                            });
                            current_words.clear();
                        }
                    }
                    if !segments.is_empty() {
                        return segments;
                    }
                }

                // 4. Raw alternative transcript
                if let Some(full) = &alt.transcript {
                    let text = full.trim().to_string();
                    if !text.is_empty() {
                        segments.push(TranscriptSegment {
                            start: 0.0,
                            end: fallback_duration,
                            text,
                        });
                    }
                }
            }
        }
    }

    segments
}

async fn transcribe_audio_internal(
    endpoint_url: &str,
    provider_name: &str,
    api_key: &str,
    raw_audio_path: &Path,
    temp_dir: &Path,
    custom_vocab: Option<&str>,
    progress_callback: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<TranscriptSegment>> {
    let preprocessed_path = temp_dir.join("preprocessed_mono16k.mp3");

    tracing::info!(
        "Preprocessing audio for {provider_name} (mono, 16kHz, 64kbps MP3): {}",
        raw_audio_path.display()
    );

    ffmpeg::preprocess_audio_for_whisper(raw_audio_path, &preprocessed_path).await?;

    if let Some(ref cb) = progress_callback {
        cb(0.15);
    }

    let metadata = tokio::fs::metadata(&preprocessed_path)
        .await
        .with_context(|| format!("reading metadata for {}", preprocessed_path.display()))?;

    let file_size = metadata.len();
    let size_mb = (file_size as f64) / (1024.0 * 1024.0);
    let duration = ffmpeg::get_media_duration(&preprocessed_path)
        .await
        .unwrap_or(0.0);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("building reqwest client")?;

    let needs_chunking = file_size > CHUNK_TRIGGER_BYTES || duration >= CHUNK_TRIGGER_DURATION_SECS;

    if !needs_chunking {
        tracing::info!(
            "Preprocessed audio fits in single request ({size_mb:.2} MB, duration: {duration:.1}s)"
        );
        let res = transcribe_single_audio_file(
            &client,
            endpoint_url,
            provider_name,
            api_key,
            &preprocessed_path,
            0.0,
            custom_vocab,
        )
        .await;
        if let Some(ref cb) = progress_callback {
            cb(1.0);
        }
        res
    } else {
        tracing::info!(
            "Preprocessed audio ({size_mb:.2} MB, duration: {duration:.1}s) triggers chunked transcription. Initiating parallel chunked flow via {provider_name}..."
        );
        transcribe_chunked_audio(
            &client,
            endpoint_url,
            provider_name,
            api_key,
            &preprocessed_path,
            temp_dir,
            duration,
            custom_vocab,
            progress_callback,
        )
        .await
    }
}

async fn transcribe_single_audio_file(
    client: &reqwest::Client,
    endpoint_url: &str,
    provider_name: &str,
    api_key: &str,
    audio_path: &Path,
    time_offset: f32,
    custom_vocab: Option<&str>,
) -> Result<Vec<TranscriptSegment>> {
    let bytes = tokio::fs::read(audio_path)
        .await
        .with_context(|| format!("reading audio file {}", audio_path.display()))?;

    let file_size = bytes.len() as u64;
    let size_mb = (file_size as f64) / (1024.0 * 1024.0);
    let filename = audio_path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("audio.mp3")
        .to_string();

    tracing::info!(
        "Sending audio to {provider_name}: {:.2} MB (file: '{}', offset: {:.2}s)",
        size_mb,
        filename,
        time_offset
    );

    let prompt = build_whisper_prompt(custom_vocab);

    let max_retries = 4;
    let mut last_err = anyhow::anyhow!("unknown transcription error");

    for attempt in 1..=max_retries {
        let part = Part::bytes(bytes.clone())
            .file_name(filename.clone())
            .mime_str("audio/mpeg")
            .context("creating multipart part")?;

        let form = Form::new()
            .text("model", WHISPER_MODEL)
            .text("response_format", "verbose_json")
            .text("prompt", prompt.clone())
            .part("file", part);

        match client
            .post(endpoint_url)
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                if status.is_success() {
                    let parsed: GroqTranscription =
                        res.json().await.context("parsing verbose_json response")?;

                    let mut segments = Vec::new();
                    if let Some(groq_segments) = parsed.segments {
                        for seg in groq_segments {
                            let start = seg.start + time_offset;
                            let end = seg.end + time_offset;
                            let text = seg.text.trim().to_string();
                            if !text.is_empty() {
                                segments.push(TranscriptSegment { start, end, text });
                            }
                        }
                    } else if let Some(full_text) = parsed.text {
                        let clean = full_text.trim().to_string();
                        if !clean.is_empty() {
                            let duration =
                                ffmpeg::get_media_duration(audio_path).await.unwrap_or(30.0);
                            segments.push(TranscriptSegment {
                                start: time_offset,
                                end: time_offset + duration,
                                text: clean,
                            });
                        }
                    }
                    return Ok(segments);
                }

                let raw_body = res.text().await.unwrap_or_default();
                let body = if raw_body.starts_with("<!DOCTYPE") || raw_body.contains("<html") {
                    if status.as_u16() == 404 {
                        format!("Endpoint not found (404) at {endpoint_url}. The provider does not support audio transcription on this endpoint.")
                    } else {
                        format!("Server returned HTTP {status}")
                    }
                } else {
                    let len = raw_body.len().min(300);
                    raw_body[..len].to_string()
                };

                let is_retryable = status.as_u16() == 429 || status.is_server_error();
                if is_retryable && attempt < max_retries {
                    let backoff = std::time::Duration::from_secs(2u64.pow(attempt));
                    tracing::warn!(
                        "{provider_name} API returned HTTP {status} (attempt {attempt}/{max_retries}). Retrying in {backoff:?}... Details: {body}"
                    );
                    tokio::time::sleep(backoff).await;
                    last_err = anyhow::anyhow!("{provider_name} API error ({status}): {body}");
                    continue;
                } else {
                    anyhow::bail!("{provider_name} API error ({status}): {body}");
                }
            }
            Err(e) => {
                let backoff = std::time::Duration::from_secs(2u64.pow(attempt));
                tracing::warn!(
                    "{provider_name} request network error on attempt {attempt}/{max_retries}: {e}. Retrying in {backoff:?}..."
                );
                last_err =
                    anyhow::anyhow!("sending transcription request to {provider_name} API: {e}");
                if attempt < max_retries {
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    Err(last_err)
}

async fn transcribe_chunked_audio(
    client: &reqwest::Client,
    endpoint_url: &str,
    provider_name: &str,
    api_key: &str,
    preprocessed_path: &Path,
    temp_dir: &Path,
    total_duration: f32,
    custom_vocab: Option<&str>,
    progress_callback: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<TranscriptSegment>> {
    let mut chunk_plan = Vec::new();
    let mut current_start = 0.0;

    while current_start < total_duration {
        let chunk_dur = CHUNK_DURATION_SECS.min(total_duration - current_start);
        chunk_plan.push((current_start, chunk_dur));
        current_start += CHUNK_DURATION_SECS - CHUNK_OVERLAP_SECS;
    }

    let total_chunks = chunk_plan.len();
    tracing::info!(
        "Audio duration: {total_duration:.1}s. Splitting into {total_chunks} overlapping chunks for {provider_name}."
    );

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut handles = Vec::new();

    for (idx, (start, dur)) in chunk_plan.into_iter().enumerate() {
        let chunk_path = temp_dir.join(format!("chunk_{idx:03}.mp3"));
        let source_path = preprocessed_path.to_path_buf();
        let client_clone = client.clone();
        let url_clone = endpoint_url.to_string();
        let provider_clone = provider_name.to_string();
        let key_clone = api_key.to_string();
        let vocab_clone = custom_vocab.map(str::to_string);
        let sem = semaphore.clone();
        let chunk_num = idx + 1;

        let handle = tokio::spawn(async move {
            ffmpeg::extract_audio_chunk(&source_path, &chunk_path, start, dur)
                .await
                .with_context(|| {
                    format!("extracting chunk {chunk_num}/{total_chunks} at {start:.1}s")
                })?;

            let _permit = sem
                .acquire_owned()
                .await
                .context("acquiring transcription permit")?;
            tracing::info!(
                "🎙️ [Transcribing Chunk {chunk_num}/{total_chunks}] Processing audio slice {start:.1}s - {:.1}s...",
                start + dur
            );
            let res = transcribe_single_audio_file(
                &client_clone,
                &url_clone,
                &provider_clone,
                &key_clone,
                &chunk_path,
                start,
                vocab_clone.as_deref(),
            )
            .await;
            if let Ok(ref segs) = res {
                tracing::info!(
                    "✅ [Chunk {chunk_num}/{total_chunks}] Transcribed successfully ({} segments)",
                    segs.len()
                );
            }
            let _ = tokio::fs::remove_file(&chunk_path).await;
            let segments =
                res.with_context(|| format!("transcribing chunk at offset {start:.1}s"))?;
            Ok::<_, anyhow::Error>((start, segments))
        });

        handles.push(handle);
    }

    let mut chunk_results = Vec::new();
    for handle in handles {
        chunk_results.push(
            handle
                .await
                .context("joining chunk transcription thread")??,
        );
    }

    if let Some(ref cb) = progress_callback {
        cb(1.0);
    }

    let stitched = stitch_transcript_chunks(chunk_results);
    tracing::info!(
        "🧵 [Transcript Stitched] Aligned {} seamless transcript segments.",
        stitched.len()
    );

    Ok(stitched)
}

pub fn stitch_transcript_chunks(
    mut chunk_results: Vec<(f32, Vec<TranscriptSegment>)>,
) -> Vec<TranscriptSegment> {
    chunk_results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut stitched: Vec<TranscriptSegment> = Vec::new();

    for (_offset, segments) in chunk_results {
        for seg in segments {
            if let Some(last) = stitched.last() {
                if seg.start <= last.end + 0.3 {
                    if seg.end <= last.end + 0.3 {
                        continue;
                    }
                    stitched.push(TranscriptSegment {
                        start: last.end,
                        end: seg.end,
                        text: seg.text,
                    });
                    continue;
                }
            }
            stitched.push(seg);
        }
    }

    stitched
}
