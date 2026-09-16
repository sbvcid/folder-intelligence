use crate::evidence::{DirectoryEvidence, ScanMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const SAMPLE_CHILD_DIRS: usize = 10;
pub const MAX_CANDIDATES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserHints {
    pub category_hint: Option<String>,
    pub force_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TargetEvidence {
    pub evidence: DirectoryEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_hints: Option<UserHints>,
}

/// Full classification input as specified in Phase 3 design
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ClassificationInput {
    pub target: DirectoryEvidence,
    pub candidates: Vec<CandidateEvidence>,
    pub metadata: ScanMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CandidateSummary {
    pub total_children: usize,
    pub top_extensions: Vec<String>,
    pub common_identifier_types: Vec<String>,
    pub avg_file_count: f64,
}

/// Evidence for a candidate directory and a bounded sample of its children
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CandidateEvidence {
    /// Full DirectoryEvidence for the candidate category
    pub directory: DirectoryEvidence,
    /// Bounded sample of child directory evidence (precedent examples)
    pub children: Vec<DirectoryEvidence>,
    /// Aggregate summary of candidate children
    pub summary: CandidateSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ClassificationConfig {
    pub min_confidence_move: f64,
    pub min_confidence_create: f64,
    pub max_candidate_children: usize,
    pub extension_weight: f64,
    pub structure_weight: f64,
    pub identifier_weight: f64,
    pub file_count_weight: f64,
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self {
            min_confidence_move: 0.80,
            min_confidence_create: 0.70,
            max_candidate_children: MAX_CANDIDATES,
            extension_weight: 0.40,
            structure_weight: 0.25,
            identifier_weight: 0.25,
            file_count_weight: 0.10,
        }
    }
}

impl ClassificationInput {
    /// Build a ClassificationInput from target evidence and candidate directories.
    /// Each candidate directory is scanned and its children are sampled (bounded).
    pub fn from_directory_evidence(
        target: DirectoryEvidence,
        candidate_dirs: &[PathBuf],
        metadata: ScanMetadata,
    ) -> anyhow::Result<Self> {
        let mut candidates = Vec::new();
        for candidate_dir in candidate_dirs {
            if !candidate_dir.is_dir() {
                continue;
            }
            let scanner = crate::scanner::Scanner::new(candidate_dir);
            let scan_result = scanner.scan()?;
            if scan_result.evidence.is_empty() {
                continue;
            }
            let mut evidence_iter = scan_result.evidence.into_iter();
            let directory = evidence_iter.next().unwrap();
            // Sample children, capped at SAMPLE_CHILD_DIRS
            let children: Vec<_> = evidence_iter.take(SAMPLE_CHILD_DIRS).collect();
            let total_children = children.len();

            let mut ext_counts: HashMap<String, u64> = HashMap::new();
            let mut file_count_sum: u64 = 0;
            let mut id_type_counts: HashMap<crate::evidence::IdentifierType, usize> =
                HashMap::new();
            for c in &children {
                file_count_sum += c.file_count;
                for (ext, cnt) in &c.extension_histogram {
                    *ext_counts.entry(ext.clone()).or_insert(0) += cnt;
                }
                for (it, cnt) in &c.identifier_summary.by_type {
                    *id_type_counts.entry(it.clone()).or_insert(0) += cnt;
                }
            }

            let mut top_exts: Vec<_> = ext_counts.into_iter().collect();
            top_exts.sort_by_key(|a| std::cmp::Reverse(a.1));
            let top_extensions = top_exts.into_iter().take(5).map(|(e, _)| e).collect();

            let mut type_counts: Vec<_> = id_type_counts.into_iter().collect();
            type_counts.sort_by_key(|a| std::cmp::Reverse(a.1));
            let common_identifier_types = type_counts
                .into_iter()
                .take(3)
                .map(|(it, _)| format!("{:?}", it))
                .collect();

            let avg_file_count = if total_children > 0 {
                file_count_sum as f64 / total_children as f64
            } else {
                0.0
            };

            candidates.push(CandidateEvidence {
                directory,
                children,
                summary: CandidateSummary {
                    total_children,
                    top_extensions,
                    common_identifier_types,
                    avg_file_count,
                },
            });
        }

        // Cap at MAX_CANDIDATES
        candidates.truncate(MAX_CANDIDATES);

        Ok(ClassificationInput {
            target,
            candidates,
            metadata,
        })
    }
}
