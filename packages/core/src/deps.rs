use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Locate a binary across environment variables, standard AppData directories,
/// repository ancestor `bin/` directories, and system PATH.
pub fn find_binary(name: &str) -> Option<PathBuf> {
    // 1. Environment variable override: e.g. FFMPEG_PATH, YT_DLP_PATH, FFPROBE_PATH
    let env_key = format!("{}_PATH", name.to_uppercase().replace('-', "_"));
    if let Ok(custom_path) = std::env::var(&env_key) {
        let trimmed = custom_path.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.exists() {
                return Some(p);
            }
            if let Ok(cwd) = std::env::current_dir() {
                let rel = cwd.join(&p);
                if rel.exists() {
                    return Some(rel);
                }
            }
        }
    }

    let exe_name = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };

    // 2. Check Windows APPDATA & LOCALAPPDATA / Unix HOME
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_p = PathBuf::from(appdata);
        for sub in &["com.dabar.app", "dabar", "com.preshdevops.dabar"] {
            let candidate = appdata_p.join(sub).join("bin").join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let local_p = PathBuf::from(localappdata);
        for sub in &["com.dabar.app", "dabar", "com.preshdevops.dabar"] {
            let candidate = local_p.join(sub).join("bin").join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_p = PathBuf::from(home);
        let cand1 = home_p.join(".dabar").join("bin").join(&exe_name);
        if cand1.exists() {
            return Some(cand1);
        }
        let cand2 = home_p.join(".local").join("bin").join(&exe_name);
        if cand2.exists() {
            return Some(cand2);
        }
    }

    // 3. Walk up ancestor directories from current_dir to locate repo `bin/` (e.g. dabar/bin/yt-dlp.exe)
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: Option<&Path> = Some(cwd.as_path());
        while let Some(ancestor) = dir {
            let bin_dir = ancestor.join("bin");
            let candidate = bin_dir.join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
            // Check subdirectories in case archive was extracted into a nested folder
            if let Ok(mut entries) = std::fs::read_dir(&bin_dir) {
                while let Some(Ok(entry)) = entries.next() {
                    let path = entry.path();
                    if path.is_dir() {
                        let sub1 = path.join(&exe_name);
                        if sub1.exists() {
                            return Some(sub1);
                        }
                        let sub2 = path.join("bin").join(&exe_name);
                        if sub2.exists() {
                            return Some(sub2);
                        }
                    }
                }
            }
            dir = ancestor.parent();
        }
    }

    // 4. Search entries in PATH environment variable
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for part in path_var.split(sep) {
            let part_trimmed = part.trim();
            if part_trimmed.is_empty() {
                continue;
            }
            let candidate = Path::new(part_trimmed).join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Create a tokio Command using the discovered binary path or falling back to the binary name.
pub fn get_binary_command(name: &str) -> Command {
    if let Some(path) = find_binary(name) {
        Command::new(path)
    } else {
        Command::new(name)
    }
}

/// Prepend a directory to the current process's in-memory PATH environment variable
/// so newly downloaded binaries are immediately discoverable.
pub fn refresh_process_path(bin_dir: &Path) {
    if bin_dir.exists() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let sep = if cfg!(windows) { ";" } else { ":" };
        let bin_str = bin_dir.to_string_lossy();
        if !current_path
            .split(if cfg!(windows) { ';' } else { ':' })
            .any(|p| p.eq_ignore_ascii_case(&bin_str))
        {
            std::env::set_var("PATH", format!("{}{sep}{current_path}", bin_dir.display()));
        }
    }
}
