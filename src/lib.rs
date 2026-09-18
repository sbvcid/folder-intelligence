pub mod agent;
pub mod classification;
pub mod cli;
pub mod evidence;
pub mod llm;
pub mod scanner;

pub use agent::{
    Ambiguity, AmbiguityReason, AnalyzerError, Anomaly, AnomalyType, ApplyError, ApplyOptions,
    ApplyResult, Approval, CandidateCategory, ClarificationEngine, ClarificationError,
    ClarificationQuestion, ClarifiedIntent, CleanRule, ConstraintCheck, ConstraintSet,
    ConstraintViolation, ContentGroup, ContentType, DecisionAnswer, DecisionCategory,
    EstimatedImpact, EvidenceAnalyzer, EvidenceGap, ExecutionStatus, Executor, FileSystemOperation,
    GapType, Goal, IntentParseError, LogEntry, OperationLog, OperationPlan, Pipeline,
    PipelineError, PipelineOptions, PipelineResult, PlanError, PlanGenerator, PlanPreview,
    PlanValidationContext, PlanValidator, Policy, PolicyDecision, ProposedCategory,
    ProposedOperation, Recommendation, RecommendationEngine, RecommendationError,
    RecommendationStrategy, RecommendationWarning, StructureSummary, TaskAnalysis, TaskIntent,
    TaskIntentParser, UndoConflict, UndoLogEntry, UndoResult, UserDecision, ValidatedOperation,
    ValidationResult, ValidationStatus, ValidationSummary, ValidationWarning, WarningType,
};
pub use classification::{
    build_request, validate_ai_result, AiClassificationConstraints, AiClassificationError,
    AiClassificationRequest, AiClassifier, AlternativeCandidate, CandidateEvidence, CandidateScore,
    CandidateSelector, CandidateSummary, ClassificationConfig, ClassificationDecision,
    ClassificationInput, ClassificationProcessor, ClassificationResult, ComparisonType,
    ConfidenceBand, DecisionEngine, EvidenceComparator, EvidenceComparison, MockAiClassifier,
    RealAiClassifier, RuleBasedProcessor, SupportingEvidence, TargetEvidence, UncertaintyReason,
    UserHints, Warning, MAX_CANDIDATES, SAMPLE_CHILD_DIRS,
};
pub use evidence::{
    DirectoryEvidence, DominantExtension, ErrorCategory, IdentifierSummary, IdentifierType,
    ScanError, ScanLimits, ScanMetadata, ScanResult, ScanStats, SyntacticIdentifier,
    TextFilePresence, SCHEMA_VERSION,
};
pub use scanner::{is_excluded_directory, Scanner};

#[cfg(feature = "network")]
pub use classification::{OpenAiProvider, OpenAiProviderConfig};
