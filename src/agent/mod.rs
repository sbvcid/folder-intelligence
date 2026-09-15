pub mod intent;
pub mod analysis;
pub mod recommendation;
pub mod clarification;
pub mod plan;

#[allow(unused_imports)]
pub use intent::{
    TaskIntent, Goal, ConstraintSet, CleanRule, TaskIntentParser, IntentParseError,
};
#[allow(unused_imports)]
pub use analysis::{
    TaskAnalysis, EvidenceAnalyzer, StructureSummary, ContentGroup, ContentType,
    Anomaly, AnomalyType, Ambiguity, AmbiguityReason, EvidenceGap, GapType,
    CandidateCategory, AnalyzerError,
};
#[allow(unused_imports)]
pub use recommendation::{
    Recommendation, RecommendationStrategy, ProposedCategory, ProposedOperation,
    ConstraintCheck, ConstraintViolation, ClarificationQuestion, RecommendationWarning,
    RecommendationEngine, RecommendationError,
};
#[allow(unused_imports)]
pub use clarification::{
    DecisionCategory, DecisionAnswer, UserDecision, ClarifiedIntent,
    ClarificationEngine, ClarificationError,
    apply_evidence_gaps_to_questions,
};
#[allow(unused_imports)]
pub use plan::{
    FileSystemOperation, OperationPlan, EstimatedImpact,
    ValidationWarning, WarningType, PlanGenerator, PlanError,
};
