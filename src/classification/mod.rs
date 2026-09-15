pub mod input;
pub mod comparator;
pub mod selector;
pub mod result;
pub mod decision;
pub mod processor;

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
