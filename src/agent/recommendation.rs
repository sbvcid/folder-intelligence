use crate::agent::intent::{Goal, TaskIntent};
use crate::agent::analysis::TaskAnalysis;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationStrategy {
    CategoryBased,
    ProjectBased,
    Chronological,
    BySize,
    PreserveExisting,
    Custom(String),
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
            ProposedOperation::ArchiveFiles { category, file_count } => {
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

        let strategy = self.determine_strategy(intent);
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

        Ok(Recommendation {
            id,
            strategy,
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
    ) -> RecommendationStrategy {
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
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
        _strategy: &RecommendationStrategy,
    ) -> Vec<ProposedCategory> {
        let mut categories = Vec::new();
        let content_groups = &analysis.structure_summary.content_groups;
        let intent_purpose = match &intent.goal {
            Goal::Organize { purpose, .. } => purpose.as_str(),
            Goal::Reorganize {
                strategy: Some(s), ..
            } => s.as_str(),
            _ => "general_organization",
        };

        for group in content_groups {
            let purpose = match group.category {
                crate::agent::analysis::ContentType::Documents => "document_storage".to_string(),
                crate::agent::analysis::ContentType::Images => "image_storage".to_string(),
                crate::agent::analysis::ContentType::Archives => "archive_storage".to_string(),
                crate::agent::analysis::ContentType::Media => "media_storage".to_string(),
                crate::agent::analysis::ContentType::Code => "code_storage".to_string(),
                crate::agent::analysis::ContentType::Installers => "installer_storage".to_string(),
                crate::agent::analysis::ContentType::Data => "data_storage".to_string(),
                crate::agent::analysis::ContentType::Config => "config_storage".to_string(),
                crate::agent::analysis::ContentType::Other => "misc_storage".to_string(),
            };

            let target_path = analysis.candidate_categories.iter().find(|c| {
                c.name.to_lowercase().contains(&purpose) || c.name.to_lowercase().contains(&group.extension)
            }).map(|c| c.path.clone());

            let is_existing = target_path.is_some();

            categories.push(ProposedCategory {
                name: purpose.clone(),
                purpose,
                target_content_types: vec![group.extension.clone()],
                confidence: group.percentage_of_scope / 100.0,
                is_existing,
                target_path,
            });
        }

        if intent_purpose != "general_organization" && intent_purpose != "by_category" {
            if let Some(custom) = categories.first_mut() {
                custom.purpose = format!("{} ({})", custom.purpose, intent_purpose);
            }
        }

        categories
    }

    fn generate_operations(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
        proposed_categories: &[ProposedCategory],
    ) -> Vec<ProposedOperation> {
        let mut operations = Vec::new();
        let content_groups = &analysis.structure_summary.content_groups;

        for group in content_groups {
            let category_name = match group.category {
                crate::agent::analysis::ContentType::Documents => "document_storage",
                crate::agent::analysis::ContentType::Images => "image_storage",
                crate::agent::analysis::ContentType::Archives => "archive_storage",
                crate::agent::analysis::ContentType::Media => "media_storage",
                crate::agent::analysis::ContentType::Code => "code_storage",
                crate::agent::analysis::ContentType::Installers => "installer_storage",
                crate::agent::analysis::ContentType::Data => "data_storage",
                crate::agent::analysis::ContentType::Config => "config_storage",
                crate::agent::analysis::ContentType::Other => "misc_storage",
            };

            let has_existing = proposed_categories.iter().any(|c| c.name == category_name && c.is_existing);

            if !has_existing {
                operations.push(ProposedOperation::CreateCategory {
                    name: category_name.to_string(),
                    purpose: category_name.to_string(),
                });
            }

            operations.push(ProposedOperation::MoveCategory {
                strategy: self.strategy_name(intent),
                content_type: group.extension.clone(),
                file_count: group.file_count,
                to_category: category_name.to_string(),
            });
        }

        for candidate in &analysis.candidate_categories {
            let is_preserved = proposed_categories.iter().any(|c| {
                c.target_path.as_ref().map(|p| p == &candidate.path).unwrap_or(false)
            });
            if is_preserved {
                operations.push(ProposedOperation::PreserveDirectory {
                    path: candidate.path.clone(),
                });
            }
        }

        if intent.constraints.auto_delete_temps {
            let temps_count = analysis
                .scope_evidence
                .extension_histogram
                .iter()
                .filter(|(ext, _)| {
                    ext.as_str() == "tmp" || ext.as_str() == "temp" || ext.as_str() == "crdownload"
                })
                .map(|(_, count)| *count)
                .sum::<u64>();

            if temps_count > 0 {
                operations.push(ProposedOperation::ArchiveFiles {
                    category: "temp_files".to_string(),
                    file_count: temps_count,
                });
            }
        }

        if let Some(archive_duration) = &intent.constraints.archive_old {
            let archive_secs = archive_duration.as_secs();
            operations.push(ProposedOperation::ArchiveFiles {
                category: format!("files_older_than_{}s", archive_secs),
                file_count: 0,
            });
        }

        let leftover_count: u64 = analysis
            .classification_results
            .iter()
            .filter(|r| r.decision == crate::classification::ClassificationDecision::LeaveUnclassified)
            .map(|_r| {
                analysis
                    .content_groups
                    .iter()
                    .filter(|g| g.category == crate::agent::analysis::ContentType::Other)
                    .map(|g| g.file_count)
                    .sum::<u64>()
            })
            .sum();

        if leftover_count > 0 {
            operations.push(ProposedOperation::LeaveUnclassified {
                file_count: leftover_count,
                reason: "No matching category found".to_string(),
            });
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
            passed: preserve || !operations.iter().any(|op| matches!(op, ProposedOperation::PreserveDirectory { .. })),
            message: if preserve {
                "Existing directories will be preserved".to_string()
            } else {
                "Preserve constraint not explicitly set".to_string()
            },
        });

        checks.push(ConstraintCheck {
            name: "auto_delete_temps".to_string(),
            passed: auto_delete || !operations.iter().any(|op| {
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
            passed: archive || !operations.iter().any(|op| {
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
                    crate::agent::analysis::GapType::Taxonomy => {
                        ("What category structure do you prefer?".to_string(), vec![
                            "By file type".to_string(),
                            "By project".to_string(),
                            "By date".to_string(),
                        ])
                    }
                    crate::agent::analysis::GapType::FileDisposition => {
                        ("How should unclassified files be handled?".to_string(), vec![
                            "Leave in place".to_string(),
                            "Move to 'misc' folder".to_string(),
                            "Archive".to_string(),
                        ])
                    }
                    crate::agent::analysis::GapType::DuplicateHandling => {
                        ("How should duplicate files be handled?".to_string(), vec![
                            "Keep newer".to_string(),
                            "Keep larger".to_string(),
                            "Merge if identical".to_string(),
                        ])
                    }
                    crate::agent::analysis::GapType::ArchivePolicy => {
                        ("What is your archive policy for old files?".to_string(), vec![
                            "Archive after 90 days".to_string(),
                            "Archive after 1 year".to_string(),
                            "Do not archive".to_string(),
                        ])
                    }
                    crate::agent::analysis::GapType::ScopeBoundary => {
                        ("Should the scope include subdirectories?".to_string(), vec![
                            "Yes, include all".to_string(),
                            "No, top-level only".to_string(),
                        ])
                    }
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
        let total_files = analysis.structure_summary.total_files;
        if total_files == 0 {
            return 0.0;
        }

        let classified_files: u64 = analysis
            .classification_results
            .iter()
            .filter(|r| matches!(r.decision, crate::classification::ClassificationDecision::MoveExisting | crate::classification::ClassificationDecision::CreateCategory))
            .count() as u64;

        let mut confidence = if total_files > 0 {
            (classified_files as f64 / total_files as f64).min(1.0)
        } else {
            0.5
        };

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
            RecommendationStrategy::PreserveExisting => "preserve existing".to_string(),
            RecommendationStrategy::Custom(s) => s.clone(),
        };

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

        format!(
            "Analyzed {} directory ({} files, {} subdirectories). \
            Detected {} content type groups. \
            Strategy: {} (purpose: {}). \
            Proposed {} categories. \
            Classification confidence: {:.0}% of files matched to categories. \
            {} ambiguities and {} evidence gaps require attention.",
            analysis.scope_evidence.path.display(),
            total_files,
            total_dirs,
            num_groups,
            strategy_name,
            purpose,
            num_categories,
            analysis.classification_results.iter()
                .filter(|r| matches!(r.decision, crate::classification::ClassificationDecision::MoveExisting | crate::classification::ClassificationDecision::CreateCategory))
                .count() as f64 / total_files.max(1) as f64 * 100.0,
            analysis.ambiguities.len(),
            analysis.evidence_gaps.len(),
        )
    }

    fn strategy_name(&self, intent: &TaskIntent) -> String {
        match self.determine_strategy(intent) {
            RecommendationStrategy::CategoryBased => "category".to_string(),
            RecommendationStrategy::ProjectBased => "project".to_string(),
            RecommendationStrategy::Chronological => "chronological".to_string(),
            RecommendationStrategy::BySize => "size".to_string(),
            RecommendationStrategy::PreserveExisting => "preserve".to_string(),
            RecommendationStrategy::Custom(s) => s,
        }
    }
}

#[cfg(test)]
#[path = "recommendation_tests.rs"]
mod tests;