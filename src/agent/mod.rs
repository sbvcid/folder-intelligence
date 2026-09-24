pub mod analysis;
pub mod clarification;
pub mod executor;
pub mod intent;
pub mod operation_guard;
pub mod pipeline;
pub mod plan;
pub mod policy;
pub mod proposal_converter;
pub mod recommendation;
pub mod validate;
pub mod verification;

#[allow(unused_imports)]
pub use analysis::{
    Ambiguity, AmbiguityReason, AnalyzerError, Anomaly, AnomalyType, CandidateCategory,
    ContentGroup, ContentType, EvidenceAnalyzer, EvidenceGap, GapType, StructureSummary,
    TaskAnalysis,
};
#[allow(unused_imports)]
pub use clarification::{
    apply_evidence_gaps_to_questions, ClarificationEngine, ClarificationError, ClarifiedIntent,
    DecisionAnswer, DecisionCategory, UserDecision,
};
#[allow(unused_imports)]
pub use executor::{
    ApplyError, ApplyResult, ExecutionStatus, Executor, LogEntry, OperationExecutionState,
    OperationLog, RecoveryResult, UndoConflict, UndoLogEntry, UndoResult,
};
#[allow(unused_imports)]
pub use intent::{
    CleanRule, ConstraintSet, Goal, IntentParseError, TaskIntent, TaskIntentParser, UserIntent,
};
#[allow(unused_imports)]
pub use operation_guard::{ExecutionResult, FileMetadata, OperationGuard, Precondition, ScopeLock};
#[allow(unused_imports)]
pub use pipeline::{ApplyOptions, Pipeline, PipelineError, PipelineOptions, PipelineResult};
#[allow(unused_imports)]
pub use plan::{
    EstimatedImpact, FileSystemOperation, OperationPlan, PlanError, PlanGenerator,
    PlanValidationContext, ValidationWarning, WarningType,
};
#[allow(unused_imports)]
pub use policy::{Approval, Policy, PolicyDecision};
#[allow(unused_imports)]
pub use proposal_converter::{
    ProposalConversionResult, ProposalConverter, UnresolvedItem, UnresolvedReason,
};
#[allow(unused_imports)]
pub use recommendation::{
    map_llm_strategy, ClarificationQuestion, ConstraintCheck, ConstraintViolation,
    OrganizationProposal, ProposedCategory, ProposedOperation, Recommendation,
    RecommendationEngine, RecommendationError, RecommendationStrategy, RecommendationWarning,
    StrategyInfo,
};
#[allow(unused_imports)]
pub use validate::{
    PlanPreview, PlanValidator, ValidatedOperation, ValidationResult, ValidationStatus,
    ValidationSummary,
};
#[allow(unused_imports)]
pub use verification::{
    ExecutionVerificationResult, ExecutionVerifier, FilesystemState, VerificationCheck,
    VerificationStatus, VerificationSummary,
};
