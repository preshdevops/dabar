use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct DownloadedAudio {
    pub path: PathBuf,
    pub title: Option<String>,
}

/// Returns true if the given URL is a Google Drive file share link.
///
/// Accepts both `drive.google.com/file/d/<ID>/...` and `drive.google.com/open?id=<ID>` formats.
pub fn is_gdrive_url(url: &str) -> bool {
    let s = url.trim();
    s.contains("drive.google.com/file/d/")
        || s.contains("drive.google.com/open?id=")
        || (s.contains("drive.google.com/") && s.contains("/view"))
}

/// Extract the Google Drive file ID from a share link.
///
/// Handles:
/// - `https://drive.google.com/file/d/<ID>/view?usp=sharing`
/// - `https://drive.google.com/open?id=<ID>`
pub fn extract_gdrive_id(url: &str) -> Option<String> {
    let trimmed = url.trim();

    // Format: /file/d/<ID>/
    if let Some(pos) = trimmed.find("/file/d/") {
        let after = &trimmed[pos + 8..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() > 10 {
            return Some(id);
        }
    }

    // Format: ?id=<ID> or &id=<ID>
    if let Some(pos) = trimmed.find("id=") {
        let after = &trimmed[pos + 3..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() > 10 {
            return Some(id);
        }
    }

    None
}

/// Download audio from a Google Drive share link using yt-dlp.
///
/// yt-dlp supports Google Drive natively. The link must have public
/// ("Anyone with the link") share permission enabled.
/// Returns the downloaded file path and the filename as title.
pub async fn download_gdrive_audio(gdrive_url: &str, output_dir: &Path) -> Result<DownloadedAudio> {
    tokio::fs::create_dir_all(output_dir)
        .await
        .with_context(|| format!("creating audio output directory {}", output_dir.display()))?;

    let file_id = extract_gdrive_id(gdrive_url).with_context(|| {
        format!("could not extract Google Drive file ID from '{gdrive_url}' — expected a valid drive.google.com share link")
    })?;

    tracing::info!("📥 [Download] Starting Google Drive download (File ID: {file_id})...");

    let dest_template = output_dir.join(&file_id);

    let mut cmd = get_binary_command("yt-dlp");
    cmd.arg("-f")
        .arg("ba[abr<=128]/ba/bestaudio/b")
        .arg("-N")
        .arg("4")
        .arg("--newline")
        .arg("--print")
        .arg("after_video:title")
        .arg("--no-check-certificates")
        .arg("--no-playlist")
        .arg("--no-cache-dir")
        .arg("-o")
        .arg(format!("{}.%(ext)s", dest_template.display()))
        .arg(gdrive_url);

    let (stdout, _) =
        run_yt_dlp_streaming(cmd, "Google Drive audio download", Duration::from_secs(900)).await?;

    let title = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('['))
        .map(str::to_string);

    let found_path = find_downloaded_file(output_dir, &file_id).await?;
    tracing::info!(
        "✅ [Download] Google Drive audio ready: {}",
        found_path.display()
    );

    Ok(DownloadedAudio {
        path: found_path,
        title,
    })
}

/// Extract a YouTube video ID from various URL formats or a raw 11-char ID.
pub fn extract_youtube_id(url_or_id: &str) -> Option<String> {
    let trimmed = url_or_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    // If it's already a raw 11-char ID
    if trimmed.len() == 11
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Some(trimmed.to_string());
    }

    if let Some(pos) = trimmed.find("v=") {
        let after = &trimmed[pos + 2..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() == 11 {
            return Some(id);
        }
    }

    if let Some(pos) = trimmed.find("youtu.be/") {
        let after = &trimmed[pos + 9..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() == 11 {
            return Some(id);
        }
    }

    if let Some(pos) = trimmed.find("/embed/") {
        let after = &trimmed[pos + 7..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() == 11 {
            return Some(id);
        }
    }

    if let Some(pos) = trimmed.find("/shorts/") {
        let after = &trimmed[pos + 8..];
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if id.len() == 11 {
            return Some(id);
        }
    }

    None
}

/// Download audio from a YouTube URL using yt-dlp (audio-only, mp3 format).
///
/// Shells out to yt-dlp in a logged-out/no-cookie context. The downloaded
/// file is saved to `output_dir` with a unique filename derived from the
/// video ID. Returns the local file path and video title on success.
/// Strip the `&t=` / `?t=` timestamp fragment from a YouTube URL so yt-dlp
/// does not treat it as a separate stream selector or get confused during retries.
fn strip_youtube_timestamp(url: &str) -> String {
    // Remove &t=... or ?t=... query parameters (case-insensitive)
    let url = if let Some(pos) = url.find("&t=") {
        &url[..pos]
    } else if let Some(pos) = url.find("?t=") {
        &url[..pos]
    } else {
        url
    };
    url.to_string()
}

pub async fn download_youtube_audio(
    youtube_url: &str,
    output_dir: &Path,
) -> Result<DownloadedAudio> {
    let youtube_url = strip_youtube_timestamp(youtube_url);
    tokio::fs::create_dir_all(output_dir)
        .await
        .with_context(|| format!("creating audio output directory {}", output_dir.display()))?;

    let video_id = extract_youtube_id(&youtube_url)
        .with_context(|| format!("could not extract YouTube video ID from '{youtube_url}' — expected a valid youtube.com or youtu.be URL"))?;

    tracing::info!("📥 [Download] Starting YouTube audio download (Video ID: {video_id})...");

    // Use a deterministic filename template
    let dest_template = output_dir.join(&video_id);

    // High-performance single-pass yt-dlp download:
    // 1. '-f "ba/ba*/bestaudio/b[height<=720]/best"' selects best available audio stream with multi-format fallback.
    // 2. '--print after_video:title' captures the title in the same single invocation (eliminates extra 4-8s latency).
    // 3. Skips yt-dlp FFmpeg re-encoding step since Whisper preprocessor directly downsamples to 16kHz mono.
    let mut cmd = get_binary_command("yt-dlp");
    cmd.arg("-f")
        .arg("ba[abr<=128]/ba/ba*/bestaudio/ba*")
        .arg("-N")
        .arg("4")
        .arg("--buffer-size")
        .arg("1M")
        .arg("--print")
        .arg("after_video:title")
        .arg("-o")
        .arg(format!("{}.%(ext)s", dest_template.display()));

    apply_yt_dlp_common_args(&mut cmd);
    cmd.arg(&youtube_url);

    let (stdout, _) = run_yt_dlp_streaming(cmd, "audio download", Duration::from_secs(900)).await?;

    let title = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('['))
        .map(str::to_string);

    let found_path = find_downloaded_file(output_dir, &video_id).await?;
    tracing::info!(
        "✅ [Download] YouTube audio ready: {}",
        found_path.display()
    );

    Ok(DownloadedAudio {
        path: found_path,
        title,
    })
}

/// Download only the requested section of a YouTube video using `yt-dlp --download-sections`.
///
/// Returns the path to the downloaded section file (e.g. .mp4). This fetches real video
/// footage for the clip without downloading the entire full-length sermon video.
pub async fn download_youtube_video_section(
    youtube_url: &str,
    start_time: f32,
    end_time: f32,
    output_dir: &Path,
) -> Result<PathBuf> {
    if start_time < 0.0 || end_time <= start_time {
        anyhow::bail!(
            "invalid clip duration bounds: start_time ({start_time:.2}) must be >= 0 and < end_time ({end_time:.2})"
        );
    }

    let video_id = extract_youtube_id(youtube_url).unwrap_or_else(|| "clip".to_string());

    tokio::fs::create_dir_all(output_dir).await?;

    let base_name = format!("yt_sec_{video_id}_{start_time:.0}_{end_time:.0}");
    let dest_template = output_dir.join(&base_name);

    let mut cmd = get_binary_command("yt-dlp");
    cmd.arg("--download-sections")
        .arg(format!("*{start_time:.3}-{end_time:.3}"))
        .arg("-f")
        // 720p is plenty for social clips and downloads ~2-3x faster than 1080p;
        // --force-keyframes-at-cuts removed: it triggers a full post-download re-encode
        // which regularly exceeds the timeout. FFmpeg handles precise trimming via -ss/-t.
        .arg("bestvideo[height<=720]+bestaudio/best[height<=720]/best")
        .arg("-N")
        .arg("4")
        .arg("--buffer-size")
        .arg("1M")
        .arg("-o")
        .arg(format!("{}.%(ext)s", dest_template.display()));

    apply_yt_dlp_common_args(&mut cmd);
    cmd.arg(youtube_url);

    let (_, _) =
        run_yt_dlp_streaming(cmd, "video section download", Duration::from_secs(600)).await?;

    let found_path = find_downloaded_file(output_dir, &base_name).await?;
    tracing::info!(
        "✅ [Download] YouTube video section ready: {}",
        found_path.display()
    );

    Ok(found_path)
}

/// Resolve a direct stream URL for a YouTube video using yt-dlp.
///
/// Returns a best-audio URL suitable for piping directly into ffmpeg. Used
/// by the clip-download endpoints to extract vertical clips without a full
/// file download step.
pub async fn resolve_stream_url(youtube_url: &str) -> Result<String> {
    let _video_id = extract_youtube_id(youtube_url)
        .with_context(|| format!("could not extract YouTube video ID from '{youtube_url}'"))?;

    let mut cmd = get_binary_command("yt-dlp");
    cmd.arg("--get-url")
        .arg("-f")
        .arg("bestvideo*+bestaudio/best");

    apply_yt_dlp_common_args(&mut cmd);
    cmd.arg(youtube_url);

    let output = cmd
        .output()
        .await
        .context("failed to spawn yt-dlp process for stream URL resolution")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format_yt_dlp_error("stream URL resolution", &stderr));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let clean_url = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && (l.starts_with("http://") || l.starts_with("https://")))
        .unwrap_or_else(|| raw.trim())
        .to_string();

    if clean_url.is_empty() {
        anyhow::bail!("yt-dlp returned an empty stream URL for '{youtube_url}'");
    }

    Ok(clean_url)
}

pub async fn check_yt_dlp_installed() -> Result<String> {
    let output = get_binary_command("yt-dlp")
        .arg("--version")
        .output()
        .await
        .context("executing yt-dlp --version")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp returned non-zero exit code: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn run_yt_dlp_streaming(
    mut cmd: Command,
    action_name: &str,
    timeout_duration: Duration,
) -> Result<(String, String)> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .context("failed to spawn yt-dlp process — is yt-dlp installed and on PATH?")?;

    let stdout = child.stdout.take().context("taking yt-dlp stdout")?;
    let stderr = child.stderr.take().context("taking yt-dlp stderr")?;

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let mut stdout_lines = stdout_reader.lines();
    let mut stderr_lines = stderr_reader.lines();

    let mut all_stdout = Vec::new();
    let mut all_stderr = Vec::new();

    let stream_task = async {
        loop {
            tokio::select! {
                line = stdout_lines.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            let trimmed = l.trim();
                            if !trimmed.is_empty() {
                                if trimmed.starts_with('[') {
                                    tracing::info!("{trimmed}");
                                }
                                all_stdout.push(l);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::debug!("error reading yt-dlp stdout: {e}");
                            break;
                        }
                    }
                }
                line = stderr_lines.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            let trimmed = l.trim();
                            if !trimmed.is_empty() {
                                if trimmed.starts_with("WARNING:") {
                                    tracing::warn!("{trimmed}");
                                } else if trimmed.starts_with('[') || trimmed.starts_with("ERROR:") {
                                    tracing::info!("{trimmed}");
                                }
                                all_stderr.push(l);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::debug!("error reading yt-dlp stderr: {e}");
                            break;
                        }
                    }
                }
            }
        }
        child.wait().await
    };

    let status = tokio::time::timeout(timeout_duration, stream_task)
        .await
        .context(format!("yt-dlp {action_name} timed out"))?
        .context(format!("waiting for yt-dlp {action_name} process"))?;

    let stdout_combined = all_stdout.join("\n");
    let stderr_combined = all_stderr.join("\n");

    if !status.success() {
        return Err(format_yt_dlp_error(action_name, &stderr_combined));
    }

    Ok((stdout_combined, stderr_combined))
}

async fn find_downloaded_file(dir: &Path, base_name: &str) -> Result<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("reading audio directory {}", dir.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == base_name {
                    return Ok(path);
                }
            }
        }
    }

    anyhow::bail!(
        "could not find downloaded audio file with base name '{base_name}' in {}",
        dir.display()
    )
}

fn format_yt_dlp_error(action: &str, stderr: &str) -> anyhow::Error {
    let trimmed = stderr.trim();

    if trimmed.contains("Sign in to confirm you’re not a bot")
        || trimmed.contains("Sign in to confirm you're not a bot")
        || trimmed.contains("bot confirmation")
    {
        return anyhow::anyhow!(
            "YouTube requires bot verification for this video. \
             Place a 'cookies.txt' file in your workspace or set the YT_DLP_COOKIES_PATH \
             environment variable."
        );
    }

    if trimmed.contains("Private video") {
        return anyhow::anyhow!("Sermon download failed: this YouTube video is private.");
    }

    if trimmed.contains("Video unavailable") {
        return anyhow::anyhow!(
            "Sermon download failed: this YouTube video is unavailable or deleted."
        );
    }

    if trimmed.contains("members-only") || trimmed.contains("Join this channel") {
        return anyhow::anyhow!(
            "Sermon download failed: this is a members-only video. \
             Export cookies from an account with membership and provide them via the YT_DLP_COOKIES_PATH \
             environment variable."
        );
    }

    if trimmed.contains("age-restricted") || trimmed.contains("age verification") {
        return anyhow::anyhow!(
            "Sermon download failed: the YouTube video is age-restricted. \
             Provide cookies from a logged-in session via YT_DLP_COOKIES_PATH."
        );
    }

    // Extract the most informative error line (lines starting with ERROR:)
    for line in trimmed.lines() {
        if line.starts_with("ERROR:") {
            return anyhow::anyhow!("yt-dlp failed during {action}: {}", line.trim());
        }
    }

    anyhow::anyhow!("yt-dlp failed during {action}: {trimmed}")
}

/// Applies common yt-dlp flags for robust YouTube extraction:
/// - Passes player_client fallback to prevent HTTP 403 Forbidden throttling on video chunks.
/// - Sets modern browser user-agent and geo-bypass.
/// - Passes cookies if YT_DLP_COOKIES_PATH, YT_DLP_COOKIES_FROM_BROWSER, or a local cookies.txt exists.
/// - Enables JS runtime if Node.js is present.
fn apply_yt_dlp_common_args(cmd: &mut Command) {
    cmd.arg("--no-playlist")
        .arg("--no-check-certificates")
        .arg("--no-cache-dir")
        .arg("--geo-bypass")
        .arg("--newline")
        // Hard network timeout so yt-dlp never hangs indefinitely waiting for a CDN response
        .arg("--socket-timeout")
        .arg("30")
        // Retry on transient failures
        .arg("--retries")
        .arg("5")
        .arg("--fragment-retries")
        .arg("5")
        .arg("--retry-sleep")
        .arg("2");

    // Optional cookies support
    if let Ok(cookies_path) = std::env::var("YT_DLP_COOKIES_PATH") {
        let p = Path::new(&cookies_path);
        if p.exists() {
            cmd.arg("--cookies").arg(p);
        }
    } else {
        // Auto-detect cookies.txt in workspace root or cwd
        if let Ok(cwd) = std::env::current_dir() {
            let direct = cwd.join("cookies.txt");
            let parent = cwd.parent().map(|p| p.join("cookies.txt"));
            if direct.exists() {
                cmd.arg("--cookies").arg(direct);
            } else if let Some(parent_p) = parent {
                if parent_p.exists() {
                    cmd.arg("--cookies").arg(parent_p);
                }
            }
        }
    }

    if let Ok(browser) = std::env::var("YT_DLP_COOKIES_FROM_BROWSER") {
        if !browser.trim().is_empty() {
            cmd.arg("--cookies-from-browser").arg(browser.trim());
        }
    }

    if which_node_exists() {
        cmd.arg("--js-runtimes").arg("node");
    }
}

fn which_node_exists() -> bool {
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("bin/node/bin/node.exe").exists()
            || cwd.join("bin/node.exe").exists()
            || cwd
                .parent()
                .map(|p| p.join("bin/node.exe").exists())
                .unwrap_or(false)
        {
            return true;
        }
    }
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Binary resolution — delegates to unified discovery in crate::ffmpeg::deps
// while setting augmented search PATH for child processes.
// ---------------------------------------------------------------------------

fn get_binary_command(name: &str) -> Command {
    let mut cmd = crate::ffmpeg::get_binary_command(name);
    let current_path = std::env::var("PATH").unwrap_or_default();
    let separator = if cfg!(windows) { ";" } else { ":" };

    if let Ok(cwd) = std::env::current_dir() {
        let node_bin = cwd.join("bin").join("node").join("bin");
        let local_bin = cwd.join("bin");
        let parent_bin = cwd
            .parent()
            .map(|p| p.join("bin"))
            .unwrap_or_else(|| cwd.clone());
        let home_bin = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local/bin"))
            .unwrap_or_else(|_| PathBuf::from("/home/render/.local/bin"));

        let updated_path = format!(
            "{}{separator}{}{separator}{}{separator}{}{separator}{current_path}",
            node_bin.display(),
            local_bin.display(),
            parent_bin.display(),
            home_bin.display()
        );
        cmd.env("PATH", updated_path);
    }

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_youtube_id() {
        assert_eq!(
            extract_youtube_id("dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_youtube_id("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_youtube_id("https://www.youtube.com/embed/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_youtube_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(extract_youtube_id("invalid url"), None);
        assert_eq!(extract_youtube_id(""), None);
    }

    #[test]
    fn test_format_yt_dlp_error() {
        let err_private = format_yt_dlp_error("test", "ERROR: Private video");
        assert!(err_private.to_string().contains("private"));

        let err_geo = format_yt_dlp_error(
            "test",
            "ERROR: Video not available in your country due to geo restriction",
        );
        assert!(err_geo.to_string().contains("geo") || err_geo.to_string().contains("failed"));

        let err_invalid = format_yt_dlp_error("test", "ERROR: 'not_a_url' is not a valid URL");
        assert!(
            err_invalid.to_string().contains("not_a_url")
                || err_invalid.to_string().contains("valid")
        );

        let err_bot = format_yt_dlp_error("test", "ERROR: Sign in to confirm you're not a bot");
        assert!(err_bot.to_string().contains("bot"));
    }

    #[test]
    fn test_is_gdrive_url() {
        assert!(is_gdrive_url(
            "https://drive.google.com/file/d/1a2b3c4d5e6f7g8h9i0j/view?usp=sharing"
        ));
        assert!(is_gdrive_url(
            "https://drive.google.com/open?id=1a2b3c4d5e6f7g8h9i0j"
        ));
        assert!(!is_gdrive_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(!is_gdrive_url("/path/to/local/file.mp4"));
    }

    #[test]
    fn test_extract_gdrive_id() {
        assert_eq!(
            extract_gdrive_id(
                "https://drive.google.com/file/d/1a2b3c4d5e6f7g8h9i0j/view?usp=sharing"
            ),
            Some("1a2b3c4d5e6f7g8h9i0j".to_string())
        );
        assert_eq!(
            extract_gdrive_id("https://drive.google.com/open?id=1a2b3c4d5e6f7g8h9i0j"),
            Some("1a2b3c4d5e6f7g8h9i0j".to_string())
        );
        assert_eq!(extract_gdrive_id("https://youtube.com"), None);
    }
}
