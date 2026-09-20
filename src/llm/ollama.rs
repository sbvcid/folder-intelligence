use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaChatResponse {
    #[allow(dead_code)]
    model: String,
    message: OllamaChatMessageResponse,
    #[allow(dead_code)]
    done: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaChatMessageResponse {
    #[allow(dead_code)]
    role: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaErrorResponse {
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    endpoint: String,
    model: String,
    timeout: Duration,
}

impl OllamaProvider {
    pub fn new(endpoint: String, model: String, timeout: Duration) -> Self {
        Self {
            endpoint,
            model,
            timeout,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl LlmProvider for OllamaProvider {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String, LlmError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| LlmError::ProviderError(format!("HTTP client error: {}", e)))?;

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|m| OllamaChatMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            stream: false,
        };

        let response = client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(self.timeout)
                } else if e.is_connect() {
                    LlmError::ProviderError(format!(
                        "Failed to connect to Ollama at '{}': {}",
                        self.endpoint, e
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
            if let Ok(error_resp) = serde_json::from_str::<OllamaErrorResponse>(&body) {
                if let Some(error_msg) = error_resp.error {
                    return Err(LlmError::HttpError(status.as_u16(), error_msg));
                }
            }
            return Err(LlmError::HttpError(
                status.as_u16(),
                format!("Ollama returned HTTP {}: {}", status, body),
            ));
        }

        let chat_response: OllamaChatResponse = serde_json::from_str(&body)
            .map_err(|e| LlmError::InvalidResponse(format!("Response parse error: {}", e)))?;

        if chat_response.message.content.is_empty() {
            return Err(LlmError::InvalidResponse(
                "Ollama returned empty response message".to_string(),
            ));
        }

        Ok(chat_response.message.content)
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaChatRequest {
            model: "llama2".to_string(),
            messages: vec![
                OllamaChatMessage {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                },
                OllamaChatMessage {
                    role: "user".to_string(),
                    content: "Hello!".to_string(),
                },
            ],
            stream: false,
        };

        let json = serde_json::to_string(&request).expect("should serialize");

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse as JSON");

        assert_eq!(
            parsed["model"].as_str(),
            Some("llama2"),
            "model should be in request"
        );
        assert_eq!(
            parsed["messages"][0]["role"].as_str(),
            Some("system"),
            "first message should be system"
        );
        assert_eq!(
            parsed["messages"][1]["content"].as_str(),
            Some("Hello!"),
            "second message content should match"
        );
        assert_eq!(
            parsed["stream"].as_bool(),
            Some(false),
            "stream should be false"
        );
        assert!(
            !parsed.as_object().unwrap().contains_key("stream")
                || parsed["stream"].as_bool() == Some(false),
            "stream must be false (no SSE)"
        );
    }

    #[test]
    fn test_parse_ollama_success_response() {
        let response_json = r#"{
            "model": "llama2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello there! How can I help you today?"
            },
            "done": true
        }"#;

        let response: OllamaChatResponse =
            serde_json::from_str(response_json).expect("should parse success response");

        assert_eq!(response.model, "llama2");
        assert_eq!(response.message.role, "assistant");
        assert_eq!(
            response.message.content,
            "Hello there! How can I help you today?"
        );
    }

    #[test]
    fn test_parse_ollama_error_response() {
        let error_json = r#"{"error": "model 'nonexistent' not found"}"#;

        let error: OllamaErrorResponse =
            serde_json::from_str(error_json).expect("should parse error response");

        assert_eq!(
            error.error,
            Some("model 'nonexistent' not found".to_string())
        );
    }

    #[test]
    fn test_parse_malformed_ollama_response() {
        let malformed_json = r#"{"model": "llama2", "message": invalid}"#;

        let result: Result<OllamaChatResponse, _> = serde_json::from_str(malformed_json);
        assert!(result.is_err(), "malformed JSON should fail to parse");
    }

    #[test]
    fn test_ollama_chat_response_missing_message_field() {
        let incomplete_json = r#"{"model": "llama2"}"#;

        let result: Result<OllamaChatResponse, _> = serde_json::from_str(incomplete_json);
        assert!(
            result.is_err(),
            "missing message field should fail to parse"
        );
    }

    #[test]
    fn test_ollama_error_response_empty_error() {
        let error_json = r#"{"error": null}"#;

        let error: OllamaErrorResponse =
            serde_json::from_str(error_json).expect("should parse with null error");

        assert!(error.error.is_none());
    }

    #[test]
    fn test_ollama_provider_construct_and_accessors() {
        let provider = OllamaProvider::new(
            "http://localhost:11434/api/chat".to_string(),
            "llama2".to_string(),
            Duration::from_secs(30),
        );

        assert_eq!(provider.endpoint(), "http://localhost:11434/api/chat");
        assert_eq!(provider.model(), "llama2");
        assert_eq!(provider.timeout(), Duration::from_secs(30));
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_provider_name() {
        let provider = OllamaProvider::new(
            "http://localhost:11434/api/chat".to_string(),
            "llama2".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_successful_chat_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "model": "llama2",
                    "created_at": "2024-01-01T00:00:00Z",
                    "message": {
                        "role": "assistant",
                        "content": "Hello from Ollama!"
                    },
                    "done": true
                }"#,
            )
            .create();

        let provider = OllamaProvider::new(
            server.url() + "/api/chat",
            "llama2".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap(), "Hello from Ollama!");

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_http_error_model_not_found() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "model 'nonexistent' not found"}"#)
            .create();

        let provider = OllamaProvider::new(
            server.url() + "/api/chat",
            "nonexistent".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::HttpError(code, msg) => {
                assert_eq!(code, 404);
                assert_eq!(msg, "model 'nonexistent' not found");
            }
            other => panic!("expected HttpError, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_connection_failure() {
        let provider = OllamaProvider::new(
            "http://127.0.0.1:1/api/chat".to_string(),
            "llama2".to_string(),
            Duration::from_secs(5),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ProviderError(msg) => {
                assert!(
                    msg.contains("Failed to connect to Ollama"),
                    "error should mention Ollama connection: {}",
                    msg
                );
            }
            other => panic!("expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_malformed_response() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"not valid json at all"#)
            .create();

        let provider = OllamaProvider::new(
            server.url() + "/api/chat",
            "llama2".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::InvalidResponse(msg) => {
                assert!(
                    msg.contains("Response parse error"),
                    "error should mention response parse error: {}",
                    msg
                );
            }
            other => panic!("expected InvalidResponse, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_empty_response_content() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "model": "llama2",
                    "message": {
                        "role": "assistant",
                        "content": ""
                    },
                    "done": true
                }"#,
            )
            .create();

        let provider = OllamaProvider::new(
            server.url() + "/api/chat",
            "llama2".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::InvalidResponse(msg) => {
                assert!(
                    msg.contains("empty response"),
                    "error should mention empty response: {}",
                    msg
                );
            }
            other => panic!("expected InvalidResponse, got: {:?}", other),
        }

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_internal_server_error() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/chat")
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "internal server error"}"#)
            .create();

        let provider = OllamaProvider::new(
            server.url() + "/api/chat",
            "llama2".to_string(),
            Duration::from_secs(10),
        );

        let messages = vec![ChatMessage::user("Hello")];
        let result = provider.chat(&messages);

        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::HttpError(code, _) => {
                assert_eq!(code, 500);
            }
            other => panic!("expected HttpError, got: {:?}", other),
        }

        mock.assert();
    }
}
