use crate::ffmpeg::{compute_segment_audio_energy, AudioPeak};
use crate::models::{Chapter, Highlight, TranscriptSegment};
use crate::structuring::detect_scripture_references;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const GROQ_CHAT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

// Groq-hosted chat models, ordered from strongest to fastest fallback.
// Prioritizes active Groq production models while keeping Llama fallbacks.
pub const GROQ_MODELS: &[&str] = &[
    "openai/gpt-oss-120b",
    "openai/gpt-oss-20b",
    "qwen/qwen3.8-27b",
    "llama-3.3-70b-versatile",
    "llama-3.1-8b-instant",
];

// OpenAI models
pub const OPENAI_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini", "o3-mini"];

// OpenRouter models (if OPENROUTER_API_KEY is provided)
pub const OPENROUTER_MODELS: &[&str] = &[
    "openai/gpt-oss-120b",
    "openai/gpt-oss-20b",
    "anthropic/claude-3.5-sonnet",
];

pub const CLIP_MIN_SECS: f32 = 30.0;
pub const CLIP_MAX_SECS: f32 = 90.0;
// Cloud prompt constraint
const MAX_PROMPT_WORDS: usize = 1_400;

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}
#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}
#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightDetectionStatus {
    Success,
    Heuristic,
    NoCandidatesProposed,
    AllCandidatesFiltered,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscardedCandidate {
    pub title: String,
    pub start_time: Option<f32>,
    pub end_time: Option<f32>,
    pub duration: Option<f32>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightDetectionReport {
    pub highlights: Vec<Highlight>,
    pub total_proposed: usize,
    pub total_passed: usize,
    pub discarded: Vec<DiscardedCandidate>,
    pub status: HighlightDetectionStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SermonAnalysisResult {
    pub chapters: Vec<Chapter>,
    pub highlights_report: HighlightDetectionReport,
}

pub fn format_timestamp(seconds: f32) -> String {
    let total_secs = seconds.max(0.0) as u32;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    format!("[{:02}:{:02}:{:02}]", hours, minutes, secs)
}

pub fn format_segments_to_prompt(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .map(|seg| format!("{} {}", format_timestamp(seg.start), seg.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn condense_transcript_to_words(segments: &[TranscriptSegment], max_words: usize) -> String {
    let full = format_segments_to_prompt(segments);
    let word_count: usize = full.split_whitespace().count();
    if word_count <= max_words {
        return full;
    }
    tracing::info!("Transcript ~{word_count} words — condensing to ~{max_words} for LLM");
    let scores: Vec<f32> = segments.iter().map(segment_importance_score).collect();
    let avg_words: f32 = (word_count as f32 / segments.len() as f32).max(1.0);
    let target_segs = (max_words as f32 / avg_words) as usize;
    let step = (segments.len() / target_segs.max(1)).max(1);
    let mut selected: Vec<usize> = vec![0];
    let mut i = step;
    while i < segments.len().saturating_sub(1) {
        let end = (i + step).min(segments.len());
        let best = (i..end)
            .max_by(|&a, &b| {
                scores[a]
                    .partial_cmp(&scores[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(i);
        selected.push(best);
        i += step;
    }
    if segments.len() > 1 {
        selected.push(segments.len() - 1);
    }
    selected.sort_unstable();
    selected.dedup();
    let mut lines = Vec::new();
    let mut prev: Option<usize> = None;
    let mut accumulated_words = 0;
    for idx in &selected {
        let line_text = format!(
            "{} {}",
            format_timestamp(segments[*idx].start),
            segments[*idx].text.trim()
        );
        let line_words = line_text.split_whitespace().count();
        if accumulated_words + line_words > max_words && !lines.is_empty() {
            break;
        }
        if let Some(p) = prev {
            if *idx > p + 1 {
                let gap = segments[*idx].start - segments[p].end;
                lines.push(format!("  ... [{:.0}s omitted] ...", gap));
            }
        }
        accumulated_words += line_words;
        lines.push(line_text);
        prev = Some(*idx);
    }
    lines.join("\n")
}

fn condense_transcript(segments: &[TranscriptSegment]) -> String {
    condense_transcript_to_words(segments, MAX_PROMPT_WORDS)
}

pub fn extract_json_payload(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            return &trimmed[start..=end];
        }
    }
    trimmed
}

pub fn parse_timestamp_value(val: &serde_json::Value) -> Option<f32> {
    if let Some(n) = val.as_f64() {
        return Some(n as f32);
    }
    if let Some(s) = val.as_str() {
        let t = s.trim().trim_matches('[').trim_matches(']');
        let p: Vec<&str> = t.split(':').collect();
        if p.len() == 3 {
            let h: f32 = p[0].parse().ok()?;
            let m: f32 = p[1].parse().ok()?;
            let s: f32 = p[2].parse().ok()?;
            return Some(h * 3600.0 + m * 60.0 + s);
        } else if p.len() == 2 {
            let m: f32 = p[0].parse().ok()?;
            let s: f32 = p[1].parse().ok()?;
            return Some(m * 60.0 + s);
        } else if let Ok(v) = t.parse::<f32>() {
            return Some(v);
        }
    }
    None
}

pub fn validate_chapters(mut chapters: Vec<Chapter>) -> Vec<Chapter> {
    if chapters.is_empty() {
        return Vec::new();
    }
    chapters.sort_by(|a, b| {
        a.start_time
            .partial_cmp(&b.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut validated = Vec::new();
    for ch in chapters {
        if ch.end_time <= ch.start_time {
            continue;
        }
        if let Some(prev) = validated.last_mut() {
            let p: &mut Chapter = prev;
            if ch.start_time < p.end_time {
                p.end_time = ch.start_time;
            }
        }
        validated.push(ch);
    }
    validated
}

pub fn parse_and_validate_chapters(json_value: &serde_json::Value) -> Vec<Chapter> {
    let arr = match json_value.get("chapters").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut result = Vec::new();
    for item in arr {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Sermon Section")
            .trim()
            .to_string();
        let summary = item
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let start = item
            .get("start_time")
            .or_else(|| item.get("start_timestamp"))
            .and_then(parse_timestamp_value);
        let end = item
            .get("end_time")
            .or_else(|| item.get("end_timestamp"))
            .and_then(parse_timestamp_value);
        if let (Some(s), Some(e)) = (start, end) {
            if e > s {
                result.push(Chapter {
                    id: Uuid::new_v4(),
                    title,
                    summary,
                    start_time: s,
                    end_time: e,
                });
            }
        }
    }
    validate_chapters(result)
}

pub fn parse_and_validate_highlights_detailed(
    json_value: &serde_json::Value,
) -> (Vec<Highlight>, Vec<DiscardedCandidate>, usize) {
    let arr = match json_value
        .get("clips")
        .and_then(|v| v.as_array())
        .or_else(|| json_value.get("highlights").and_then(|v| v.as_array()))
    {
        Some(a) => a,
        None => return (Vec::new(), Vec::new(), 0),
    };
    let total = arr.len();
    let mut valid = Vec::new();
    let mut disc = Vec::new();
    for item in arr {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Sermon Highlight")
            .trim()
            .to_string();
        let start = match item
            .get("start_timestamp")
            .or_else(|| item.get("start_time"))
            .and_then(parse_timestamp_value)
        {
            Some(t) => t,
            None => {
                disc.push(DiscardedCandidate {
                    title,
                    start_time: None,
                    end_time: None,
                    duration: None,
                    reason: "Missing start".to_string(),
                });
                continue;
            }
        };
        let end = match item
            .get("end_timestamp")
            .or_else(|| item.get("end_time"))
            .and_then(parse_timestamp_value)
        {
            Some(t) => t,
            None => {
                disc.push(DiscardedCandidate {
                    title,
                    start_time: Some(start),
                    end_time: None,
                    duration: None,
                    reason: "Missing end".to_string(),
                });
                continue;
            }
        };
        let reason = item
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("High-impact moment.")
            .trim()
            .to_string();
        let hook = item
            .get("suggested_hook_text")
            .or_else(|| item.get("hook_text"))
            .and_then(|v| v.as_str())
            .unwrap_or(&title)
            .trim()
            .to_string();
        let score = item
            .get("score")
            .and_then(|v| v.as_f64())
            .map(|f| (f as f32).clamp(0.0, 1.0))
            .unwrap_or(0.90);
        if end <= start {
            disc.push(DiscardedCandidate {
                title,
                start_time: Some(start),
                end_time: Some(end),
                duration: Some(end - start),
                reason: "end <= start".to_string(),
            });
            continue;
        }
        let dur = end - start;
        if dur < CLIP_MIN_SECS {
            disc.push(DiscardedCandidate {
                title,
                start_time: Some(start),
                end_time: Some(end),
                duration: Some(dur),
                reason: format!("{dur:.0}s < min {CLIP_MIN_SECS}s"),
            });
            continue;
        }
        // No upper clamp — preserve the full natural duration of the moment
        valid.push(Highlight {
            id: Uuid::new_v4(),
            title,
            start_time: start,
            end_time: end,
            score,
            reason,
            suggested_hook_text: hook,
        });
    }
    (valid, disc, total)
}

pub fn parse_and_validate_highlights(json_value: &serde_json::Value) -> Vec<Highlight> {
    parse_and_validate_highlights_detailed(json_value).0
}

pub async fn analyze_sermon(
    api_key: Option<&str>,
    segments: &[TranscriptSegment],
) -> Result<SermonAnalysisResult> {
    if segments.is_empty() {
        return Ok(SermonAnalysisResult {
            chapters: Vec::new(),
            highlights_report: HighlightDetectionReport {
                highlights: Vec::new(),
                total_proposed: 0,
                total_passed: 0,
                discarded: Vec::new(),
                status: HighlightDetectionStatus::NoCandidatesProposed,
                error_message: None,
            },
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .context("building client")?;
    let condensed = condense_transcript(segments);
    let total_dur = segments.last().map(|s| s.end).unwrap_or(0.0);
    let sys = format!(
        r#"You are a pastoral editor. Analyze the timestamped sermon transcript and return JSON:
{{"chapters":[{{"title":"3-7 word title","summary":"section overview","start_timestamp":0.0,"end_timestamp":300.0}}],"clips":[{{"title":"3-7 word title","start_timestamp":45.0,"end_timestamp":345.0,"reason":"why this impacts listeners","suggested_hook_text":"key quote"}}]}}
Rules: 3-8 chapters spanning the full {total_dur:.0}s sermon. 3-6 clips with NO upper time limit — a powerful altar call or testimony may run 5-10 minutes, preserve it fully. Minimum 60s per clip. Use exact timestamps from the transcript."#
    );
    let usr = format!("Analyze this sermon transcript:\n\n{condensed}");

    // 1. Try Groq.
    let groq_env = std::env::var("GROQ_API_KEY").ok();
    let groq_key = if let Some(k) =
        api_key.filter(|k| k.starts_with("gsk_") || (!k.starts_with("sk-") && !k.trim().is_empty()))
    {
        Some(k)
    } else {
        groq_env.as_deref().filter(|k| !k.trim().is_empty())
    };

    if let Some(gk) = groq_key {
        for &model in GROQ_MODELS {
            match try_chat_completion(&client, GROQ_CHAT_URL, gk, model, &sys, &usr).await {
                Ok(Some(r)) => {
                    tracing::info!("LLM succeeded: Groq/{model}");
                    return Ok(r);
                }
                Ok(None) => tracing::warn!("Groq/{model}: no usable JSON"),
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("413")
                        || err_str.contains("Request too large")
                        || err_str.contains("TPM")
                    {
                        tracing::warn!(
                            "Groq/{model} TPM exceeded — retrying with ultra-compact prompt..."
                        );
                        let compact_usr = format!(
                            "Analyze this sermon transcript:\n\n{}",
                            condense_transcript_to_words(segments, 700)
                        );
                        match try_chat_completion(
                            &client,
                            GROQ_CHAT_URL,
                            gk,
                            model,
                            &sys,
                            &compact_usr,
                        )
                        .await
                        {
                            Ok(Some(r)) => {
                                tracing::info!("LLM succeeded on compact retry: Groq/{model}");
                                return Ok(r);
                            }
                            Ok(None) => tracing::warn!("Groq/{model} (compact): no usable JSON"),
                            Err(e2) => tracing::warn!("Groq/{model} (compact retry): {e2}"),
                        }
                    } else {
                        tracing::warn!("Groq/{model}: {e}");
                    }
                }
            }
        }
    }

    // 2. Try OpenAI if OpenAI API key is provided
    let openai_env = std::env::var("OPENAI_API_KEY").ok();
    let openai_key =
        if let Some(k) = api_key.filter(|k| k.starts_with("sk-") && !k.starts_with("sk-or-")) {
            Some(k)
        } else {
            openai_env.as_deref().filter(|k| !k.trim().is_empty())
        };

    if let Some(ok) = openai_key {
        for &model in OPENAI_MODELS {
            match try_chat_completion(&client, OPENAI_CHAT_URL, ok, model, &sys, &usr).await {
                Ok(Some(r)) => {
                    tracing::info!("LLM succeeded: OpenAI/{model}");
                    return Ok(r);
                }
                Ok(None) => tracing::warn!("OpenAI/{model}: no usable JSON"),
                Err(e) => tracing::warn!("OpenAI/{model}: {e}"),
            }
        }
    }

    // 3. Try OpenRouter fallback if key is configured
    let or_env = std::env::var("OPENROUTER_API_KEY").ok();
    let or_key = if let Some(k) = api_key.filter(|k| k.starts_with("sk-or-")) {
        Some(k)
    } else {
        or_env.as_deref().filter(|k| !k.trim().is_empty())
    };

    if let Some(ork) = or_key {
        for &model in OPENROUTER_MODELS {
            match try_chat_completion(&client, OPENROUTER_CHAT_URL, ork, model, &sys, &usr).await {
                Ok(Some(r)) => {
                    tracing::info!("LLM succeeded: OpenRouter/{model}");
                    return Ok(r);
                }
                Ok(None) => tracing::warn!("OpenRouter/{model}: no usable JSON"),
                Err(e) => tracing::warn!("OpenRouter/{model}: {e}"),
            }
        }
    }

    tracing::info!(
        "All cloud LLMs unavailable or not configured — using upgraded offline heuristics."
    );
    Ok(analyze_sermon_offline_heuristics(segments))
}

async fn try_chat_completion(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Option<SermonAnalysisResult>> {
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 3500,
        "response_format": {"type": "json_object"}
    });
    let mut req = client.post(url).json(&body);
    if !api_key.trim().is_empty() {
        req = req.bearer_auth(api_key);
    }
    if url.contains("openrouter") {
        req = req
            .header("HTTP-Referer", "https://dabar.app")
            .header("X-Title", "Dabar");
    }
    let resp = req
        .send()
        .await
        .with_context(|| format!("request to {url}"))?;
    let status = resp.status();
    if status.as_u16() == 429 {
        let b = resp.text().await.unwrap_or_default();
        anyhow::bail!("rate limited: {}", &b[..b.len().min(200)]);
    }
    if !status.is_success() {
        let b = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status}: {}", &b[..b.len().min(200)]);
    }
    let cr = resp
        .json::<ChatResponse>()
        .await
        .context("parsing response")?;
    let content = match cr.choices.first() {
        Some(c) => &c.message.content,
        None => return Ok(None),
    };
    let clean_json = extract_json_payload(content);
    let json_val = match serde_json::from_str::<serde_json::Value>(clean_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to parse JSON response from {model}: {e}. Raw content: {content}");
            return Ok(None);
        }
    };
    let chapters = parse_and_validate_chapters(&json_val);
    let (highlights, discarded, total_proposed) = parse_and_validate_highlights_detailed(&json_val);
    let total_passed = highlights.len();
    if chapters.is_empty() && highlights.is_empty() {
        return Ok(None);
    }
    Ok(Some(SermonAnalysisResult {
        chapters,
        highlights_report: HighlightDetectionReport {
            highlights,
            total_proposed,
            total_passed,
            discarded,
            status: if total_passed > 0 {
                HighlightDetectionStatus::Success
            } else {
                HighlightDetectionStatus::AllCandidatesFiltered
            },
            error_message: None,
        },
    }))
}

pub fn segment_importance_score(seg: &TranscriptSegment) -> f32 {
    let text = seg.text.to_lowercase();
    let mut score: f32 = 0.0;

    // 1. Scripture citations
    let refs = detect_scripture_references(&seg.text, seg.start);
    if !refs.is_empty() {
        score += 0.40;
        if refs.len() > 1 {
            score += 0.15;
        }
    }
    for p in [
        "open your bibles",
        "turn with me to",
        "turn to",
        "the bible says",
        "scripture says",
        "in the book of",
        "look at verse",
        "hear the word of the lord",
        "the word of god",
        "chapter and verse",
    ] {
        if text.contains(p) {
            score += 0.15;
            break;
        }
    }

    // 2. Structural sermon markers & key points
    for p in [
        "point number one",
        "point number two",
        "point number three",
        "point number four",
        "point number five",
        "first point",
        "second point",
        "third point",
        "my first point",
        "my second point",
        "my third point",
        "first of all",
        "firstly",
        "secondly",
        "thirdly",
        "finally",
        "in conclusion",
        "to conclude",
        "in closing",
        "the key is",
        "the main thing",
        "principle number",
        "write this down",
    ] {
        if text.contains(p) {
            score += 0.25;
            break;
        }
    }

    // 3. Imperative & attention calls
    for p in [
        "listen to me",
        "listen closely",
        "look at me",
        "hear me",
        "don't miss this",
        "do not miss this",
        "you cannot miss this",
        "pay attention",
        "somebody needs to hear this",
        "mark my words",
        "catch this",
        "i want you to hear this",
        "take this down",
        "you must",
        "you need to",
        "you have to",
        "don't give up",
        "stand up",
        "rise up",
        "hold on",
        "receive this",
    ] {
        if text.contains(p) {
            score += 0.20;
            break;
        }
    }

    // 4. Theological declarations & affirmations
    for p in [
        "the lord told me",
        "god told me",
        "the lord said to me",
        "the spirit of the lord",
        "holy spirit",
        "by the grace of god",
        "grace of god",
        "jesus said",
        "jesus christ",
        "the blood of jesus",
        "salvation",
        "born again",
        "eternal life",
        "the cross",
        "resurrection",
        "he is risen",
        "god so loved",
        "i declare",
        "i prophesy",
        "i decree",
        "in the name of jesus",
        "in jesus' name",
        "god is able",
        "god is going to",
    ] {
        if text.contains(p) {
            score += 0.20;
            break;
        }
    }

    // 5. Cadence, punctuation, rhetorical emphasis
    score += (seg.text.matches('?').count() as f32 * 0.10).min(0.20);
    score += (seg.text.matches('!').count() as f32 * 0.10).min(0.20);

    // Repetition check for oratorical cadence
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() >= 4 {
        let mut counts = std::collections::HashMap::new();
        for w in &words {
            if w.len() > 3 {
                *counts.entry(w).or_insert(0usize) += 1;
            }
        }
        let reps: usize = counts.values().filter(|&&c| c >= 2).sum();
        score += (reps as f32 * 0.05).min(0.20);
    }

    // Sub-second or tiny fragment dampening
    let dur = (seg.end - seg.start).max(0.0);
    if dur < 2.5 {
        score *= 0.4;
    } else if dur < 5.0 {
        score *= 0.75;
    }

    score.clamp(0.0, 1.0)
}

fn detect_chapter_boundaries(segments: &[TranscriptSegment], target: usize) -> Vec<usize> {
    if segments.len() < 4 || target <= 1 {
        return Vec::new();
    }
    let win = (segments.len() / (target * 2)).max(3);
    let word_set = |segs: &[TranscriptSegment]| -> std::collections::HashSet<String> {
        segs.iter()
            .flat_map(|s| s.text.split_whitespace().map(|w| w.to_lowercase()))
            .filter(|w| w.len() > 4)
            .collect()
    };
    let mut scores: Vec<(usize, f32)> = Vec::new();
    let mut i = win;
    while i + win < segments.len() {
        let a = word_set(&segments[i.saturating_sub(win)..i]);
        let b = word_set(&segments[i..(i + win).min(segments.len())]);
        if !a.is_empty() && !b.is_empty() {
            let inter = a.intersection(&b).count() as f32;
            let uni = a.union(&b).count() as f32;
            scores.push((i, 1.0 - if uni > 0.0 { inter / uni } else { 0.0 }));
        }
        i += win;
    }
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut boundaries: Vec<usize> = scores
        .iter()
        .take(target - 1)
        .map(|(idx, _)| *idx)
        .collect();
    boundaries.sort_unstable();
    boundaries
}

fn derive_chapter_title(segments: &[TranscriptSegment], fallback: &str) -> String {
    for seg in segments.iter().take(10) {
        if let Some(r) = detect_scripture_references(&seg.text, seg.start)
            .into_iter()
            .next()
        {
            return format!("Teaching from {}", r.reference);
        }
    }
    let best = segments.iter().max_by(|a, b| {
        segment_importance_score(a)
            .partial_cmp(&segment_importance_score(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(seg) = best {
        let words: Vec<&str> = seg.text.split_whitespace().take(8).collect();
        if words.len() >= 4 {
            let mut phrase = words.join(" ");
            if phrase.len() > 50 {
                phrase.truncate(50);
                if let Some(p) = phrase.rfind(' ') {
                    phrase.truncate(p);
                }
            }
            phrase = phrase
                .trim_end_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            if !phrase.is_empty() {
                return phrase;
            }
        }
    }
    fallback.to_string()
}

fn derive_clip_title(segs: &[&TranscriptSegment]) -> String {
    // 1. Check for scripture references in the clip
    for seg in segs {
        let refs = detect_scripture_references(&seg.text, seg.start);
        if let Some(r) = refs.first() {
            let lower = seg.text.to_lowercase();
            for theme in [
                "all things work together",
                "god is able",
                "by the grace of god",
                "grace of god",
                "love is patient",
                "fear not",
                "faith moves mountains",
                "he is risen",
                "salvation",
            ] {
                if lower.contains(theme) {
                    let cased = theme
                        .split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    return format!("{}: {}", r.reference, cased);
                }
            }
            return format!("Teaching: {}", r.reference);
        }
    }

    // 2. Structural key point
    for seg in segs {
        let lower = seg.text.to_lowercase();
        if lower.contains("point number one") || lower.contains("first point") || lower.contains("firstly") {
            return format_clip_marker_title("Point 1", seg);
        } else if lower.contains("point number two") || lower.contains("second point") || lower.contains("secondly") {
            return format_clip_marker_title("Point 2", seg);
        } else if lower.contains("point number three") || lower.contains("third point") || lower.contains("thirdly") {
            return format_clip_marker_title("Point 3", seg);
        } else if lower.contains("finally") || lower.contains("in conclusion") || lower.contains("in closing") {
            return format_clip_marker_title("In Conclusion", seg);
        }
    }

    // 3. Imperative / Attention call
    for seg in segs {
        let lower = seg.text.to_lowercase();
        if lower.contains("listen to me") || lower.contains("listen closely") || lower.contains("don't miss this") || lower.contains("hear me") {
            return format_clip_marker_title("Listen Closely", seg);
        } else if lower.contains("write this down") || lower.contains("take this down") {
            return format_clip_marker_title("Take Note", seg);
        }
    }

    // 4. Theological declaration
    for seg in segs {
        let lower = seg.text.to_lowercase();
        if lower.contains("the lord told me") || lower.contains("god told me") {
            return format_clip_marker_title("Prophetic Word", seg);
        } else if lower.contains("by the grace of god") || lower.contains("grace of god") {
            return format_clip_marker_title("The Grace of God", seg);
        } else if lower.contains("jesus said") || lower.contains("jesus christ") {
            return format_clip_marker_title("Declaration of Faith", seg);
        }
    }

    // 5. Oratorical punchline fallback
    let best = segs.iter().max_by(|a, b| {
        segment_importance_score(a)
            .partial_cmp(&segment_importance_score(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(seg) = best {
        let words: Vec<&str> = seg.text.split_whitespace().take(7).collect();
        if words.len() >= 3 {
            let mut p = words.join(" ");
            p = p
                .trim_end_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            if !p.is_empty() {
                return p;
            }
        }
    }
    "Key Preaching Moment".to_string()
}

fn format_clip_marker_title(label: &str, seg: &TranscriptSegment) -> String {
    let words: Vec<&str> = seg.text.split_whitespace().collect();
    let phrase = words.iter().take(6).copied().collect::<Vec<_>>().join(" ");
    let cleaned = phrase.trim_end_matches(|c: char| !c.is_alphanumeric());
    if cleaned.len() > 10 {
        format!("{}: {}", label, cleaned)
    } else {
        label.to_string()
    }
}

fn derive_clip_reason(segs: &[&TranscriptSegment], max_audio_energy: f32) -> String {
    let mut scriptures = Vec::new();
    let mut has_key_point = false;
    let mut has_imperative = false;
    let mut has_theological = false;

    for seg in segs {
        let refs = detect_scripture_references(&seg.text, seg.start);
        for r in refs {
            if !scriptures.contains(&r.reference) {
                scriptures.push(r.reference);
            }
        }
        let lower = seg.text.to_lowercase();
        if lower.contains("point number")
            || lower.contains("first point")
            || lower.contains("second point")
            || lower.contains("finally")
            || lower.contains("in conclusion")
        {
            has_key_point = true;
        }
        if lower.contains("listen to me")
            || lower.contains("look at verse")
            || lower.contains("write this down")
            || lower.contains("don't miss this")
            || lower.contains("hear the word")
        {
            has_imperative = true;
        }
        if lower.contains("the lord told me")
            || lower.contains("by the grace of god")
            || lower.contains("jesus said")
            || lower.contains("salvation")
            || lower.contains("blood of jesus")
        {
            has_theological = true;
        }
    }

    let audio_note = if max_audio_energy >= 0.45 {
        " paired with dynamic vocal elevation and acoustic emphasis"
    } else if max_audio_energy >= 0.25 {
        " with heightened oratorical energy"
    } else {
        ""
    };

    if !scriptures.is_empty() {
        let sc_str = scriptures.join(", ");
        if has_imperative {
            format!("Scripture focus on {sc_str} with an urgent attention call{audio_note}.")
        } else if has_key_point {
            format!("Structural teaching from {sc_str}{audio_note}.")
        } else {
            format!("Powerful scripture exposition on {sc_str}{audio_note}.")
        }
    } else if has_key_point {
        if has_imperative {
            format!("Core sermon key point delivered with direct audience engagement{audio_note}.")
        } else {
            format!("Structural sermon principle highlighting key takeaway{audio_note}.")
        }
    } else if has_imperative {
        format!("Urgent oratorical exhortation and call to action{audio_note}.")
    } else if has_theological {
        format!("Theological declaration of faith and conviction{audio_note}.")
    } else {
        format!("High-impact preaching moment with oratorical emphasis{audio_note}.")
    }
}

fn derive_clip_hook(segs: &[&TranscriptSegment], scores: &[f32], title: &str) -> String {
    if let Some((best_seg, _)) = segs
        .iter()
        .zip(scores.iter())
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    {
        let trimmed = best_seg.text.trim();
        if trimmed.len() <= 120 {
            return trimmed.to_string();
        }
        let truncated: String = trimmed.chars().take(117).collect();
        return format!("{truncated}...");
    }
    title.to_string()
}

/// Analyzes a sermon transcript using offline sermon heuristics (scripture citations,
/// structural markers, attention calls, theological affirmations) combined with optional
/// FFmpeg acoustic loudness peaks. Clusters consecutive high-value segments into coherent
/// 30-90 second highlight clips with natural boundaries.
pub fn analyze_sermon_offline_heuristics_with_audio(
    segments: &[TranscriptSegment],
    audio_peaks: Option<&[AudioPeak]>,
) -> SermonAnalysisResult {
    if segments.is_empty() {
        return SermonAnalysisResult {
            chapters: Vec::new(),
            highlights_report: HighlightDetectionReport {
                highlights: Vec::new(),
                total_proposed: 0,
                total_passed: 0,
                discarded: Vec::new(),
                status: HighlightDetectionStatus::NoCandidatesProposed,
                error_message: None,
            },
        };
    }

    let total_dur = segments.last().map(|s| s.end).unwrap_or(0.0);
    let text_scores: Vec<f32> = segments.iter().map(segment_importance_score).collect();
    let audio_energies: Vec<f32> = segments
        .iter()
        .map(|s| {
            audio_peaks
                .map(|p| compute_segment_audio_energy(p, s.start, s.end))
                .unwrap_or(0.0)
        })
        .collect();

    let combined_scores: Vec<f32> = text_scores
        .iter()
        .zip(audio_energies.iter())
        .map(|(&t, &a)| (t * 0.70 + a * 0.30).clamp(0.0, 1.0))
        .collect();

    // Identify candidate anchor moments:
    // Any segment with high combined score or containing scripture or key point or attention call
    let mut candidate_clips: Vec<(usize, usize, f32)> = Vec::new(); // (start_idx, end_idx, composite_score)

    for i in 0..segments.len() {
        let seg = &segments[i];
        let has_refs = !detect_scripture_references(&seg.text, seg.start).is_empty();
        let lower = seg.text.to_lowercase();
        let is_marker = lower.contains("point number")
            || lower.contains("first point")
            || lower.contains("second point")
            || lower.contains("finally")
            || lower.contains("in conclusion")
            || lower.contains("listen to me")
            || lower.contains("don't miss this")
            || lower.contains("the lord told me")
            || lower.contains("by the grace of god");

        if combined_scores[i] < 0.18 && !has_refs && !is_marker {
            continue;
        }

        // Determine start boundary: check if previous segment provided natural lead-in
        let start_idx = if i > 0 {
            let prev_text = segments[i - 1].text.to_lowercase();
            if prev_text.contains("listen to me")
                || prev_text.contains("look at verse")
                || prev_text.contains("turn to")
                || prev_text.contains("open your bibles")
                || prev_text.contains("point number")
            {
                i - 1
            } else {
                i
            }
        } else {
            i
        };

        let start_time = segments[start_idx].start;

        // Find best end boundary within [CLIP_MIN_SECS, CLIP_MAX_SECS]
        let mut best_boundary: Option<(usize, f32)> = None; // (end_idx, boundary_score)
        let mut last_valid_idx: Option<usize> = None;

        for j in start_idx..segments.len() {
            let dur = segments[j].end - start_time;
            if dur < CLIP_MIN_SECS {
                continue;
            }
            if dur > CLIP_MAX_SECS {
                break;
            }

            last_valid_idx = Some(j);

            // Score this potential boundary
            let trimmed = segments[j].text.trim_end();
            let ends_punct = trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?');
            let pause_after = if j + 1 < segments.len() {
                segments[j + 1].start - segments[j].end
            } else {
                1.0
            };

            // Sweet spot for short-form highlights is 45-75 seconds
            let sweet_spot_score = 1.0 - ((dur - 60.0).abs() / 40.0).clamp(0.0, 1.0);
            let boundary_score = sweet_spot_score * 0.40
                + if ends_punct { 0.35 } else { 0.0 }
                + if pause_after >= 0.4 { 0.25 } else { 0.0 };

            if let Some((_, best_sc)) = best_boundary {
                if boundary_score > best_sc {
                    best_boundary = Some((j, boundary_score));
                }
            } else {
                best_boundary = Some((j, boundary_score));
            }
        }

        let final_end_idx = match best_boundary {
            Some((idx, _)) => idx,
            None => match last_valid_idx {
                Some(idx) => idx,
                None => continue,
            },
        };

        let clip_dur = segments[final_end_idx].end - start_time;
        if clip_dur < CLIP_MIN_SECS {
            continue;
        }

        // Calculate clip composite score
        let clip_combined: &[f32] = &combined_scores[start_idx..=final_end_idx];
        let avg_combined = clip_combined.iter().sum::<f32>() / clip_combined.len() as f32;

        let mut has_scripture = false;
        let mut has_marker = false;
        for s in &segments[start_idx..=final_end_idx] {
            if !detect_scripture_references(&s.text, s.start).is_empty() {
                has_scripture = true;
            }
            let l = s.text.to_lowercase();
            if l.contains("point number")
                || l.contains("listen to me")
                || l.contains("don't miss this")
                || l.contains("the lord told me")
                || l.contains("by the grace of god")
            {
                has_marker = true;
            }
        }

        let max_audio = audio_energies[start_idx..=final_end_idx]
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);

        let composite = (0.75
            + avg_combined * 0.12
            + if has_scripture { 0.08 } else { 0.0 }
            + if has_marker { 0.06 } else { 0.0 }
            + max_audio * 0.06)
            .clamp(0.78, 0.98);

        candidate_clips.push((start_idx, final_end_idx, composite));
    }

    // Sort candidate clips by composite score descending
    candidate_clips.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let target_clips = if total_dur > 3600.0 {
        6
    } else if total_dur > 1200.0 {
        5
    } else {
        3
    };

    let mut highlights: Vec<Highlight> = Vec::new();
    let min_gap_secs = 35.0_f32;

    for (s_idx, e_idx, score) in candidate_clips {
        if highlights.len() >= target_clips {
            break;
        }

        let start = segments[s_idx].start;
        let end = segments[e_idx].end;

        // Verify non-overlapping with existing selected highlights
        let overlaps = highlights.iter().any(|h| {
            let gap_before = h.start_time - end;
            let gap_after = start - h.end_time;
            gap_before < min_gap_secs && gap_after < min_gap_secs
        });

        if overlaps {
            continue;
        }

        let clip_segs: Vec<&TranscriptSegment> = segments[s_idx..=e_idx].iter().collect();
        let clip_scores = &combined_scores[s_idx..=e_idx];
        let max_audio = audio_energies[s_idx..=e_idx]
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);

        let title = derive_clip_title(&clip_segs);
        let reason = derive_clip_reason(&clip_segs, max_audio);
        let hook = derive_clip_hook(&clip_segs, clip_scores, &title);

        highlights.push(Highlight {
            id: Uuid::new_v4(),
            title,
            start_time: start,
            end_time: end,
            score,
            reason,
            suggested_hook_text: hook,
        });
    }

    // Fallback if no highlights met threshold but sermon duration is sufficient
    if highlights.is_empty() && total_dur >= CLIP_MIN_SECS {
        let count = 3_usize.min((total_dur / 300.0) as usize + 1);
        let interval = total_dur / (count as f32 + 1.0);
        let clip_target_dur = 60.0_f32.min(CLIP_MAX_SECS);

        for i in 1..=count {
            let s = interval * i as f32;
            let e = (s + clip_target_dur).min(total_dur);
            if e >= s + CLIP_MIN_SECS {
                highlights.push(Highlight {
                    id: Uuid::new_v4(),
                    title: format!("Key Moment · Part {i}"),
                    start_time: s,
                    end_time: e,
                    score: 0.80,
                    reason: "Evenly-spaced preaching segment with core thematic content.".to_string(),
                    suggested_hook_text: "Key sermon moment.".to_string(),
                });
            }
        }
    }

    // Sort highlights by chronological start time
    highlights.sort_by(|a, b| {
        a.start_time
            .partial_cmp(&b.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let target_chs = if total_dur > 3600.0 {
        8
    } else if total_dur > 1800.0 {
        6
    } else if total_dur > 600.0 {
        4
    } else {
        3
    };
    let boundaries = detect_chapter_boundaries(segments, target_chs);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut prev = 0;
    for &b in &boundaries {
        if b > prev {
            ranges.push((prev, b));
            prev = b;
        }
    }
    ranges.push((prev, segments.len()));
    let names = [
        "Introduction & Opening",
        "Scripture & Context",
        "Core Teaching",
        "Illustration & Application",
        "Altar Call & Exhortation",
        "Testimony & Encouragement",
        "Prayer & Intercession",
        "Closing & Benediction",
    ];
    let mut chapters: Vec<Chapter> = ranges
        .iter()
        .enumerate()
        .filter_map(|(i, (si, ei))| {
            let ch = &segments[*si..*ei];
            if ch.is_empty() {
                return None;
            }
            let cs = ch.first().unwrap().start;
            let ce = ch.last().unwrap().end;
            if ce <= cs {
                return None;
            }
            let fb = names.get(i).copied().unwrap_or("Teaching Section");
            Some(Chapter {
                id: Uuid::new_v4(),
                title: derive_chapter_title(ch, fb),
                summary: format!("From {} to {}.", format_timestamp(cs), format_timestamp(ce)),
                start_time: cs,
                end_time: ce,
            })
        })
        .collect();

    if chapters.is_empty() {
        let interval = 360.0_f32;
        let n = ((total_dur / interval).ceil() as usize).max(1);
        for i in 0..n {
            let cs = i as f32 * interval;
            let ce = ((i + 1) as f32 * interval).min(total_dur);
            if ce > cs {
                chapters.push(Chapter {
                    id: Uuid::new_v4(),
                    title: names
                        .get(i)
                        .copied()
                        .unwrap_or("Teaching Section")
                        .to_string(),
                    summary: format!("From {} to {}.", format_timestamp(cs), format_timestamp(ce)),
                    start_time: cs,
                    end_time: ce,
                });
            }
        }
    }

    let tp = highlights.len();
    SermonAnalysisResult {
        chapters,
        highlights_report: HighlightDetectionReport {
            highlights,
            total_proposed: tp,
            total_passed: tp,
            discarded: Vec::new(),
            status: HighlightDetectionStatus::Heuristic,
            error_message: Some("Cloud AI unavailable, used keyword detection".to_string()),
        },
    }
}

pub fn analyze_sermon_offline_heuristics(segments: &[TranscriptSegment]) -> SermonAnalysisResult {
    analyze_sermon_offline_heuristics_with_audio(segments, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_importance_score_scripture() {
        let seg_with_ref = TranscriptSegment {
            start: 10.0,
            end: 18.0,
            text: "Open your bibles and turn to Romans 8:28, where we know all things work together for good!".to_string(),
        };
        let seg_plain = TranscriptSegment {
            start: 18.0,
            end: 26.0,
            text: "We parked the car on the street yesterday morning.".to_string(),
        };

        let score_ref = segment_importance_score(&seg_with_ref);
        let score_plain = segment_importance_score(&seg_plain);

        assert!(
            score_ref >= 0.50,
            "Scripture citation + intro + exclamation should yield high score, got {score_ref}"
        );
        assert!(
            score_plain < 0.20,
            "Plain conversational segment should score low, got {score_plain}"
        );
    }

    #[test]
    fn test_segment_importance_score_markers_and_imperatives() {
        let seg_key_point = TranscriptSegment {
            start: 30.0,
            end: 38.0,
            text: "Point number one: by the grace of God, you are victorious in Christ!".to_string(),
        };
        let seg_imperative = TranscriptSegment {
            start: 40.0,
            end: 48.0,
            text: "Listen to me! Write this down right now: don't miss this truth!".to_string(),
        };

        let score_kp = segment_importance_score(&seg_key_point);
        let score_imp = segment_importance_score(&seg_imperative);

        assert!(
            score_kp >= 0.40,
            "Key point + theological declaration should score high, got {score_kp}"
        );
        assert!(
            score_imp >= 0.40,
            "Attention call + imperative + exclamation should score high, got {score_imp}"
        );
    }

    #[test]
    fn test_offline_clustering_bounds_and_duration() {
        // Build a synthetic 3-minute sermon with a clear central preaching moment
        let mut segments = Vec::new();
        let mut t = 0.0_f32;

        // Intro (0 to 45s)
        for i in 0..9 {
            segments.push(TranscriptSegment {
                start: t,
                end: t + 5.0,
                text: format!("Good morning church, welcome to service part {i}."),
            });
            t += 5.0;
        }

        // Climax moment with scripture & imperative (45s to 105s = 60s span)
        segments.push(TranscriptSegment {
            start: t,
            end: t + 6.0,
            text: "Listen to me closely church. Open your bibles to John 3:16.".to_string(),
        });
        t += 6.0;

        segments.push(TranscriptSegment {
            start: t,
            end: t + 8.0,
            text: "For God so loved the world that He gave His only begotten Son!".to_string(),
        });
        t += 8.0;

        segments.push(TranscriptSegment {
            start: t,
            end: t + 10.0,
            text: "Point number one: by the grace of God you have eternal life through Jesus Christ!".to_string(),
        });
        t += 10.0;

        segments.push(TranscriptSegment {
            start: t,
            end: t + 10.0,
            text: "Don't miss this! Write this down in your heart forever.".to_string(),
        });
        t += 10.0;

        segments.push(TranscriptSegment {
            start: t,
            end: t + 8.0,
            text: "Whoever believes in Him shall not perish but have everlasting peace.".to_string(),
        });
        t += 8.0;

        // Conclusion
        while t < 180.0 {
            segments.push(TranscriptSegment {
                start: t,
                end: (t + 6.0).min(180.0),
                text: "Let us pray as we conclude our gathering today.".to_string(),
            });
            t += 6.0;
        }

        let result = analyze_sermon_offline_heuristics(&segments);
        assert!(!result.highlights_report.highlights.is_empty(), "Should generate highlights");

        for hl in &result.highlights_report.highlights {
            let dur = hl.end_time - hl.start_time;
            assert!(
                dur >= CLIP_MIN_SECS - 0.1,
                "Clip duration {dur:.1}s must be >= min {CLIP_MIN_SECS}s"
            );
            assert!(
                dur <= CLIP_MAX_SECS + 0.1,
                "Clip duration {dur:.1}s must be <= max {CLIP_MAX_SECS}s"
            );
            assert!(
                hl.score >= 0.75 && hl.score <= 1.0,
                "Highlight score must be valid, got {}",
                hl.score
            );
            assert!(!hl.title.is_empty(), "Highlight title should not be empty");
            assert!(!hl.reason.is_empty(), "Highlight reason should not be empty");
        }

        // Check that John 3:16 was caught in the highlights
        let found_scripture = result
            .highlights_report
            .highlights
            .iter()
            .any(|h| h.title.contains("John 3:16") || h.reason.contains("John 3:16"));
        assert!(found_scripture, "Climax highlight should detect John 3:16");
    }

    #[test]
    fn test_analyze_sermon_offline_with_audio_peaks() {
        let segments = vec![
            TranscriptSegment {
                start: 0.0,
                end: 15.0,
                text: "Welcome to today's teaching.".to_string(),
            },
            TranscriptSegment {
                start: 15.0,
                end: 35.0,
                text: "Listen to me! Romans 8:28 is the foundation of our hope!".to_string(),
            },
            TranscriptSegment {
                start: 35.0,
                end: 55.0,
                text: "All things work together for good to those who love God!".to_string(),
            },
            TranscriptSegment {
                start: 55.0,
                end: 75.0,
                text: "Amen and hallelujah.".to_string(),
            },
        ];

        // Audio peak right at Romans 8:28 (20s)
        let peaks = vec![AudioPeak {
            timestamp: 20.0,
            loudness_lufs: -12.0,
            relative_energy: 0.95,
        }];

        let result_audio = analyze_sermon_offline_heuristics_with_audio(&segments, Some(&peaks));
        let result_no_audio = analyze_sermon_offline_heuristics(&segments);

        assert!(!result_audio.highlights_report.highlights.is_empty());
        let hl_audio = &result_audio.highlights_report.highlights[0];
        let hl_no_audio = &result_no_audio.highlights_report.highlights[0];

        // Audio peak boost should elevate the composite score or mention audio emphasis
        assert!(
            hl_audio.score >= hl_no_audio.score,
            "Audio peak should boost highlight score"
        );
        assert!(
            hl_audio.reason.contains("vocal") || hl_audio.reason.contains("emphasis") || hl_audio.reason.contains("energy"),
            "Highlight reason should note dynamic audio energy: {}",
            hl_audio.reason
        );
    }
}
