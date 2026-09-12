use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Status of all external tools the app depends on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepsStatus {
    pub ffmpeg: BinaryStatus,
    pub yt_dlp: BinaryStatus,
    pub ffprobe: BinaryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// Check all external dependency statuses without downloading anything.
pub async fn check_all(app_data_dir: &std::path::Path) -> DepsStatus {
    let ffmpeg = check_binary("ffmpeg", app_data_dir).await;
    let yt_dlp = check_binary("yt-dlp", app_data_dir).await;
    let ffprobe = check_binary("ffprobe", app_data_dir).await;

    DepsStatus {
        ffmpeg,
        yt_dlp,
        ffprobe,
    }
}

async fn check_binary(name: &str, app_data_dir: &std::path::Path) -> BinaryStatus {
    // 1. Check environment variable override
    let env_key = name.to_uppercase().replace('-', "_") + "_PATH";
    if let Ok(custom) = std::env::var(&env_key) {
        let p = std::path::Path::new(&custom);
        if p.exists() {
            let version = get_version(p.to_str().unwrap_or(name)).await;
            return BinaryStatus {
                found: true,
                path: Some(custom),
                version,
            };
        }
    }

    // 2. Check app data bin directory
    let bin_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };

    let app_bin = app_data_dir.join("bin").join(&bin_name);
    if app_bin.exists() {
        let version = get_version(app_bin.to_str().unwrap_or(name)).await;
        return BinaryStatus {
            found: true,
            path: Some(app_bin.to_string_lossy().to_string()),
            version,
        };
    }

    // 3. Check via unified discovery across AppData and repository ancestor bin/ dirs
    if let Some(discovered) = dabar_core::ffmpeg::find_binary(name) {
        if discovered.exists() {
            let version = get_version(discovered.to_str().unwrap_or(name)).await;
            return BinaryStatus {
                found: true,
                path: Some(discovered.to_string_lossy().to_string()),
                version,
            };
        }
    }

    // 4. Check system PATH
    let version_arg = if name.contains("ffmpeg") || name.contains("ffprobe") {
        "-version"
    } else {
        "--version"
    };

    match tokio::process::Command::new(name)
        .arg(version_arg)
        .output()
        .await
    {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .map(|l| l.trim().to_string());
            BinaryStatus {
                found: true,
                path: Some(name.to_string()),
                version,
            }
        }
        _ => BinaryStatus {
            found: false,
            path: None,
            version: None,
        },
    }
}

async fn get_version(path: &str) -> Option<String> {
    let arg = if path.contains("ffmpeg") {
        "-version"
    } else {
        "--version"
    };

    tokio::process::Command::new(path)
        .arg(arg)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
}

pub fn available_ram_gb() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory() / (1024 * 1024 * 1024)
}

pub fn is_low_end() -> bool {
    available_ram_gb() <= 4
}

pub fn recommended_ffmpeg_preset() -> &'static str {
    if is_low_end() {
        "ultrafast"
    } else {
        "veryfast"
    }
}

pub async fn download_yt_dlp(app_data_dir: &std::path::Path) -> Result<PathBuf> {
    let bin_dir = app_data_dir.join("bin");
    tokio::fs::create_dir_all(&bin_dir).await?;

    let bin_name = if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    let dest = bin_dir.join(bin_name);

    if dest.exists() {
        tracing::info!("yt-dlp already present at {}", dest.display());
        return Ok(dest);
    }

    let url = if cfg!(windows) {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    };

    tracing::info!("Downloading yt-dlp from {url} to {}", dest.display());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let bytes = client.get(url).send().await?.bytes().await?;
    tokio::fs::write(&dest, &bytes).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&dest).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&dest, perms).await?;
    }

    tracing::info!(
        "yt-dlp downloaded successfully: {} MB",
        bytes.len() / (1024 * 1024)
    );
    dabar_core::ffmpeg::refresh_process_path(&bin_dir);
    Ok(dest)
}

pub async fn download_ffmpeg(
    app_data_dir: &std::path::Path,
    progress_cb: impl Fn(u64, u64) + Send + 'static,
) -> Result<PathBuf> {
    let bin_dir = app_data_dir.join("bin");
    tokio::fs::create_dir_all(&bin_dir).await?;

    let bin_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let dest = bin_dir.join(bin_name);

    if dest.exists() {
        tracing::info!("ffmpeg already present at {}", dest.display());
        return Ok(dest);
    }

    let url = if cfg!(windows) {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
    } else if cfg!(target_os = "macos") {
        "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip"
    } else {
        "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
    };

    tracing::info!("Downloading FFmpeg from {url}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let mut response = client.get(url).send().await?;
    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut bytes: Vec<u8> = Vec::with_capacity(if total > 0 {
        total as usize
    } else {
        30 * 1024 * 1024
    });

    while let Some(chunk) = response.chunk().await? {
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        progress_cb(downloaded, total);
    }

    let temp_archive = bin_dir.join("ffmpeg_download.zip");
    tokio::fs::write(&temp_archive, &bytes).await?;

    let extract_temp_dir = bin_dir.join("ffmpeg_extract_temp");
    let _ = tokio::fs::remove_dir_all(&extract_temp_dir).await;
    tokio::fs::create_dir_all(&extract_temp_dir).await?;

    #[cfg(windows)]
    {
        let tar_res = tokio::process::Command::new("tar")
            .arg("-xf")
            .arg(&temp_archive)
            .arg("-C")
            .arg(&extract_temp_dir)
            .output()
            .await;

        if tar_res.is_err() || !tar_res.unwrap().status.success() {
            let _ = tokio::process::Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(format!(
                    "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
                    temp_archive.display(),
                    extract_temp_dir.display()
                ))
                .output()
                .await;
        }

        if let Ok(entries) =
            find_files_recursive(&extract_temp_dir, &["ffmpeg.exe", "ffprobe.exe"]).await
        {
            for (name, path) in entries {
                let target = bin_dir.join(&name);
                let _ = tokio::fs::copy(&path, &target).await;
            }
        }
    }

    #[cfg(unix)]
    {
        let _ = tokio::process::Command::new("unzip")
            .arg("-o")
            .arg(&temp_archive)
            .arg("-d")
            .arg(&extract_temp_dir)
            .output()
            .await;

        if let Ok(entries) = find_files_recursive(&extract_temp_dir, &["ffmpeg", "ffprobe"]).await {
            for (name, path) in entries {
                let target = bin_dir.join(&name);
                let _ = tokio::fs::copy(&path, &target).await;
                use std::os::unix::fs::PermissionsExt;
                if let Ok(mut perms) = tokio::fs::metadata(&target).await.map(|m| m.permissions()) {
                    perms.set_mode(0o755);
                    let _ = tokio::fs::set_permissions(&target, perms).await;
                }
            }
        }
    }

    let _ = tokio::fs::remove_file(&temp_archive).await;
    let _ = tokio::fs::remove_dir_all(&extract_temp_dir).await;

    if dest.exists() {
        tracing::info!("FFmpeg successfully installed to {}", dest.display());
        dabar_core::ffmpeg::refresh_process_path(&bin_dir);
        Ok(dest)
    } else {
        anyhow::bail!(
            "FFmpeg archive was downloaded but binary could not be extracted to {}",
            dest.display()
        )
    }
}

async fn find_files_recursive(
    dir: &std::path::Path,
    target_names: &[&str],
) -> Result<Vec<(String, PathBuf)>> {
    let mut results = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    for &target in target_names {
                        if file_name_lower == target.to_lowercase() {
                            results.push((target.to_string(), path.clone()));
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Offline tool status summary for the Settings screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineStatus {
    pub ffmpeg_ready: bool,
    pub yt_dlp_ready: bool,
}

/// Check which tools are already installed.
pub async fn get_offline_status(app_data_dir: &std::path::Path) -> OfflineStatus {
    let status = check_all(app_data_dir).await;
    OfflineStatus {
        ffmpeg_ready: status.ffmpeg.found,
        yt_dlp_ready: status.yt_dlp.found,
    }
}
