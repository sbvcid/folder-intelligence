pub mod cli;
pub mod evidence;
pub mod scanner;
pub mod classification;
pub mod agent;

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
    RuleBasedProcessor, ClassificationResult, ClassificationDecision, ConfidenceBand,
    SupportingEvidence, AlternativeCandidate, UncertaintyReason, Warning,
    AiClassifier, AiClassificationConstraints, AiClassificationError, AiClassificationRequest,
    build_request, validate_ai_result, RealAiClassifier, MockAiClassifier,
    SAMPLE_CHILD_DIRS, MAX_CANDIDATES,
};
pub use agent::{
    TaskIntent, Goal, ConstraintSet, CleanRule, TaskIntentParser, IntentParseError,
    TaskAnalysis, EvidenceAnalyzer, AnalyzerError,
};

#[cfg(feature = "network")]
pub use classification::{OpenAiProvider, OpenAiProviderConfig};
