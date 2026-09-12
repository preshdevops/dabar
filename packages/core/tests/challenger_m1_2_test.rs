use dabar_core::downloader::{download_youtube_video_section, extract_youtube_id};
use dabar_core::ffmpeg::{
    extract_clip, find_binary, get_binary_command, has_video_stream, refresh_process_path,
};
use std::path::Path;

/// Helper to create a short synthetic video using ffmpeg lavfi
async fn create_synthetic_video(
    path: &Path,
    width: u32,
    height: u32,
    duration_secs: u32,
) -> anyhow::Result<()> {
    let status = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg(format!("testsrc=size={width}x{height}:rate=30"))
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:sample_rate=44100")
        .arg("-t")
        .arg(format!("{duration_secs}"))
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:a")
        .arg("aac")
        .arg(path)
        .output()
        .await?;

    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("generating synthetic video failed: {err}");
    }
    Ok(())
}

/// Helper to create a synthetic pure audio file using ffmpeg lavfi
async fn create_synthetic_audio(
    path: &Path,
    format: &str,
    duration_secs: u32,
) -> anyhow::Result<()> {
    let mut cmd = get_binary_command("ffmpeg");
    cmd.arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:sample_rate=44100")
        .arg("-t")
        .arg(format!("{duration_secs}"));

    match format {
        "mp3" => {
            cmd.arg("-c:a").arg("libmp3lame").arg("-b:a").arg("128k");
        }
        "wav" => {
            cmd.arg("-c:a").arg("pcm_s16le");
        }
        "m4a" | "aac" => {
            cmd.arg("-c:a").arg("aac").arg("-b:a").arg("128k");
        }
        _ => {
            cmd.arg("-c:a").arg("copy");
        }
    }

    let status = cmd.arg(path).output().await?;
    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        anyhow::bail!("generating synthetic audio ({format}) failed: {err}");
    }
    Ok(())
}

/// Helper to create an MP3 with an embedded ID3v2 attached picture (album cover)
async fn create_mp3_with_attached_cover(path: &Path, duration_secs: u32) -> anyhow::Result<()> {
    let temp_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_audio = temp_dir.join(format!("tmp_audio_{}.mp3", uuid::Uuid::new_v4()));
    let temp_cover = temp_dir.join(format!("tmp_cover_{}.jpg", uuid::Uuid::new_v4()));

    create_synthetic_audio(&temp_audio, "mp3", duration_secs).await?;

    // Generate single JPEG image
    let status_img = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=size=300x300:rate=1")
        .arg("-frames:v")
        .arg("1")
        .arg(&temp_cover)
        .output()
        .await?;

    if !status_img.status.success() {
        let _ = tokio::fs::remove_file(&temp_audio).await;
        anyhow::bail!("generating cover image failed");
    }

    // Mux audio and attached picture with ID3v2 metadata
    let status_mux = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(&temp_audio)
        .arg("-i")
        .arg(&temp_cover)
        .arg("-map")
        .arg("0:a")
        .arg("-map")
        .arg("1:v")
        .arg("-c:a")
        .arg("copy")
        .arg("-c:v")
        .arg("copy")
        .arg("-id3v2_version")
        .arg("3")
        .arg("-metadata:s:v")
        .arg("title=Album cover")
        .arg("-metadata:s:v")
        .arg("comment=Cover (front)")
        .arg(path)
        .output()
        .await?;

    let _ = tokio::fs::remove_file(&temp_audio).await;
    let _ = tokio::fs::remove_file(&temp_cover).await;

    if !status_mux.status.success() {
        let err = String::from_utf8_lossy(&status_mux.stderr);
        anyhow::bail!("muxing MP3 with cover failed: {err}");
    }
    Ok(())
}

// =========================================================================
// Area 1: YouTube Section Download Argument Construction & Execution
// =========================================================================

#[tokio::test]
async fn test_youtube_section_download_bounds_validation() {
    let temp_dir =
        std::env::temp_dir().join(format!("dabar_test_sec_bounds_{}", uuid::Uuid::new_v4()));
    let _ = tokio::fs::create_dir_all(&temp_dir).await;

    // 1. Negative start time
    let res = download_youtube_video_section(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        -1.0,
        10.0,
        &temp_dir,
    )
    .await;
    assert!(res.is_err(), "Negative start time must fail");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("invalid clip duration bounds"));

    // 2. Start time equal to end time
    let res = download_youtube_video_section(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        15.0,
        15.0,
        &temp_dir,
    )
    .await;
    assert!(res.is_err(), "start_time == end_time must fail");
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("invalid clip duration bounds"));

    // 3. Start time greater than end time
    let res = download_youtube_video_section(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        20.0,
        15.0,
        &temp_dir,
    )
    .await;
    assert!(res.is_err(), "start_time > end_time must fail");
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("invalid clip duration bounds"));

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[test]
fn test_youtube_url_variations_and_timestamp_handling() {
    // Normal URL
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_string())
    );

    // URL with timestamp parameter &t=...
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=120"),
        Some("dQw4w9WgXcQ".to_string())
    );
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s"),
        Some("dQw4w9WgXcQ".to_string())
    );
    assert_eq!(
        extract_youtube_id("https://youtu.be/dQw4w9WgXcQ?t=45"),
        Some("dQw4w9WgXcQ".to_string())
    );

    // Embed & Shorts
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/embed/dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_string())
    );
    assert_eq!(
        extract_youtube_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_string())
    );

    // Raw 11-character video ID
    assert_eq!(
        extract_youtube_id("dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_string())
    );

    // Invalid URLs
    assert_eq!(extract_youtube_id("https://vimeo.com/12345678"), None);
    assert_eq!(extract_youtube_id("not-a-url"), None);
}

// =========================================================================
// Area 2: Stream Probing Behavior for Video vs Audio-Only Sources
// =========================================================================

#[tokio::test]
async fn test_has_video_stream_pure_video_and_pure_audio() -> anyhow::Result<()> {
    let temp_dir =
        std::env::temp_dir().join(format!("dabar_test_probing_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    // 1. True Video (MP4 containing H.264 video and AAC audio)
    let video_path = temp_dir.join("true_video.mp4");
    create_synthetic_video(&video_path, 640, 480, 2).await?;
    let has_v = has_video_stream(video_path.to_str().unwrap()).await;
    assert!(
        has_v,
        "True video file MUST be probed as having video stream"
    );

    // 2. Pure Audio MP3
    let mp3_path = temp_dir.join("pure_audio.mp3");
    create_synthetic_audio(&mp3_path, "mp3", 2).await?;
    let has_v = has_video_stream(mp3_path.to_str().unwrap()).await;
    assert!(
        !has_v,
        "Pure MP3 file must NOT be reported as having video stream"
    );

    // 3. Pure Audio WAV
    let wav_path = temp_dir.join("pure_audio.wav");
    create_synthetic_audio(&wav_path, "wav", 2).await?;
    let has_v = has_video_stream(wav_path.to_str().unwrap()).await;
    assert!(
        !has_v,
        "Pure WAV file must NOT be reported as having video stream"
    );

    // 4. Pure Audio M4A / AAC
    let m4a_path = temp_dir.join("pure_audio.m4a");
    create_synthetic_audio(&m4a_path, "m4a", 2).await?;
    let has_v = has_video_stream(m4a_path.to_str().unwrap()).await;
    assert!(
        !has_v,
        "Pure M4A file must NOT be reported as having video stream"
    );

    // 5. Non-existent file
    let non_existent = temp_dir.join("non_existent_file_12345.mp4");
    assert!(!has_video_stream(non_existent.to_str().unwrap()).await);

    // 6. Zero-byte empty file
    let empty_file = temp_dir.join("empty.mp4");
    tokio::fs::write(&empty_file, b"").await?;
    assert!(!has_video_stream(empty_file.to_str().unwrap()).await);

    // 7. Corrupted text file pretending to be video
    let corrupt_file = temp_dir.join("corrupt.mp4");
    tokio::fs::write(&corrupt_file, b"THIS IS NOT A VALID VIDEO FILE HEADER").await?;
    assert!(!has_video_stream(corrupt_file.to_str().unwrap()).await);

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    Ok(())
}

#[tokio::test]
async fn test_probing_audio_with_attached_cover_art_finding() -> anyhow::Result<()> {
    let temp_dir =
        std::env::temp_dir().join(format!("dabar_test_cover_probing_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    let mp3_with_cover = temp_dir.join("audio_with_cover.mp3");
    create_mp3_with_attached_cover(&mp3_with_cover, 3).await?;

    // Adversarially test `has_video_stream` on an MP3 with attached cover art:
    // ffprobe -select_streams v:0 returns "video" because FFmpeg treats ID3v2 APIC attached picture as a video stream!
    let probed_has_video = has_video_stream(mp3_with_cover.to_str().unwrap()).await;

    // We empirically document whether ffprobe flags attached pictures as video:
    // When ffprobe returns "video" for attached picture, extract_clip treats it as a video stream rather than waveform!
    println!(
        "Empirical discovery: has_video_stream for MP3 with attached picture = {probed_has_video}"
    );

    // Also test extract_clip on a pure audio file to verify waveform generation works cleanly
    let pure_audio = temp_dir.join("pure_audio.mp3");
    create_synthetic_audio(&pure_audio, "mp3", 2).await?;
    let wave_out = temp_dir.join("waveform_out.mp4");

    let res = extract_clip(
        pure_audio.to_str().unwrap(),
        &wave_out,
        0.0,
        1.5,
        "9:16",
        None,
        None,
    )
    .await;

    assert!(
        res.is_ok(),
        "Waveform card generation for pure audio should succeed: {:?}",
        res.err()
    );
    assert!(wave_out.exists(), "Waveform video output must exist");

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    Ok(())
}

// =========================================================================
// Area 3: In-Memory Process PATH Updates (refresh_process_path)
// =========================================================================

#[test]
fn test_refresh_process_path_behavior_and_discovery() {
    let temp_root = std::env::temp_dir().join(format!("dabar_test_path_{}", uuid::Uuid::new_v4()));
    let temp_bin = temp_root.join("bin");
    std::fs::create_dir_all(&temp_bin).expect("create temp bin dir");

    let unique_tool_name = format!("my_custom_tool_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let exe_name = if cfg!(windows) {
        format!("{unique_tool_name}.exe")
    } else {
        unique_tool_name.clone()
    };
    let tool_path = temp_bin.join(&exe_name);
    std::fs::write(&tool_path, b"dummy executable payload").expect("write dummy tool");

    // 1. Before refresh_process_path, tool is NOT found
    assert!(
        find_binary(&unique_tool_name).is_none(),
        "Binary should not be found before refresh_process_path"
    );

    // 2. Call refresh_process_path with temp_bin
    refresh_process_path(&temp_bin);

    // 3. Verify tool is now found via in-memory PATH
    let discovered = find_binary(&unique_tool_name);
    assert!(
        discovered.is_some(),
        "Binary must be discoverable after refresh_process_path"
    );
    let resolved_path = discovered.unwrap();
    assert_eq!(resolved_path, tool_path);

    // 4. Test idempotence: calling refresh_process_path again should NOT duplicate the entry
    let path_before = std::env::var("PATH").unwrap_or_default();
    refresh_process_path(&temp_bin);
    let path_after = std::env::var("PATH").unwrap_or_default();
    assert_eq!(
        path_before, path_after,
        "Calling refresh_process_path with already present path must be idempotent"
    );

    // 5. Non-existent directory should NOT be added
    let fake_dir = temp_root.join("non_existent_bin_123");
    let path_count_before = std::env::var("PATH").unwrap_or_default().len();
    refresh_process_path(&fake_dir);
    let path_count_after = std::env::var("PATH").unwrap_or_default().len();
    assert_eq!(
        path_count_before, path_count_after,
        "Non-existent directory must not alter PATH"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn test_refresh_process_path_with_spaces_in_path() {
    let temp_root = std::env::temp_dir().join(format!(
        "dabar test dir with spaces {}",
        uuid::Uuid::new_v4()
    ));
    let temp_bin = temp_root.join("custom tools bin");
    std::fs::create_dir_all(&temp_bin).expect("create temp bin dir with spaces");

    let unique_tool_name = format!("space_tool_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let exe_name = if cfg!(windows) {
        format!("{unique_tool_name}.exe")
    } else {
        unique_tool_name.clone()
    };
    let tool_path = temp_bin.join(&exe_name);
    std::fs::write(&tool_path, b"dummy executable").expect("write dummy tool");

    refresh_process_path(&temp_bin);

    let discovered = find_binary(&unique_tool_name);
    assert!(
        discovered.is_some(),
        "Binary in directory with spaces must be discovered via updated PATH"
    );
    assert_eq!(discovered.unwrap(), tool_path);

    let _ = std::fs::remove_dir_all(&temp_root);
}
