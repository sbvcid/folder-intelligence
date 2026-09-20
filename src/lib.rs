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
    EstimatedImpact, EvidenceAnalyzer, EvidenceGap, ExecutionResult, ExecutionStatus,
    ExecutionVerificationResult, Executor, FileMetadata, FileSystemOperation, FilesystemState,
    GapType, Goal, IntentParseError, LogEntry, OperationExecutionState, OperationGuard,
    OperationLog, OperationPlan, Pipeline, PipelineError, PipelineOptions, PipelineResult,
    PlanError, PlanGenerator, PlanPreview, PlanValidationContext, PlanValidator, Policy,
    PolicyDecision, Precondition, ProposedCategory, ProposedOperation, Recommendation,
    RecommendationEngine, RecommendationError, RecommendationStrategy, RecommendationWarning,
    ScopeLock, StructureSummary, TaskAnalysis, TaskIntent, TaskIntentParser, UndoConflict,
    UndoLogEntry, UndoResult, UserDecision, ValidatedOperation, ValidationResult, ValidationStatus,
    ValidationSummary, ValidationWarning, VerificationCheck, VerificationStatus,
    VerificationSummary, WarningType,
};
pub use classification::{
    build_classification_request, build_request, validate_ai_result, AiClassificationConstraints,
    AiClassificationError, AiClassificationRequest, AiClassifier, AlternativeCandidate,
    CandidateEvidence, CandidateScore, CandidateSelector, CandidateSummary, ClassificationConfig,
    ClassificationDecision, ClassificationInput, ClassificationProcessor, ClassificationResult,
    ComparisonType, ConfidenceBand, DecisionEngine, EvidenceComparator, EvidenceComparison,
    FileObservation, LlmClassificationError, LlmClassificationItem, LlmClassificationOutput,
    LlmClassificationRequest, LlmClassifier, MockAiClassifier, RealAiClassifier,
    RuleBasedProcessor, SupportingEvidence, TargetEvidence, UncertaintyReason, UserHints, Warning,
    MAX_CANDIDATES, SAMPLE_CHILD_DIRS,
};
pub use evidence::{
    DirectoryEvidence, DominantExtension, ErrorCategory, IdentifierSummary, IdentifierType,
    ScanError, ScanLimits, ScanMetadata, ScanResult, ScanStats, SyntacticIdentifier,
    TextFilePresence, SCHEMA_VERSION,
};
pub use scanner::{is_excluded_directory, Scanner};

#[cfg(feature = "network")]
pub use classification::{OpenAiProvider, OpenAiProviderConfig};
