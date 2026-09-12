use dabar_core::ffmpeg::{
    extract_clip, extract_clip_with_timeline_offset, find_binary, generate_ass_subtitles,
    get_binary_command, has_video_stream, refresh_process_path,
};
use dabar_core::models::TranscriptSegment;
use std::path::Path;

/// Helper to get the video dimensions of a media file via ffprobe
async fn probe_video_dimensions(path: &Path) -> anyhow::Result<(u32, u32)> {
    let output = get_binary_command("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=s=x:p=0")
        .arg(path)
        .output()
        .await?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed: {err}");
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('x').collect();
    if parts.len() != 2 {
        anyhow::bail!("unexpected ffprobe output: '{trimmed}'");
    }

    let w: u32 = parts[0].trim().parse()?;
    let h: u32 = parts[1].trim().parse()?;
    Ok((w, h))
}

/// Helper to generate a short synthetic test video using ffmpeg testsrc
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
        .arg("mjpeg")
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

// =========================================================================
// Category 1: Extreme Aspect Ratios & Non-Standard/Odd Dimensions
// =========================================================================

#[tokio::test]
async fn test_extreme_aspect_ratios_and_odd_dimensions() -> anyhow::Result<()> {
    let temp_dir = std::env::temp_dir().join(format!("dabar_adv_aspect_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Matrix of adversarial input dimensions:
    // 1. Ultrawide 21:9 odd: 2561x1079
    // 2. Vertical 9:16 odd: 721x1281
    // 3. Square 1:1 odd: 721x721
    // 4. Widescreen 16:9 odd: 1919x1079
    // 5. Extreme panoramic: 3840x1080 (32:9)
    // 6. Extreme vertical tower: 400x1600 (1:4)
    let test_inputs = vec![
        ("ultrawide_odd", 2561, 1079),
        ("vertical_odd", 721, 1281),
        ("square_odd", 721, 721),
        ("widescreen_odd", 1919, 1079),
        ("panoramic_extreme", 3840, 1080),
        ("vertical_tower", 400, 1600),
    ];

    let target_ratios = vec![
        ("9:16", 1080u32, 1920u32),
        ("1:1", 1080u32, 1080u32),
        ("16:9", 1920u32, 1080u32),
    ];

    for (name, in_w, in_h) in &test_inputs {
        let input_file = temp_dir.join(format!("src_{name}.mp4"));
        create_synthetic_video(&input_file, *in_w, *in_h, 2).await?;
        assert!(has_video_stream(input_file.to_str().unwrap()).await);

        for (ratio_str, expected_w, expected_h) in &target_ratios {
            let output_file =
                temp_dir.join(format!("out_{name}_{}.mp4", ratio_str.replace(':', "_")));

            // Extract clip using dabar_core::ffmpeg::extract_clip
            let res = extract_clip(
                input_file.to_str().unwrap(),
                &output_file,
                0.0,
                1.5,
                ratio_str,
                None,
                None,
            )
            .await;

            assert!(res.is_ok(), "Rendering failed for input {name} ({in_w}x{in_h}) to aspect ratio {ratio_str}: {:?}", res.err());
            assert!(
                output_file.exists(),
                "Output file does not exist: {:?}",
                output_file
            );

            // Probe dimensions and verify even bounds and exact match
            let (out_w, out_h) = probe_video_dimensions(&output_file).await?;
            assert_eq!(out_w % 2, 0, "Width must be even for {name} -> {ratio_str}");
            assert_eq!(
                out_h % 2,
                0,
                "Height must be even for {name} -> {ratio_str}"
            );
            assert_eq!(
                out_w, *expected_w,
                "Width mismatch for {name} -> {ratio_str}"
            );
            assert_eq!(
                out_h, *expected_h,
                "Height mismatch for {name} -> {ratio_str}"
            );

            let _ = tokio::fs::remove_file(&output_file).await;
        }

        let _ = tokio::fs::remove_file(&input_file).await;
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    Ok(())
}

// =========================================================================
// Category 2: ASS Subtitle Generator Adversarial Stress-Testing
// =========================================================================

#[test]
fn test_ass_subtitles_adversarial_strings() {
    let adversarial_segments = vec![
        // Colons, Scripture references, percent signs, quotes, math symbols
        TranscriptSegment {
            start: 0.0,
            end: 4.0,
            text: "Read John 3:16 & Genesis 1:1-3: \"God's 100% Grace > 0% Works\"!".to_string(),
        },
        // Emojis & multi-lingual Unicode
        TranscriptSegment {
            start: 4.5,
            end: 8.0,
            text: "Blessings 🙏🔥✝️! Hebrew: בְּרֵאשִׁית, Greek: Ἐν ἀρχῇ, Chinese: 神爱世人".to_string(),
        },
        // ASS curly braces injection attempt (must be sanitized to parentheses)
        TranscriptSegment {
            start: 8.5,
            end: 12.0,
            text: "Malicious tag attempt: {\\b1\\c&H0000FF&}Injected{\\b0} and {override_tag}"
                .to_string(),
        },
        // Backslashes, Windows path lookalikes, newlines
        TranscriptSegment {
            start: 12.5,
            end: 16.0,
            text: "Path: C:\\Program Files\\Dabar\\bin\\ffmpeg.exe\r\nLine 2 with \\N and \\h"
                .to_string(),
        },
        // Consecutive commas (testing ASS format comma-delimited parser safety)
        TranscriptSegment {
            start: 16.5,
            end: 20.0,
            text: "Comma, test,, triple,,, quadruple,,,, multiple fields!".to_string(),
        },
        // Semicolons and single quotes
        TranscriptSegment {
            start: 20.5,
            end: 24.0,
            text: "Pastor's sermon; part 'A'; part 'B'; don't fail!".to_string(),
        },
    ];

    // Test across all 3 style presets
    for style in &["amber", "kinetic", "editorial"] {
        let ass = generate_ass_subtitles(&adversarial_segments, 0.0, 25.0, 0.0, 1080, 1920, style)
            .expect("should generate ASS without error");

        // 1. Check ASS structure
        assert!(ass.contains("[Script Info]"));
        assert!(ass.contains("[V4+ Styles]"));
        assert!(ass.contains("[Events]"));

        // 2. Verify curly braces were sanitized to avoid ASS tag injection
        assert!(!ass.contains("{\\b1"), "Curly braces must be sanitized");
        if *style == "kinetic" {
            assert!(
                ass.contains("(\\B1\\C&H0000FF&)INJECTED(\\B0)"),
                "Braces should be converted to parentheses and text uppercased"
            );
        } else {
            assert!(
                ass.contains("(\\b1\\c&H0000FF&)Injected(\\b0)"),
                "Braces should be converted to parentheses"
            );
        }

        // 3. Verify scripture reference with colon survived intact
        if *style == "kinetic" {
            assert!(ass.contains("JOHN 3:16"));
        } else {
            assert!(ass.contains("John 3:16"));
        }

        // 4. Verify emojis and Unicode survived intact
        assert!(ass.contains("🙏🔥✝️"));
        assert!(ass.contains("בְּרֵאשִׁית"));
        assert!(ass.contains("神爱世人"));

        // 5. Verify percent signs survived intact
        assert!(ass.contains("100%"));

        // 6. Verify Dialogue line format has exactly 9 commas before text
        for line in ass.lines().filter(|l| l.starts_with("Dialogue:")) {
            let prefix_until_text: Vec<&str> = line.splitn(10, ',').collect();
            assert_eq!(
                prefix_until_text.len(),
                10,
                "Dialogue line must have 10 parts (9 comma separators): {line}"
            );
        }
    }
}

#[test]
fn test_ass_subtitles_segment_timing_edge_cases() {
    let segments = vec![
        // Normal segment
        TranscriptSegment {
            start: 10.0,
            end: 15.0,
            text: "Valid segment".to_string(),
        },
        // Zero duration segment (must be skipped or handled cleanly)
        TranscriptSegment {
            start: 16.0,
            end: 16.0,
            text: "Zero duration".to_string(),
        },
        // Sub-50ms segment (too short for human perception)
        TranscriptSegment {
            start: 17.0,
            end: 17.02,
            text: "Micro segment".to_string(),
        },
        // Inverted duration segment (end < start)
        TranscriptSegment {
            start: 19.0,
            end: 18.0,
            text: "Inverted segment".to_string(),
        },
        // Straddling start boundary: starts before clip, ends inside clip
        TranscriptSegment {
            start: 8.0,
            end: 12.0,
            text: "Straddling start".to_string(),
        },
        // Straddling end boundary: starts inside clip, ends after clip
        TranscriptSegment {
            start: 18.0,
            end: 23.0,
            text: "Straddling end".to_string(),
        },
        // Completely before clip
        TranscriptSegment {
            start: 0.0,
            end: 5.0,
            text: "Before clip".to_string(),
        },
        // Completely after clip
        TranscriptSegment {
            start: 25.0,
            end: 30.0,
            text: "After clip".to_string(),
        },
        // Empty text
        TranscriptSegment {
            start: 13.0,
            end: 14.0,
            text: "   \n\r\t  ".to_string(),
        },
    ];

    // Clip is from 10.0 to 20.0
    let ass = generate_ass_subtitles(&segments, 10.0, 20.0, 10.0, 1080, 1920, "amber")
        .expect("should generate ASS subtitles");

    // "Before clip" and "After clip" must NOT be in the events
    assert!(!ass.contains("Before clip"));
    assert!(!ass.contains("After clip"));

    // "Zero duration" and "Inverted segment" must NOT be in events
    assert!(!ass.contains("Zero duration"));
    assert!(!ass.contains("Inverted segment"));
    assert!(!ass.contains("Micro segment"));

    // Straddling segments should be clamped to clip duration
    assert!(ass.contains("Straddling start"));
    assert!(ass.contains("Straddling end"));
    assert!(ass.contains("Valid segment"));
}

#[tokio::test]
async fn test_burn_in_adversarial_subtitles_with_ffmpeg() -> anyhow::Result<()> {
    let temp_dir = std::env::temp_dir().join(format!("dabar_adv_subburn_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    let src_file = temp_dir.join("src_sub.mp4");
    create_synthetic_video(&src_file, 1280, 720, 5).await?;

    let segments = vec![
        TranscriptSegment {
            start: 0.0,
            end: 2.0,
            text: "Read John 3:16: \"100% Grace, 0% Works!\" 🙏✝️".to_string(),
        },
        TranscriptSegment {
            start: 2.2,
            end: 4.5,
            text: "Path: C:\\Dabar\\data; tag: {override}; Special: %s %d".to_string(),
        },
    ];

    for style in &["amber", "kinetic", "editorial"] {
        let out_file = temp_dir.join(format!("out_sub_{style}.mp4"));

        let res = extract_clip_with_timeline_offset(
            src_file.to_str().unwrap(),
            &out_file,
            0.0,
            4.0,
            0.0,
            "9:16",
            Some(&segments),
            Some(style),
        )
        .await;

        assert!(
            res.is_ok(),
            "Failed burning subtitle style {style}: {:?}",
            res.err()
        );
        assert!(out_file.exists());

        let (w, h) = probe_video_dimensions(&out_file).await?;
        assert_eq!((w, h), (1080, 1920));

        let _ = tokio::fs::remove_file(&out_file).await;
    }

    let _ = tokio::fs::remove_file(&src_file).await;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    Ok(())
}

// =========================================================================
// Category 3: Tool Discovery Resolution
// =========================================================================

#[test]
fn test_tool_discovery_resolution() {
    // 1. Normal resolution: ffmpeg, ffprobe, and yt-dlp must be found
    let ffmpeg_path = find_binary("ffmpeg");
    assert!(ffmpeg_path.is_some(), "ffmpeg should be discovered");
    assert!(
        ffmpeg_path.unwrap().exists(),
        "discovered ffmpeg path must exist"
    );

    let ffprobe_path = find_binary("ffprobe");
    assert!(ffprobe_path.is_some(), "ffprobe should be discovered");
    assert!(
        ffprobe_path.unwrap().exists(),
        "discovered ffprobe path must exist"
    );

    let yt_dlp_path = find_binary("yt-dlp");
    assert!(yt_dlp_path.is_some(), "yt-dlp should be discovered");
    assert!(
        yt_dlp_path.unwrap().exists(),
        "discovered yt-dlp path must exist"
    );

    // 2. Non-existent binary returns None
    assert!(find_binary("non_existent_binary_foo_bar_xyz_12345").is_none());

    // 3. Environment variable override
    let dummy_file = std::env::temp_dir().join(format!("dummy_bin_{}.exe", uuid::Uuid::new_v4()));
    std::fs::write(&dummy_file, b"dummy binary").unwrap();

    std::env::set_var("CUSTOM_TOOL_TEST_PATH", dummy_file.to_str().unwrap());
    let discovered = find_binary("custom-tool-test");
    assert!(discovered.is_some());
    assert_eq!(discovered.unwrap(), dummy_file);

    // Clean up
    std::env::remove_var("CUSTOM_TOOL_TEST_PATH");
    let _ = std::fs::remove_file(&dummy_file);

    // 4. Invalid environment variable override falls back gracefully
    std::env::set_var(
        "NON_EXISTENT_TOOL_PATH",
        "C:\\non\\existent\\path\\to\\bin.exe",
    );
    let res = find_binary("non-existent-tool");
    assert!(res.is_none());
    std::env::remove_var("NON_EXISTENT_TOOL_PATH");

    // 5. refresh_process_path tests
    let temp_bin = std::env::temp_dir().join(format!("test_bin_dir_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_bin).unwrap();
    refresh_process_path(&temp_bin);
    let current_path = std::env::var("PATH").unwrap_or_default();
    assert!(current_path.contains(temp_bin.to_str().unwrap()));
    let _ = std::fs::remove_dir_all(&temp_bin);
}

#[tokio::test]
async fn test_audio_only_source_rendering() -> anyhow::Result<()> {
    let temp_dir = std::env::temp_dir().join(format!("dabar_adv_audio_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    // Create an audio-only MP3
    let audio_file = temp_dir.join("test_speech.mp3");
    let gen_status = get_binary_command("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=440:sample_rate=44100")
        .arg("-t")
        .arg("3")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg(&audio_file)
        .output()
        .await?;
    assert!(gen_status.status.success());
    assert!(!has_video_stream(audio_file.to_str().unwrap()).await);

    let subtitles = vec![TranscriptSegment {
        start: 0.5,
        end: 2.5,
        text: "Audio-only Sermon: Faith Comes By Hearing! 🙏".to_string(),
    }];

    let target_ratios = vec![
        ("9:16", 1080u32, 1920u32),
        ("1:1", 1080u32, 1080u32),
        ("16:9", 1920u32, 1080u32),
    ];

    for (ratio, exp_w, exp_h) in target_ratios {
        let out_file = temp_dir.join(format!("audio_card_{}.mp4", ratio.replace(':', "_")));
        let res = extract_clip(
            audio_file.to_str().unwrap(),
            &out_file,
            0.0,
            2.0,
            ratio,
            Some(&subtitles),
            Some("amber"),
        )
        .await;

        assert!(
            res.is_ok(),
            "Audio-only render failed for {ratio}: {:?}",
            res.err()
        );
        assert!(out_file.exists());

        let (w, h) = probe_video_dimensions(&out_file).await?;
        assert_eq!((w, h), (exp_w, exp_h));
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);

        let _ = tokio::fs::remove_file(&out_file).await;
    }

    let _ = tokio::fs::remove_file(&audio_file).await;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    Ok(())
}
