use crate::classification::input::ClassificationInput;
use crate::classification::result::ClassificationResult;
use anyhow::Result;

pub trait ClassificationProcessor {
    fn classify(&self, input: &ClassificationInput) -> Result<ClassificationResult>;
}

pub struct RuleBasedProcessor;

impl ClassificationProcessor for RuleBasedProcessor {
    fn classify(&self, input: &ClassificationInput) -> Result<ClassificationResult> {
        let target = crate::classification::TargetEvidence {
            evidence: input.target.clone(),
            user_hints: None,
        };
        let config = crate::classification::ClassificationConfig::default();
        Ok(crate::classification::DecisionEngine::classify(
            &target,
            &input.candidates,
            &config,
        ))
    }
}
