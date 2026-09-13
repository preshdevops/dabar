use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::models::TranscriptSegment;

#[path = "deps.rs"]
pub mod deps;
pub use deps::{find_binary, get_binary_command, refresh_process_path};

pub async fn has_video_stream(input_source: &str) -> bool {
    let output = get_binary_command("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=codec_type")
        .arg("-of")
        .arg("csv=p=0")
        .arg(input_source)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.trim() == "video"
        }
        Err(_) => false,
    }
}

/// Format seconds into ASS timestamp format: H:MM:SS.cc
pub fn format_ass_time(seconds: f32) -> String {
    let non_neg = seconds.max(0.0);
    let total_cs = (non_neg * 100.0).round() as u64;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// Escape a path for inclusion in an FFmpeg filter option (e.g. subtitles='C\\:/path/to/file.ass')
pub fn escape_ffmpeg_filter_path(path: &Path) -> String {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let escaped = path_str.replace(':', "\\:").replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Generate timed ASS subtitle content from transcript segments overlapping the clip range.
/// Supports style presets: "amber" (Warm Amber with gold glow), "kinetic" (bold uppercase with black box),
/// and "editorial" (sacred serif).
pub fn generate_ass_subtitles(
    segments: &[TranscriptSegment],
    clip_start: f32,
    clip_end: f32,
    timeline_offset: f32,
    target_w: u32,
    target_h: u32,
    style_name: &str,
) -> Option<String> {
    let clip_duration = clip_end - clip_start;
    let timeline_end = timeline_offset + clip_duration;

    let overlapping: Vec<&TranscriptSegment> = segments
        .iter()
        .filter(|s| s.end > timeline_offset && s.start < timeline_end)
        .collect();

    if overlapping.is_empty() {
        return None;
    }

    let margin_lr = ((target_w as f32 * 0.08) as u32).max(20);

    // Style presets
    // Colors in ASS format: &HAABBGGRR (alpha, blue, green, red)
    let (
        font_name,
        font_size,
        primary_color,
        outline_color,
        back_color,
        bold,
        italic,
        border_style,
        outline_w,
        shadow_w,
        margin_v,
        is_uppercase,
    ) = match style_name.to_lowercase().as_str() {
        "kinetic" => {
            // Bold uppercase with black backing box
            let fsize = if target_h > 1500 { 58 } else { 44 };
            let mv = if target_h > 1500 { 280 } else { 120 };
            (
                "Arial",
                fsize,
                "&H00FFFFFF",
                "&H00000000",
                "&HB0000000",
                1,
                0,
                3,
                6.0,
                0.0,
                mv,
                true,
            )
        }
        "editorial" => {
            // Sacred serif, italic, soft cream
            let fsize = if target_h > 1500 { 50 } else { 40 };
            let mv = if target_h > 1500 { 260 } else { 110 };
            (
                "Georgia",
                fsize,
                "&H00E8F4FD",
                "&H00141010",
                "&H80000000",
                0,
                1,
                1,
                1.8,
                1.5,
                mv,
                false,
            )
        }
        // "amber" / "cobalt" or default: Warm Amber with gold glow
        _ => {
            let fsize = if target_h > 1500 { 54 } else { 42 };
            let mv = if target_h > 1500 { 260 } else { 110 };
            // Primary: crisp white (&H00FFFFFF), Outline: Warm Amber #D4913A -> BGR &H003A91D4
            (
                "Segoe UI",
                fsize,
                "&H00FFFFFF",
                "&H003A91D4",
                "&H80000000",
                1,
                0,
                1,
                3.2,
                2.0,
                mv,
                false,
            )
        }
    };

    let mut ass = String::new();
    ass.push_str("[Script Info]\n");
    ass.push_str("ScriptType: v4.00+\n");
    ass.push_str(&format!("PlayResX: {target_w}\n"));
    ass.push_str(&format!("PlayResY: {target_h}\n"));
    ass.push_str("WrapStyle: 0\n");
    ass.push_str("ScaledBorderAndShadow: yes\n\n");

    ass.push_str("[V4+ Styles]\n");
    ass.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    ass.push_str(&format!(
        "Style: Default,{font_name},{font_size},{primary_color},&H000000FF,{outline_color},{back_color},{bold},{italic},0,0,100,100,0,0,{border_style},{outline_w:.1},{shadow_w:.1},2,{margin_lr},{margin_lr},{margin_v},1\n\n"
    ));

    ass.push_str("[Events]\n");
    ass.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    for seg in overlapping {
        let seg_rel_start = (seg.start - timeline_offset).max(0.0) + clip_start;
        let seg_rel_end = (seg.end - timeline_offset).min(clip_duration) + clip_start;
        if seg_rel_end <= seg_rel_start || (seg_rel_end - seg_rel_start) < 0.05 {
            continue;
        }

        let start_str = format_ass_time(seg_rel_start);
        let end_str = format_ass_time(seg_rel_end);

        let clean_text = seg
            .text
            .trim()
            .replace('{', "(")
            .replace('}', ")")
            .replace("\r\n", "\\N")
            .replace('\n', "\\N");
        if clean_text.is_empty() {
            continue;
        }

        let final_text = if is_uppercase {
            clean_text.to_uppercase()
        } else {
            clean_text
        };

        ass.push_str(&format!(
            "Dialogue: 0,{start_str},{end_str},Default,,0,0,0,,{final_text}\n"
        ));
    }

    Some(ass)
}

pub async fn extract_vertical_clip(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
) -> Result<()> {
    extract_clip(
        input_source,
        output_path,
        start_time,
        end_time,
        "9:16",
        None,
        None,
    )
    .await
}

pub async fn extract_clip(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
    aspect_ratio: &str,
    caption_segments: Option<&[TranscriptSegment]>,
    caption_style: Option<&str>,
) -> Result<()> {
    extract_clip_with_timeline_offset(
        input_source,
        output_path,
        start_time,
        end_time,
        start_time,
        aspect_ratio,
        caption_segments,
        caption_style,
    )
    .await
}

pub async fn extract_clip_with_timeline_offset(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
    _timeline_offset: f32,
    aspect_ratio: &str,
    _caption_segments: Option<&[TranscriptSegment]>,
    _caption_style: Option<&str>,
) -> Result<()> {
    if start_time < 0.0 || end_time <= start_time {
        anyhow::bail!(
            "invalid clip duration bounds: start_time ({start_time:.2}) must be >= 0 and < end_time ({end_time:.2})"
        );
    }

    let duration = end_time - start_time;
    let has_video = has_video_stream(input_source).await;

    let (target_w, target_h) = match aspect_ratio {
        "1:1" => (1080u32, 1080u32),
        "16:9" => (1920u32, 1080u32),
        _ => (1080u32, 1920u32), // Default 9:16 vertical
    };

    // No subtitle burn-in: exported clips are clean video without burnt-in captions.
    let mut cmd = get_binary_command("ffmpeg");
    cmd.arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-ss")
        .arg(format!("{start_time:.3}"))
        .arg("-i")
        .arg(input_source)
        .arg("-t")
        .arg(format!("{duration:.3}"))
        .arg("-avoid_negative_ts")
        .arg("make_zero");

    if has_video {
        let filter_str = if aspect_ratio == "16:9" {
            format!(
                "[0:v]scale={target_w}:{target_h}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={target_w}:{target_h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1[v]"
            )
        } else {
            // For 9:16 vertical or 1:1 square:
            // 1. Background: scale to cover (force_original_aspect_ratio=increase), center crop, blur, setsar=1.
            //    Never scale directly without aspect ratio preservation (which caused the severe horizontal stretch bug).
            // 2. Foreground: scale to fit inside target box, preserve exact original aspect ratio, setsar=1.
            // 3. Overlay: center foreground on background, setsar=1.
            format!(
                "[0:v]split[fg_in][bg_in];\
                 [bg_in]scale={target_w}:{target_h}:force_original_aspect_ratio=increase,crop={target_w}:{target_h},boxblur=20:5,setsar=1[bg];\
                 [fg_in]scale={target_w}:{target_h}:force_original_aspect_ratio=decrease:force_divisible_by=2,setsar=1[fg];\
                 [bg][fg]overlay=(W-w)/2:(H-h)/2,setsar=1[v]"
            )
        };

        cmd.arg("-filter_complex")
            .arg(&filter_str)
            .arg("-map")
            .arg("[v]")
            .arg("-map")
            .arg("0:a?")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-crf")
            .arg("23")
            .arg("-preset")
            .arg("ultrafast") // 2-3x faster than veryfast; imperceptible for social clips
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k");
    } else {
        // Audio-only source: render clean video card with waveform visualizer and proper asplit
        let wave_w = (((target_w as f32 * 0.88) as u32) / 2) * 2;
        let wave_h = (((target_h as f32 * 0.22) as u32) / 2) * 2;
        let filter_str = format!(
            "color=c=0x080c14:s={target_w}x{target_h}:d={duration:.3}:r=30,setsar=1[bg];\
             [0:a]asplit[a_wave][a_out];\
             [a_wave]showwaves=s={wave_w}x{wave_h}:mode=cline:colors=0xd4913a:r=30[wave];\
             [bg][wave]overlay=(W-w)/2:(H-h)/2:shortest=1,setsar=1[v]"
        );
        cmd.arg("-filter_complex")
            .arg(&filter_str)
            .arg("-map")
            .arg("[v]")
            .arg("-map")
            .arg("[a_out]")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-crf")
            .arg("23")
            .arg("-preset")
            .arg("ultrafast")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k")
            .arg("-shortest");
    }

    let output = cmd
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg process");

    let output = output?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg extraction failed: {}", stderr.trim());
    }

    Ok(())
}

/// Extract an audio-only clip segment as MP3 (e.g. for radio, podcasts, WhatsApp voice shares).
pub async fn extract_audio_clip(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
) -> Result<()> {
    if start_time < 0.0 || end_time <= start_time {
        anyhow::bail!(
            "invalid audio clip bounds: start_time ({start_time:.2}) must be >= 0 and < end_time ({end_time:.2})"
        );
    }

    let duration = end_time - start_time;
    let output = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-ss")
        .arg(format!("{start_time:.3}"))
        .arg("-i")
        .arg(input_source)
        .arg("-t")
        .arg(format!("{duration:.3}"))
        .arg("-vn")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("128k")
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg audio clip extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg audio clip extraction failed: {}", stderr.trim());
    }

    Ok(())
}

pub async fn preprocess_audio_for_whisper(input_path: &Path, output_path: &Path) -> Result<()> {
    let output = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-i")
        .arg(input_path)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("48k")
        .arg("-compression_level")
        .arg("2")
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg audio preprocessing for Whisper")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg audio preprocessing failed: {}", stderr.trim());
    }

    Ok(())
}

/// Converts any media file to a 16kHz 16-bit mono PCM WAV file required by local whisper.cpp.
pub async fn convert_audio_to_wav_16k(input_path: &Path, output_path: &Path) -> Result<()> {
    let output = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-i")
        .arg(input_path)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg audio conversion to 16kHz WAV for local Whisper")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg WAV conversion failed: {}", stderr.trim());
    }

    Ok(())
}

pub async fn extract_audio_chunk(
    input_path: &Path,
    output_path: &Path,
    start_time: f32,
    duration: f32,
) -> Result<()> {
    let output = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-ss")
        .arg(format!("{start_time:.3}"))
        .arg("-i")
        .arg(input_path)
        .arg("-t")
        .arg(format!("{duration:.3}"))
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("48k")
        .arg("-compression_level")
        .arg("2")
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg audio chunk extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg audio chunk extraction failed: {}", stderr.trim());
    }

    Ok(())
}

/// Extract an audio chunk as 16kHz mono WAV (for local whisper-cli).
pub async fn extract_audio_chunk_wav(
    input_path: &Path,
    output_path: &Path,
    start_time: f32,
    duration: f32,
) -> Result<()> {
    let output = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-threads")
        .arg("0")
        .arg("-ss")
        .arg(format!("{start_time:.3}"))
        .arg("-i")
        .arg(input_path)
        .arg("-t")
        .arg(format!("{duration:.3}"))
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output_path)
        .output()
        .await
        .context("executing ffmpeg audio chunk WAV extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "ffmpeg audio chunk WAV extraction failed: {}",
            stderr.trim()
        );
    }

    Ok(())
}

pub async fn get_media_duration(input_path: &Path) -> Result<f32> {
    let output = get_binary_command("ffmpeg")
        .arg("-i")
        .arg(input_path)
        .output()
        .await
        .context("executing ffmpeg to detect duration")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(dur) = parse_ffmpeg_duration(&stderr) {
        return Ok(dur);
    }

    anyhow::bail!(
        "could not determine audio duration from ffmpeg output for {}",
        input_path.display()
    )
}

pub fn parse_ffmpeg_duration(stderr: &str) -> Option<f32> {
    let pos = stderr.find("Duration: ")?;
    let after = &stderr[pos + 10..];
    let duration_str: String = after
        .chars()
        .take_while(|c| *c != ',' && *c != '\n' && *c != '\r')
        .collect();
    let parts: Vec<&str> = duration_str.trim().split(':').collect();
    if parts.len() == 3 {
        let hours: f32 = parts[0].trim().parse().ok()?;
        let mins: f32 = parts[1].trim().parse().ok()?;
        let secs: f32 = parts[2].trim().parse().ok()?;
        Some(hours * 3600.0 + mins * 60.0 + secs)
    } else {
        None
    }
}

pub async fn check_ffmpeg_installed() -> Result<String> {
    let output = get_binary_command("ffmpeg")
        .arg("-version")
        .output()
        .await
        .context("executing ffmpeg -version")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg execution failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout
        .lines()
        .next()
        .map(str::trim)
        .unwrap_or("ffmpeg")
        .to_string();

    Ok(version_line)
}

/// A detected audio loudness/energy peak moment in sermon delivery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPeak {
    /// Timestamp in seconds.
    pub timestamp: f32,
    /// Momentary loudness in LUFS.
    pub loudness_lufs: f32,
    /// Normalized dynamic emphasis above the baseline (0.0 = baseline/soft, 1.0 = intense climax/shout).
    pub relative_energy: f32,
}

/// Overall loudness metrics and peak profile of an audio file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioLoudnessProfile {
    /// Integrated loudness in LUFS across the recording (e.g. -24.0 LUFS).
    pub integrated_loudness: f32,
    /// Loudness range in LU (dynamic variation).
    pub loudness_range: f32,
    /// Detected vocal energy peaks.
    pub peaks: Vec<AudioPeak>,
}

/// Parses FFmpeg ebur128 filter output from stderr into an `AudioLoudnessProfile`.
/// Handles both the per-frame timestamps and the trailing summary block.
pub fn parse_ebur128_output(output: &str) -> AudioLoudnessProfile {
    let mut integrated_loudness: Option<f32> = None;
    let mut loudness_range: Option<f32> = None;

    // Scan lines for summary and per-moment data
    let mut raw_samples: Vec<(f32, f32)> = Vec::new(); // (timestamp, momentary_lufs)

    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Check for Integrated loudness summary
        if trimmed.starts_with("Integrated loudness:") {
            if let Some(next_line) = lines.next() {
                let nt = next_line.trim();
                if let Some(rest) = nt.strip_prefix("I:") {
                    if let Some(val_str) = rest.split_whitespace().next() {
                        if let Ok(val) = val_str.parse::<f32>() {
                            integrated_loudness = Some(val);
                        }
                    }
                }
            }
            continue;
        }

        // Check for Loudness range summary
        if trimmed.starts_with("Loudness range:") {
            if let Some(next_line) = lines.next() {
                let nt = next_line.trim();
                if let Some(rest) = nt.strip_prefix("LRA:") {
                    if let Some(val_str) = rest.split_whitespace().next() {
                        if let Ok(val) = val_str.parse::<f32>() {
                            loudness_range = Some(val);
                        }
                    }
                }
            }
            continue;
        }

        // Parse momentary frame output lines:
        // "[Parsed_ebur128_0 @ ...] t: 0.399977 TARGET:-23 LUFS M: -21.1 S:-120.7 I: -21.1 LUFS LRA: 0.0 LU"
        if trimmed.contains("t:") && trimmed.contains("M:") {
            if let Some(t_idx) = trimmed.find("t:") {
                let t_part = &trimmed[t_idx + 2..];
                if let Some(t_str) = t_part.split_whitespace().next() {
                    if let Ok(t) = t_str.parse::<f32>() {
                        if let Some(m_idx) = trimmed.find("M:") {
                            let m_part = &trimmed[m_idx + 2..];
                            if let Some(m_str) = m_part.split_whitespace().next() {
                                if let Ok(m) = m_str.parse::<f32>() {
                                    // Ignore silence / uninitialized frames (< -70 LUFS)
                                    if m > -70.0 {
                                        raw_samples.push((t, m));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Determine baseline integrated loudness
    let baseline = match integrated_loudness {
        Some(i) if i > -70.0 => i,
        _ => {
            if raw_samples.is_empty() {
                -24.0
            } else {
                let sum: f32 = raw_samples.iter().map(|(_, m)| *m).sum();
                sum / raw_samples.len() as f32
            }
        }
    };

    let lra = loudness_range.unwrap_or(0.0);

    // Bucket into 1.0-second intervals to aggregate high-frequency frames into clean peaks
    let mut buckets: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
    for (t, m) in raw_samples {
        let sec = t.max(0.0) as u32;
        let entry = buckets.entry(sec).or_insert(m);
        if m > *entry {
            *entry = m;
        }
    }

    // Identify peaks that rise noticeably above the baseline delivery loudness
    let threshold = baseline + 1.5;
    let mut peaks = Vec::new();

    for (sec, max_m) in buckets {
        if max_m >= threshold {
            let diff = max_m - baseline;
            // 10 dB elevation above integrated conversational baseline represents full shouting/peak emphasis
            let rel = (diff / 10.0).clamp(0.0, 1.0);
            peaks.push(AudioPeak {
                timestamp: sec as f32 + 0.5,
                loudness_lufs: max_m,
                relative_energy: rel,
            });
        }
    }

    AudioLoudnessProfile {
        integrated_loudness: baseline,
        loudness_range: lra,
        peaks,
    }
}

/// Returns the dynamic audio energy boost (0.0 to 1.0) for a time span [start, end].
/// If loudness peaks are present in this window, returns the maximum relative energy observed.
pub fn compute_segment_audio_energy(peaks: &[AudioPeak], start: f32, end: f32) -> f32 {
    let mut max_energy = 0.0_f32;
    for peak in peaks {
        if peak.timestamp >= start && peak.timestamp <= end {
            if peak.relative_energy > max_energy {
                max_energy = peak.relative_energy;
            }
        }
    }
    max_energy
}

/// Runs FFmpeg ebur128 loudness analysis on the given audio file, detecting moments of
/// vocal energy spikes, oratorical emphasis, and dynamic loudness elevation.
pub async fn detect_audio_loudness_peaks(audio_path: &Path) -> Result<AudioLoudnessProfile> {
    if !audio_path.exists() {
        anyhow::bail!("Audio file not found: {}", audio_path.display());
    }

    let output = get_binary_command("ffmpeg")
        .arg("-hide_banner")
        .arg("-nostats")
        .arg("-i")
        .arg(audio_path)
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-af")
        .arg("ebur128")
        .arg("-f")
        .arg("null")
        .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
        .output()
        .await
        .context("executing ffmpeg ebur128 loudness analysis")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let profile = parse_ebur128_output(&stderr);
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_ass_time() {
        assert_eq!(format_ass_time(0.0), "0:00:00.00");
        assert_eq!(format_ass_time(1.234), "0:00:01.23");
        assert_eq!(format_ass_time(65.5), "0:01:05.50");
        assert_eq!(format_ass_time(3661.05), "1:01:01.05");
    }

    #[test]
    fn test_escape_ffmpeg_filter_path() {
        let p1 = Path::new("C:\\Users\\Dabar\\output.ass");
        let escaped = escape_ffmpeg_filter_path(p1);
        assert!(escaped.starts_with('\'') && escaped.ends_with('\''));
        assert!(escaped.contains("C\\:/Users/Dabar/output.ass"));
    }

    #[test]
    fn test_generate_ass_subtitles_amber() {
        let segs = vec![
            TranscriptSegment {
                start: 10.0,
                end: 15.0,
                text: "The grace of God appeared to all.".to_string(),
            },
            TranscriptSegment {
                start: 16.0,
                end: 20.0,
                text: "Bringing salvation for everyone.".to_string(),
            },
        ];

        let ass = generate_ass_subtitles(&segs, 10.0, 20.0, 10.0, 1080, 1920, "amber")
            .expect("should generate ASS subtitles");

        assert!(ass.contains("[Script Info]"));
        assert!(ass.contains("PlayResX: 1080"));
        assert!(ass.contains("PlayResY: 1920"));
        assert!(
            ass.contains("Style: Default,Segoe UI,54,&H00FFFFFF,&H000000FF,&H003A91D4,&H80000000")
        );
        assert!(ass.contains(
            "Dialogue: 0,0:00:10.00,0:00:15.00,Default,,0,0,0,,The grace of God appeared to all."
        ));
        assert!(ass.contains(
            "Dialogue: 0,0:00:16.00,0:00:20.00,Default,,0,0,0,,Bringing salvation for everyone."
        ));
    }

    #[test]
    fn test_generate_ass_subtitles_kinetic() {
        let segs = vec![TranscriptSegment {
            start: 0.0,
            end: 5.0,
            text: "Faith moves mountains!".to_string(),
        }];

        let ass = generate_ass_subtitles(&segs, 0.0, 5.0, 0.0, 1920, 1080, "kinetic")
            .expect("should generate kinetic ASS");

        assert!(ass.contains("BorderStyle, Outline, Shadow"));
        // Kinetic must uppercase text and use borderstyle 3
        assert!(ass.contains("FAITH MOVES MOUNTAINS!"));
        assert!(ass.contains(",3,6.0,0.0,2,"));
    }

    #[test]
    fn test_generate_ass_subtitles_editorial() {
        let segs = vec![TranscriptSegment {
            start: 5.0,
            end: 12.0,
            text: "In the beginning was the Word.".to_string(),
        }];

        let ass = generate_ass_subtitles(&segs, 5.0, 12.0, 5.0, 1080, 1080, "editorial")
            .expect("should generate editorial ASS");

        assert!(ass.contains("Georgia"));
        assert!(ass.contains("&H00E8F4FD"));
        assert!(ass.contains("In the beginning was the Word."));
    }

    #[test]
    fn test_generate_ass_subtitles_out_of_bounds() {
        let segs = vec![TranscriptSegment {
            start: 100.0,
            end: 105.0,
            text: "Far away in the sermon.".to_string(),
        }];

        // Clip is 10.0 to 20.0, segment is at 100.0
        let ass = generate_ass_subtitles(&segs, 10.0, 20.0, 10.0, 1080, 1920, "amber");
        assert!(ass.is_none());
    }

    #[test]
    fn test_parse_ffmpeg_duration_standard() {
        let sample = "Input #0, mp3, from 'test.mp3':\n  Duration: 01:23:45.67, start: 0.000000, bitrate: 128 kb/s";
        let dur = parse_ffmpeg_duration(sample).expect("should parse duration");
        assert!((dur - 5025.67).abs() < 0.001);
    }

    #[test]
    fn test_parse_ffmpeg_duration_short() {
        let sample = "Duration: 00:02:15.50, start: 0.000000";
        let dur = parse_ffmpeg_duration(sample).expect("should parse duration");
        assert!((dur - 135.50).abs() < 0.001);
    }

    #[test]
    fn test_parse_ffmpeg_duration_invalid() {
        let sample = "No duration line present here";
        assert!(parse_ffmpeg_duration(sample).is_none());
    }

    #[test]
    fn test_parse_ebur128_output() {
        let sample = r#"
[Parsed_ebur128_0 @ 0000028fb2e64f80] t: 0.100000 TARGET:-23 LUFS M: -24.0 S:-120.7 I: -24.0 LUFS LRA: 0.0 LU
[Parsed_ebur128_0 @ 0000028fb2e64f80] t: 1.200000 TARGET:-23 LUFS M: -24.5 S:-120.7 I: -24.0 LUFS LRA: 0.0 LU
[Parsed_ebur128_0 @ 0000028fb2e64f80] t: 5.100000 TARGET:-23 LUFS M: -14.0 S:-20.0 I: -22.0 LUFS LRA: 2.0 LU
[Parsed_ebur128_0 @ 0000028fb2e64f80] t: 5.500000 TARGET:-23 LUFS M: -12.0 S:-18.0 I: -21.0 LUFS LRA: 3.0 LU
[Parsed_ebur128_0 @ 0000028fb2e64f80] Summary:

  Integrated loudness:
    I:         -24.0 LUFS
    Threshold: -34.0 LUFS

  Loudness range:
    LRA:         5.0 LU
"#;
        let profile = parse_ebur128_output(sample);
        assert!((profile.integrated_loudness - (-24.0)).abs() < 0.01);
        assert!((profile.loudness_range - 5.0).abs() < 0.01);
        // Moment at 5.x is -12.0 LUFS, which is 12 dB above -24.0 baseline -> relative_energy clamped to 1.0
        assert_eq!(profile.peaks.len(), 1);
        let peak = &profile.peaks[0];
        assert_eq!(peak.timestamp, 5.5);
        assert!((peak.loudness_lufs - (-12.0)).abs() < 0.01);
        assert!((peak.relative_energy - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_segment_audio_energy() {
        let peaks = vec![
            AudioPeak {
                timestamp: 10.5,
                loudness_lufs: -18.0,
                relative_energy: 0.6,
            },
            AudioPeak {
                timestamp: 25.5,
                loudness_lufs: -12.0,
                relative_energy: 0.95,
            },
        ];

        // Segment overlapping first peak
        let energy1 = compute_segment_audio_energy(&peaks, 10.0, 15.0);
        assert!((energy1 - 0.6).abs() < 0.01);

        // Segment overlapping second peak
        let energy2 = compute_segment_audio_energy(&peaks, 20.0, 30.0);
        assert!((energy2 - 0.95).abs() < 0.01);

        // Segment with no peaks
        let energy3 = compute_segment_audio_energy(&peaks, 0.0, 5.0);
        assert_eq!(energy3, 0.0);
    }
}
