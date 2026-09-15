use crate::classification::input::ClassificationInput;
use crate::classification::result::{ClassificationDecision, ClassificationResult};
#[cfg(feature = "network")]
use crate::classification::OpenAiProvider;
use crate::classification::MockAiClassifier;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AiClassificationConstraints {
    pub max_confidence_delta: f64,
    pub allowed_decisions: Vec<ClassificationDecision>,
    pub force_hint: Option<String>,
}

impl Default for AiClassificationConstraints {
    fn default() -> Self {
        Self {
            max_confidence_delta: 0.2,
            allowed_decisions: vec![
                ClassificationDecision::MoveExisting,
                ClassificationDecision::CreateCategory,
                ClassificationDecision::LeaveUnclassified,
                ClassificationDecision::AskUser,
            ],
            force_hint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AiClassificationRequest {
    pub target_evidence: crate::evidence::DirectoryEvidence,
    pub candidates: Vec<crate::classification::CandidateEvidence>,
    pub rule_based_result: ClassificationResult,
    pub constraints: AiClassificationConstraints,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Error)]
pub enum AiClassificationError {
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Request timed out")]
    Timeout,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Rate limited")]
    RateLimited,
}

pub trait AiClassifier {
    fn classify(
        &self,
        input: &ClassificationInput,
        baseline: &ClassificationResult,
    ) -> Result<ClassificationResult, AiClassificationError>;
}

pub enum RealAiClassifier {
    Mock(MockAiClassifier),
    #[cfg(feature = "network")]
    OpenAi(OpenAiProvider),
}

#[allow(dead_code)]
pub fn build_request(
    input: &ClassificationInput,
    baseline: &ClassificationResult,
    constraints: Option<AiClassificationConstraints>,
) -> AiClassificationRequest {
    AiClassificationRequest {
        target_evidence: input.target.clone(),
        candidates: input.candidates.clone(),
        rule_based_result: baseline.clone(),
        constraints: constraints.unwrap_or_default(),
    }
}

pub fn validate_ai_result(
    result: &ClassificationResult,
    request: &AiClassificationRequest,
) -> Result<(), AiClassificationError> {
    if !request
        .constraints
        .allowed_decisions
        .contains(&result.decision)
    {
        return Err(AiClassificationError::InvalidResponse(
            "Decision not in allowed_decisions".to_string(),
        ));
    }

    let baseline_conf = request.rule_based_result.confidence;
    let max_delta = request.constraints.max_confidence_delta;
    let min_conf = (baseline_conf - max_delta).max(0.0);
    let max_conf = (baseline_conf + max_delta).min(1.0);

    if result.confidence < min_conf || result.confidence > max_conf {
        return Err(AiClassificationError::InvalidResponse(format!(
            "Confidence {} outside allowed range [{:.2}, {:.2}]",
            result.confidence, min_conf, max_conf
        )));
    }

    let has_candidate = result.selected_candidate.is_some();
    if has_candidate && !matches!(result.decision, crate::classification::ClassificationDecision::MoveExisting) {
        return Err(AiClassificationError::InvalidResponse(
            "selected_candidate set but decision is not MoveExisting".to_string(),
        ));
    }

    let no_candidate = result.selected_candidate.is_none();
    if no_candidate && matches!(result.decision, crate::classification::ClassificationDecision::MoveExisting) {
        return Err(AiClassificationError::InvalidResponse(
            "MoveExisting decision but no selected_candidate".to_string(),
        ));
    }

    if matches!(result.decision, crate::classification::ClassificationDecision::MoveExisting) && !result.warnings.is_empty() {
        return Err(AiClassificationError::InvalidResponse(
            "MoveExisting with warnings is not allowed".to_string(),
        ));
    }

    Ok(())
}
