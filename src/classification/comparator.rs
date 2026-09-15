use crate::evidence::DirectoryEvidence;
use crate::classification::input::{CandidateEvidence, TargetEvidence};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonType {
    ExtensionSimilarity,
    FileCountSimilarity,
    StructureSimilarity,
    IdentifierOverlap,
    IdentifierTypeMatch,
    DepthSimilarity,
    EmptyMatch,
    SemanticSimilarity,
    NamingPatternSimilarity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceComparison {
    pub comparison_type: ComparisonType,
    pub score: f64, // 0.0 to 1.0
    pub observed: String,
}

pub struct EvidenceComparator;

impl EvidenceComparator {
    pub fn compare(target: &TargetEvidence, candidate: &CandidateEvidence) -> Vec<EvidenceComparison> {
        let mut comparisons = Vec::new();

        // Build aggregate evidence from candidate directory + its children
        let agg_ext_hist = Self::aggregate_extensions(&candidate.directory, &candidate.children);
        let agg_file_count = Self::aggregate_file_count(&candidate.directory, &candidate.children);
        let agg_child_dir_names = Self::aggregate_child_dir_names(&candidate.directory, &candidate.children);
        let agg_identifiers = Self::aggregate_identifiers(&candidate.directory, &candidate.children);
        let agg_id_types = Self::aggregate_id_types(&candidate.directory, &candidate.children);

        // 1. Extension similarity (Jaccard on aggregated extension histograms)
        let ext_score = Self::compare_extensions(&target.evidence.extension_histogram, &agg_ext_hist);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::ExtensionSimilarity,
            score: ext_score,
            observed: format!("Extension distribution similarity score: {:.2}", ext_score),
        });

        // 2. File count similarity
        let count_score = Self::compare_file_counts(target.evidence.file_count, agg_file_count);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::FileCountSimilarity,
            score: count_score,
            observed: format!("File count similarity score: {:.2} (target: {}, candidate: {})", count_score, target.evidence.file_count, agg_file_count),
        });

        // 3. Structure similarity (child directory names / subfolder profile)
        let struct_score = Self::compare_structure(&target.evidence.child_directory_names, &agg_child_dir_names);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::StructureSimilarity,
            score: struct_score,
            observed: format!("Structure similarity score: {:.2}", struct_score),
        });

        // 4. Identifier overlap
        let id_overlap = Self::compare_identifiers(&target.evidence.syntactic_identifiers, &agg_identifiers);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::IdentifierOverlap,
            score: id_overlap,
            observed: format!("Syntactic identifier overlap score: {:.2}", id_overlap),
        });

        // 5. Identifier type match
        let type_match = Self::compare_identifier_types(&target.evidence.identifier_summary.by_type, &agg_id_types);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::IdentifierTypeMatch,
            score: type_match,
            observed: format!("Identifier type profile match score: {:.2}", type_match),
        });

        // 6. Depth similarity
        let depth_score = Self::compare_depth(target.evidence.depth, &candidate.directory);
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::DepthSimilarity,
            score: depth_score,
            observed: format!("Tree depth similarity score: {:.2} (target depth: {})", depth_score, target.evidence.depth),
        });

        // 7. Empty match
        let empty_score = if target.evidence.is_empty && candidate.directory.is_empty {
            1.0
        } else if target.evidence.is_empty == candidate.directory.is_empty {
            0.5
        } else {
            0.0
        };
        comparisons.push(EvidenceComparison {
            comparison_type: ComparisonType::EmptyMatch,
            score: empty_score,
            observed: format!("Empty state match: target is_empty={}, candidate is_empty={}", target.evidence.is_empty, candidate.directory.is_empty),
        });

        comparisons
    }

    // --- Aggregation helpers: combine candidate directory + children ---

    fn aggregate_extensions(
        dir: &DirectoryEvidence,
        children: &[DirectoryEvidence],
    ) -> HashMap<String, u64> {
        let mut hist: HashMap<String, u64> = HashMap::new();
        for (ext, count) in &dir.extension_histogram {
            *hist.entry(ext.clone()).or_insert(0) += count;
        }
        for child in children {
            for (ext, count) in &child.extension_histogram {
                *hist.entry(ext.clone()).or_insert(0) += count;
            }
        }
        hist
    }

    fn aggregate_file_count(
        dir: &DirectoryEvidence,
        children: &[DirectoryEvidence],
    ) -> u64 {
        let child_total: u64 = children.iter().map(|c| c.file_count).sum();
        dir.file_count + child_total
    }

    fn aggregate_child_dir_names(
        dir: &DirectoryEvidence,
        children: &[DirectoryEvidence],
    ) -> Vec<String> {
        let mut names = dir.child_directory_names.clone();
        for child in children {
            names.extend(child.child_directory_names.iter().cloned());
        }
        names
    }

    fn aggregate_identifiers(
        dir: &DirectoryEvidence,
        children: &[DirectoryEvidence],
    ) -> Vec<crate::evidence::SyntacticIdentifier> {
        let mut ids = dir.syntactic_identifiers.clone();
        for child in children {
            ids.extend(child.syntactic_identifiers.iter().cloned());
        }
        ids
    }

    fn aggregate_id_types(
        dir: &DirectoryEvidence,
        children: &[DirectoryEvidence],
    ) -> HashMap<crate::evidence::IdentifierType, usize> {
        let mut types: HashMap<crate::evidence::IdentifierType, usize> = HashMap::new();
        for (t, count) in &dir.identifier_summary.by_type {
            *types.entry(t.clone()).or_insert(0) += count;
        }
        for child in children {
            for (t, count) in &child.identifier_summary.by_type {
                *types.entry(t.clone()).or_insert(0) += count;
            }
        }
        types
    }

    fn compare_extensions(target_hist: &HashMap<String, u64>, candidate_hist: &HashMap<String, u64>) -> f64 {
        if target_hist.is_empty() && candidate_hist.is_empty() {
            return 1.0;
        }
        if target_hist.is_empty() || candidate_hist.is_empty() {
            return 0.0;
        }

        let mut intersection = 0.0;
        let mut union = 0.0;

        let all_keys: std::collections::HashSet<_> = target_hist.keys().chain(candidate_hist.keys()).collect();
        for ext in all_keys {
            let t_count = *target_hist.get(ext).unwrap_or(&0) as f64;
            let c_count = *candidate_hist.get(ext).unwrap_or(&0) as f64;
            intersection += t_count.min(c_count);
            union += t_count.max(c_count);
        }

        if union == 0.0 { 0.0 } else { intersection / union }
    }

    fn compare_file_counts(target_count: u64, candidate_total: u64) -> f64 {
        if candidate_total == 0 && target_count == 0 {
            return 1.0;
        }
        if candidate_total == 0 {
            return 0.0;
        }
        let ratio = target_count as f64 / candidate_total as f64;
        // Score based on ratio — 1.0 when equal, falls off as ratio diverges
        let score = if ratio >= 1.0 {
            1.0 / ratio
        } else {
            ratio
        };
        score.clamp(0.0, 1.0)
    }

    fn compare_structure(target_names: &[String], candidate_names: &[String]) -> f64 {
        if target_names.is_empty() && candidate_names.is_empty() {
            return 1.0;
        }
        let candidate_set: std::collections::HashSet<String> = candidate_names.iter().cloned().collect();
        let target_set: std::collections::HashSet<String> = target_names.iter().cloned().collect();
        if target_set.is_empty() && candidate_set.is_empty() {
            return 1.0;
        }
        let intersection = target_set.iter().filter(|n| candidate_set.contains(n.as_str())).count() as f64;
        let union = target_set.len() + candidate_set.len() - intersection as usize;
        if union == 0 { 0.0 } else { intersection / union as f64 }
    }

    fn compare_identifiers(target_ids: &[crate::evidence::SyntacticIdentifier], candidate_ids: &[crate::evidence::SyntacticIdentifier]) -> f64 {
        let candidate_set: std::collections::HashSet<_> = candidate_ids.iter().map(|id| id.value.clone()).collect();
        let target_set: std::collections::HashSet<_> = target_ids.iter().map(|id| id.value.clone()).collect();
        if target_set.is_empty() && candidate_set.is_empty() {
            return 0.5;
        }
        let intersection = target_set.iter().filter(|v| candidate_set.contains(*v)).count() as f64;
        let union = target_set.len() + candidate_set.len() - intersection as usize;
        if union == 0 { 0.5 } else { intersection / union as f64 }
    }

    fn compare_identifier_types(target_types: &HashMap<crate::evidence::IdentifierType, usize>, candidate_types: &HashMap<crate::evidence::IdentifierType, usize>) -> f64 {
        if target_types.is_empty() && candidate_types.is_empty() {
            return 0.5;
        }
        let all_keys: std::collections::HashSet<_> = target_types.keys().chain(candidate_types.keys()).collect();
        let mut intersection = 0.0;
        let mut union = 0.0;
        for t in all_keys {
            let t_cnt = *target_types.get(t).unwrap_or(&0) as f64;
            let c_cnt = *candidate_types.get(t).unwrap_or(&0) as f64;
            intersection += t_cnt.min(c_cnt);
            union += t_cnt.max(c_cnt);
        }
        if union == 0.0 { 0.5 } else { intersection / union }
    }

    fn compare_depth(target_depth: usize, candidate_dir: &DirectoryEvidence) -> f64 {
        let diff = (target_depth as i32 - candidate_dir.depth as i32).abs();
        match diff {
            0 => 1.0,
            1 => 0.8,
            2 => 0.5,
            _ => 0.2,
        }
    }
}
