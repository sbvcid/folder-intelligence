pub mod analysis;
pub mod clarification;
pub mod executor;
pub mod intent;
pub mod pipeline;
pub mod plan;
pub mod recommendation;
pub mod validate;

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
    ApplyError, ApplyResult, ExecutionStatus, Executor, LogEntry, OperationLog, UndoConflict,
    UndoLogEntry, UndoResult,
};
#[allow(unused_imports)]
pub use intent::{CleanRule, ConstraintSet, Goal, IntentParseError, TaskIntent, TaskIntentParser};
#[allow(unused_imports)]
pub use pipeline::{ApplyOptions, Pipeline, PipelineError, PipelineOptions, PipelineResult};
#[allow(unused_imports)]
pub use plan::{
    EstimatedImpact, FileSystemOperation, OperationPlan, PlanError, PlanGenerator,
    PlanValidationContext, ValidationWarning, WarningType,
};
#[allow(unused_imports)]
pub use recommendation::{
    ClarificationQuestion, ConstraintCheck, ConstraintViolation, ProposedCategory,
    ProposedOperation, Recommendation, RecommendationEngine, RecommendationError,
    RecommendationStrategy, RecommendationWarning,
};
#[allow(unused_imports)]
pub use validate::{
    PlanPreview, PlanValidator, ValidatedOperation, ValidationResult, ValidationStatus,
    ValidationSummary,
};
