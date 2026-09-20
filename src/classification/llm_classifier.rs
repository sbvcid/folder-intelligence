use crate::classification::result::{
    AlternativeCandidate, ClassificationDecision, ClassificationResult, ConfidenceBand,
    SupportingEvidence, UncertaintyReason,
};
use crate::llm::provider::{ChatMessage, LlmProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CLASSIFICATION_SCHEMA_VERSION: &str = "4.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileObservation {
    pub relative_path: String,
    pub filename: String,
    pub extension: Option<String>,
    pub size_bytes: Option<u64>,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClassificationRequest {
    pub observations: Vec<FileObservation>,
    pub allowed_categories: Vec<String>,
}

impl LlmClassificationRequest {
    pub fn prompt(&self) -> String {
        let categories_json = serde_json::to_string_pretty(&self.allowed_categories)
            .unwrap_or_else(|_| "[]".to_string());
        let observations_json =
            serde_json::to_string_pretty(&self.observations).unwrap_or_else(|_| "[]".to_string());

        format!(
            "You are a file classification assistant. Your task is to classify files into pre-existing categories.\n\
            \n\
            You will receive filesystem observations (relative paths, filenames, extensions, sizes) and a list of ALLOWED category names.\n\
            \n\
            Return a JSON array where each entry classifies one file:\n\
            [{{\"path\": \"relative/path/file.pdf\", \"category\": \"Documents\", \"confidence\": 0.95}}]\n\
            \n\
            Rules:\n\
            1. You MUST only use category names from the ALLOWED list.\n\
            2. You MUST NOT invent new categories.\n\
            3. You MUST NOT invent files that were not provided in the observations.\n\
            4. You MUST only return classifications for files you are confident about.\n\
            5. If uncertain, OMIT the file from the response (do not classify it).\n\
            6. Confidence must be a number between 0.0 and 1.0 (inclusive).\n\
            7. Paths must be relative and must not escape the analyzed scope.\n\
            8. You do NOT have filesystem execution authority. Your output is only used for classification decisions.\n\
            \n\
            Allowed categories:\n\
            {categories_json}\n\
            \n\
            Observations:\n\
            {observations_json}\n\
            \n\
            Respond ONLY with the JSON array."
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClassificationItem {
    pub path: String,
    pub category: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClassificationOutput {
    #[serde(default)]
    pub classifications: Vec<LlmClassificationItem>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmClassificationError {
    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Malformed JSON response: {0}")]
    MalformedJson(String),

    #[error("Missing 'path' in classification item")]
    MissingPath,

    #[error("Missing 'category' in classification item")]
    MissingCategory,

    #[error("Missing 'confidence' in classification item")]
    MissingConfidence,

    #[error("Invalid confidence value {0}: must be between 0.0 and 1.0")]
    InvalidConfidence(f64),

    #[error("Unknown category '{0}': not in allowed categories")]
    UnknownCategory(String),

    #[error("Absolute path not allowed: {0}")]
    AbsolutePath(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("File not in observations: {0}")]
    UnobservedFile(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

pub struct LlmClassifier {
    provider: Arc<dyn LlmProvider>,
}

impl LlmClassifier {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub fn name(&self) -> &str {
        self.provider.name()
    }

    pub fn classify(
        &self,
        request: &LlmClassificationRequest,
        target_path: &Path,
        allowed_category_paths: &[(String, PathBuf)],
    ) -> Result<ClassificationResult, LlmClassificationError> {
        let prompt = request.prompt();
        let messages = vec![ChatMessage::user(prompt)];

        let response = self
            .provider
            .chat(&messages)
            .map_err(|e| LlmClassificationError::ProviderError(e.to_string()))?;

        let raw_output: Vec<LlmClassificationItem> =
            serde_json::from_str(&response).map_err(|e| {
                LlmClassificationError::MalformedJson(format!(
                    "Failed to parse LLM response as JSON array: {}",
                    e
                ))
            })?;

        let validated = validate_classifications(
            &raw_output,
            &collect_observed_paths(request),
            &collect_allowed_categories(request),
        )?;

        Ok(adapter_to_classification_result(
            &validated,
            target_path,
            allowed_category_paths,
            request.observations.len(),
        ))
    }
}

fn collect_observed_paths(request: &LlmClassificationRequest) -> HashSet<String> {
    request
        .observations
        .iter()
        .map(|obs| obs.relative_path.clone())
        .collect()
}

fn collect_allowed_categories(request: &LlmClassificationRequest) -> HashSet<String> {
    request.allowed_categories.iter().cloned().collect()
}

fn validate_classifications(
    items: &[LlmClassificationItem],
    observed_paths: &HashSet<String>,
    allowed_categories: &HashSet<String>,
) -> Result<Vec<LlmClassificationItem>, LlmClassificationError> {
    let mut result = Vec::new();

    for item in items {
        validate_path(&item.path, observed_paths)?;
        validate_category(&item.category, allowed_categories)?;
        validate_confidence(item.confidence)?;
        result.push(item.clone());
    }

    Ok(result)
}

fn validate_path(
    path: &str,
    observed_paths: &HashSet<String>,
) -> Result<(), LlmClassificationError> {
    let p = Path::new(path);

    for component in p.components() {
        use std::path::Component;
        match component {
            Component::RootDir | Component::Prefix(_) => {
                return Err(LlmClassificationError::AbsolutePath(path.to_string()));
            }
            Component::ParentDir => {
                return Err(LlmClassificationError::PathTraversal(path.to_string()));
            }
            _ => {}
        }
    }

    if !observed_paths.contains(path) {
        return Err(LlmClassificationError::UnobservedFile(path.to_string()));
    }

    Ok(())
}

fn validate_category(
    category: &str,
    allowed_categories: &HashSet<String>,
) -> Result<(), LlmClassificationError> {
    if !allowed_categories.contains(category) {
        return Err(LlmClassificationError::UnknownCategory(
            category.to_string(),
        ));
    }
    Ok(())
}

fn validate_confidence(confidence: f64) -> Result<(), LlmClassificationError> {
    if confidence.is_nan() || confidence.is_infinite() {
        return Err(LlmClassificationError::InvalidConfidence(confidence));
    }
    if confidence < 0.0 || confidence > 1.0 {
        return Err(LlmClassificationError::InvalidConfidence(confidence));
    }
    Ok(())
}

fn adapter_to_classification_result(
    validated: &[LlmClassificationItem],
    target_path: &Path,
    allowed_category_paths: &[(String, PathBuf)],
    observation_count: usize,
) -> ClassificationResult {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut selected_candidate = None;
    let mut proposed_category_name = None;
    let mut supporting_evidence = Vec::new();
    let mut alternatives = Vec::new();
    let mut confidence = 0.0;
    let mut confidence_band = ConfidenceBand::Low;
    let mut decision = ClassificationDecision::LeaveUnclassified;

    let best = validated.iter().max_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(best_item) = best {
        confidence = best_item.confidence;
        confidence_band = ConfidenceBand::from_confidence(confidence);

        let matched_category = allowed_category_paths
            .iter()
            .find(|(name, _)| name == &best_item.category);

        if let Some((_, cat_path)) = matched_category {
            selected_candidate = Some(cat_path.clone());
            proposed_category_name = Some(best_item.category.clone());
            decision = ClassificationDecision::MoveExisting;

            supporting_evidence.push(SupportingEvidence {
                evidence_type: "LLMClassification".to_string(),
                description: format!("LLM classified as '{}'", best_item.category),
                score: confidence,
            });

            for item in validated.iter().filter(|item| item.path != best_item.path) {
                alternatives.push(AlternativeCandidate {
                    candidate_name: item.category.clone(),
                    candidate_path: matched_category.map(|(_, p)| p.clone()).unwrap_or_default(),
                    score: item.confidence,
                    rejection_reason: format!(
                        "Lower confidence ({:.2} vs {:.2})",
                        item.confidence, confidence
                    ),
                });
            }
        }
    }

    let unclassified_count = observation_count.saturating_sub(validated.len());
    let uncertainty: Vec<UncertaintyReason> = if unclassified_count > 0 {
        vec![UncertaintyReason::InsufficientPrecedent]
    } else {
        vec![]
    };

    ClassificationResult {
        target_path: target_path.to_path_buf(),
        decision,
        selected_candidate,
        proposed_category_name,
        confidence,
        confidence_band,
        candidates_considered: observation_count,
        supporting_evidence,
        alternatives,
        uncertainty,
        warnings: vec![],
        classified_at: now,
        schema_version: CLASSIFICATION_SCHEMA_VERSION.to_string(),
        provider: None,
        model: None,
    }
}

pub fn build_classification_request(
    target: &crate::evidence::DirectoryEvidence,
    allowed_categories: &[String],
) -> LlmClassificationRequest {
    let observations: Vec<FileObservation> = target
        .filename_sample
        .iter()
        .map(|filename| {
            let extension = Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());

            let size_bytes = target.path.join(filename).metadata().ok().map(|m| m.len());

            FileObservation {
                relative_path: filename.clone(),
                filename: filename.clone(),
                extension,
                size_bytes,
                file_count: 1,
            }
        })
        .collect();

    LlmClassificationRequest {
        observations,
        allowed_categories: allowed_categories.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmError;

    fn mock_provider(responses: Vec<(&'static str, &'static str)>) -> MockTestProvider {
        MockTestProvider {
            responses: responses
                .into_iter()
                .map(|(_, resp)| resp.to_string())
                .collect(),
        }
    }

    #[derive(Debug)]
    struct MockTestProvider {
        responses: Vec<String>,
    }

    impl LlmProvider for MockTestProvider {
        fn chat(&self, _messages: &[ChatMessage]) -> Result<String, LlmError> {
            Ok(self.responses[0].clone())
        }

        fn name(&self) -> &str {
            "mock-test"
        }
    }

    fn sample_observations() -> Vec<FileObservation> {
        vec![
            FileObservation {
                relative_path: "doc.pdf".to_string(),
                filename: "doc.pdf".to_string(),
                extension: Some("pdf".to_string()),
                size_bytes: Some(1024),
                file_count: 0,
            },
            FileObservation {
                relative_path: "photo.jpg".to_string(),
                filename: "photo.jpg".to_string(),
                extension: Some("jpg".to_string()),
                size_bytes: Some(512),
                file_count: 0,
            },
        ]
    }

    #[test]
    fn test_valid_structured_classification() {
        let json = r#"[{"path":"doc.pdf","category":"Documents","confidence":0.95}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths: Vec<(String, PathBuf)> = vec![
            (
                "Documents".to_string(),
                PathBuf::from("/test/scope/Documents"),
            ),
            ("Images".to_string(), PathBuf::from("/test/scope/Images")),
        ];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());

        let classification = result.unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
        assert_eq!(classification.confidence, 0.95);
        assert!(classification.selected_candidate.is_some());
        assert!(classification.proposed_category_name.is_some());
        assert_eq!(
            classification.proposed_category_name.as_deref(),
            Some("Documents")
        );
    }

    #[test]
    fn test_malformed_json() {
        let json = r#"not valid json at all"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths: Vec<(String, PathBuf)> = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::MalformedJson(_) => {}
            other => panic!("expected MalformedJson, got: {:?}", other),
        }
    }

    #[test]
    fn test_missing_path() {
        let json = r#"[{"category":"Documents","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LlmClassificationError::MalformedJson(_)
        ));
    }

    #[test]
    fn test_missing_category() {
        let json = r#"[{"path":"doc.pdf","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LlmClassificationError::MalformedJson(_)
        ));
    }

    #[test]
    fn test_invalid_confidence() {
        let json = r#"[{"path":"doc.pdf","category":"Documents","confidence":1.5}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::InvalidConfidence(c) => {
                assert_eq!(c, 1.5);
            }
            other => panic!("expected InvalidConfidence, got: {:?}", other),
        }
    }

    #[test]
    fn test_unknown_category() {
        let json = r#"[{"path":"doc.pdf","category":"InventedCategory","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![
            (
                "Documents".to_string(),
                PathBuf::from("/test/scope/Documents"),
            ),
            ("Images".to_string(), PathBuf::from("/test/scope/Images")),
        ];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::UnknownCategory(cat) => {
                assert_eq!(cat, "InventedCategory");
            }
            other => panic!("expected UnknownCategory, got: {:?}", other),
        }
    }

    #[test]
    fn test_absolute_path_rejection() {
        let json = r#"[{"path":"/etc/passwd","category":"Documents","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::AbsolutePath(path) => {
                assert_eq!(path, "/etc/passwd");
            }
            other => panic!("expected AbsolutePath, got: {:?}", other),
        }
    }

    #[test]
    fn test_path_traversal_rejection() {
        let json = r#"[{"path":"../../etc/passwd","category":"Documents","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::PathTraversal(path) => {
                assert_eq!(path, "../../etc/passwd");
            }
            other => panic!("expected PathTraversal, got: {:?}", other),
        }
    }

    #[test]
    fn test_unobserved_file_rejection() {
        let json = r#"[{"path":"nonexistent.pdf","category":"Documents","confidence":0.9}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmClassificationError::UnobservedFile(path) => {
                assert_eq!(path, "nonexistent.pdf");
            }
            other => panic!("expected UnobservedFile, got: {:?}", other),
        }
    }

    #[test]
    fn test_incomplete_llm_response() {
        let json = r#"[]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths: Vec<(String, PathBuf)> = vec![
            (
                "Documents".to_string(),
                PathBuf::from("/test/scope/Documents"),
            ),
            ("Images".to_string(), PathBuf::from("/test/scope/Images")),
        ];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(
            result.is_ok(),
            "empty response should be accepted: {:?}",
            result.err()
        );

        let classification = result.unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::LeaveUnclassified
        );
        assert!(classification
            .uncertainty
            .contains(&UncertaintyReason::InsufficientPrecedent));
    }

    #[test]
    fn test_uncertain_classification_preserved_as_unclassified() {
        let json = r#"[{"path":"doc.pdf","category":"Documents","confidence":0.3}]"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths: Vec<(String, PathBuf)> = vec![
            (
                "Documents".to_string(),
                PathBuf::from("/test/scope/Documents"),
            ),
            ("Images".to_string(), PathBuf::from("/test/scope/Images")),
        ];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok());

        let classification = result.unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
        assert_eq!(classification.confidence, 0.3);
    }

    #[test]
    fn test_provider_independent_classification() {
        let json = r#"[{"path":"doc.pdf","category":"Documents","confidence":0.95}]"#;

        let ollama_provider = mock_provider(vec![("chat", json)]);
        let classifier_ollama = LlmClassifier::new(Arc::new(ollama_provider));
        assert_eq!(classifier_ollama.name(), "mock-test");

        let openai_provider = mock_provider(vec![("chat", json)]);
        let classifier_openai = LlmClassifier::new(Arc::new(openai_provider));
        assert_eq!(classifier_openai.name(), "mock-test");

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result_o = classifier_ollama.classify(&request, &target_path, &allowed_paths);
        let result_p = classifier_openai.classify(&request, &target_path, &allowed_paths);

        assert!(result_o.is_ok());
        assert!(result_p.is_ok());
        assert_eq!(result_o.as_ref().unwrap().confidence, 0.95);
        assert_eq!(result_p.as_ref().unwrap().confidence, 0.95);
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_ollama_provider_integration() {
        use crate::llm::OllamaProvider;
        use std::time::Duration;

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
                        "content": "[{\"path\":\"doc.pdf\",\"category\":\"Documents\",\"confidence\":0.9}]"
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

        let classifier = LlmClassifier::new(Arc::new(provider));
        assert_eq!(classifier.name(), "ollama");

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap().confidence, 0.9);

        mock.assert();
    }

    #[test]
    #[cfg(feature = "network")]
    fn test_openai_compatible_provider_integration() {
        use crate::llm::OpenAiCompatibleProvider;
        use std::time::Duration;

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
                            "content": "[{\"path\":\"doc.pdf\",\"category\":\"Documents\",\"confidence\":0.95}]"
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

        let classifier = LlmClassifier::new(Arc::new(provider));
        assert_eq!(classifier.name(), "openai-compatible");

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap().confidence, 0.95);

        mock.assert();
    }
}
