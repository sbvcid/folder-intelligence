use crate::agent::analysis::{CandidateCategory, TaskAnalysis};
use crate::agent::plan::{FileSystemOperation, OperationPlan, PlanError};
use crate::agent::recommendation::{OrganizationProposal, ProposedCategory, Recommendation};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    MissingFile,
    Ambiguous,
    AlreadyOrganized,
    DestinationOutsideScope,
    InvalidDestination,
    ConflictingDestination,
}

impl std::fmt::Display for UnresolvedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnresolvedReason::MissingFile => write!(f, "Missing file"),
            UnresolvedReason::Ambiguous => write!(f, "Ambiguous category"),
            UnresolvedReason::AlreadyOrganized => write!(f, "Already organized"),
            UnresolvedReason::DestinationOutsideScope => write!(f, "Destination outside scope"),
            UnresolvedReason::InvalidDestination => write!(f, "Invalid destination"),
            UnresolvedReason::ConflictingDestination => write!(f, "Conflicting destination"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedItem {
    pub file: PathBuf,
    pub category: String,
    pub reason: UnresolvedReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProposalConversionResult {
    pub operation_plan: OperationPlan,
    pub unresolved: Vec<UnresolvedItem>,
}

pub struct ProposalConverter;

impl ProposalConverter {
    pub fn convert(
        &self,
        proposal: &OrganizationProposal,
        analysis: &TaskAnalysis,
        scope: &Path,
    ) -> Result<(Vec<FileSystemOperation>, Vec<UnresolvedItem>), PlanError> {
        let mut operations = Vec::new();
        let mut unresolved = Vec::new();
        let mut created_dirs: HashSet<PathBuf> = HashSet::new();

        // Pass 1: Pre-detect file claims across all categories to identify ambiguity
        // Map: absolute source file path -> Vec<(category_name, dest_dir, filename)>
        let mut file_requests: HashMap<PathBuf, Vec<(String, PathBuf, PathBuf)>> = HashMap::new();

        for cat in &proposal.proposed_categories {
            let dest_dir = self.resolve_category_dir(scope, &analysis.candidate_categories, cat);

            if !ProposalConverter::is_within_scope(&dest_dir, scope) {
                for file_path in &cat.source_files {
                    let source = if file_path.is_absolute() {
                        file_path.clone()
                    } else {
                        scope.join(file_path)
                    };
                    unresolved.push(UnresolvedItem {
                        file: source,
                        category: cat.name.clone(),
                        reason: UnresolvedReason::DestinationOutsideScope,
                    });
                }
                continue;
            }

            // Deduplicate source files within the same category to prevent self-ambiguity
            let unique_files: HashSet<&PathBuf> = cat.source_files.iter().collect();

            for file_path in unique_files {
                let filename = file_path
                    .file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| file_path.clone());
                let source = if file_path.is_absolute() {
                    file_path.clone()
                } else {
                    scope.join(file_path)
                };

                if !ProposalConverter::is_within_scope(&source, scope) {
                    unresolved.push(UnresolvedItem {
                        file: source,
                        category: cat.name.clone(),
                        reason: UnresolvedReason::DestinationOutsideScope,
                    });
                    continue;
                }

                file_requests.entry(source).or_default().push((
                    cat.name.clone(),
                    dest_dir.clone(),
                    filename,
                ));
            }
        }

        let mut dest_files: HashSet<PathBuf> = HashSet::new();

        // Pass 2: Evaluate each file request deterministically against filesystem evidence
        let mut sorted_sources: Vec<PathBuf> = file_requests.keys().cloned().collect();
        sorted_sources.sort();

        for source in sorted_sources {
            let requests = file_requests.remove(&source).unwrap();
            let unique_categories: HashSet<&String> =
                requests.iter().map(|(cat, _, _)| cat).collect();
            if unique_categories.len() > 1 {
                for (cat_name, _, _) in requests {
                    unresolved.push(UnresolvedItem {
                        file: source.clone(),
                        category: cat_name,
                        reason: UnresolvedReason::Ambiguous,
                    });
                }
                continue;
            }

            let (cat_name, dest_dir, filename) = requests.into_iter().next().unwrap();

            if !source.exists() || source.is_dir() {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::MissingFile,
                });
                continue;
            }

            let dest = dest_dir.join(&filename);

            if !ProposalConverter::is_within_scope(&dest, scope) {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::DestinationOutsideScope,
                });
                continue;
            }

            if source == dest {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::AlreadyOrganized,
                });
                continue;
            }

            if dest.exists() {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::ConflictingDestination,
                });
                continue;
            }

            if !dest_files.insert(dest.clone()) {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::ConflictingDestination,
                });
                continue;
            }

            if !created_dirs.contains(&dest_dir) && !dest_dir.exists() {
                operations.push(FileSystemOperation::CreateDir {
                    path: dest_dir.clone(),
                });
                created_dirs.insert(dest_dir.clone());
            }

            operations.push(FileSystemOperation::Move { source, dest });
        }

        Ok((operations, unresolved))
    }

    pub fn convert_to_plan(
        &self,
        proposal: &OrganizationProposal,
        recommendation: &Recommendation,
        analysis: &TaskAnalysis,
        scope: &Path,
    ) -> Result<ProposalConversionResult, PlanError> {
        let (operations, unresolved) = self.convert(proposal, analysis, scope)?;
        let mut estimated = crate::agent::plan::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        };
        for op in &operations {
            match op {
                FileSystemOperation::Move { .. } => estimated.files_moved += 1,
                FileSystemOperation::CreateDir { .. } => estimated.dirs_created += 1,
                FileSystemOperation::Delete { .. } => estimated.files_deleted += 1,
            }
        }
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let plan_id = format!("plan-{}", created_at);
        let operation_plan = OperationPlan {
            id: plan_id,
            recommendation_id: recommendation.id.clone(),
            scope: scope.to_path_buf(),
            operations,
            estimated_impact: estimated,
            validation_warnings: Vec::new(),
            has_conflicts: false,
            unresolved_proposals: unresolved.clone(),
            dry_run: true,
            created_at,
            validation_context: Some(crate::agent::plan::PlanValidationContext::default()),
        };
        Ok(ProposalConversionResult {
            operation_plan,
            unresolved,
        })
    }

    fn resolve_category_dir(
        &self,
        scope: &Path,
        candidates: &[CandidateCategory],
        cat: &ProposedCategory,
    ) -> PathBuf {
        if let Some(ref target) = cat.target_path {
            if target.is_absolute() {
                return target.clone();
            } else {
                return scope.join(target);
            }
        }
        if let Some(candidate) = candidates.iter().find(|c| c.name == cat.name) {
            return candidate.path.clone();
        }
        let human_name = Self::category_to_human_name(&cat.name);
        scope.join(human_name)
    }

    fn category_to_human_name(category: &str) -> String {
        match category {
            "document_storage" => "Documents",
            "image_storage" => "Images",
            "archive_storage" => "Archives",
            "media_storage" => "Media",
            "code_storage" => "Code",
            "installer_storage" => "Installers",
            "data_storage" => "Data",
            "config_storage" => "Config",
            "misc_storage" => "Misc",
            "temp_files" => "Temp",
            _ => category,
        }
        .to_string()
    }

    fn is_within_scope(path: &Path, scope: &Path) -> bool {
        let mut scope_stack = Vec::new();
        for c in scope.components() {
            match c {
                std::path::Component::Normal(s) => scope_stack.push(s),
                _ => {}
            }
        }

        let mut path_stack = Vec::new();
        for c in path.components() {
            match c {
                std::path::Component::Normal(s) => path_stack.push(s),
                std::path::Component::ParentDir => {
                    path_stack.pop();
                }
                _ => {}
            }
        }

        if path_stack.len() < scope_stack.len() {
            return false;
        }
        path_stack[..scope_stack.len()] == scope_stack[..]
    }
}
