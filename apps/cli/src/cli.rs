use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

use crate::config::Config;
use crate::db::Db;

#[derive(Parser)]
#[command(name = "dabar")]
#[command(about = "Dabar CLI — turn sermon audio into shareable clips from your terminal")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Process a local file or YouTube/Google Drive URL into transcript + highlights
    Process {
        input: String,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, env = "GROQ_API_KEY")]
        groq_key: Option<String>,
        #[arg(long)]
        no_highlights: bool,
    },
    /// List all processed sermons
    List,
    /// Export a specific highlight clip to disk as a vertical MP4
    Export {
        sermon_id: String,
        #[arg(long)]
        clip: usize,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print the full transcript for a sermon
    Transcript { sermon_id: String },
    /// Print detected highlights for a sermon
    Highlights { sermon_id: String },
    /// Download a Whisper GGML model for offline transcription
    DownloadModel { model: String },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        cmd: ConfigCommands,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Set a config value (keys: groq-key, output-dir, ollama-url, ollama-model, offline-model)
    Set { key: String, value: String },
    /// Show current configuration
    Show,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load()?;

    let db_path = Config::db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = Db::connect(&db_url).await?;

    match cli.command {
        Commands::Process { input, offline, model, groq_key, no_highlights } => {
            process_sermon(input, offline, model, groq_key, no_highlights, &config, &db).await?;
        }

        Commands::List => {
            let sermons = db.list_sermons().await?;
            if sermons.is_empty() {
                println!("{}", "No sermons found. Run `dabar process <file|url>` to start.".dimmed());
            } else {
                println!("{}", format!("{:<38} {:<25} {:<10} {}", "ID", "Title", "Status", "Date").bold());
                println!("{}", "-".repeat(85).dimmed());
                for s in sermons {
                    let status_colored = match s.status.as_str() {
                        "ready" => s.status.green().to_string(),
                        "failed" => s.status.red().to_string(),
                        _ => s.status.yellow().to_string(),
                    };
                    println!(
                        "{:<38} {:<25} {:<10} {}",
                        s.id.cyan(),
                        s.title.chars().take(24).collect::<String>().bold(),
                        status_colored,
                        s.created_at.chars().take(10).collect::<String>().dimmed(),
                    );
                }
            }
        }

        Commands::Highlights { sermon_id } => {
            let sermon_id = Uuid::parse_str(&sermon_id).context("invalid sermon ID")?;
            let highlights = db.get_sermon_highlights(sermon_id).await?;
            if highlights.is_empty() {
                println!("{}", "No highlights found for this sermon.".dimmed());
            } else {
                println!("{}", format!("{} highlights found:", highlights.len()).bold());
                for (i, hl) in highlights.iter().enumerate() {
                    let dur = hl.end_time - hl.start_time;
                    println!("\n{}. {} ({:.0}s–{:.0}s, {:.0}s clip)",
                        (i + 1).to_string().bold().cyan(),
                        hl.title.bold(),
                        hl.start_time, hl.end_time, dur);
                    println!("   {}", hl.reason);
                    if !hl.suggested_hook_text.is_empty() {
                        println!("   {}", format!("\"{}\"", hl.suggested_hook_text).italic().dimmed());
                    }
                }
            }
        }

        Commands::Transcript { sermon_id } => {
            let sermon_id = Uuid::parse_str(&sermon_id).context("invalid sermon ID")?;
            let segments = db.get_sermon_segments(sermon_id).await?;
            if segments.is_empty() {
                println!("{}", "No transcript found.".dimmed());
            } else {
                for seg in &segments {
                    let mins = (seg.start / 60.0) as u32;
                    let secs = (seg.start % 60.0) as u32;
                    println!("{} {}", format!("{:02}:{:02}", mins, secs).dimmed(), seg.text);
                }
            }
        }

        Commands::Export { sermon_id, clip, out } => {
            let sermon_id = Uuid::parse_str(&sermon_id).context("invalid sermon ID")?;
            let highlights = db.get_sermon_highlights(sermon_id).await?;

            if clip < 1 || clip > highlights.len() {
                bail!("Clip {} not found. This sermon has {} highlights.", clip, highlights.len());
            }
            let hl = &highlights[clip - 1];

            let sermons = db.list_sermons().await?;
            let sermon = sermons.iter().find(|s| s.id == sermon_id.to_string())
                .context("sermon not found")?;
            let audio_path = sermon.audio_path.as_deref()
                .context("no audio path for this sermon")?;

            let out_dir = out.unwrap_or_else(|| {
                dirs::video_dir().unwrap_or_else(|| PathBuf::from(".")).join("Dabar")
            });
            std::fs::create_dir_all(&out_dir)?;

            let safe_title: String = hl.title.chars()
                .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
                .take(50).collect();
            let out_path = out_dir.join(format!("dabar_{safe_title}.mp4"));

            let pb = ProgressBar::new_spinner();
            pb.enable_steady_tick(Duration::from_millis(120));
            pb.set_style(ProgressStyle::default_spinner().template("{spinner:.yellow} {msg}")?);
            pb.set_message(format!("Rendering clip {}…", clip));

            dabar_core::ffmpeg::extract_vertical_clip(audio_path, &out_path, hl.start_time, hl.end_time).await?;

            pb.finish_and_clear();
            println!("{} Exported: {}", "✓".green().bold(), out_path.display().to_string().cyan());
        }

        Commands::DownloadModel { model } => {
            let models_dir = Config::models_dir();
            std::fs::create_dir_all(&models_dir)?;

            let (filename, url) = match model.as_str() {
                "tiny" => ("ggml-tiny.bin", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"),
                _ => ("ggml-base.bin", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"),
            };

            let dest = models_dir.join(filename);
            if dest.exists() {
                println!("{} Already downloaded: {}", "✓".green(), dest.display());
                return Ok(());
            }

            println!("Downloading {} model…", model.bold());
            let client = reqwest::Client::builder().timeout(Duration::from_secs(600)).build()?;
            let mut response = client.get(url).send().await?;
            let total = response.content_length().unwrap_or(0);

            let pb = ProgressBar::new(total);
            pb.set_style(ProgressStyle::default_bar()
                .template("{bar:40.green/dim} {percent}% — {bytes}/{total_bytes}")?);

            let mut bytes: Vec<u8> = Vec::new();
            let mut downloaded: u64 = 0;
            while let Some(chunk) = response.chunk().await? {
                downloaded += chunk.len() as u64;
                bytes.extend_from_slice(&chunk);
                pb.set_position(downloaded);
            }

            tokio::fs::write(&dest, &bytes).await?;
            pb.finish_and_clear();
            println!("{} Saved: {}", "✓".green().bold(), dest.display().to_string().cyan());
        }

        Commands::Config { cmd } => match cmd {
            ConfigCommands::Set { key, value } => {
                match key.as_str() {
                    "groq-key" => config.groq_api_key = Some(value),
                    "output-dir" => config.output_dir = Some(value),
                    "ollama-url" => config.ollama_url = Some(value),
                    "ollama-model" => config.ollama_model = Some(value),
                    "offline-model" => config.offline_model = value,
                    _ => bail!("Unknown config key '{}'. Valid keys: groq-key, output-dir, ollama-url, ollama-model, offline-model", key),
                }
                config.save()?;
                println!("{} Config updated: {}", "✓".green(), key.bold());
            }
            ConfigCommands::Show => {
                println!("{}", "Dabar CLI Configuration:".bold());
                println!("  Config: {}", Config::config_path().display().to_string().cyan());
                println!("  DB:     {}", Config::db_path().display().to_string().cyan());
                println!("  Models: {}", Config::models_dir().display().to_string().cyan());
                println!();
                println!("  groq_api_key:  {}", config.groq_api_key.as_deref().map(|_| "***set***").unwrap_or("(not set)").yellow());
                println!("  offline_mode:  {}", if config.offline_mode { "true".green().to_string() } else { "false".dimmed().to_string() });
                println!("  offline_model: {}", config.offline_model.yellow());
                println!("  ollama_url:    {}", config.ollama_url.as_deref().unwrap_or("(not set)").dimmed());
                println!("  ollama_model:  {}", config.ollama_model.as_deref().unwrap_or("(not set)").dimmed());
            }
        },
    }

    Ok(())
}

async fn process_sermon(
    input: String,
    offline: bool,
    model_opt: Option<String>,
    groq_key: Option<String>,
    no_highlights: bool,
    config: &Config,
    db: &Db,
) -> Result<()> {
    let sermon_id = Uuid::new_v4();
    let title = if input.starts_with("http") {
        "Processing…".to_string()
    } else {
        std::path::Path::new(&input)
            .file_stem().and_then(|s| s.to_str()).unwrap_or("Sermon").to_string()
    };

    let mut sermon = dabar_core::Sermon::queued(input.clone());
    sermon.title = title;
    db.insert_sermon(&sermon).await?;

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner:.yellow} {msg}")?);

    // Stage 1: Download / locate
    let audio_path: PathBuf = if input.contains("youtube.com/") || input.contains("youtu.be/") {
        pb.set_message("Downloading from YouTube…");
        db.update_status(sermon_id, dabar_core::SermonStatus::Downloading).await?;
        let temp_dir = std::env::temp_dir().join(format!("dabar_{sermon_id}"));
        tokio::fs::create_dir_all(&temp_dir).await?;
        let audio_dir = Config::audio_dir();
        tokio::fs::create_dir_all(&audio_dir).await?;
        let result = dabar_core::downloader::download_youtube_audio(&input, &temp_dir).await?;
        let persistent = audio_dir.join(format!("{sermon_id}.mp3"));
        if tokio::fs::rename(&result.path, &persistent).await.is_err() {
            tokio::fs::copy(&result.path, &persistent).await?;
        }
        persistent
    } else if dabar_core::downloader::is_gdrive_url(&input) {
        pb.set_message("Downloading from Google Drive…");
        db.update_status(sermon_id, dabar_core::SermonStatus::Downloading).await?;
        let temp_dir = std::env::temp_dir().join(format!("dabar_{sermon_id}"));
        tokio::fs::create_dir_all(&temp_dir).await?;
        let audio_dir = Config::audio_dir();
        tokio::fs::create_dir_all(&audio_dir).await?;
        let result = dabar_core::downloader::download_gdrive_audio(&input, &temp_dir).await?;
        let persistent = audio_dir.join(format!("{sermon_id}.mp3"));
        if tokio::fs::rename(&result.path, &persistent).await.is_err() {
            tokio::fs::copy(&result.path, &persistent).await?;
        }
        persistent
    } else {
        let p = PathBuf::from(&input);
        if !p.exists() { bail!("File not found: {}", input); }
        p
    };

    // Stage 2: Transcribe
    pb.set_message(if offline { "Transcribing offline (whisper.cpp)…" } else { "Transcribing via Groq Whisper…" });
    db.update_status(sermon_id, dabar_core::SermonStatus::Transcribing).await?;

    let transcription_backend = if offline {
        let model_name = model_opt.as_deref().unwrap_or(&config.offline_model);
        let model_path = Config::models_dir().join(format!("ggml-{model_name}.bin"));
        if !model_path.exists() {
            bail!("Offline model not found at {}.\nRun: dabar download-model {}", model_path.display(), model_name);
        }
        dabar_core::whisper::TranscriptionBackend::Local { model_path }
    } else {
        let key = groq_key.clone().or_else(|| config.effective_groq_key()).unwrap_or_default();
        dabar_core::whisper::TranscriptionBackend::Groq { api_key: key }
    };

    let transcription_result = dabar_core::whisper::transcribe_audio(
        &transcription_backend, &audio_path, None, None,
    ).await?;
    let segments = transcription_result.segments;
    pb.set_message(format!("Transcript ready — {} segments", segments.len()));

    // Stage 3: Highlights
    let (highlights, chapters) = if no_highlights {
        (vec![], vec![])
    } else {
        pb.set_message("Detecting pastoral highlights…");
        db.update_status(sermon_id, dabar_core::SermonStatus::Detecting).await?;

        let backend = if offline {
            dabar_core::llm::LlmBackend::Ollama {
                base_url: config.ollama_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
                model: config.ollama_model.clone().unwrap_or_else(|| "llama3.2:3b".into()),
            }
        } else {
            let key = groq_key.or_else(|| config.effective_groq_key()).unwrap_or_default();
            dabar_core::llm::LlmBackend::Groq { api_key: key }
        };

        match dabar_core::llm::detect_sermon_analysis_report_with_backend(&backend, &segments).await {
            Ok(analysis) => (analysis.highlights_report.highlights, analysis.chapters),
            Err(e) => { eprintln!("{} Highlight detection failed: {}", "!".yellow(), e); (vec![], vec![]) }
        }
    };

    // Stage 4: Save
    pb.set_message("Saving…");
    db.save_results(sermon_id, audio_path.to_str(), &segments, &highlights, &chapters).await?;
    pb.finish_and_clear();

    println!("\n{} Sermon processed!", "✓".green().bold());
    println!("  ID:         {}", sermon_id.to_string().cyan());
    println!("  Segments:   {}", segments.len().to_string().bold());
    println!("  Highlights: {}", highlights.len().to_string().bold());
    println!("  Chapters:   {}", chapters.len().to_string().bold());
    if !highlights.is_empty() {
        println!("\n  {} dabar highlights {}", "→".dimmed(), sermon_id.to_string().cyan());
        println!("  {} dabar export {} --clip 1", "→".dimmed(), sermon_id.to_string().cyan());
    }
    Ok(())
}
