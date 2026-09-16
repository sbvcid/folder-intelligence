use crate::classification::comparator::ComparisonType;
use crate::classification::comparator::{EvidenceComparator, EvidenceComparison};
use crate::classification::input::{CandidateEvidence, ClassificationConfig, TargetEvidence};

const MAX_SCORE: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct CandidateScore {
    pub candidate_path: std::path::PathBuf,
    pub candidate_name: String,
    pub composite_score: f64,
    pub comparisons: Vec<EvidenceComparison>,
    pub candidate_partial_scan: bool,
}

pub struct CandidateSelector;

impl CandidateSelector {
    pub fn evaluate_candidates(
        target: &TargetEvidence,
        candidates: &[CandidateEvidence],
        config: &ClassificationConfig,
    ) -> Vec<CandidateScore> {
        let mut scores = Vec::new();

        // Limit candidates to max_candidate_children for bounded evaluation
        let bounded_candidates = candidates.iter().take(config.max_candidate_children);

        for candidate in bounded_candidates {
            let comparisons = EvidenceComparator::compare(target, candidate);

            let mut ext_score: f64 = 0.0;
            let mut struct_score: f64 = 0.0;
            let mut id_score: f64 = 0.0;
            let mut count_score: f64 = 0.0;

            for comp in &comparisons {
                match comp.comparison_type {
                    ComparisonType::ExtensionSimilarity => ext_score = comp.score,
                    ComparisonType::StructureSimilarity => struct_score = comp.score,
                    ComparisonType::IdentifierOverlap | ComparisonType::IdentifierTypeMatch => {
                        id_score = id_score.max(comp.score);
                    }
                    ComparisonType::FileCountSimilarity => count_score = comp.score,
                    _ => {}
                }
            }

            let composite = (ext_score * config.extension_weight)
                + (struct_score * config.structure_weight)
                + (id_score * config.identifier_weight)
                + (count_score * config.file_count_weight);

            let normalized = composite.clamp(0.0, MAX_SCORE);

            scores.push(CandidateScore {
                candidate_path: candidate.directory.path.clone(),
                candidate_name: candidate.directory.name.clone(),
                composite_score: normalized,
                comparisons,
                candidate_partial_scan: candidate.directory.partial_scan,
            });
        }

        scores.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scores
    }
}
