use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationDecision {
    MoveExisting,
    CreateCategory,
    LeaveUnclassified,
    AskUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    High,
    Medium,
    Low,
}

impl ConfidenceBand {
    pub fn from_confidence(confidence: f64) -> Self {
        if confidence >= 0.8 {
            ConfidenceBand::High
        } else if confidence >= 0.4 {
            ConfidenceBand::Medium
        } else {
            ConfidenceBand::Low
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SupportingEvidence {
    pub evidence_type: String,
    pub description: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AlternativeCandidate {
    pub candidate_name: String,
    pub candidate_path: std::path::PathBuf,
    pub score: f64,
    pub rejection_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    LowConfidence,
    AmbiguousCandidates,
    InsufficientPrecedent,
    ConflictingEvidence,
    UserHintConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    PartialScanTarget,
    PartialScanCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ClassificationResult {
    pub target_path: std::path::PathBuf,
    pub decision: ClassificationDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_candidate: Option<std::path::PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_category_name: Option<String>,
    pub confidence: f64,
    pub confidence_band: ConfidenceBand,
    pub candidates_considered: usize,
    pub supporting_evidence: Vec<SupportingEvidence>,
    pub alternatives: Vec<AlternativeCandidate>,
    pub uncertainty: Vec<UncertaintyReason>,
    pub warnings: Vec<Warning>,
    pub classified_at: u64,
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
