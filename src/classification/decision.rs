use crate::classification::input::{CandidateEvidence, ClassificationConfig, TargetEvidence};
use crate::classification::result::{
    AlternativeCandidate, ClassificationDecision, ClassificationResult, ConfidenceBand,
    SupportingEvidence, UncertaintyReason, Warning,
};
use crate::classification::selector::CandidateSelector;

const MIN_SCORE_CREATE_CATEGORY: f64 = 0.3;
const MIN_SCORE_LEAVE_UNCLASSIFIED: f64 = 0.4;
const MIN_SCORE_MOVE_EXISTING: f64 = 0.8;
const SCORE_GAP_FOR_MOVE: f64 = 0.1;
const SCORE_GAP_AMBIGUOUS: f64 = 0.05;
const PARTIAL_SCAN_PENALTY: f64 = 0.2;

pub struct DecisionEngine;

impl DecisionEngine {
    pub fn classify(
        target: &TargetEvidence,
        candidates: &[CandidateEvidence],
        config: &ClassificationConfig,
    ) -> ClassificationResult {
        let candidate_scores = CandidateSelector::evaluate_candidates(target, candidates, config);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let candidates_considered = candidate_scores.len();
        let mut warnings: Vec<Warning> = Vec::new();

        // Check for partial_scan on target
        let target_partial = target.evidence.partial_scan;
        if target_partial {
            warnings.push(Warning::PartialScanTarget);
        }

        // Build result for no candidates case
        if candidate_scores.is_empty() {
            let decision = if target.evidence.is_empty {
                ClassificationDecision::LeaveUnclassified
            } else if target_partial {
                ClassificationDecision::AskUser
            } else {
                ClassificationDecision::CreateCategory
            };

            let proposed = if matches!(decision, ClassificationDecision::CreateCategory) {
                Some(target.evidence.name.clone())
            } else {
                None
            };

            return ClassificationResult {
                target_path: target.evidence.path.clone(),
                decision,
                selected_candidate: None,
                proposed_category_name: proposed,
                confidence: 0.0,
                confidence_band: ConfidenceBand::Low,
                candidates_considered: 0,
                supporting_evidence: vec![],
                alternatives: vec![],
                uncertainty: vec![UncertaintyReason::InsufficientPrecedent],
                classification_reason: None,
                warnings,
                classified_at: now,
                schema_version: "3.0.0".to_string(),
                provider: None,
                model: None,
            };
        }

        // Apply partial_scan penalty to candidates
        let mut penalized_scores: Vec<_> = candidate_scores
            .into_iter()
            .map(|mut cs| {
                if cs.candidate_partial_scan {
                    warnings.push(Warning::PartialScanCandidate);
                    cs.composite_score = (cs.composite_score - PARTIAL_SCAN_PENALTY).max(0.0);
                    cs.candidate_partial_scan = false; // already recorded
                }
                cs
            })
            .collect();

        // Re-sort after penalty
        penalized_scores.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best = &penalized_scores[0];
        let second_best = penalized_scores.get(1);

        let best_score = best.composite_score;
        let second_score = second_best.map(|s| s.composite_score).unwrap_or(-1.0);
        let score_gap = best_score - second_score;

        let mut uncertainty: Vec<UncertaintyReason> = Vec::new();

        // Determine ambiguity
        let is_ambiguous = second_best.is_some()
            && score_gap < SCORE_GAP_AMBIGUOUS
            && best_score >= MIN_SCORE_LEAVE_UNCLASSIFIED;

        if is_ambiguous {
            uncertainty.push(UncertaintyReason::AmbiguousCandidates);
        }

        if best_score < MIN_SCORE_LEAVE_UNCLASSIFIED {
            uncertainty.push(UncertaintyReason::LowConfidence);
        }

        // Decision logic per Phase 3 spec
        let decision = if target_partial {
            // Never MOVE_EXISTING when target has partial scan
            if best_score >= MIN_SCORE_LEAVE_UNCLASSIFIED {
                ClassificationDecision::AskUser
            } else if target.evidence.is_empty {
                ClassificationDecision::LeaveUnclassified
            } else {
                ClassificationDecision::CreateCategory
            }
        } else if best_score >= MIN_SCORE_MOVE_EXISTING && score_gap >= SCORE_GAP_FOR_MOVE {
            ClassificationDecision::MoveExisting
        } else if is_ambiguous {
            ClassificationDecision::AskUser
        } else if best_score >= MIN_SCORE_CREATE_CATEGORY || target.evidence.is_empty {
            ClassificationDecision::LeaveUnclassified
        } else {
            ClassificationDecision::CreateCategory
        };

        // Build supporting evidence from best candidate's comparisons
        let mut supporting_evidence = Vec::new();
        for comp in &best.comparisons {
            if comp.score > 0.4 {
                supporting_evidence.push(SupportingEvidence {
                    evidence_type: format!("{:?}", comp.comparison_type),
                    description: comp.observed.clone(),
                    score: comp.score,
                });
            }
        }

        // Build alternatives — include all candidates that scored above the create threshold
        let mut alternatives = Vec::new();
        for other in penalized_scores.iter().skip(1) {
            if other.composite_score >= MIN_SCORE_CREATE_CATEGORY {
                alternatives.push(AlternativeCandidate {
                    candidate_name: other.candidate_name.clone(),
                    candidate_path: other.candidate_path.clone(),
                    score: other.composite_score,
                    rejection_reason: format!(
                        "Lower score ({:.2} vs top {:.2})",
                        other.composite_score, best_score
                    ),
                });
            }
        }

        // If LEAVE_UNCLASSIFIED, alternatives should list all candidates above 0.3
        if matches!(decision, ClassificationDecision::LeaveUnclassified) {
            for other in penalized_scores.iter().skip(1) {
                if other.composite_score >= MIN_SCORE_CREATE_CATEGORY
                    && !alternatives
                        .iter()
                        .any(|a| a.candidate_path == other.candidate_path)
                {
                    alternatives.push(AlternativeCandidate {
                        candidate_name: other.candidate_name.clone(),
                        candidate_path: other.candidate_path.clone(),
                        score: other.composite_score,
                        rejection_reason: format!(
                            "Score {:.2} below leave threshold of {}",
                            other.composite_score, MIN_SCORE_LEAVE_UNCLASSIFIED
                        ),
                    });
                }
            }
        }

        let selected_candidate = if matches!(decision, ClassificationDecision::MoveExisting) {
            Some(best.candidate_path.clone())
        } else {
            None
        };

        let proposed_category_name = if matches!(decision, ClassificationDecision::CreateCategory) {
            Some(best.candidate_name.clone())
        } else {
            None
        };

        let confidence = if matches!(decision, ClassificationDecision::MoveExisting)
            || matches!(decision, ClassificationDecision::AskUser)
        {
            best_score
        } else {
            0.0
        };

        ClassificationResult {
            target_path: target.evidence.path.clone(),
            decision,
            selected_candidate,
            proposed_category_name,
            confidence,
            confidence_band: ConfidenceBand::from_confidence(confidence),
            candidates_considered,
            supporting_evidence,
            alternatives,
            uncertainty,
            classification_reason: None,
            warnings,
            classified_at: now,
            schema_version: "3.0.0".to_string(),
            provider: None,
            model: None,
        }
    }
}
