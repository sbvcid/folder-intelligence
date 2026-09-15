use crate::classification::ai_provider::{
    validate_ai_result, AiClassificationConstraints, AiClassifier, AiClassificationError,
    AiClassificationRequest,
};
use crate::classification::input::ClassificationInput;
use crate::classification::result::{
    ClassificationDecision, ClassificationResult, ConfidenceBand,
    SupportingEvidence, UncertaintyReason,
};
use crate::classification::{ClassificationConfig, DecisionEngine, TargetEvidence};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MockAiClassifier {
    pub simulated_latency: Duration,
}

impl Default for MockAiClassifier {
    fn default() -> Self {
        Self {
            simulated_latency: Duration::from_millis(0),
        }
    }
}

impl AiClassifier for MockAiClassifier {
    fn classify(
        &self,
        input: &ClassificationInput,
        baseline: &ClassificationResult,
    ) -> Result<ClassificationResult, AiClassificationError> {
        if !self.simulated_latency.is_zero() {
            std::thread::sleep(self.simulated_latency);
        }

        let target = TargetEvidence {
            evidence: input.target.clone(),
            user_hints: None,
        };
        let config = ClassificationConfig::default();
        let rule_result = DecisionEngine::classify(&target, &input.candidates, &config);

        let mut simulated = rule_result;

        if baseline.confidence >= 0.80 {
            simulated.decision = ClassificationDecision::MoveExisting;
            simulated.confidence = (baseline.confidence).min(0.85);
            simulated.selected_candidate = baseline.selected_candidate.clone();
            simulated.supporting_evidence = BaselineEvidence::enhance(&baseline.supporting_evidence);
            simulated.uncertainty.clear();
        } else if baseline.confidence >= 0.40 {
            if baseline.uncertainty.contains(&UncertaintyReason::AmbiguousCandidates) {
                simulated.decision = ClassificationDecision::AskUser;
                simulated.confidence = baseline.confidence.min(0.60);
            } else {
                simulated.decision = ClassificationDecision::LeaveUnclassified;
                simulated.confidence = 0.0;
            }
        } else if !baseline.evidence_is_empty() {
            simulated.decision = ClassificationDecision::CreateCategory;
            simulated.confidence = 0.0;
            simulated.proposed_category_name = Some(input.target.name.clone());
        } else {
            simulated.decision = ClassificationDecision::LeaveUnclassified;
            simulated.confidence = 0.0;
        }

        simulated.confidence_band = ConfidenceBand::from_confidence(simulated.confidence);
        simulated.model = Some("mock-ai-v1.0".to_string());
        simulated.provider = Some("mock".to_string());

        let request = AiClassificationRequest {
            target_evidence: input.target.clone(),
            candidates: input.candidates.clone(),
            rule_based_result: baseline.clone(),
            constraints: AiClassificationConstraints::default(),
        };

        validate_ai_result(&simulated, &request)?;

        Ok(simulated)
    }
}

trait ClassificationResultExt {
    fn evidence_is_empty(&self) -> bool;
}

impl ClassificationResultExt for ClassificationResult {
    fn evidence_is_empty(&self) -> bool {
        self.decision == ClassificationDecision::LeaveUnclassified
            && self.candidates_considered == 0
    }
}

struct BaselineEvidence;

impl BaselineEvidence {
    fn enhance(evidence: &[SupportingEvidence]) -> Vec<SupportingEvidence> {
        evidence
            .iter()
            .map(|e| SupportingEvidence {
                evidence_type: e.evidence_type.clone(),
                description: format!("AI-enhanced: {}", e.description),
                score: e.score,
            })
            .collect()
    }
}
