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
        let normalized = normalize_endpoint(&base_url);
        Self {
            base_url: normalized,
            api_key,
            model,
            timeout,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

fn normalize_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
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
    #[allow(dead_code)]
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
                } else if e.is_connect() {
                    LlmError::ProviderError(format!(
                        "Failed to connect to provider at '{}': {}",
                        self.base_url, e
                    ))
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
                return Err(LlmError::MissingApiKey("***".to_string()));
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

        if content.is_empty() {
            return Err(LlmError::InvalidResponse(
                "Provider returned empty response content".to_string(),
            ));
        }

        Ok(content)
    }

    fn name(&self) -> &str {
        "openai-compatible"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com/v1".to_string(),
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(60),
        );
        assert_eq!(provider.name(), "openai-compatible");
        assert_eq!(provider.model(), "gpt-4");
        assert_eq!(provider.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_endpoint_without_trailing_slash() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com/v1".to_string(),
            "key".to_string(),
            "model".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(
            provider.base_url(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_endpoint_with_trailing_slash() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com/v1/".to_string(),
            "key".to_string(),
            "model".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(
            provider.base_url(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_endpoint_already_has_chat_completions() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com/v1/chat/completions".to_string(),
            "key".to_string(),
            "model".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(
            provider.base_url(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_endpoint_with_trailing_slash_and_chat_completions() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com/v1/chat/completions/".to_string(),
            "key".to_string(),
            "model".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(
            provider.base_url(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_endpoint_without_v1_prefix() {
        let provider = OpenAiCompatibleProvider::new(
            "https://example.com".to_string(),
            "key".to_string(),
            "model".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(provider.base_url(), "https://example.com/chat/completions");
    }

    #[test]
    fn test_request_does_not_include_response_format() {
        let request = OpenAiChatRequest {
            model: "gemma4:12b".to_string(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
        };

        let json = serde_json::to_string(&request).expect("should serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse JSON");

        assert!(
            !parsed.as_object().unwrap().contains_key("response_format"),
            "request must NOT include response_format"
        );
    }

    #[test]
    fn test_parse_openai_compatible_success_response() {
        let response_json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from OpenAI-compatible!"
                },
                "finish_reason": "stop"
            }]
        }"#;

        let chat_response: OpenAiChatResponse =
            serde_json::from_str(response_json).expect("should parse success response");

        let content = chat_response
            .choices
            .first()
            .expect("should have choices")
            .message
            .content
            .clone();

        assert_eq!(content, "Hello from OpenAI-compatible!");
    }

    #[test]
    fn test_parse_openai_error_response() {
        let error_json =
            r#"{"error": {"message": "Invalid API key", "type": "invalid_request_error"}}"#;

        let error: OpenAiErrorResponse =
            serde_json::from_str(error_json).expect("should parse error response");

        assert_eq!(error.error.unwrap().message, "Invalid API key");
    }

    #[test]
    fn test_normalize_endpoint_appends_chat_completions() {
        assert_eq!(
            normalize_endpoint("https://example.com/v1"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_endpoint("https://example.com/v1/"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_endpoint("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_endpoint("https://example.com"),
            "https://example.com/chat/completions"
        );
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_successful_chat_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "id": "chatcmpl-123",
                    "object": "chat.completion",
                    "created": 1700000000,
                    "model": "gpt-4",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from OpenAI!"
                        },
                        "finish_reason": "stop"
                    }]
                }"#,
            )
            .create();

        let provider = OpenAiCompatibleProvider::new(
            server.url() + "/v1",
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap(), "Hello from OpenAI!");

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_http_error() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": {"message": "Unauthorized"}}"#)
            .create();

        let provider = OpenAiCompatibleProvider::new(
            server.url() + "/v1",
            "bad-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::MissingApiKey(_) => {}
            other => panic!("expected MissingApiKey, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_malformed_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"not valid json at all"#)
            .create();

        let provider = OpenAiCompatibleProvider::new(
            server.url() + "/v1",
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::InvalidResponse(msg) => {
                assert!(msg.contains("Response parse error"));
            }
            other => panic!("expected InvalidResponse, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_connection_failure() {
        let provider = OpenAiCompatibleProvider::new(
            "http://127.0.0.1:1/v1/chat/completions".to_string(),
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(5),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ProviderError(msg) => {
                assert!(msg.contains("Failed to connect"));
            }
            other => panic!("expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_empty_response_content() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "id": "chatcmpl-123",
                    "object": "chat.completion",
                    "choices": [{
                        "message": {"role": "assistant", "content": ""}
                    }]
                }"#,
            )
            .create();

        let provider = OpenAiCompatibleProvider::new(
            server.url() + "/v1",
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::InvalidResponse(msg) => {
                assert!(msg.contains("empty response content"));
            }
            other => panic!("expected InvalidResponse, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_timeout_error_propagation() {
        let provider = OpenAiCompatibleProvider::new(
            "http://192.0.2.1:1/v1".to_string(),
            "sk-test-key".to_string(),
            "gpt-4".to_string(),
            Duration::from_millis(1),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        match result {
            Err(LlmError::Timeout(d)) => {
                assert_eq!(d, Duration::from_millis(1));
            }
            Err(LlmError::ProviderError(_)) => {}
            Err(e) => panic!("expected Timeout or ProviderError, got: {:?}", e),
            Ok(_) => panic!("expected error for unreachable endpoint"),
        }
    }
}
