pub mod ai_provider;
pub mod comparator;
pub mod decision;
pub mod input;
pub mod mock_ai;
pub mod processor;
pub mod result;
pub mod selector;

#[cfg(feature = "network")]
pub mod openai_provider;

#[allow(unused_imports)]
pub use ai_provider::{
    build_request, validate_ai_result, AiClassificationConstraints, AiClassificationError,
    AiClassificationRequest, AiClassifier, RealAiClassifier,
};
#[allow(unused_imports)]
pub use comparator::{ComparisonType, EvidenceComparator, EvidenceComparison};
#[allow(unused_imports)]
pub use decision::DecisionEngine;
#[allow(unused_imports)]
pub use input::{
    CandidateEvidence, CandidateSummary, ClassificationConfig, ClassificationInput, TargetEvidence,
    UserHints, MAX_CANDIDATES, SAMPLE_CHILD_DIRS,
};
#[allow(unused_imports)]
pub use mock_ai::MockAiClassifier;
#[allow(unused_imports)]
pub use processor::{ClassificationProcessor, RuleBasedProcessor};
#[allow(unused_imports)]
pub use result::{
    AlternativeCandidate, ClassificationDecision, ClassificationResult, ConfidenceBand,
    SupportingEvidence, UncertaintyReason, Warning,
};
#[allow(unused_imports)]
pub use selector::{CandidateScore, CandidateSelector};

#[cfg(feature = "network")]
#[allow(unused_imports)]
pub use openai_provider::{OpenAiProvider, OpenAiProviderConfig};
