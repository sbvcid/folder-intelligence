use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct MockLlmProvider {
    responses: Arc<Mutex<Vec<String>>>,
    call_count: Arc<AtomicUsize>,
}

impl Clone for MockLlmProvider {
    fn clone(&self) -> Self {
        Self {
            responses: Arc::clone(&self.responses),
            call_count: Arc::clone(&self.call_count),
        }
    }
}

impl MockLlmProvider {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn default_organize_response() -> String {
        r#"{
  "goal": "organize",
  "scope": null,
  "purpose": "general_organization",
  "strategy": null,
  "clean_rules": null,
  "constraints": {
    "preserve_existing_folders": true,
    "merge_duplicates": true,
    "auto_delete_temps": false,
    "max_interactive_questions": 3
  },
  "user_hints": {},
  "unknown_factors": []
}"#
        .to_string()
    }

    pub fn with_default_organize() -> Self {
        Self::new(vec![Self::default_organize_response()])
    }

    #[allow(dead_code)]
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::Relaxed);
        let mut responses = self.responses.lock().unwrap();
        responses.clear();
        responses.push(Self::default_organize_response());
    }
}

impl LlmProvider for MockLlmProvider {
    fn chat(&self, _messages: &[ChatMessage]) -> Result<String, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(LlmError::InvalidResponse(
                "Mock provider has no more responses queued".to_string(),
            ));
        }
        Ok(responses.remove(0))
    }

    fn name(&self) -> &str {
        "mock"
    }
}
