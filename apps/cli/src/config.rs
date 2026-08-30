use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub groq_api_key: Option<String>,
    pub output_dir: Option<String>,
    pub offline_mode: bool,
    pub offline_model: String,  // "tiny", "base"
    pub ollama_url: Option<String>,
    pub ollama_model: Option<String>,
    pub custom_vocabulary: Option<String>,
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join("dabar")
            .join("config.toml")
    }

    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local").join("share"))
            .join("dabar")
    }

    pub fn db_path() -> PathBuf {
        Self::data_dir().join("dabar.db")
    }

    pub fn audio_dir() -> PathBuf {
        Self::data_dir().join("audio")
    }

    pub fn models_dir() -> PathBuf {
        Self::data_dir().join("whisper-models")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self {
                offline_model: "base".into(),
                ..Default::default()
            });
        }
        let contents = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    pub fn effective_groq_key(&self) -> Option<String> {
        self.groq_api_key
            .clone()
            .filter(|k| !k.trim().is_empty())
            .or_else(|| std::env::var("GROQ_API_KEY").ok())
    }
}
