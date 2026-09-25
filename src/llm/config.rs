use crate::llm::provider::LlmError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    #[serde(alias = "endpoint")]
    pub base_url: Option<String>,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub api_key: Option<String>,
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
            base_url: None,
            api_key_env: default_api_key_env(),
            timeout_seconds: default_timeout_seconds(),
            api_key: None,
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

    pub fn resolve_api_key(&self) -> Result<String, LlmError> {
        if self.provider == "mock" || self.provider == "ollama" {
            return Ok(String::new());
        }
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                return Ok(key.clone());
            }
        }
        let var_name = self.api_key_env.clone();
        std::env::var(&var_name).map_err(|_| LlmError::MissingApiKey(var_name))
    }

    pub fn resolved_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| Self::default_base_url_for(&self.provider))
    }

    fn default_base_url_for(provider: &str) -> String {
        match provider {
            "ollama" => "http://localhost:11434/api/chat".to_string(),
            _ => "https://api.openai.com/v1/chat/completions".to_string(),
        }
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_config_defaults() {
        let config = LlmConfig {
            provider: "ollama".to_string(),
            model: "llama2".to_string(),
            base_url: None,
            api_key_env: default_api_key_env(),
            timeout_seconds: 60,
            api_key: None,
        };

        assert_eq!(
            config.resolved_base_url(),
            "http://localhost:11434/api/chat"
        );
        assert_eq!(config.resolve_api_key().unwrap(), "");
    }

    #[test]
    fn test_ollama_config_with_explicit_endpoint() {
        let config = LlmConfig {
            provider: "ollama".to_string(),
            model: "llama2".to_string(),
            base_url: Some("http://my-ollama-server:11434/api/chat".to_string()),
            api_key_env: default_api_key_env(),
            timeout_seconds: 30,
            api_key: None,
        };

        assert_eq!(
            config.resolved_base_url(),
            "http://my-ollama-server:11434/api/chat"
        );
        assert_eq!(config.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_ollama_config_endpoint_alias() {
        let toml_str = r#"
provider = "ollama"
model = "llama2"
endpoint = "http://localhost:11434/api/chat"
timeout_seconds = 45
"#;
        let config: LlmConfig = toml::from_str(toml_str).expect("should parse with endpoint alias");

        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "llama2");
        assert_eq!(
            config.resolved_base_url(),
            "http://localhost:11434/api/chat"
        );
        assert_eq!(config.timeout(), Duration::from_secs(45));
    }

    #[test]
    fn test_openai_config_base_url_alias() {
        let toml_str = r#"
provider = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1/chat/completions"
api_key_env = "MY_KEY"
timeout_seconds = 120
"#;
        let config: LlmConfig = toml::from_str(toml_str).expect("should parse with endpoint alias");

        assert_eq!(config.provider, "openai");
        assert_eq!(
            config.resolved_base_url(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(config.api_key_env, "MY_KEY");
    }

    #[test]
    fn test_openai_compatible_config_with_env_key() {
        let config = LlmConfig {
            provider: "openai-compatible".to_string(),
            model: "gpt-4".to_string(),
            base_url: Some("https://example.com/v1".to_string()),
            api_key_env: "TEST_OPENAI_COMPATIBLE_KEY".to_string(),
            timeout_seconds: 120,
            api_key: None,
        };

        assert_eq!(config.provider, "openai-compatible");
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.api_key_env, "TEST_OPENAI_COMPATIBLE_KEY");
        assert_eq!(config.timeout(), Duration::from_secs(120));
        assert_eq!(
            config.resolved_base_url(),
            "https://example.com/v1",
            "resolved_base_url returns raw endpoint; provider normalizes to /chat/completions"
        );
    }

    #[test]
    fn test_openai_compatible_requires_api_key() {
        let config = LlmConfig {
            provider: "openai-compatible".to_string(),
            model: "gpt-4".to_string(),
            base_url: Some("https://example.com/v1".to_string()),
            api_key_env: "NONEXISTENT_TEST_KEY_12345".to_string(),
            timeout_seconds: 60,
            api_key: None,
        };

        let result = config.resolve_api_key();
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::MissingApiKey(env_var) => {
                assert_eq!(env_var, "NONEXISTENT_TEST_KEY_12345");
            }
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }
    }

    #[test]
    fn test_mock_config_does_not_require_api_key() {
        let config = LlmConfig {
            provider: "mock".to_string(),
            model: "mock-ai-v1.0".to_string(),
            base_url: None,
            api_key_env: default_api_key_env(),
            timeout_seconds: 60,
            api_key: None,
        };

        assert_eq!(config.resolve_api_key().unwrap(), "");
    }

    #[test]
    fn test_load_from_missing_file() {
        let result = LlmConfig::load_from(std::path::Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to read config file"));
    }

    #[test]
    fn test_load_from_malformed_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "this is not valid TOML = =").unwrap();

        let result = LlmConfig::load_from(&config_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to parse config file"));
    }
}
