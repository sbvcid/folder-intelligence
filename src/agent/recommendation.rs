use crate::agent::analysis::TaskAnalysis;
use crate::agent::intent::{Goal, TaskIntent};
use crate::classification::LlmOrganizationStrategy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationStrategy {
    CategoryBased,
    ProjectBased,
    Chronological,
    BySize,
    ByAuthor,
    ByTitle,
    ByYear,
    PreserveExisting,
    Unknown,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategyInfo {
    pub strategy: RecommendationStrategy,
    pub confidence: f64,
    pub reason: Option<String>,
}

impl Default for StrategyInfo {
    fn default() -> Self {
        StrategyInfo {
            strategy: RecommendationStrategy::Unknown,
            confidence: 0.0,
            reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProposedCategory {
    pub name: String,
    pub purpose: String,
    pub target_content_types: Vec<String>,
    pub confidence: f64,
    pub is_existing: bool,
    pub target_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProposedOperation {
    MoveCategory {
        strategy: String,
        content_type: String,
        file_count: u64,
        to_category: String,
    },
    CreateCategory {
        name: String,
        purpose: String,
    },
    PreserveDirectory {
        path: PathBuf,
    },
    ArchiveFiles {
        category: String,
        file_count: u64,
    },
    LeaveUnclassified {
        file_count: u64,
        reason: String,
    },
}

impl ProposedOperation {
    pub fn description(&self) -> String {
        match self {
            ProposedOperation::MoveCategory {
                strategy: _,
                content_type,
                file_count,
                to_category,
            } => format!(
                "Move {} {} files to '{}'",
                file_count, content_type, to_category
            ),
            ProposedOperation::CreateCategory { name, purpose } => {
                format!("Create category '{}' for {}", name, purpose)
            }
            ProposedOperation::PreserveDirectory { path } => {
                format!("Preserve existing directory: {}", path.display())
            }
            ProposedOperation::ArchiveFiles {
                category,
                file_count,
            } => {
                format!("Archive {} files under {}", file_count, category)
            }
            ProposedOperation::LeaveUnclassified { file_count, reason } => {
                format!("Leave {} files unclassified: {}", file_count, reason)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConstraintCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConstraintViolation {
    pub constraint: String,
    pub reason: String,
    pub blocked_operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ClarificationQuestion {
    pub id: String,
    pub question: String,
    pub context: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationWarning {
    LowEvidence,
    AmbiguousContent,
    NoCandidateCategories,
    ConflictingCategories,
    HighRisk,
    InsufficientContentGroups,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Recommendation {
    pub id: String,
    pub strategy: RecommendationStrategy,
    pub strategy_info: Option<StrategyInfo>,
    pub rationale: String,
    pub proposed_categories: Vec<ProposedCategory>,
    pub proposed_operations: Vec<ProposedOperation>,
    pub unresolved_questions: Vec<ClarificationQuestion>,
    pub confidence: f64,
    pub constraint_checks: Vec<ConstraintCheck>,
    pub constraint_violation: Option<ConstraintViolation>,
    pub warnings: Vec<RecommendationWarning>,
    pub generated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RecommendationError {
    AnalysisInvalid(String),
    ScopeMismatch,
    NoContentGroups,
}

impl std::fmt::Display for RecommendationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecommendationError::AnalysisInvalid(msg) => {
                write!(f, "Analysis is invalid: {}", msg)
            }
            RecommendationError::ScopeMismatch => {
                write!(f, "Analysis scope does not match intent scope")
            }
            RecommendationError::NoContentGroups => {
                write!(f, "No content groups available for recommendation")
            }
        }
    }
}

impl std::error::Error for RecommendationError {}

pub struct RecommendationEngine;

impl Default for RecommendationEngine {
    fn default() -> Self {
        Self
    }
}

impl RecommendationEngine {
    pub fn recommend(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
    ) -> Result<Recommendation, RecommendationError> {
        self.validate_analysis(intent, analysis)?;

        let strategy = self.determine_strategy(intent, analysis);
        let proposed_categories = self.propose_categories(intent, analysis, &strategy);
        let mut proposed_operations =
            self.generate_operations(intent, analysis, &proposed_categories);

        let (constraint_checks, constraint_violation) =
            self.enforce_constraints(intent, &mut proposed_operations);

        let unresolved_questions = self.generate_questions(intent, analysis);
        let warnings = self.detect_warnings(intent, analysis, &proposed_categories);
        let confidence = self.calculate_confidence(analysis, &warnings);
        let rationale = self.build_rationale(intent, analysis, &strategy, &proposed_categories);

        let generated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let id = format!("rec-{}", generated_at);

        let strategy_info = analysis
            .organization_strategy
            .as_ref()
            .map(|s| StrategyInfo {
                strategy: map_llm_strategy(&s.strategy),
                confidence: s.confidence,
                reason: s.reason.clone(),
            });

        Ok(Recommendation {
            id,
            strategy,
            strategy_info,
            rationale,
            proposed_categories,
            proposed_operations,
            unresolved_questions,
            confidence,
            constraint_checks,
            constraint_violation,
            warnings,
            generated_at,
        })
    }

    fn validate_analysis(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
    ) -> Result<(), RecommendationError> {
        let intent_scope = match &intent.goal {
            Goal::Organize { scope, .. }
            | Goal::Reorganize { scope, .. }
            | Goal::Clean { scope, .. } => scope.clone(),
        };

        if analysis.scope_evidence.path != intent_scope {
            return Err(RecommendationError::ScopeMismatch);
        }

        if analysis.structure_summary.content_groups.is_empty() {
            return Err(RecommendationError::NoContentGroups);
        }

        Ok(())
    }

    fn determine_strategy(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
    ) -> RecommendationStrategy {
        if let Some(ref llm_strategy) = analysis.organization_strategy {
            let mapped = map_llm_strategy(&llm_strategy.strategy);
            if mapped != RecommendationStrategy::Unknown {
                return mapped;
            }
        }

        match &intent.goal {
            Goal::Organize { purpose, .. } => match purpose.as_str() {
                "by_category" | "by_type" => RecommendationStrategy::CategoryBased,
                "by_project" | "work/project" => RecommendationStrategy::ProjectBased,
                "by_date" => RecommendationStrategy::Chronological,
                "by_size" => RecommendationStrategy::BySize,
                "general_organization" => RecommendationStrategy::CategoryBased,
                custom => RecommendationStrategy::Custom(custom.to_string()),
            },
            Goal::Reorganize { strategy, .. } => {
                if let Some(s) = strategy {
                    match s.as_str() {
                        "by_category" | "by_type" => RecommendationStrategy::CategoryBased,
                        "by_project" => RecommendationStrategy::ProjectBased,
                        "by_date" => RecommendationStrategy::Chronological,
                        "by_size" => RecommendationStrategy::BySize,
                        _ => RecommendationStrategy::Custom(s.clone()),
                    }
                } else {
                    RecommendationStrategy::CategoryBased
                }
            }
            Goal::Clean { rules, .. } => {
                if rules.is_empty() {
                    RecommendationStrategy::PreserveExisting
                } else {
                    RecommendationStrategy::CategoryBased
                }
            }
        }
    }

    fn propose_categories(
        &self,
        _intent: &TaskIntent,
        analysis: &TaskAnalysis,
        _strategy: &RecommendationStrategy,
    ) -> Vec<ProposedCategory> {
        let mut categories = Vec::new();
        let content_groups = &analysis.structure_summary.content_groups;

        let classification = analysis.classification_results.first();

        if let Some(cr) = classification {
            use crate::classification::ClassificationDecision;
            match cr.decision {
                ClassificationDecision::MoveExisting => {
                    if let Some(ref selected) = cr.selected_candidate {
                        let candidate = analysis
                            .candidate_categories
                            .iter()
                            .find(|c| c.path == *selected);
                        if let Some(cand) = candidate {
                            categories.push(ProposedCategory {
                                name: cand.name.clone(),
                                purpose: cand.name.clone(),
                                target_content_types: content_groups
                                    .iter()
                                    .map(|g| g.extension.clone())
                                    .collect(),
                                confidence: cr.confidence,
                                is_existing: true,
                                target_path: Some(selected.clone()),
                            });
                        }
                    }
                }
                ClassificationDecision::CreateCategory => {
                    if let Some(ref proposed_name) = cr.proposed_category_name {
                        let category_name = Self::normalize_category_name(proposed_name);
                        categories.push(ProposedCategory {
                            name: category_name.clone(),
                            purpose: proposed_name.clone(),
                            target_content_types: content_groups
                                .iter()
                                .map(|g| g.extension.clone())
                                .collect(),
                            confidence: cr.confidence,
                            is_existing: false,
                            target_path: None,
                        });
                    }
                }
                ClassificationDecision::LeaveUnclassified | ClassificationDecision::AskUser => {
                    // No categories proposed for LeaveUnclassified or AskUser
                }
            }
        }

        categories
    }

    fn normalize_category_name(name: &str) -> String {
        name.to_lowercase().replace(' ', "_").replace('-', "_")
    }

    fn generate_operations(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
        _proposed_categories: &[ProposedCategory],
    ) -> Vec<ProposedOperation> {
        let mut operations = Vec::new();

        let classification = analysis.classification_results.first();

        if let Some(cr) = classification {
            use crate::classification::ClassificationDecision;
            match cr.decision {
                ClassificationDecision::MoveExisting => {
                    if let Some(ref selected) = cr.selected_candidate {
                        let candidate = analysis
                            .candidate_categories
                            .iter()
                            .find(|c| c.path == *selected);
                        if let Some(cand) = candidate {
                            let content_groups = &analysis.structure_summary.content_groups;
                            for group in content_groups {
                                operations.push(ProposedOperation::MoveCategory {
                                    strategy: self.strategy_name(intent),
                                    content_type: group.extension.clone(),
                                    file_count: group.file_count,
                                    to_category: cand.name.clone(),
                                });
                            }
                            operations.push(ProposedOperation::PreserveDirectory {
                                path: selected.clone(),
                            });
                        }
                    }
                }
                ClassificationDecision::CreateCategory => {
                    if let Some(ref proposed_name) = cr.proposed_category_name {
                        let category_name = Self::normalize_category_name(proposed_name);
                        let content_groups = &analysis.structure_summary.content_groups;
                        operations.push(ProposedOperation::CreateCategory {
                            name: category_name.clone(),
                            purpose: proposed_name.clone(),
                        });
                        for group in content_groups {
                            operations.push(ProposedOperation::MoveCategory {
                                strategy: self.strategy_name(intent),
                                content_type: group.extension.clone(),
                                file_count: group.file_count,
                                to_category: category_name.clone(),
                            });
                        }
                    }
                }
                ClassificationDecision::LeaveUnclassified => {
                    // LeaveUnclassified: zero mutation
                }
                ClassificationDecision::AskUser => {
                    // AskUser: zero mutation
                }
            }
        }

        operations
    }

    fn enforce_constraints(
        &self,
        intent: &TaskIntent,
        operations: &mut Vec<ProposedOperation>,
    ) -> (Vec<ConstraintCheck>, Option<ConstraintViolation>) {
        let mut checks = Vec::new();
        let mut blocked_ops: Vec<String> = Vec::new();

        let preserve = intent.constraints.preserve_existing_folders;
        let auto_delete = intent.constraints.auto_delete_temps;
        let merge = intent.constraints.merge_duplicates;
        let archive = intent.constraints.archive_old.is_some();

        checks.push(ConstraintCheck {
            name: "preserve_existing_folders".to_string(),
            passed: preserve
                || !operations
                    .iter()
                    .any(|op| matches!(op, ProposedOperation::PreserveDirectory { .. })),
            message: if preserve {
                "Existing directories will be preserved".to_string()
            } else {
                "Preserve constraint not explicitly set".to_string()
            },
        });

        checks.push(ConstraintCheck {
            name: "auto_delete_temps".to_string(),
            passed: auto_delete
                || !operations.iter().any(|op| {
                    if let ProposedOperation::ArchiveFiles { category, .. } = op {
                        category.contains("temp")
                    } else {
                        false
                    }
                }),
            message: if auto_delete {
                "Temp file handling permitted by constraint".to_string()
            } else {
                "Temp file auto-deletion disabled".to_string()
            },
        });

        checks.push(ConstraintCheck {
            name: "merge_duplicates".to_string(),
            passed: true,
            message: if merge {
                "Duplicate merging enabled".to_string()
            } else {
                "Duplicate merging not requested".to_string()
            },
        });

        checks.push(ConstraintCheck {
            name: "archive_old_files".to_string(),
            passed: archive
                || !operations.iter().any(|op| {
                    if let ProposedOperation::ArchiveFiles { category, .. } = op {
                        category.starts_with("files_older_than_")
                    } else {
                        false
                    }
                }),
            message: if archive {
                "Old file archiving enabled".to_string()
            } else {
                "Old file archiving not requested".to_string()
            },
        });

        let mut violation = None;

        if !auto_delete {
            let violating: Vec<_> = operations
                .iter()
                .filter(|op| {
                    if let ProposedOperation::ArchiveFiles { category, .. } = op {
                        category.contains("temp")
                    } else {
                        false
                    }
                })
                .map(|op| op.description())
                .collect();

            if !violating.is_empty() {
                blocked_ops.extend(violating.clone());
                violation = Some(ConstraintViolation {
                    constraint: "auto_delete_temps".to_string(),
                    reason: "auto_delete_temps is false; temp file deletion proposed".to_string(),
                    blocked_operations: violating,
                });
            }
        }

        if violation.is_none() && !archive {
            let violating: Vec<_> = operations
                .iter()
                .filter(|op| {
                    if let ProposedOperation::ArchiveFiles { category, .. } = op {
                        category.starts_with("files_older_than_")
                    } else {
                        false
                    }
                })
                .map(|op| op.description())
                .collect();

            if !violating.is_empty() {
                blocked_ops.extend(violating);
                violation = Some(ConstraintViolation {
                    constraint: "archive_old_files".to_string(),
                    reason: "Archive duration not set; archiving old files proposed".to_string(),
                    blocked_operations: blocked_ops.clone(),
                });
            }
        }

        if let Some(ref v) = violation {
            operations.retain(|op| !v.blocked_operations.contains(&op.description()));
        }

        (checks, violation)
    }

    fn generate_questions(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
    ) -> Vec<ClarificationQuestion> {
        let mut questions = Vec::new();
        let mut idx = 0;

        for gap in &analysis.evidence_gaps {
            if gap.requires_user_input {
                idx += 1;
                let question_id = format!("q{}", idx);
                let (question_text, options) = match gap.gap_type {
                    crate::agent::analysis::GapType::Taxonomy => (
                        "What category structure do you prefer?".to_string(),
                        vec![
                            "By file type".to_string(),
                            "By project".to_string(),
                            "By date".to_string(),
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
                        ],
                    ),
                    crate::agent::analysis::GapType::ArchivePolicy => (
                        "What is your archive policy for old files?".to_string(),
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
        }

        let intent_unknowns: Vec<String> = intent.unknown_factors.clone();
        for unknown in &intent_unknowns {
            idx += 1;
            if !questions.iter().any(|q| q.question.contains(unknown)) {
                questions.push(ClarificationQuestion {
                    id: format!("q{}", idx),
                    question: format!("Please specify your preference for: {}", unknown),
                    context: "Unknown factor identified during intent parsing".to_string(),
                    options: vec!["Leave as-is".to_string(), "Use default".to_string()],
                });
            }
        }

        questions.truncate(intent.constraints.max_interactive_questions);

        questions
    }

    fn detect_warnings(
        &self,
        _intent: &TaskIntent,
        analysis: &TaskAnalysis,
        proposed_categories: &[ProposedCategory],
    ) -> Vec<RecommendationWarning> {
        let mut warnings = Vec::new();

        if analysis.structure_summary.partial_scan {
            warnings.push(RecommendationWarning::LowEvidence);
        }

        if analysis.ambiguities.len() > 5 {
            warnings.push(RecommendationWarning::AmbiguousContent);
        }

        if analysis.candidate_categories.is_empty() && !proposed_categories.is_empty() {
            warnings.push(RecommendationWarning::NoCandidateCategories);
        }

        if analysis.classification_results.iter().any(|r| {
            r.confidence < 0.3
                && matches!(
                    r.decision,
                    crate::classification::ClassificationDecision::AskUser
                        | crate::classification::ClassificationDecision::LeaveUnclassified
                )
        }) {
            warnings.push(RecommendationWarning::ConflictingCategories);
        }

        let total_files = analysis.structure_summary.total_files;
        let other_files: u64 = analysis
            .content_groups
            .iter()
            .filter(|g| g.category == crate::agent::analysis::ContentType::Other)
            .map(|g| g.file_count)
            .sum();

        if total_files > 0 && (other_files as f64 / total_files as f64) > 0.5 {
            warnings.push(RecommendationWarning::InsufficientContentGroups);
        }

        warnings
    }

    fn calculate_confidence(
        &self,
        analysis: &TaskAnalysis,
        warnings: &[RecommendationWarning],
    ) -> f64 {
        // Use classification result's confidence directly as the base,
        // rather than incorrectly treating classification_results.len() as file count ratio.
        let base_confidence = analysis
            .classification_results
            .first()
            .map(|r| r.confidence)
            .unwrap_or(0.5);

        let mut confidence = base_confidence;

        if analysis.ambiguities.is_empty() {
            confidence += 0.1;
        } else {
            confidence -= (analysis.ambiguities.len() as f64 * 0.05).min(0.3);
        }

        if analysis.evidence_gaps.is_empty() {
            confidence += 0.05;
        } else {
            confidence -= (analysis.evidence_gaps.len() as f64 * 0.03).min(0.15);
        }

        for warning in warnings {
            match warning {
                RecommendationWarning::LowEvidence => confidence -= 0.2,
                RecommendationWarning::AmbiguousContent => confidence -= 0.15,
                RecommendationWarning::ConflictingCategories => confidence -= 0.1,
                RecommendationWarning::InsufficientContentGroups => confidence -= 0.2,
                _ => {}
            }
        }

        confidence.clamp(0.0, 1.0)
    }

    fn build_rationale(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
        strategy: &RecommendationStrategy,
        proposed_categories: &[ProposedCategory],
    ) -> String {
        let strategy_name = match strategy {
            RecommendationStrategy::CategoryBased => "category-based".to_string(),
            RecommendationStrategy::ProjectBased => "project-based".to_string(),
            RecommendationStrategy::Chronological => "chronological".to_string(),
            RecommendationStrategy::BySize => "by-size".to_string(),
            RecommendationStrategy::ByAuthor => "by-author".to_string(),
            RecommendationStrategy::ByTitle => "by-title".to_string(),
            RecommendationStrategy::ByYear => "by-year".to_string(),
            RecommendationStrategy::PreserveExisting => "preserve existing".to_string(),
            RecommendationStrategy::Unknown => "unknown".to_string(),
            RecommendationStrategy::Custom(s) => s.clone(),
        };

        let strategy_reason = analysis
            .organization_strategy
            .as_ref()
            .and_then(|s| s.reason.as_ref());

        let total_files = analysis.structure_summary.total_files;
        let total_dirs = analysis.structure_summary.total_directories;
        let num_groups = analysis.content_groups.len();
        let num_categories = proposed_categories.len();

        let purpose = match &intent.goal {
            Goal::Organize { purpose, .. } => purpose.as_str(),
            Goal::Reorganize {
                strategy: Some(s), ..
            } => s.as_str(),
            Goal::Clean { .. } => "clean-up",
            _ => "general",
        };

        let classification_confidence = analysis
            .classification_results
            .first()
            .map(|r| r.confidence)
            .unwrap_or(0.5);

        let strategy_reason_text = strategy_reason
            .map(|r| format!(" Strategy reason: {}.", r))
            .unwrap_or_default();

        let evidence_gaps_text = if !analysis.evidence_gaps.is_empty() {
            let gaps: Vec<&str> = analysis
                .evidence_gaps
                .iter()
                .map(|g| g.description.as_str())
                .collect();
            format!(" Evidence gaps: {}.", gaps.join("; "))
        } else {
            String::new()
        };

        format!(
            "Analyzed {} directory ({} files, {} subdirectories). \
            Detected {} content type groups. \
            Strategy: {} (purpose: {}).{} \
            Proposed {} categories. \
            Classification confidence: {:.0}%. \
            {} ambiguities and {} evidence gaps require attention.{}",
            analysis.scope_evidence.path.display(),
            total_files,
            total_dirs,
            num_groups,
            strategy_name,
            purpose,
            strategy_reason_text,
            num_categories,
            classification_confidence * 100.0,
            analysis.ambiguities.len(),
            analysis.evidence_gaps.len(),
            evidence_gaps_text,
        )
    }

    fn strategy_name(&self, intent: &TaskIntent) -> String {
        match &intent.goal {
            Goal::Organize { purpose, .. } => match purpose.as_str() {
                "by_category" | "by_type" => "category".to_string(),
                "by_project" | "work/project" => "project".to_string(),
                "by_date" => "chronological".to_string(),
                "by_size" => "size".to_string(),
                "general_organization" => "category".to_string(),
                custom => custom.to_string(),
            },
            Goal::Reorganize { strategy, .. } => {
                if let Some(s) = strategy {
                    match s.as_str() {
                        "by_category" | "by_type" => "category".to_string(),
                        "by_project" => "project".to_string(),
                        "by_date" => "chronological".to_string(),
                        "by_size" => "size".to_string(),
                        other => other.to_string(),
                    }
                } else {
                    "category".to_string()
                }
            }
            Goal::Clean { rules, .. } => {
                if rules.is_empty() {
                    "preserve".to_string()
                } else {
                    "category".to_string()
                }
            }
        }
    }
}

pub fn map_llm_strategy(llm: &LlmOrganizationStrategy) -> RecommendationStrategy {
    match llm {
        LlmOrganizationStrategy::ByAuthor => RecommendationStrategy::ByAuthor,
        LlmOrganizationStrategy::ByTitle => RecommendationStrategy::ByTitle,
        LlmOrganizationStrategy::ByType => RecommendationStrategy::CategoryBased,
        LlmOrganizationStrategy::ByYear => RecommendationStrategy::ByYear,
        LlmOrganizationStrategy::PreserveExisting => RecommendationStrategy::PreserveExisting,
        LlmOrganizationStrategy::CategoryBased => RecommendationStrategy::CategoryBased,
        LlmOrganizationStrategy::Unknown => RecommendationStrategy::Unknown,
    }
}

#[cfg(test)]
#[path = "recommendation_tests.rs"]
mod tests;
