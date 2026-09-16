use crate::agent::analysis::EvidenceGap;
use crate::agent::intent::{Goal, TaskIntent};
use crate::agent::recommendation::{
    ClarificationQuestion, ConstraintViolation, Recommendation, RecommendationEngine,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DecisionCategory {
    PreserveExistingFolders,
    AutoDeleteTemps,
    MergeDuplicates,
    ArchiveOld,
    TaxonomyChoice,
    FileDisposition,
    DuplicateHandling,
    AmbiguousFileResolution,
    CleanupScopeDefinition,
}

#[allow(dead_code)]
impl DecisionCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            DecisionCategory::PreserveExistingFolders => "Preserve existing folders",
            DecisionCategory::AutoDeleteTemps => "Auto-delete temporary files",
            DecisionCategory::MergeDuplicates => "Merge duplicate files",
            DecisionCategory::ArchiveOld => "Archive old files",
            DecisionCategory::TaxonomyChoice => "Category structure",
            DecisionCategory::FileDisposition => "File disposition policy",
            DecisionCategory::DuplicateHandling => "Duplicate handling strategy",
            DecisionCategory::AmbiguousFileResolution => "Ambiguous file resolution",
            DecisionCategory::CleanupScopeDefinition => "Cleanup scope definition",
        }
    }

    pub fn maps_to_constraint(&self) -> Option<&'static str> {
        match self {
            DecisionCategory::PreserveExistingFolders => Some("preserve_existing_folders"),
            DecisionCategory::AutoDeleteTemps => Some("auto_delete_temps"),
            DecisionCategory::MergeDuplicates => Some("merge_duplicates"),
            DecisionCategory::ArchiveOld => Some("archive_old"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DecisionAnswer {
    Yes,
    No,
    Choice(String),
    Duration(Duration),
    Text(String),
}

#[allow(dead_code)]
impl DecisionAnswer {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DecisionAnswer::Yes => Some(true),
            DecisionAnswer::No => Some(false),
            _ => None,
        }
    }

    pub fn as_choice(&self) -> Option<&str> {
        match self {
            DecisionAnswer::Choice(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub struct UserDecision {
    pub category: DecisionCategory,
    pub answer: DecisionAnswer,
    pub question_id: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ClarificationError {
    QuestionNotFound(String),
    InvalidAnswer(String),
    SessionCompleted,
}

impl std::fmt::Display for ClarificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClarificationError::QuestionNotFound(id) => {
                write!(f, "Question not found: {}", id)
            }
            ClarificationError::InvalidAnswer(msg) => {
                write!(f, "Invalid answer: {}", msg)
            }
            ClarificationError::SessionCompleted => {
                write!(f, "Clarification session is already complete")
            }
        }
    }
}

impl std::error::Error for ClarificationError {}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClarifiedIntent {
    pub intent: TaskIntent,
    pub decisions: Vec<UserDecision>,
    pub remaining_questions: Vec<ClarificationQuestion>,
    pub blocked_operations_visible: Vec<String>,
    pub constraint_violation: Option<ConstraintViolation>,
}

pub struct ClarificationEngine;

impl Default for ClarificationEngine {
    fn default() -> Self {
        Self
    }
}

#[allow(dead_code)]
impl ClarificationEngine {
    pub fn start(&self, recommendation: &Recommendation) -> Vec<ClarificationQuestion> {
        recommendation.unresolved_questions.clone()
    }

    pub fn blocked_operations(&self, recommendation: &Recommendation) -> Vec<String> {
        if let Some(ref violation) = recommendation.constraint_violation {
            violation.blocked_operations.clone()
        } else {
            Vec::new()
        }
    }

    pub fn apply_decisions(&self, intent: &TaskIntent, decisions: &[UserDecision]) -> TaskIntent {
        let mut updated = intent.clone();

        for decision in decisions {
            match decision.category {
                DecisionCategory::PreserveExistingFolders => {
                    if let Some(b) = decision.answer.as_bool() {
                        updated.constraints.preserve_existing_folders = b;
                    }
                }
                DecisionCategory::AutoDeleteTemps => {
                    if let Some(b) = decision.answer.as_bool() {
                        updated.constraints.auto_delete_temps = b;
                    }
                }
                DecisionCategory::MergeDuplicates => {
                    if let Some(b) = decision.answer.as_bool() {
                        updated.constraints.merge_duplicates = b;
                    }
                }
                DecisionCategory::ArchiveOld => {
                    if let Some(b) = decision.answer.as_bool() {
                        if b {
                            updated.constraints.archive_old =
                                Some(Duration::from_secs(365 * 86400));
                        } else {
                            updated.constraints.archive_old = None;
                        }
                    }
                    if let DecisionAnswer::Duration(d) = &decision.answer {
                        updated.constraints.archive_old = Some(*d);
                    }
                }
                DecisionCategory::TaxonomyChoice => {
                    if let Some(choice) = decision.answer.as_choice() {
                        match &mut updated.goal {
                            Goal::Organize { purpose, .. } => {
                                *purpose = choice.to_string();
                            }
                            Goal::Reorganize { strategy, .. } => {
                                *strategy = Some(choice.to_string());
                            }
                            _ => {}
                        }
                    }
                }
                DecisionCategory::FileDisposition => {
                    if let Some(choice) = decision.answer.as_choice() {
                        updated
                            .user_hints
                            .insert("file_disposition".to_string(), choice.to_string());
                    }
                }
                DecisionCategory::DuplicateHandling => {
                    if let Some(choice) = decision.answer.as_choice() {
                        updated
                            .user_hints
                            .insert("duplicate_handling".to_string(), choice.to_string());
                    }
                }
                DecisionCategory::AmbiguousFileResolution => {
                    if let Some(choice) = decision.answer.as_choice() {
                        updated
                            .user_hints
                            .insert("ambiguous_resolution".to_string(), choice.to_string());
                    }
                }
                DecisionCategory::CleanupScopeDefinition => {
                    if let Some(choice) = decision.answer.as_choice() {
                        updated
                            .user_hints
                            .insert("cleanup_scope".to_string(), choice.to_string());
                    }
                }
            }
        }

        updated
    }

    pub fn resolve_question(
        &self,
        questions: &[ClarificationQuestion],
        question_id: &str,
        answer: DecisionAnswer,
    ) -> Result<UserDecision, ClarificationError> {
        let question = questions
            .iter()
            .find(|q| q.id == question_id)
            .ok_or_else(|| ClarificationError::QuestionNotFound(question_id.to_string()))?;

        let category = Self::infer_category(question);

        match &answer {
            DecisionAnswer::Yes | DecisionAnswer::No => {
                if category.maps_to_constraint().is_some() {
                    Ok(UserDecision {
                        category,
                        answer,
                        question_id: question_id.to_string(),
                        rationale: None,
                    })
                } else {
                    Err(ClarificationError::InvalidAnswer(format!(
                        "Question '{}' expects a choice, not yes/no",
                        question_id
                    )))
                }
            }
            DecisionAnswer::Choice(ref choice) => {
                if questions
                    .iter()
                    .find(|q| q.id == question_id)
                    .map(|q| q.options.contains(choice))
                    .unwrap_or(false)
                {
                    Ok(UserDecision {
                        category,
                        answer,
                        question_id: question_id.to_string(),
                        rationale: None,
                    })
                } else {
                    Err(ClarificationError::InvalidAnswer(format!(
                        "Choice '{}' is not a valid option for question '{}'",
                        choice, question_id
                    )))
                }
            }
            DecisionAnswer::Duration(_) => {
                if category == DecisionCategory::ArchiveOld {
                    Ok(UserDecision {
                        category,
                        answer,
                        question_id: question_id.to_string(),
                        rationale: None,
                    })
                } else {
                    Err(ClarificationError::InvalidAnswer(format!(
                        "Duration answer not applicable to question '{}'",
                        question_id
                    )))
                }
            }
            DecisionAnswer::Text(_) => Ok(UserDecision {
                category,
                answer,
                question_id: question_id.to_string(),
                rationale: None,
            }),
        }
    }

    fn infer_category(question: &ClarificationQuestion) -> DecisionCategory {
        let q = question.question.to_lowercase();
        let context = question.context.to_lowercase();
        let combined = format!("{} {}", q, context);

        if combined.contains("archive")
            || combined.contains("archive_old")
            || combined.contains("archive policy")
        {
            DecisionCategory::ArchiveOld
        } else if combined.contains("preserve") || combined.contains("folder") {
            DecisionCategory::PreserveExistingFolders
        } else if combined.contains("duplicate") || combined.contains("merge") {
            DecisionCategory::DuplicateHandling
        } else if combined.contains("category")
            || combined.contains("structure")
            || combined.contains("taxonomy")
        {
            DecisionCategory::TaxonomyChoice
        } else if combined.contains("disposition") || combined.contains("unclassified") {
            DecisionCategory::FileDisposition
        } else if combined.contains("temp") {
            DecisionCategory::AutoDeleteTemps
        } else if combined.contains("ambiguous") {
            DecisionCategory::AmbiguousFileResolution
        } else if combined.contains("cleanup scope") || combined.contains("scope definition") {
            DecisionCategory::CleanupScopeDefinition
        } else {
            DecisionCategory::FileDisposition
        }
    }

    pub fn recompute_recommendation(
        &self,
        intent: &TaskIntent,
        analysis: &crate::agent::analysis::TaskAnalysis,
    ) -> Result<Recommendation, crate::agent::recommendation::RecommendationError> {
        let engine = RecommendationEngine;
        engine.recommend(intent, analysis)
    }

    pub fn is_complete(&self, recommendation: &Recommendation) -> bool {
        recommendation.unresolved_questions.is_empty()
    }

    pub fn summarize(&self, recommendation: &Recommendation) -> String {
        let mut summary = String::new();

        summary.push_str(&format!("Strategy: {:?}\n", recommendation.strategy));
        summary.push('\n');
        summary.push_str(&format!("Rationale: {}\n", recommendation.rationale));
        summary.push('\n');

        if !recommendation.proposed_operations.is_empty() {
            summary.push_str("Proposed Operations:\n");
            for op in &recommendation.proposed_operations {
                summary.push_str(&format!("  • {}\n", op.description()));
            }
            summary.push('\n');
        }

        if let Some(ref violation) = recommendation.constraint_violation {
            summary.push_str(&format!(
                "⚠️  Constraint violation: {} — {}\n",
                violation.constraint, violation.reason
            ));
            summary.push_str("Blocked operations:\n");
            for op in &violation.blocked_operations {
                summary.push_str(&format!("  ✗ {}\n", op));
            }
            summary.push('\n');
        }

        if !recommendation.unresolved_questions.is_empty() {
            summary.push_str(&format!(
                "Clarification needed ({} questions):\n",
                recommendation.unresolved_questions.len()
            ));
            for q in &recommendation.unresolved_questions {
                summary.push_str(&format!("  [{}] {}\n", q.id, q.question));
                for (i, opt) in q.options.iter().enumerate() {
                    summary.push_str(&format!("    {}: {}\n", i, opt));
                }
            }
        }

        summary.push('\n');
        summary.push_str(&format!(
            "Confidence: {:.0}%\n",
            recommendation.confidence * 100.0
        ));

        if !recommendation.warnings.is_empty() {
            summary.push_str("Warnings:\n");
            for w in &recommendation.warnings {
                summary.push_str(&format!("  • {:?}\n", w));
            }
        }

        summary
    }
}

#[allow(dead_code)]
pub fn apply_evidence_gaps_to_questions(gaps: &[EvidenceGap]) -> Vec<ClarificationQuestion> {
    let mut questions = Vec::new();
    let mut idx = 0;

    for gap in gaps.iter().filter(|g| g.requires_user_input) {
        idx += 1;
        let question_id = format!("gap-{}", idx);
        let (question_text, options) = match gap.gap_type {
            crate::agent::analysis::GapType::Taxonomy => (
                "What category structure do you prefer?".to_string(),
                vec![
                    "By file type".to_string(),
                    "By project".to_string(),
                    "By date".to_string(),
                    "By size".to_string(),
                ],
            ),
            crate::agent::analysis::GapType::FileDisposition => (
                "How should unclassified files be handled?".to_string(),
                vec![
                    "Leave in place".to_string(),
                    "Move to 'misc' folder".to_string(),
                    "Archive".to_string(),
                ],
            ),
            crate::agent::analysis::GapType::DuplicateHandling => (
                "How should duplicate files be handled?".to_string(),
                vec![
                    "Keep newer".to_string(),
                    "Keep larger".to_string(),
                    "Merge if identical".to_string(),
                    "Review manually".to_string(),
                ],
            ),
            crate::agent::analysis::GapType::ArchivePolicy => (
                "What archive policy for old files?".to_string(),
                vec![
                    "Archive after 90 days".to_string(),
                    "Archive after 1 year".to_string(),
                    "Do not archive".to_string(),
                ],
            ),
            crate::agent::analysis::GapType::ScopeBoundary => (
                "Should the scope include subdirectories?".to_string(),
                vec![
                    "Yes, include all".to_string(),
                    "No, top-level only".to_string(),
                ],
            ),
        };

        questions.push(ClarificationQuestion {
            id: question_id,
            question: question_text,
            context: gap.description.clone(),
            options,
        });
    }

    questions
}

#[cfg(test)]
#[path = "clarification_tests.rs"]
mod tests;
