use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: String,
    model: String,
    timeout: Duration,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, api_key: String, model: String, timeout: Duration) -> Self {
        Self {
            base_url,
            api_key,
            model,
            timeout,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
    response_format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAiChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenAiChoice {
    message: OpenAiChatMessageResponse,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenAiChatMessageResponse {
    content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenAiErrorResponse {
    error: Option<OpenAiErrorDetail>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OpenAiErrorDetail {
    message: String,
    #[serde(default)]
    r#type: Option<String>,
}

impl LlmProvider for OpenAiCompatibleProvider {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String, LlmError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| LlmError::ProviderError(format!("HTTP client error: {}", e)))?;

        let request = OpenAiChatRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|m| OpenAiChatMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            response_format: Some(serde_json::json!({
                "type": "json_object"
            })),
        };

        let response = client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(self.timeout)
                } else {
                    LlmError::ProviderError(format!("Request error: {}", e))
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| LlmError::ProviderError(format!("Response read error: {}", e)))?;

        if !status.is_success() {
            let error: Option<OpenAiErrorResponse> = serde_json::from_str(&body).ok();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(LlmError::MissingApiKey(self.api_key.clone()));
            }
            if status.as_u16() == 429 {
                return Err(LlmError::HttpError(429, "Rate limited".to_string()));
            }
            let msg = error
                .and_then(|e| e.error)
                .map(|d| d.message)
                .unwrap_or_else(|| format!("HTTP {}: {}", status, body));
            return Err(LlmError::HttpError(status.as_u16(), msg));
        }

        let chat_response: OpenAiChatResponse = serde_json::from_str(&body)
            .map_err(|e| LlmError::InvalidResponse(format!("Response parse error: {}", e)))?;

        let content = chat_response
            .choices
            .first()
            .ok_or_else(|| LlmError::InvalidResponse("No choices in response".to_string()))?
            .message
            .content
            .clone();

        Ok(content)
    }

    fn name(&self) -> &str {
        "openai-compatible"
    }
}
