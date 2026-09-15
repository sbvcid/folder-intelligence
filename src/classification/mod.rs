pub mod input;
pub mod comparator;
pub mod selector;
pub mod result;
pub mod decision;
pub mod processor;
pub mod ai_provider;
pub mod mock_ai;

#[cfg(feature = "network")]
pub mod openai_provider;

#[allow(unused_imports)]
pub use input::{
    TargetEvidence, CandidateEvidence, CandidateSummary, ClassificationConfig, UserHints,
    ClassificationInput, SAMPLE_CHILD_DIRS, MAX_CANDIDATES,
};
#[allow(unused_imports)]
pub use comparator::{EvidenceComparator, EvidenceComparison, ComparisonType};
#[allow(unused_imports)]
pub use selector::{CandidateSelector, CandidateScore};
#[allow(unused_imports)]
pub use result::{
    ClassificationResult, ClassificationDecision, ConfidenceBand, SupportingEvidence,
    AlternativeCandidate, UncertaintyReason, Warning,
};
#[allow(unused_imports)]
pub use decision::DecisionEngine;
#[allow(unused_imports)]
pub use processor::{ClassificationProcessor, RuleBasedProcessor};
#[allow(unused_imports)]
pub use ai_provider::{
    AiClassifier, AiClassificationConstraints, AiClassificationError, AiClassificationRequest,
    build_request, validate_ai_result, RealAiClassifier,
};
#[allow(unused_imports)]
pub use mock_ai::MockAiClassifier;

#[cfg(feature = "network")]
#[allow(unused_imports)]
pub use openai_provider::{OpenAiProvider, OpenAiProviderConfig};
