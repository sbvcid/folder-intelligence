use crate::agent::plan::OperationPlan;
use crate::agent::validate::ValidationResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Approved,
    RequiresApproval,
    Rejected,
}

impl PolicyDecision {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_approved(&self) -> bool {
        matches!(self, PolicyDecision::Approved)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_requires_approval(&self) -> bool {
        matches!(self, PolicyDecision::RequiresApproval)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_rejected(&self) -> bool {
        matches!(self, PolicyDecision::Rejected)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub auto_approve: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub max_files_moved: Option<u64>,
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn auto_approve(mut self, enabled: bool) -> Self {
        self.auto_approve = enabled;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn max_files_moved(mut self, limit: u64) -> Self {
        self.max_files_moved = Some(limit);
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn max_directories_created(mut self, limit: u64) -> Self {
        self.max_directories_created = Some(limit);
        self
    }

    pub fn evaluate(&self, plan: &OperationPlan, validation: &ValidationResult) -> PolicyDecision {
        // Hard rejection conditions (checked first — force cannot bypass these)
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

        // Eligible plans: auto-approve or require explicit approval
        if !self.auto_approve {
            return PolicyDecision::RequiresApproval;
        }

        PolicyDecision::Approved
    }
}

/// An explicit approval bound to a specific plan.
///
/// Created after `Policy::evaluate` returns `RequiresApproval`.
/// The approval is bound to `plan_id` so that an approval for one
/// plan cannot be used to authorize a different plan.
///
/// Approval is NOT persisted by this module — callers that need
/// persistence wrap this struct themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub plan_id: String,
}

impl Approval {
    pub fn for_plan(plan: &OperationPlan) -> Self {
        Approval {
            plan_id: plan.id.clone(),
        }
    }

    /// Verify that this approval matches the given plan's identity.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn verify(&self, plan: &OperationPlan) -> bool {
        self.plan_id == plan.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::plan::{EstimatedImpact, FileSystemOperation, PlanValidationContext};
    use crate::agent::validate::{ValidatedOperation, ValidationResult, ValidationStatus};
    use std::path::PathBuf;

    fn make_valid_plan() -> OperationPlan {
        OperationPlan {
            unresolved_proposals: Vec::new(),
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
    fn test_policy_requires_approval_when_auto_disabled() {
        let plan = make_valid_plan();
        let validation = make_valid_validation();
        let policy = Policy::default().auto_approve(false);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::RequiresApproval
        );
    }

    #[test]
    fn test_policy_rejects_invalid_even_when_auto_disabled() {
        // INVALID must be Rejected, NOT RequiresApproval, even when auto_approve=false
        let plan = make_valid_plan();
        let mut validation = make_valid_validation();
        validation.has_invalid = true;
        let policy = Policy::default().auto_approve(false);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_conflict_even_when_auto_disabled() {
        let plan = make_valid_plan();
        let mut validation = make_valid_validation();
        validation.has_conflicts = true;
        let policy = Policy::default().auto_approve(false);
        assert_eq!(
            policy.evaluate(&plan, &validation),
            PolicyDecision::Rejected
        );
    }

    #[test]
    fn test_policy_rejects_dry_run_even_when_auto_disabled() {
        let mut plan = make_valid_plan();
        plan.dry_run = true;
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

        let decision = PolicyDecision::RequiresApproval;
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
        assert!(!PolicyDecision::Approved.is_requires_approval());
        assert!(!PolicyDecision::Approved.is_rejected());

        assert!(PolicyDecision::RequiresApproval.is_requires_approval());
        assert!(!PolicyDecision::RequiresApproval.is_approved());
        assert!(!PolicyDecision::RequiresApproval.is_rejected());

        assert!(PolicyDecision::Rejected.is_rejected());
        assert!(!PolicyDecision::Rejected.is_approved());
        assert!(!PolicyDecision::Rejected.is_requires_approval());
    }

    #[test]
    fn test_approval_for_plan_contains_plan_id() {
        let plan = make_valid_plan();
        let approval = Approval::for_plan(&plan);
        assert_eq!(approval.plan_id, "test-plan");
    }

    #[test]
    fn test_approval_verify_matches_original_plan() {
        let plan = make_valid_plan();
        let approval = Approval::for_plan(&plan);
        assert!(approval.verify(&plan));
    }

    #[test]
    fn test_approval_verify_rejects_different_plan_id() {
        let plan = make_valid_plan();
        let approval = Approval::for_plan(&plan);

        let mut other = plan.clone();
        other.id = "different-plan".to_string();

        assert!(
            !approval.verify(&other),
            "approval must not verify a plan with different ID"
        );
    }

    #[test]
    fn test_approval_serialization_round_trip() {
        let plan = make_valid_plan();
        let approval = Approval::for_plan(&plan);
        let json = serde_json::to_string(&approval).expect("should serialize");
        let deserialized: Approval = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(approval.plan_id, deserialized.plan_id);
        assert!(
            deserialized.verify(&plan),
            "deserialized approval must verify"
        );
    }
}
