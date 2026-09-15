pub mod cli;
pub mod evidence;
pub mod scanner;
pub mod classification;

pub use evidence::{
    DirectoryEvidence, DominantExtension, ErrorCategory, IdentifierSummary, IdentifierType,
    ScanError, ScanLimits, ScanMetadata, ScanResult, ScanStats, SyntacticIdentifier,
    TextFilePresence, SCHEMA_VERSION,
};
pub use scanner::Scanner;
pub use classification::{
    TargetEvidence, CandidateEvidence, CandidateSummary, ClassificationConfig, UserHints,
    ClassificationInput, EvidenceComparator, EvidenceComparison, ComparisonType,
    CandidateSelector, CandidateScore, DecisionEngine, ClassificationProcessor,
    ClassificationResult, ClassificationDecision, ConfidenceBand, SupportingEvidence,
    AlternativeCandidate, UncertaintyReason, Warning,
    SAMPLE_CHILD_DIRS, MAX_CANDIDATES,
};
