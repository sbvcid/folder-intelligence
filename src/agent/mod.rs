pub mod intent;
pub mod analysis;

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
