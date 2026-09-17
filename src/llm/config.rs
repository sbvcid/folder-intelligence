use crate::llm::provider::LlmError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

fn default_base_url() -> String {
    "https://api.openai.com/v1/chat/completions".to_string()
}

fn default_api_key_env() -> String {
    "FOLDER_INTELLIGENCE_API_KEY".to_string()
}

fn default_timeout_seconds() -> u64 {
    60
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(),
            model: "mock-ai-v1.0".to_string(),
            base_url: default_base_url(),
            api_key_env: default_api_key_env(),
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

impl LlmConfig {
    pub fn config_dir() -> Option<PathBuf> {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join(".folder-intelligence").join("config.toml"))
    }

    pub fn load() -> Result<Self, LlmError> {
        match Self::config_path() {
            Some(path) if path.exists() => Self::load_from(&path),
            _ => Ok(Self::default()),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self, LlmError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            LlmError::ConfigError(format!(
                "Failed to read config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        let config: LlmConfig = toml::from_str(&content).map_err(|e| {
            LlmError::ConfigError(format!(
                "Failed to parse config file '{}': {}",
                path.display(),
                e
            ))
        })?;
        Ok(config)
    }

    #[allow(dead_code)]
    pub fn resolve_api_key(&self) -> Result<String, LlmError> {
        if self.provider == "mock" {
            return Ok(String::new());
        }
        let var_name = self.api_key_env.clone();
        std::env::var(&var_name).map_err(|_| LlmError::MissingApiKey(var_name))
    }

    #[allow(dead_code)]
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}
