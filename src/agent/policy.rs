use crate::agent::plan::OperationPlan;
use crate::agent::validate::ValidationResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Approved,
    Rejected,
}

impl PolicyDecision {
    #[allow(dead_code)]
    pub fn is_approved(&self) -> bool {
        matches!(self, PolicyDecision::Approved)
    }

    #[allow(dead_code)]
    pub fn is_rejected(&self) -> bool {
        matches!(self, PolicyDecision::Rejected)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub auto_approve: bool,
    #[allow(dead_code)]
    pub max_files_moved: Option<u64>,
    #[allow(dead_code)]
    pub max_directories_created: Option<u64>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            auto_approve: true,
            max_files_moved: None,
            max_directories_created: None,
        }
    }
}

impl Policy {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn auto_approve(mut self, enabled: bool) -> Self {
        self.auto_approve = enabled;
        self
    }

    #[allow(dead_code)]
    pub fn max_files_moved(mut self, limit: u64) -> Self {
        self.max_files_moved = Some(limit);
        self
    }

    #[allow(dead_code)]
    pub fn max_directories_created(mut self, limit: u64) -> Self {
        self.max_directories_created = Some(limit);
        self
    }

    pub fn evaluate(&self, plan: &OperationPlan, validation: &ValidationResult) -> PolicyDecision {
        if plan.dry_run {
            return PolicyDecision::Rejected;
        }

        if validation.has_invalid {
            return PolicyDecision::Rejected;
        }

        if validation.has_conflicts {
            return PolicyDecision::Rejected;
        }

        if let Some(max) = self.max_files_moved {
            if plan.estimated_impact.files_moved > max {
                return PolicyDecision::Rejected;
            }
        }

        if let Some(max) = self.max_directories_created {
            if plan.estimated_impact.dirs_created > max {
                return PolicyDecision::Rejected;
            }
        }

        if !self.auto_approve {
            return PolicyDecision::Rejected;
        }

        PolicyDecision::Approved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::plan::{
        EstimatedImpact, FileSystemOperation, OperationPlan, PlanValidationContext,
    };
    use crate::agent::validate::{ValidatedOperation, ValidationResult, ValidationStatus};
    use std::path::PathBuf;

    fn make_valid_plan() -> OperationPlan {
        OperationPlan {
            id: "test-plan".to_string(),
            recommendation_id: "rec".to_string(),
            scope: PathBuf::from("/tmp/scope"),
            operations: vec![FileSystemOperation::Move {
                source: PathBuf::from("/tmp/scope/src.txt"),
                dest: PathBuf::from("/tmp/scope/Dest/src.txt"),
            }],
            estimated_impact: EstimatedImpact {
                files_moved: 1,
                dirs_created: 0,
                files_deleted: 0,
                dirs_affected: 1,
                total_bytes: 1024,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        }
    }

    fn make_valid_validation() -> ValidationResult {
        ValidationResult {
            plan_id: "test-plan".to_string(),
            scope: PathBuf::from("/tmp/scope"),
            validated_operations: vec![ValidatedOperation {
                operation: FileSystemOperation::Move {
                    source: PathBuf::from("/tmp/scope/src.txt"),
                    dest: PathBuf::from("/tmp/scope/Dest/src.txt"),
                },
                status: ValidationStatus::Valid,
                warnings: vec![],
                dependencies: vec![],
            }],
            summary: crate::agent::ValidationSummary::new(),
            has_blocked: false,
            has_conflicts: false,
            has_invalid: false,
            has_warnings: false,
            executable_operations: 1,
        }
    }

    #[test]
    fn test_policy_default_approves_valid_plan() {
        let plan = make_valid_plan();
        let validation = make_valid_validation();
        let policy = Policy::default();
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Approved
        );
    }

    #[test]
    fn test_policy_rejects_dry_run_plan() {
        let mut plan = make_valid_plan();
        plan.dry_run = true;
        let validation = make_valid_validation();
        let policy = Policy::default();
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_invalid_plan() {
        let plan = make_valid_plan();
        let mut validation = make_valid_validation();
        validation.has_invalid = true;
        let policy = Policy::default();
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_conflict_plan() {
        let plan = make_valid_plan();
        let mut validation = make_valid_validation();
        validation.has_conflicts = true;
        let policy = Policy::default();
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_when_auto_approve_disabled() {
        let plan = make_valid_plan();
        let validation = make_valid_validation();
        let policy = Policy::default().auto_approve(false);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_over_file_limit() {
        let plan = make_valid_plan();
        let validation = make_valid_validation();
        let policy = Policy::default().max_files_moved(0);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_over_dir_limit() {
        let mut plan = make_valid_plan();
        plan.estimated_impact.dirs_created = 1;
        let validation = make_valid_validation();
        let policy = Policy::default().max_directories_created(0);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_serialization_round_trip() {
        let decision = PolicyDecision::Approved;
        let json = serde_json::to_string(&decision).expect("should serialize");
        let deserialized: PolicyDecision = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(decision, deserialized);

        let decision = PolicyDecision::Rejected;
        let json = serde_json::to_string(&decision).expect("should serialize");
        let deserialized: PolicyDecision = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(decision, deserialized);
    }

    #[test]
    fn test_policy_struct_serialization() {
        let policy = Policy::default();
        let json = serde_json::to_string(&policy).expect("should serialize");
        let deserialized: Policy = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(policy.auto_approve, deserialized.auto_approve);
        assert_eq!(policy.max_files_moved, deserialized.max_files_moved);
        assert_eq!(
            policy.max_directories_created,
            deserialized.max_directories_created
        );
    }

    #[test]
    fn test_policy_does_not_mutate_plan_or_validation() {
        let plan = make_valid_plan();
        let validation = make_valid_validation();
        let plan_before = plan.clone();
        let validation_before = validation.clone();

        let policy = Policy::default();
        let _decision = policy.evaluate(&plan, &validation);

        assert_eq!(plan, plan_before, "plan must not be mutated");
        assert_eq!(
            validation, validation_before,
            "validation must not be mutated"
        );
    }

    #[test]
    fn test_policy_decision_helpers() {
        assert!(PolicyDecision::Approved.is_approved());
        assert!(!PolicyDecision::Approved.is_rejected());
        assert!(PolicyDecision::Rejected.is_rejected());
        assert!(!PolicyDecision::Rejected.is_approved());
    }
}
