use crate::agent::recommendation::{OrganizationProposal, ProposedCategory};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalRefinementError {
    CategoryNotFound(String),
    CategoryAlreadyExists(String),
    FileNotFound(String),
    FileAlreadyInCategory { file: PathBuf, category: String },
}

impl std::fmt::Display for ProposalRefinementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalRefinementError::CategoryNotFound(name) => {
                write!(f, "Category not found: {}", name)
            }
            ProposalRefinementError::CategoryAlreadyExists(name) => {
                write!(f, "Category already exists: {}", name)
            }
            ProposalRefinementError::FileNotFound(file) => {
                write!(
                    f,
                    "File not found in proposal: {}",
                    Path::new(file).display()
                )
            }
            ProposalRefinementError::FileAlreadyInCategory { file, category } => {
                write!(
                    f,
                    "File {} is already in category {}",
                    file.display(),
                    category
                )
            }
        }
    }
}

impl std::error::Error for ProposalRefinementError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalRefinement {
    RenameCategory { from: String, to: String },
    RemoveCategory { name: String },
    AddCategory { name: String, purpose: String },
    MoveFileToCategory { file: PathBuf, category: String },
}

pub struct ProposalRefiner;

impl ProposalRefiner {
    pub fn refine(
        &self,
        proposal: &OrganizationProposal,
        refinement: &ProposalRefinement,
    ) -> Result<OrganizationProposal, ProposalRefinementError> {
        match refinement {
            ProposalRefinement::RenameCategory { from, to } => {
                Self::rename_category(proposal, from, to)
            }
            ProposalRefinement::RemoveCategory { name } => Self::remove_category(proposal, name),
            ProposalRefinement::AddCategory { name, purpose } => {
                Self::add_category(proposal, name, purpose)
            }
            ProposalRefinement::MoveFileToCategory { file, category } => {
                Self::move_file_to_category(proposal, file, category)
            }
        }
    }

    fn rename_category(
        proposal: &OrganizationProposal,
        from: &str,
        to: &str,
    ) -> Result<OrganizationProposal, ProposalRefinementError> {
        if !proposal.proposed_categories.iter().any(|c| c.name == from) {
            return Err(ProposalRefinementError::CategoryNotFound(from.to_string()));
        }

        if proposal.proposed_categories.iter().any(|c| c.name == to) {
            return Err(ProposalRefinementError::CategoryAlreadyExists(
                to.to_string(),
            ));
        }

        let mut revised = proposal.clone();
        for cat in &mut revised.proposed_categories {
            if cat.name == from {
                cat.name = to.to_string();
            }
        }

        Ok(revised)
    }

    fn remove_category(
        proposal: &OrganizationProposal,
        name: &str,
    ) -> Result<OrganizationProposal, ProposalRefinementError> {
        let found = proposal.proposed_categories.iter().any(|c| c.name == name);
        if !found {
            return Err(ProposalRefinementError::CategoryNotFound(name.to_string()));
        }

        let mut revised = proposal.clone();
        revised.proposed_categories.retain(|c| c.name != name);

        Ok(revised)
    }

    fn add_category(
        proposal: &OrganizationProposal,
        name: &str,
        purpose: &str,
    ) -> Result<OrganizationProposal, ProposalRefinementError> {
        if proposal.proposed_categories.iter().any(|c| c.name == name) {
            return Err(ProposalRefinementError::CategoryAlreadyExists(
                name.to_string(),
            ));
        }

        let mut revised = proposal.clone();
        revised.proposed_categories.push(ProposedCategory {
            name: name.to_string(),
            purpose: purpose.to_string(),
            target_content_types: Vec::new(),
            confidence: 0.0,
            is_existing: false,
            target_path: None,
            source_files: Vec::new(),
        });

        Ok(revised)
    }

    fn move_file_to_category(
        proposal: &OrganizationProposal,
        file: &Path,
        category: &str,
    ) -> Result<OrganizationProposal, ProposalRefinementError> {
        let target_exists = proposal
            .proposed_categories
            .iter()
            .any(|c| c.name == category);
        if !target_exists {
            return Err(ProposalRefinementError::CategoryNotFound(
                category.to_string(),
            ));
        }

        let file_name = file.file_name().ok_or_else(|| {
            ProposalRefinementError::FileNotFound(file.to_string_lossy().to_string())
        })?;

        let source_cat_idx = proposal.proposed_categories.iter().position(|cat| {
            cat.source_files
                .iter()
                .any(|sf| sf.file_name() == Some(file_name))
        });

        let src_idx = match source_cat_idx {
            Some(idx) => idx,
            None => {
                return Err(ProposalRefinementError::FileNotFound(
                    file.to_string_lossy().to_string(),
                ));
            }
        };

        if proposal.proposed_categories[src_idx].name == category {
            return Err(ProposalRefinementError::FileAlreadyInCategory {
                file: file.to_path_buf(),
                category: category.to_string(),
            });
        }

        let mut revised = proposal.clone();

        let src_cat = &mut revised.proposed_categories[src_idx];
        let moved_file = src_cat
            .source_files
            .iter()
            .find(|sf| sf.file_name() == Some(file_name))
            .cloned();
        src_cat
            .source_files
            .retain(|sf| sf.file_name() != Some(file_name));

        let target_idx = revised
            .proposed_categories
            .iter()
            .position(|c| c.name == category)
            .unwrap();

        if let Some(f) = moved_file {
            revised.proposed_categories[target_idx].source_files.push(f);
        }

        Ok(revised)
    }
}

#[cfg(test)]
#[path = "proposal_refiner_tests.rs"]
mod tests;
