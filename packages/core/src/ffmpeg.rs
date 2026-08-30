use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

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

pub async fn extract_vertical_clip(
    input_source: &str,
    output_path: &Path,
    start_time: f32,
    end_time: f32,
) -> Result<()> {
    if start_time < 0.0 || end_time <= start_time {
        anyhow::bail!(
            "invalid clip duration bounds: start_time ({start_time:.2}) must be >= 0 and < end_time ({end_time:.2})"
        );
    }

    let duration = end_time - start_time;
    let has_video = has_video_stream(input_source).await;

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
        // High-definition 9:16 vertical video with 50x faster downscale-blur-upscale technique:
        // Background: downscaled to 108x192, softly blurred, then upscaled to 1080x1920 with bilinear filter
        // Foreground: cleanly scaled to fit within 1080x1920 keeping crisp original aspect ratio
        // Output: libx264 veryfast with yuv420p for universal mobile and desktop playback
        cmd.arg("-filter_complex")
            .arg("[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]")
            .arg("-map")
            .arg("[v]")
            .arg("-map")
            .arg("0:a?")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-crf")
            .arg("21")
            .arg("-preset")
            .arg("veryfast")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k");
    } else {
        // Audio-only source: render clean 1080x1920 vertical video card with waveform visualizer
        let filter_str = format!(
            "color=c=0x080c14:s=1080x1920:d={duration:.3}:r=30[bg];[0:a]showwaves=s=960x380:mode=cline:colors=0xe5a93c:r=30[wave];[bg][wave]overlay=(W-w)/2:(H-h)/2:shortest=1[v]"
        );
        cmd.arg("-filter_complex")
            .arg(&filter_str)
            .arg("-map")
            .arg("[v]")
            .arg("-map")
            .arg("0:a")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-crf")
            .arg("21")
            .arg("-preset")
            .arg("veryfast")
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
        .context("executing ffmpeg process")?;

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

pub async fn preprocess_audio_for_whisper(
    input_path: &Path,
    output_path: &Path,
) -> Result<()> {
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
        .arg("64k")
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
pub async fn convert_audio_to_wav_16k(
    input_path: &Path,
    output_path: &Path,
) -> Result<()> {
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
        .arg("64k")
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
        anyhow::bail!("ffmpeg audio chunk WAV extraction failed: {}", stderr.trim());
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

fn get_binary_command(name: &str) -> Command {
    let env_key = format!("{}_PATH", name.to_uppercase().replace('-', "_"));
    if let Ok(custom_path) = std::env::var(&env_key) {
        if !custom_path.trim().is_empty() {
            let p = PathBuf::from(custom_path.trim());
            if p.exists() {
                return Command::new(p);
            }
        }
    }

    let exe_name = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };

    // 1. Platform-specific app-data bin directories
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let candidate = PathBuf::from(&appdata).join("dabar").join("bin").join(&exe_name);
            if candidate.exists() {
                return Command::new(candidate);
            }
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let candidate = PathBuf::from(&localappdata).join("dabar").join("bin").join(&exe_name);
            if candidate.exists() {
                return Command::new(candidate);
            }
        }
        let candidate2 = PathBuf::from(&appdata).join("com.preshdevops.dabar").join("bin").join(&exe_name);
        if candidate2.exists() {
            return Command::new(candidate2);
        }
    }
    // XDG-compliant path for Linux/macOS: ~/.local/share/dabar/bin/<exe>
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let xdg_data = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));
            let candidate = xdg_data.join("dabar").join("bin").join(&exe_name);
            if candidate.exists() {
                return Command::new(candidate);
            }
            // Legacy ~/.dabar/bin fallback
            let legacy = PathBuf::from(&home).join(".dabar").join("bin").join(&exe_name);
            if legacy.exists() {
                return Command::new(legacy);
            }
            // ~/.local/bin fallback (user-installed system binaries)
            let local_bin = PathBuf::from(&home).join(".local").join("bin").join(&exe_name);
            if local_bin.exists() {
                return Command::new(local_bin);
            }
        }
    }

    // Shared HOME/.dabar/bin for any remaining platforms
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_p = PathBuf::from(&home);
        let cand1 = home_p.join(".dabar").join("bin").join(&exe_name);
        if cand1.exists() {
            return Command::new(cand1);
        }
        let cand2 = home_p.join(".local").join("bin").join(&exe_name);
        if cand2.exists() {
            return Command::new(cand2);
        }
    }

    // 2. Walk up ancestor directories (cwd, cwd/.., cwd/../.., ...) to find bin/<exe>
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: Option<&Path> = Some(cwd.as_path());
        while let Some(ancestor) = dir {
            let bin_dir = ancestor.join("bin");
            let candidate = bin_dir.join(&exe_name);
            if candidate.exists() {
                return Command::new(candidate);
            }
            if let Ok(mut entries) = std::fs::read_dir(&bin_dir) {
                while let Some(Ok(entry)) = entries.next() {
                    let path = entry.path();
                    if path.is_dir() {
                        let sub1 = path.join(&exe_name);
                        if sub1.exists() {
                            return Command::new(sub1);
                        }
                        let sub2 = path.join("bin").join(&exe_name);
                        if sub2.exists() {
                            return Command::new(sub2);
                        }
                    }
                }
            }
            dir = ancestor.parent();
        }
    }

    Command::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
