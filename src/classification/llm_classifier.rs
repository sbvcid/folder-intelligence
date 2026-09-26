use crate::agent::recommendation::{
    OrganizationProposal, ProposedCategory, RecommendationStrategy,
};
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
    pub instruction: Option<String>,
}

impl LlmClassificationRequest {
    pub fn prompt(&self) -> String {
        let categories_json = serde_json::to_string_pretty(&self.allowed_categories)
            .unwrap_or_else(|_| "[]".to_string());
        let observations_json =
            serde_json::to_string_pretty(&self.observations).unwrap_or_else(|_| "[]".to_string());

        let instruction_section = if let Some(instruction) = &self.instruction {
            format!("\n            User Instruction:\n            {instruction}\n\n")
        } else {
            String::new()
        };

        format!(
            "You are a file classification assistant. Your task is to classify files into pre-existing categories.\n\
            \n\
            You will receive filesystem observations (relative paths, filenames, extensions, sizes) and a list of ALLOWED category names.\n\
            \n\
            Return a JSON object with:\n\
            1. organization_strategy: An entity with type (one of: by_author, by_title, by_type, by_year, preserve_existing, category_based, unknown), confidence (0.0-1.0), and optional reason.\n\
            2. organization_proposal: An entity with rationale (string), categories (array of objects with name, purpose, files array of relative file paths from observations, and confidence), evidence_gaps (array of strings), and ambiguities (array of strings).\n\
            3. classifications: A JSON array where each entry classifies one file:\n\
            [{{\"path\": \"relative/path/file.pdf\", \"category\": \"Documents\", \"confidence\": 0.95, \"reason\": \"Filename indicates an invoice document.\"}}]\n\
            \n\
            Rules:\n\
            1. You MUST only use category names from the ALLOWED list for file classifications.\n\
            2. You MUST NOT invent files that were not provided in the observations.\n\
            3. Confidence must be a number between 0.0 and 1.0 (inclusive).\n\
            4. Paths must be relative and must not escape the analyzed scope.\n\
            5. You do NOT have filesystem execution authority. Your output is only used for classification and proposal recommendations.\n\
            6. If the user explicitly specifies an organization method (e.g. by author, by title), preserve that intent in organization_strategy and organization_proposal.\n\
            7. Do NOT invent metadata (e.g. author names, years) that is not available in the filesystem evidence. If evidence is missing, do not hallucinate categories; instead, report the limitation in evidence_gaps.\n\
            8. Proposal is a recommendation, not a filesystem operation. Do not output move/delete operations.\n\
            9. If multiple reasonable groupings exist, record them in ambiguities.\n\
            10. If data is insufficient for the requested proposal, leave categories empty and add explanation to evidence_gaps.\n\
            {instruction_section}\n\
            {categories_json}\n\
            \n\
            Observations:\n\
            {observations_json}\n\
            \n\
            Respond ONLY with the JSON object."
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClassificationItem {
    pub path: String,
    pub category: String,
    pub confidence: f64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmOrganizationStrategy {
    ByAuthor,
    ByTitle,
    ByType,
    ByYear,
    PreserveExisting,
    CategoryBased,
    Unknown,
}

impl Default for LlmOrganizationStrategy {
    fn default() -> Self {
        LlmOrganizationStrategy::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmStrategyInfo {
    #[serde(default, rename = "type")]
    pub strategy: LlmOrganizationStrategy,
    #[serde(default)]
    pub confidence: f64,
    pub reason: Option<String>,
}

impl Default for LlmStrategyInfo {
    fn default() -> Self {
        LlmStrategyInfo {
            strategy: LlmOrganizationStrategy::Unknown,
            confidence: 0.0,
            reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProposedCategory {
    pub name: String,
    pub purpose: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmOrganizationProposal {
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub categories: Vec<LlmProposedCategory>,
    #[serde(default)]
    pub evidence_gaps: Vec<String>,
    #[serde(default)]
    pub ambiguities: Vec<String>,
}

pub fn map_llm_proposal(
    llm_prop: &LlmOrganizationProposal,
    strategy: RecommendationStrategy,
    target_path: &Path,
) -> OrganizationProposal {
    let proposed_categories: Vec<ProposedCategory> = llm_prop
        .categories
        .iter()
        .map(|cat| ProposedCategory {
            name: cat.name.clone(),
            purpose: cat.purpose.clone(),
            target_content_types: Vec::new(),
            confidence: cat.confidence,
            is_existing: false,
            target_path: Some(target_path.join(&cat.name)),
            source_files: cat.files.iter().map(|f| target_path.join(f)).collect(),
        })
        .collect();

    OrganizationProposal {
        strategy,
        rationale: llm_prop.rationale.clone(),
        proposed_categories,
        evidence_gaps: llm_prop.evidence_gaps.clone(),
        ambiguities: llm_prop.ambiguities.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmClassificationOutput {
    #[serde(default)]
    pub organization_strategy: Option<LlmStrategyInfo>,
    #[serde(default)]
    pub organization_proposal: Option<LlmOrganizationProposal>,
    #[serde(default)]
    pub classifications: Vec<LlmClassificationItem>,
}

impl LlmClassificationOutput {
    pub fn from_classifications(items: Vec<LlmClassificationItem>) -> Self {
        LlmClassificationOutput {
            organization_strategy: None,
            organization_proposal: None,
            classifications: items,
        }
    }
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

#[derive(Debug, Clone)]
pub struct ClassificationWithStrategy {
    pub classifications: Vec<ClassificationResult>,
    pub strategy: Option<LlmStrategyInfo>,
    pub proposal: Option<OrganizationProposal>,
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
    ) -> Result<ClassificationWithStrategy, LlmClassificationError> {
        let prompt = request.prompt();
        let messages = vec![ChatMessage::user(prompt)];

        let response = self
            .provider
            .chat(&messages)
            .map_err(|e| LlmClassificationError::ProviderError(e.to_string()))?;

        let output: LlmClassificationOutput = serde_json::from_str(&response).map_err(|e| {
            LlmClassificationError::MalformedJson(format!(
                "Failed to parse LLM response as JSON object: {}",
                e
            ))
        })?;

        let validated = validate_classifications(
            &output.classifications,
            &collect_observed_paths(request),
            &collect_allowed_categories(request),
        )?;

        let recommendation_strategy = output
            .organization_strategy
            .as_ref()
            .map(|s| crate::agent::recommendation::map_llm_strategy(&s.strategy))
            .unwrap_or(RecommendationStrategy::CategoryBased);

        let proposal = output
            .organization_proposal
            .as_ref()
            .map(|prop| map_llm_proposal(prop, recommendation_strategy, target_path));

        // Convert each validated item to its own ClassificationResult
        let classifications = if validated.is_empty() {
            // If no items were classified, produce a single LeaveUnclassified result
            vec![item_to_classification_result(
                &LlmClassificationItem {
                    path: String::new(),
                    category: String::new(),
                    confidence: 0.0,
                    reason: None,
                },
                target_path,
                allowed_category_paths,
                request.observations.len(),
            )]
        } else {
            validated
                .iter()
                .map(|item| {
                    item_to_classification_result(
                        item,
                        target_path,
                        allowed_category_paths,
                        request.observations.len(),
                    )
                })
                .collect()
        };

        Ok(ClassificationWithStrategy {
            classifications,
            strategy: output.organization_strategy,
            proposal,
        })
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
        if !allowed_categories.is_empty() {
            validate_category(&item.category, allowed_categories)?;
        }
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
    // This function is kept for backward compatibility but is deprecated.
    // It aggregates all items into a single result (legacy behavior).
    // Use item_to_classification_result for per-file results instead.
    let best = validated.iter().max_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(best_item) = best {
        item_to_classification_result(
            best_item,
            target_path,
            allowed_category_paths,
            observation_count,
        )
    } else {
        // No items - return empty result
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        ClassificationResult {
            target_path: target_path.to_path_buf(),
            decision: ClassificationDecision::LeaveUnclassified,
            selected_candidate: None,
            proposed_category_name: None,
            confidence: 0.0,
            confidence_band: ConfidenceBand::Low,
            candidates_considered: observation_count,
            supporting_evidence: Vec::new(),
            alternatives: Vec::new(),
            uncertainty: vec![UncertaintyReason::InsufficientPrecedent],
            classification_reason: None,
            warnings: vec![],
            classified_at: now,
            schema_version: CLASSIFICATION_SCHEMA_VERSION.to_string(),
            provider: None,
            model: None,
        }
    }
}

// New function: Convert a single LlmClassificationItem to its own ClassificationResult
fn item_to_classification_result(
    item: &LlmClassificationItem,
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
    let mut confidence = item.confidence;
    let mut confidence_band = ConfidenceBand::from_confidence(confidence);
    let mut decision = ClassificationDecision::LeaveUnclassified;
    let mut classification_reason = item.reason.clone();

    let matched_category = allowed_category_paths
        .iter()
        .find(|(name, _)| name == &item.category);

    if let Some((_, cat_path)) = matched_category {
        selected_candidate = Some(cat_path.clone());
        proposed_category_name = Some(item.category.clone());
        decision = ClassificationDecision::MoveExisting;

        supporting_evidence.push(SupportingEvidence {
            evidence_type: "LLMClassification".to_string(),
            description: format!("LLM classified '{}' as '{}'", item.path, item.category),
            score: confidence,
        });
    }

    // Note: We don't add alternatives for individual items since each item
    // represents a single file's classification. Alternatives are not per-file concepts.

    // Uncertainty is per-file based on whether this file was classified
    let uncertainty: Vec<UncertaintyReason> = if item.category.is_empty() {
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
        candidates_considered: 1, // Per-file, so only 1 candidate considered
        supporting_evidence,
        alternatives,
        uncertainty,
        classification_reason,
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
    instruction: Option<&str>,
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
        instruction: instruction.map(|s| s.to_string()),
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
        let json = r#"{"organization_strategy":{"type":"by_type","confidence":0.9,"reason":"Standard classification"},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95,"reason":"Filename indicates a document file."}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: None,
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

        let classification = &result.unwrap().classifications[0];
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
        assert_eq!(
            classification.classification_reason,
            Some("Filename indicates a document file.".to_string())
        );
    }

    #[test]
    fn test_valid_structured_classification_with_strategy() {
        let json = r#"{"organization_strategy":{"type":"by_author","confidence":0.95,"reason":"User requested author-based organization."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95,"reason":"Filename indicates a document file."}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: Some("按照作者整理".to_string()),
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

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByAuthor);
        assert_eq!(strategy.confidence, 0.95);
        assert_eq!(
            strategy.reason.as_deref(),
            Some("User requested author-based organization.")
        );

        let classification = &with_strategy.classifications[0];
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
    }

    #[test]
    fn test_user_intent_by_title() {
        let json = r#"{"organization_strategy":{"type":"by_title","confidence":0.9,"reason":"User requested title-based organization."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: Some("按照漫畫名稱整理".to_string()),
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
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByTitle);
    }

    #[test]
    fn test_user_intent_by_type() {
        let json = r#"{"organization_strategy":{"type":"by_type","confidence":0.9,"reason":"User requested type-based organization."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: Some("按照檔案類型整理".to_string()),
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
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByType);
    }

    #[test]
    fn test_user_intent_preserve_existing() {
        let json = r#"{"organization_strategy":{"type":"preserve_existing","confidence":0.8,"reason":"User requested preserving existing structure."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: Some("不要重新整理現有資料夾，只整理散落的檔案".to_string()),
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::PreserveExisting);
    }

    #[test]
    fn test_no_instruction_returns_no_strategy() {
        let json =
            r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        assert!(with_strategy.strategy.is_none());
    }

    #[test]
    fn test_missing_evidence_does_not_hallucinate_author() {
        let json = r#"{"organization_strategy":{"type":"by_author","confidence":0.7,"reason":"User requested author-based organization. Evidence gap: Author metadata could not be reliably determined from the available filesystem evidence."},"classifications":[]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: Some("按照作者整理".to_string()),
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByAuthor);
        assert!(strategy.reason.is_some());
        assert!(strategy.reason.unwrap().contains("Evidence gap"));
    }

    #[test]
    fn test_explicit_intent_not_downgraded_to_category() {
        let json = r#"{"organization_strategy":{"type":"by_author","confidence":0.95,"reason":"User explicitly requested author-based organization."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: Some("按照作者整理".to_string()),
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
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByAuthor);
        assert_ne!(strategy.strategy, LlmOrganizationStrategy::ByType);
    }

    #[test]
    fn test_year_intent_produces_by_year() {
        let json = r#"{"organization_strategy":{"type":"by_year","confidence":0.85,"reason":"User requested year-based organization."},"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: Some("按照年份整理".to_string()),
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok());

        let with_strategy = result.unwrap();
        let strategy = with_strategy.strategy.expect("should have strategy");
        assert_eq!(strategy.strategy, LlmOrganizationStrategy::ByYear);
    }

    #[test]
    fn test_malformed_json() {
        let json = r#"not valid json at all"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"path":"doc.pdf","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json =
            r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":1.5}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"path":"doc.pdf","category":"InventedCategory","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"path":"/etc/passwd","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"path":"../../etc/passwd","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[{"path":"nonexistent.pdf","category":"Documents","confidence":0.9}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        let json = r#"{"classifications":[]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: None,
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

        let classification = &result.unwrap().classifications[0];
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
        let json = r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.3,"reason":"Low confidence in classification."}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string(), "Images".to_string()],
            instruction: None,
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

        let classification = &result.unwrap().classifications[0];
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
        assert_eq!(classification.confidence, 0.3);
        assert_eq!(
            classification.classification_reason,
            Some("Low confidence in classification.".to_string())
        );
    }

    #[test]
    fn test_provider_independent_classification() {
        let json =
            r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95}]}"#;

        let ollama_provider = mock_provider(vec![("chat", json)]);
        let classifier_ollama = LlmClassifier::new(Arc::new(ollama_provider));
        assert_eq!(classifier_ollama.name(), "mock-test");

        let openai_provider = mock_provider(vec![("chat", json)]);
        let classifier_openai = LlmClassifier::new(Arc::new(openai_provider));
        assert_eq!(classifier_openai.name(), "mock-test");

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
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
        assert_eq!(result_o.as_ref().unwrap().classifications[0].confidence, 0.95);
        assert_eq!(result_p.as_ref().unwrap().classifications[0].confidence, 0.95);
    }

    #[test]
    fn test_reason_field_optional_in_llm_response() {
        let json =
            r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95}]}"#;
        let provider = mock_provider(vec![("chat", json)]);
        let classifier = LlmClassifier::new(Arc::new(provider));

        let request = LlmClassificationRequest {
            observations: sample_observations(),
            allowed_categories: vec!["Documents".to_string()],
            instruction: None,
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());

        let classification = &result.unwrap().classifications[0];
        assert_eq!(classification.classification_reason, None);
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
                        "content": "{\"classifications\":[{\"path\":\"doc.pdf\",\"category\":\"Documents\",\"confidence\":0.9}]}"
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
            instruction: None,
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap().classifications[0].confidence, 0.9);

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
                            "content": "{\"classifications\":[{\"path\":\"doc.pdf\",\"category\":\"Documents\",\"confidence\":0.95}]}"
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
            instruction: None,
        };

        let target_path = PathBuf::from("/test/scope");
        let allowed_paths = vec![(
            "Documents".to_string(),
            PathBuf::from("/test/scope/Documents"),
        )];

        let result = classifier.classify(&request, &target_path, &allowed_paths);
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert_eq!(result.unwrap().classifications[0].confidence, 0.95);

        mock.assert();
    }

    #[test]
    fn test_phase17c2_parse_valid_proposal() {
        let json = r#"{
            "organization_strategy": {
                "type": "by_author",
                "confidence": 0.95,
                "reason": "By author requested"
            },
            "organization_proposal": {
                "rationale": "Group by author",
                "categories": [
                    {"name": "Author A", "purpose": "Works by Author A", "files": ["file1.txt"], "confidence": 0.9},
                    {"name": "Author B", "purpose": "Works by Author B", "files": ["file2.txt"], "confidence": 0.95}
                ],
                "evidence_gaps": [],
                "ambiguities": []
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        assert!(output.organization_proposal.is_some());
        let prop = output.organization_proposal.unwrap();
        assert_eq!(prop.categories.len(), 2);
        assert_eq!(prop.categories[0].name, "Author A");
    }

    #[test]
    fn test_phase17c2_parse_empty_categories() {
        let json = r#"{
            "organization_proposal": {
                "rationale": "No categories",
                "categories": [],
                "evidence_gaps": ["Missing author info"],
                "ambiguities": []
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        assert!(output.organization_proposal.is_some());
        assert!(output.organization_proposal.unwrap().categories.is_empty());
    }

    #[test]
    fn test_phase17c2_parse_evidence_gaps() {
        let json = r#"{
            "organization_proposal": {
                "rationale": "Gaps found",
                "categories": [],
                "evidence_gaps": ["Gap 1", "Gap 2"],
                "ambiguities": []
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        let prop = output.organization_proposal.unwrap();
        assert_eq!(prop.evidence_gaps.len(), 2);
        assert_eq!(prop.evidence_gaps[0], "Gap 1");
    }

    #[test]
    fn test_phase17c2_parse_ambiguities() {
        let json = r#"{
            "organization_proposal": {
                "rationale": "Ambiguities found",
                "categories": [],
                "evidence_gaps": [],
                "ambiguities": ["Ambiguity 1"]
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        let prop = output.organization_proposal.unwrap();
        assert_eq!(prop.ambiguities.len(), 1);
        assert_eq!(prop.ambiguities[0], "Ambiguity 1");
    }

    #[test]
    fn test_phase17c2_old_json_backward_compatibility() {
        let json = r#"{
            "organization_strategy": {
                "type": "category_based",
                "confidence": 0.9
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        assert!(output.organization_proposal.is_none());
    }

    #[test]
    fn test_phase17c2_anti_hallucination_contract() {
        let json = r#"{
            "organization_proposal": {
                "rationale": "Anti hallucination test",
                "categories": [
                    {"name": "Hallucinated Author", "purpose": "Test", "files": ["unknown.txt"], "confidence": 0.9}
                ],
                "evidence_gaps": ["Author information is unavailable."],
                "ambiguities": []
            },
            "classifications": []
        }"#;
        let output: LlmClassificationOutput = serde_json::from_str(json).unwrap();
        let prop = output.organization_proposal.unwrap();
        assert!(!prop.evidence_gaps.is_empty());
        assert_eq!(prop.evidence_gaps[0], "Author information is unavailable.");
    }
}
