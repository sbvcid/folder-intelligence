use std::time::Duration;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    #[allow(dead_code)]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum LlmError {
    #[error("API key not found in environment variable '{0}'")]
    MissingApiKey(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("HTTP {0}: {1}")]
    HttpError(u16, String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub trait LlmProvider: Send + Sync {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String, LlmError>;
    #[allow(dead_code)]
    fn name(&self) -> &str;
}
