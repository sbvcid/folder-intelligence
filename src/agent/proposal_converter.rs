use crate::agent::analysis::{CandidateCategory, TaskAnalysis};
use crate::agent::plan::{FileSystemOperation, PlanError};
use crate::agent::recommendation::OrganizationProposal;
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedItem {
    pub file: PathBuf,
    pub category: String,
    pub reason: UnresolvedReason,
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
            let dest_dir =
                self.resolve_category_dir(scope, &analysis.candidate_categories, &cat.name);

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
                    scope.join(&filename)
                };

                file_requests.entry(source).or_default().push((
                    cat.name.clone(),
                    dest_dir.clone(),
                    filename,
                ));
            }
        }

        let mut dest_files: HashSet<PathBuf> = HashSet::new();

        // Pass 2: Evaluate each file request deterministically against filesystem evidence
        for (source, requests) in file_requests {
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

            if !source.exists() {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::MissingFile,
                });
                continue;
            }

            let dest = dest_dir.join(&filename);

            if source == dest {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::AlreadyOrganized,
                });
                continue;
            }

            if !dest_files.insert(dest.clone()) {
                unresolved.push(UnresolvedItem {
                    file: source,
                    category: cat_name,
                    reason: UnresolvedReason::InvalidDestination,
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

    fn resolve_category_dir(
        &self,
        scope: &Path,
        candidates: &[CandidateCategory],
        category_name: &str,
    ) -> PathBuf {
        if let Some(candidate) = candidates.iter().find(|c| c.name == category_name) {
            return candidate.path.clone();
        }
        let human_name = Self::category_to_human_name(category_name);
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
