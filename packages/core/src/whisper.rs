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
    #[serde(default)]
    no_speech_prob: Option<f32>,
    #[serde(default)]
    compression_ratio: Option<f32>,
    #[serde(default)]
    avg_logprob: Option<f32>,
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
    let mut prompt = "Hallelujah, Amen. Praise the Lord Jesus Christ. Let us open our Bibles to the Word of God today. Pastor, Apostle, Holy Spirit, Jehovah God.".to_string();
    if let Some(vocab) = custom_vocab {
        let clean = vocab.trim();
        if !clean.is_empty() {
            prompt.push_str(" ");
            prompt.push_str(clean);
        }
    }
    prompt
}

pub fn clean_segment_text(raw: &str) -> String {
    let mut text = raw.trim().to_string();

    // Remove musical notes and symbols
    text = text.replace(['♪', '♫', '♬', '♩'], "");

    // Remove common inline bracketed sound tags
    let tags = [
        "[music]", "(music)", "[applause]", "(applause)",
        "[laughter]", "(laughter)", "[silence]", "(silence)",
        "[cheering]", "(cheering)", "[singing]", "(singing)",
    ];
    for tag in tags {
        while let Some(pos) = text.to_lowercase().find(tag) {
            let mut new_text = text[..pos].to_string();
            new_text.push_str(&text[pos + tag.len()..]);
            text = new_text;
        }
    }

    text.trim().to_string()
}

pub fn is_hallucination_or_music(
    text: &str,
    no_speech_prob: Option<f32>,
    compression_ratio: Option<f32>,
    avg_logprob: Option<f32>,
) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    // 1. Model confidence / no-speech probability checks (from Whisper verbose_json)
    if let Some(nsp) = no_speech_prob {
        // Whisper indicates silence, background music, or non-speech noise
        if nsp > 0.60 {
            return true;
        }
        // Elevated no-speech prob with poor acoustic confidence
        if nsp > 0.35 && avg_logprob.unwrap_or(0.0) < -0.80 {
            return true;
        }
    }

    // 2. High compression ratio indicates degenerate repetition loops
    if let Some(cr) = compression_ratio {
        if cr > 2.2 {
            return true;
        }
    }

    // 3. Extremely poor acoustic confidence
    if let Some(lp) = avg_logprob {
        if lp < -1.40 {
            return true;
        }
    }

    // 4. Pure non-speech / music markers
    let lower = trimmed.to_lowercase();
    let stripped_symbols = lower
        .replace(['♪', '♫', '♬', '♩', '*', '#', '-', '_', '~', ' '], "")
        .trim()
        .to_string();
    if stripped_symbols.is_empty() {
        return true;
    }

    // Check bracketed music/sound tags
    if (lower.starts_with('[') && lower.ends_with(']'))
        || (lower.starts_with('(') && lower.ends_with(')'))
    {
        let inner = lower[1..lower.len() - 1].trim();
        if inner.contains("music")
            || inner.contains("applause")
            || inner.contains("laughter")
            || inner.contains("cheering")
            || inner.contains("silence")
            || inner.contains("instrumental")
            || inner.contains("singing")
            || inner.contains("sound")
            || inner.is_empty()
        {
            return true;
        }
    }

    // 5. Common YouTube subtitle credits and web hallucination artifacts
    const CREDIT_HALLUCINATIONS: &[&str] = &[
        "subtitles by",
        "subtitles made by",
        "subtitles created by",
        "subtitled by",
        "captioned by",
        "captions by",
        "closed captions by",
        "translated by",
        "transcribed by",
        "transcript by",
        "reading text in",
        "community contributor",
        "amara.org",
        "opensubtitles",
        "youtube.com",
        "like and subscribe",
        "thanks for watching",
        "thank you for watching",
        "subscribe to my channel",
        "subscribe to the channel",
        "bell icon",
        "see you next time",
        "all rights reserved",
        "http://",
        "https://",
        "www.",
    ];

    for credit in CREDIT_HALLUCINATIONS {
        if lower.contains(credit) {
            return true;
        }
    }

    // 6. Prompt echo detection
    const PROMPT_ECHOES: &[&str] = &[
        "sermon transcript",
        "nigerian english",
        "church vocabulary",
        "bible exposition",
        "yoruba interjections",
    ];
    for echo in PROMPT_ECHOES {
        if lower.contains(echo) {
            return true;
        }
    }

    // 7. Repetition loops & degenerate sequences
    let words: Vec<&str> = trimmed
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();

    if words.len() >= 3 {
        let lower_words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();

        // 7a. Single word dominating the segment (>= 70% of words, at least 4 occurrences)
        let mut word_counts = std::collections::HashMap::new();
        for w in &lower_words {
            *word_counts.entry(w.as_str()).or_insert(0usize) += 1;
        }
        if let Some((_, &max_count)) = word_counts.iter().max_by_key(|(_, &c)| c) {
            if max_count >= 4 && (max_count as f32 / lower_words.len() as f32) >= 0.70 {
                return true;
            }
        }

        // 7b. Consecutive 2-gram repetitions (e.g. "The Merec, The Merec, The Merec.")
        if lower_words.len() >= 6 {
            let mut repeat_count = 1;
            for i in (0..lower_words.len().saturating_sub(3)).step_by(2) {
                if lower_words[i] == lower_words[i + 2] && lower_words[i + 1] == lower_words[i + 3] {
                    repeat_count += 1;
                    if repeat_count >= 3 {
                        return true;
                    }
                } else {
                    repeat_count = 1;
                }
            }
        }

        // 7c. Low vocabulary diversity for longer segments
        if lower_words.len() >= 6 {
            let unique_ratio = (word_counts.len() as f32) / (lower_words.len() as f32);
            if unique_ratio < 0.35 {
                return true;
            }
        }
    }

    // 8. Consonant salad / Gibberish detection
    // e.g. "CKG, SLP H5 R6 Rea Apoosa... CTZ, H2 OMS TRIGS, MDT, PARA, KWDIEN, PPT, PARA..."
    if words.len() >= 4 {
        let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'A' | 'E' | 'I' | 'O' | 'U' | 'Y');
        let words_without_vowels = words
            .iter()
            .filter(|w| w.len() >= 2 && !w.chars().any(is_vowel))
            .count();

        let non_single_letter_words = words.iter().filter(|w| w.len() >= 2).count();
        if non_single_letter_words >= 3 && (words_without_vowels as f32 / non_single_letter_words as f32) >= 0.30 {
            return true;
        }

        // All-caps acronym clustering (e.g. CKG, SLP, CTZ, MDT, PARA, KWDIEN, PPT...)
        let all_caps_words = words
            .iter()
            .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_uppercase() || c.is_numeric()))
            .count();
        if non_single_letter_words >= 4 && (all_caps_words as f32 / non_single_letter_words as f32) >= 0.65 {
            return true;
        }
    }

    false
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
                let clean = clean_segment_text(&u.transcript);
                if !clean.is_empty() && !is_hallucination_or_music(&clean, None, None, None) {
                    segments.push(TranscriptSegment {
                        start: u.start,
                        end: u.end,
                        text: clean,
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
                                    let clean = clean_segment_text(&s.text);
                                    if !clean.is_empty()
                                        && !is_hallucination_or_music(&clean, None, None, None)
                                    {
                                        segments.push(TranscriptSegment {
                                            start: s.start,
                                            end: s.end,
                                            text: clean,
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
                            let text = current_words.join(" ");
                            let clean = clean_segment_text(&text);
                            if !clean.is_empty()
                                && !is_hallucination_or_music(&clean, None, None, None)
                            {
                                segments.push(TranscriptSegment {
                                    start: seg_start,
                                    end: w.end,
                                    text: clean,
                                });
                            }
                            current_words.clear();
                        }
                    }
                    if !segments.is_empty() {
                        return segments;
                    }
                }

                // 4. Raw alternative transcript
                if let Some(full) = &alt.transcript {
                    let clean = clean_segment_text(full);
                    if !clean.is_empty() && !is_hallucination_or_music(&clean, None, None, None) {
                        segments.push(TranscriptSegment {
                            start: 0.0,
                            end: fallback_duration,
                            text: clean,
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
                            if is_hallucination_or_music(
                                &seg.text,
                                seg.no_speech_prob,
                                seg.compression_ratio,
                                seg.avg_logprob,
                            ) {
                                tracing::info!(
                                    "Filtering out music/hallucination segment [{:.2}s - {:.2}s]: \"{}\" (no_speech_prob={:?}, compression_ratio={:?}, avg_logprob={:?})",
                                    seg.start + time_offset,
                                    seg.end + time_offset,
                                    seg.text.trim(),
                                    seg.no_speech_prob,
                                    seg.compression_ratio,
                                    seg.avg_logprob,
                                );
                                continue;
                            }

                            let start = seg.start + time_offset;
                            let end = seg.end + time_offset;
                            let text = clean_segment_text(&seg.text);
                            if !text.is_empty() {
                                segments.push(TranscriptSegment { start, end, text });
                            }
                        }
                    } else if let Some(full_text) = parsed.text {
                        if !is_hallucination_or_music(&full_text, None, None, None) {
                            let clean = clean_segment_text(&full_text);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hallucination_or_music_youtube_credits() {
        let text = "Subtitles made by Jnkoil Lormar reading text in Nigerian English TCG John Lee Kankaraman Chilana Digo Stardom to Gordand Shurda Hydebo Loon Bible Podcasts";
        assert!(is_hallucination_or_music(text, None, None, None));

        assert!(is_hallucination_or_music("Subtitles by Community Contributor", None, None, None));
        assert!(is_hallucination_or_music("Closed captions by amara.org", None, None, None));
        assert!(is_hallucination_or_music("Thanks for watching, like and subscribe!", None, None, None));
    }

    #[test]
    fn test_is_hallucination_or_music_repetition_loops() {
        let repeating_phrase = "The Merec, The Merec, The Merec.";
        assert!(is_hallucination_or_music(repeating_phrase, None, None, None));

        let single_word_loop = "Amen. Amen. Amen. Amen. Amen.";
        assert!(is_hallucination_or_music(single_word_loop, None, None, None));
    }

    #[test]
    fn test_is_hallucination_or_music_consonant_salad() {
        let salad = "CKG, SLP H5 R6 Rea Apoosa, Bibernese, Tarniawongens... CTZ, H2 OMS TRIGS, MDT, PARA, KWDIEN, PPT, PARA, KULAGOSY, MIGATEI, ELMoHA GOLBYEN";
        assert!(is_hallucination_or_music(salad, None, None, None));
    }

    #[test]
    fn test_is_hallucination_or_music_probabilities() {
        // High no_speech_prob
        assert!(is_hallucination_or_music("Some murmur", Some(0.85), None, None));

        // High no_speech_prob combined with poor logprob
        assert!(is_hallucination_or_music("Some whisper", Some(0.45), None, Some(-0.95)));

        // High compression ratio
        assert!(is_hallucination_or_music("A loop text", None, Some(2.4), None));

        // Valid speech
        let valid = "In the beginning was the Word, and the Word was with God, and the Word was God.";
        assert!(!is_hallucination_or_music(valid, Some(0.02), Some(1.2), Some(-0.15)));
    }

    #[test]
    fn test_is_hallucination_or_music_music_tags() {
        assert!(is_hallucination_or_music("[Music]", None, None, None));
        assert!(is_hallucination_or_music("(music)", None, None, None));
        assert!(is_hallucination_or_music("♪ ♫ ♬ ♩", None, None, None));
        assert!(is_hallucination_or_music("[Applause]", None, None, None));
    }

    #[test]
    fn test_clean_segment_text() {
        assert_eq!(
            clean_segment_text("♪ Jesus loves me this I know ♪"),
            "Jesus loves me this I know"
        );
        assert_eq!(
            clean_segment_text("[Music] Hallelujah Jesus"),
            "Hallelujah Jesus"
        );
        assert_eq!(clean_segment_text("♪ ♫ ♬"), "");
    }

    #[test]
    fn test_valid_sermon_speech_is_not_filtered() {
        assert!(!is_hallucination_or_music(
            "Praise the Lord Jesus Christ, let us open our Bibles to the Book of Psalms.",
            None,
            None,
            None
        ));
        assert!(!is_hallucination_or_music(
            "Holy, Holy, Holy is the Lord God Almighty, who was and is and is to come.",
            None,
            None,
            None
        ));
        assert!(!is_hallucination_or_music(
            "Amen.",
            None,
            None,
            None
        ));
    }
}
