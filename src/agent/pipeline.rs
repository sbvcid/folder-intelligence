use crate::agent::analysis::{AnalyzerError, EvidenceAnalyzer, TaskAnalysis};
use crate::agent::clarification::ClarificationEngine;
use crate::agent::executor::{
    ApplyError, ApplyResult, Executor, OperationLog, RecoveryResult, UndoResult,
};
use crate::agent::intent::{Goal, IntentParseError, TaskIntent, TaskIntentParser};
use crate::agent::plan::{OperationPlan, PlanError, PlanGenerator, PlanValidationContext};
use crate::agent::policy::{Approval, Policy, PolicyDecision};
use crate::agent::recommendation::{
    ProposedOperation, Recommendation, RecommendationEngine, RecommendationError,
};
use crate::agent::validate::{PlanPreview, PlanValidator, ValidationResult};
use crate::agent::verification::ExecutionVerifier;
use crate::classification::LlmClassifier;
use crate::evidence::ScanLimits;
use crate::scanner::Scanner;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Pipeline {
    parser: TaskIntentParser,
    analyzer: EvidenceAnalyzer,
    recommender: RecommendationEngine,
    clarifier: ClarificationEngine,
    planner: PlanGenerator,
    validator: PlanValidator,
    policy: Policy,
    executor: Executor,
}

impl Default for Pipeline {
    fn default() -> Self {
        Pipeline {
            parser: TaskIntentParser::default(),
            analyzer: EvidenceAnalyzer::default(),
            recommender: RecommendationEngine,
            clarifier: ClarificationEngine,
            planner: PlanGenerator,
            validator: PlanValidator,
            policy: Policy::default(),
            executor: Executor,
        }
    }
}

impl Pipeline {
    pub fn new(scope: &Path) -> Self {
        Pipeline {
            parser: TaskIntentParser::new(scope.to_path_buf()),
            analyzer: EvidenceAnalyzer::default(),
            recommender: RecommendationEngine,
            clarifier: ClarificationEngine,
            planner: PlanGenerator,
            validator: PlanValidator,
            policy: Policy::default(),
            executor: Executor,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_llm_classifier(mut self, classifier: LlmClassifier) -> Self {
        self.analyzer = self.analyzer.with_llm_classifier(classifier);
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_classifier_instruction(mut self, instruction: Option<String>) -> Self {
        self.analyzer = self.analyzer.with_classifier_instruction(instruction);
        self
    }

    /// Parse a natural language request into a structured `TaskIntent`.
    pub fn parse_intent(&self, request: &str) -> Result<TaskIntent, PipelineError> {
        Ok(self.parser.parse(request)?)
    }

    /// Analyze the filesystem scope from pre-parsed intent.
    pub fn analyze(&self, intent: &TaskIntent) -> Result<TaskAnalysis, PipelineError> {
        let start = Instant::now();
        let scope = self.extract_scope(intent);

        if !scope.is_dir() {
            return Err(PipelineError::Analysis(AnalyzerError::ScopeNotADirectory(
                scope,
            )));
        }

        let limits = ScanLimits::default();
        let scanner = Scanner::with_limits(&scope, limits);
        let scan_result = scanner
            .scan()
            .map_err(|e| PipelineError::Io(e.to_string()))?;

        if scan_result.evidence.is_empty() {
            return Err(PipelineError::Analysis(AnalyzerError::ScopeNotScannable(
                scope,
            )));
        }

        let scope_evidence = scan_result.evidence.into_iter().next().unwrap();
        let scan_metadata = scan_result.metadata;

        self.analyzer
            .analyze_with_evidence(intent, scope_evidence, scan_metadata, start)
            .map_err(PipelineError::Analysis)
    }

    /// Analyze using pre-scanned evidence (no filesystem re-scan).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn analyze_with_evidence(
        &self,
        intent: &TaskIntent,
        scope_evidence: crate::evidence::DirectoryEvidence,
        scan_metadata: crate::evidence::ScanMetadata,
    ) -> Result<TaskAnalysis, PipelineError> {
        let start = Instant::now();
        self.analyzer
            .analyze_with_evidence(intent, scope_evidence, scan_metadata, start)
            .map_err(PipelineError::Analysis)
    }

    /// Generate a recommendation from intent and analysis.
    pub fn recommend(
        &self,
        intent: &TaskIntent,
        analysis: &TaskAnalysis,
    ) -> Result<Recommendation, PipelineError> {
        Ok(self.recommender.recommend(intent, analysis)?)
    }

    /// Summarize a recommendation for user clarification.
    pub fn clarify(&self, recommendation: &Recommendation) -> String {
        self.clarifier.summarize(recommendation)
    }

    /// Generate an operation plan from a recommendation.
    /// Does NOT mutate the filesystem.
    pub fn plan(
        &self,
        recommendation: &Recommendation,
        analysis: &TaskAnalysis,
        intent: &TaskIntent,
    ) -> Result<OperationPlan, PipelineError> {
        // Enforce classification -> recommendation -> plan integrity
        Self::validate_recommendation_plan_integrity(recommendation, analysis)?;

        let mut plan = self.planner.generate(recommendation, analysis)?;

        // Validate Recommendation -> Plan integrity
        Self::validate_plan_integrity(&plan, recommendation)?;

        plan.validation_context = Some(PlanValidationContext::from(&intent.constraints));
        plan.dry_run = false;
        Ok(plan)
    }

    pub fn validate_recommendation_plan_integrity(
        recommendation: &Recommendation,
        analysis: &TaskAnalysis,
    ) -> Result<(), PipelineError> {
        let used_proposal = recommendation
            .organization_proposal
            .as_ref()
            .map(|p| {
                p.proposed_categories
                    .iter()
                    .any(|c| !c.source_files.is_empty())
            })
            .unwrap_or(false);
        if used_proposal {
            return Ok(());
        }

        if let Some(classification) = analysis.classification_results.first() {
            match classification.decision {
                crate::classification::ClassificationDecision::LeaveUnclassified => {
                    // LeaveUnclassified must not produce mutations
                    if recommendation.proposed_operations.iter().any(|op| {
                        matches!(
                            op,
                            ProposedOperation::MoveCategory { .. }
                                | ProposedOperation::CreateCategory { .. }
                                | ProposedOperation::ArchiveFiles { .. }
                        )
                    }) {
                        return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                            "Integrity violation: Classification is LeaveUnclassified but recommendation proposes mutations".to_string(),
                        )));
                    }
                }
                crate::classification::ClassificationDecision::AskUser => {
                    // AskUser must remain unresolved and produce no mutations
                    if recommendation.proposed_operations.iter().any(|op| {
                        matches!(
                            op,
                            ProposedOperation::MoveCategory { .. }
                                | ProposedOperation::CreateCategory { .. }
                                | ProposedOperation::ArchiveFiles { .. }
                        )
                    }) {
                        return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                            "Integrity violation: Classification is AskUser but recommendation proposes mutations".to_string(),
                        )));
                    }
                }
                crate::classification::ClassificationDecision::MoveExisting => {
                    if let Some(ref selected) = classification.selected_candidate {
                        // Find the candidate name for the selected path
                        let expected_category = analysis
                            .candidate_categories
                            .iter()
                            .find(|c| c.path == *selected)
                            .map(|c| c.name.clone());

                        if let Some(ref expected_name) = expected_category {
                            // Check that at least one MoveCategory operation targets the expected category
                            let has_matching_move =
                                recommendation.proposed_operations.iter().any(|op| {
                                    if let ProposedOperation::MoveCategory { to_category, .. } = op
                                    {
                                        to_category.to_lowercase() == expected_name.to_lowercase()
                                            || expected_name.to_lowercase().contains(to_category)
                                            || to_category.to_lowercase().contains(expected_name)
                                    } else {
                                        false
                                    }
                                });
                            if !has_matching_move && !recommendation.proposed_operations.is_empty()
                            {
                                return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                                    format!("Integrity violation: Classification MoveExisting targets '{}' but recommendation moves to different category", expected_name),
                                )));
                            }
                        }

                        // Also verify proposed_categories has matching target_path
                        let has_matching_category =
                            recommendation.proposed_categories.iter().any(|cat| {
                                cat.target_path
                                    .as_ref()
                                    .map(|p| p == selected)
                                    .unwrap_or(false)
                            });
                        if !has_matching_category && !recommendation.proposed_categories.is_empty()
                        {
                            return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                                format!("Integrity violation: Classification MoveExisting targets candidate but recommendation proposes different category"),
                            )));
                        }
                    }
                }
                crate::classification::ClassificationDecision::CreateCategory => {
                    if let Some(ref proposed_name) = classification.proposed_category_name {
                        let expected_normalized = proposed_name
                            .to_lowercase()
                            .replace(' ', "_")
                            .replace('-', "_");
                        // Ensure CreateCategory operation matches
                        let has_matching_create =
                            recommendation.proposed_operations.iter().any(|op| {
                                if let ProposedOperation::CreateCategory { name, .. } = op {
                                    name.to_lowercase() == expected_normalized
                                        || expected_normalized.contains(name)
                                } else {
                                    false
                                }
                            });
                        if !has_matching_create && !recommendation.proposed_operations.is_empty() {
                            return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                                format!("Integrity violation: Classification CreateCategory proposes '{}' but recommendation creates different category", proposed_name),
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_plan_integrity(
        plan: &OperationPlan,
        recommendation: &Recommendation,
    ) -> Result<(), PipelineError> {
        let used_proposal = recommendation
            .organization_proposal
            .as_ref()
            .map(|p| {
                p.proposed_categories
                    .iter()
                    .any(|c| !c.source_files.is_empty())
            })
            .unwrap_or(false);
        if used_proposal {
            return Ok(());
        }

        // Check that every MoveCategory in recommendation has corresponding Move operations in plan
        for rec_op in &recommendation.proposed_operations {
            match rec_op {
                ProposedOperation::MoveCategory { to_category, .. } => {
                    // Check if plan has a Move operation targeting this category
                    let has_matching_move = plan.operations.iter().any(|plan_op| {
                        if let crate::agent::plan::FileSystemOperation::Move { dest, .. } = plan_op
                        {
                            // Check if dest is within the expected category directory
                            let path_str = dest.to_string_lossy().to_lowercase();
                            let cat_lower = to_category.to_lowercase();
                            path_str.contains(&cat_lower)
                        } else {
                            false
                        }
                    });
                    if !has_matching_move {
                        return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                            format!("Integrity violation: Recommendation proposes MoveCategory to '{}' but plan has no matching move operation", to_category),
                        )));
                    }
                }
                ProposedOperation::CreateCategory { name, .. } => {
                    let has_matching_create = plan.operations.iter().any(|plan_op| {
                        if let crate::agent::plan::FileSystemOperation::CreateDir { path } = plan_op
                        {
                            let path_str = path.to_string_lossy().to_lowercase();
                            let name_lower = name.to_lowercase();
                            path_str.contains(&name_lower)
                        } else {
                            false
                        }
                    });
                    if !has_matching_create {
                        let dest = plan.scope.join(Self::category_to_dir_name(name));
                        if !dest.exists() {
                            return Err(PipelineError::Plan(PlanError::ConflictingOperations(
                                format!("Integrity violation: Recommendation proposes CreateCategory '{}' but plan has no matching create directory operation", name),
                            )));
                        }
                    }
                }
                ProposedOperation::LeaveUnclassified { .. } => {
                    // LeaveUnclassified should not produce mutations in plan
                }
                ProposedOperation::ArchiveFiles { .. } => {
                    // ArchiveFiles should have corresponding operations in plan
                    // We don't strictly enforce this as archive can be complex
                }
                ProposedOperation::PreserveDirectory { .. } => {
                    // PreserveDirectory doesn't create operations
                }
            }
        }
        Ok(())
    }

    fn category_to_dir_name(category: &str) -> String {
        match category {
            "document_storage" => "Documents",
            "image_storage" => "Images",
            "archive_storage" => "Archive",
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

    /// Validate a plan against its persisted constraints and current filesystem state.
    pub fn validate(&self, plan: &OperationPlan) -> ValidationResult {
        self.validator.validate(plan)
    }

    /// Evaluate whether a plan should be approved for execution by the policy.
    ///
    /// This does NOT execute anything and does NOT mutate the filesystem.
    /// Callers can inspect the decision before calling `apply`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn policy_evaluate(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
    ) -> PolicyDecision {
        self.policy.evaluate(plan, validation)
    }

    /// Render a human-readable preview of a plan and its validation.
    pub fn preview(&self, plan: &OperationPlan, validation: &ValidationResult) -> String {
        PlanPreview.render(plan, validation)
    }

    /// Apply a validated plan to the filesystem.
    ///
    /// This is the ONLY method that mutates the filesystem.
    /// The pipeline's policy is evaluated before execution: INVALID,
    /// CONFLICT, and dry-run plans are always rejected by policy.
    /// Plans requiring approval return `PipelineError::ApprovalRequired`.
    /// BLOCKED operations are skipped when `force = true`.
    pub fn apply(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        options: &ApplyOptions,
    ) -> Result<ApplyResult, PipelineError> {
        match self.policy.evaluate(plan, validation) {
            PolicyDecision::Approved => {}
            PolicyDecision::RequiresApproval => {
                return Err(PipelineError::ApprovalRequired(Approval::for_plan(plan)));
            }
            PolicyDecision::Rejected => {
                return Err(PipelineError::PolicyRejected);
            }
        }

        if validation.has_blocked && !options.force {
            return Err(PipelineError::Apply(ApplyError::InvalidPlan(
                "Plan has BLOCKED operations. Use --force to skip them.".to_string(),
            )));
        }

        if options.dry_run {
            let mut log = OperationLog::new(&plan.id);
            log.finalize();
            return Ok(ApplyResult {
                plan_id: plan.id.clone(),
                log,
                is_complete: false,
                can_undo: false,
                undo_supported_count: validation.executable_operations,
                undo_unsupported_count: 0,
                execution_verification: None,
            });
        }

        let before_state = ExecutionVerifier::capture_state(&plan.scope);

        let mut result = self
            .executor
            .execute_with_options(plan, validation, options.force)
            .map_err(PipelineError::Apply)?;

        let verification = ExecutionVerifier.verify(plan, &result.log, &before_state);
        result.execution_verification = Some(verification);

        Ok(result)
    }

    /// Apply a plan that has received explicit approval.
    ///
    /// Verifies the approval matches the plan (plan_id), re-validates
    /// against the current filesystem state, and re-checks
    /// policy hard constraints before delegating to the Executor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_with_approval(
        &self,
        plan: &OperationPlan,
        approval: &Approval,
        options: &ApplyOptions,
    ) -> Result<ApplyResult, PipelineError> {
        if !approval.verify(plan) {
            return Err(PipelineError::ApprovalMismatch);
        }

        let fresh_validation = self.validator.validate(plan);

        match self.policy.evaluate(plan, &fresh_validation) {
            PolicyDecision::Rejected => {
                return Err(PipelineError::PolicyRejected);
            }
            PolicyDecision::RequiresApproval | PolicyDecision::Approved => {
                // Explicit approval overrides RequiresApproval; Allowed
            }
        }

        if fresh_validation.has_blocked && !options.force {
            return Err(PipelineError::Apply(ApplyError::InvalidPlan(
                "Plan has BLOCKED operations. Use --force to skip them.".to_string(),
            )));
        }

        if options.dry_run {
            let mut log = OperationLog::new(&plan.id);
            log.finalize();
            return Ok(ApplyResult {
                plan_id: plan.id.clone(),
                log,
                is_complete: false,
                can_undo: false,
                undo_supported_count: fresh_validation.executable_operations,
                undo_unsupported_count: 0,
                execution_verification: None,
            });
        }

        let before_state = ExecutionVerifier::capture_state(&plan.scope);

        let mut result = self
            .executor
            .execute_with_options(plan, &fresh_validation, options.force)
            .map_err(PipelineError::Apply)?;

        let verification = ExecutionVerifier.verify(plan, &result.log, &before_state);
        result.execution_verification = Some(verification);

        Ok(result)
    }

    /// Create an approval token for a plan, after policy evaluation.
    ///
    /// If the policy returns `Rejected`, an error is returned.
    /// If the policy returns `Approved` or `RequiresApproval`, an
    /// `Approval` token is produced that the caller can present
    /// later via `apply_with_approval()`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn create_approval(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
    ) -> Result<Approval, PipelineError> {
        match self.policy.evaluate(plan, validation) {
            PolicyDecision::Rejected => Err(PipelineError::PolicyRejected),
            PolicyDecision::Approved | PolicyDecision::RequiresApproval => {
                Ok(Approval::for_plan(plan))
            }
        }
    }

    /// Undo operations from a previous apply.
    ///
    /// Only reverses operations that executed successfully and support undo.
    pub fn undo(&self, log: &OperationLog) -> Result<UndoResult, PipelineError> {
        self.executor.undo(log).map_err(PipelineError::Apply)
    }

    /// Preview what undoing an operation log would do, without mutating the filesystem.
    pub fn preview_undo(&self, log: &OperationLog) -> UndoResult {
        self.executor.preview_undo(log)
    }

    /// Inspect a plan's current filesystem state for recovery.
    ///
    /// Filesystem state is the primary source of truth — the OperationLog
    /// is only consulted as supplementary evidence. Returns the execution
    /// state for each operation without mutating the filesystem.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn inspect_plan(&self, plan: &OperationPlan, log: Option<&OperationLog>) -> RecoveryResult {
        self.executor.inspect_plan_state(plan, log)
    }

    /// Resume a plan that may have been partially executed.
    ///
    /// Uses filesystem inspection to determine which operations are
    /// `AlreadyApplied` (skipped) and which are `Pending` (executed).
    /// Operations in `Conflict` state cause the entire resume to fail.
    ///
    /// The filesystem state is the authority — no trust is placed in
    /// prior OperationLog entries without filesystem verification.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn resume(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        options: &ApplyOptions,
    ) -> Result<ApplyResult, PipelineError> {
        if plan.dry_run {
            return Err(PipelineError::Apply(ApplyError::DryRunFlagSet));
        }

        let recovery = self.executor.inspect_plan_state(plan, None);

        if recovery.has_conflicts {
            return Err(PipelineError::Apply(ApplyError::InvalidPlan(
                "Plan has operations in conflict with current filesystem state. Cannot resume."
                    .to_string(),
            )));
        }

        self.executor
            .resume_execution(plan, validation, options.force)
            .map_err(PipelineError::Apply)
    }

    /// Resume a plan with concurrency protection and precondition revalidation.
    ///
    /// Acquires a scope-level file lock to prevent concurrent execution,
    /// captures preconditions before execution, and revalidates preconditions
    /// before each mutation to detect TOCTOU races.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn resume_guarded(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        options: &ApplyOptions,
    ) -> Result<ApplyResult, PipelineError> {
        if plan.dry_run {
            return Err(PipelineError::Apply(ApplyError::DryRunFlagSet));
        }

        self.executor
            .resume_execution_guarded(plan, validation, options.force)
            .map_err(PipelineError::Apply)
    }

    /// Convenience method: run the full pipeline in one call.
    ///
    /// By default, this runs to plan generation and validation but does NOT execute.
    /// Set `options.execute = true` to apply (requires explicit approval).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn run(
        &self,
        request: &str,
        options: &PipelineOptions,
    ) -> Result<PipelineResult, PipelineError> {
        let intent = self.parse_intent(request)?;
        let analysis = self.analyze(&intent)?;
        let recommendation = self.recommend(&intent, &analysis)?;
        let mut plan = self.plan(&recommendation, &analysis, &intent)?;
        let validation = self.validate(&plan);
        let preview = self.preview(&plan, &validation);

        plan.dry_run = options.dry_run;

        let apply = if options.dry_run {
            None
        } else if options.execute {
            Some(self.apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: options.force,
                    dry_run: false,
                },
            )?)
        } else {
            None
        };

        Ok(PipelineResult {
            intent,
            user_intent: options.user_intent.clone(),
            analysis,
            recommendation,
            plan,
            validation,
            preview,
            apply,
        })
    }

    fn extract_scope(&self, intent: &TaskIntent) -> PathBuf {
        match &intent.goal {
            Goal::Organize { scope, .. }
            | Goal::Reorganize { scope, .. }
            | Goal::Clean { scope, .. } => scope.clone(),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    pub force: bool,
    pub dry_run: bool,
    pub execute: bool,
    pub user_intent: Option<crate::agent::UserIntent>,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    pub force: bool,
    pub dry_run: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub intent: TaskIntent,
    pub user_intent: Option<crate::agent::UserIntent>,
    pub analysis: TaskAnalysis,
    pub recommendation: Recommendation,
    pub plan: OperationPlan,
    pub validation: ValidationResult,
    pub preview: String,
    pub apply: Option<ApplyResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineError {
    IntentParse(String),
    Analysis(AnalyzerError),
    Recommendation(RecommendationError),
    Plan(PlanError),
    Apply(ApplyError),
    PolicyRejected,
    ApprovalRequired(Approval),
    #[cfg_attr(not(test), allow(dead_code))]
    ApprovalMismatch,
    Io(String),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::IntentParse(msg) => write!(f, "Intent parse error: {}", msg),
            PipelineError::Analysis(e) => write!(f, "Analysis error: {}", e),
            PipelineError::Recommendation(e) => write!(f, "Recommendation error: {}", e),
            PipelineError::Plan(e) => write!(f, "Plan generation error: {}", e),
            PipelineError::Apply(e) => write!(f, "Apply error: {}", e),
            PipelineError::PolicyRejected => write!(f, "Plan rejected by policy"),
            PipelineError::ApprovalRequired(approval) => {
                write!(f, "Plan requires approval (plan_id={})", approval.plan_id)
            }
            PipelineError::ApprovalMismatch => write!(f, "Approval mismatch: does not match plan"),
            PipelineError::Io(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<IntentParseError> for PipelineError {
    fn from(e: IntentParseError) -> Self {
        PipelineError::IntentParse(e.to_string())
    }
}

impl From<AnalyzerError> for PipelineError {
    fn from(e: AnalyzerError) -> Self {
        PipelineError::Analysis(e)
    }
}

impl From<RecommendationError> for PipelineError {
    fn from(e: RecommendationError) -> Self {
        PipelineError::Recommendation(e)
    }
}

impl From<PlanError> for PipelineError {
    fn from(e: PlanError) -> Self {
        PipelineError::Plan(e)
    }
}

impl From<ApplyError> for PipelineError {
    fn from(e: ApplyError) -> Self {
        PipelineError::Apply(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::plan::FileSystemOperation;
    use crate::agent::validate::{ValidatedOperation, ValidationStatus};
    use crate::classification::{ClassificationDecision, ConfidenceBand};
    use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_pipeline_default_constructs() {
        let _pipeline = Pipeline::default();
    }

    #[test]
    fn test_pipeline_options_defaults() {
        let opts = PipelineOptions::default();
        assert!(!opts.force);
        assert!(!opts.dry_run);
        assert!(!opts.execute);
    }

    #[test]
    fn test_apply_options_defaults() {
        let opts = ApplyOptions::default();
        assert!(!opts.force);
        assert!(!opts.dry_run);
    }

    #[test]
    fn test_pipeline_error_display() {
        let err = PipelineError::IntentParse("test".to_string());
        assert!(format!("{}", err).contains("Intent parse error"));

        let approval = Approval {
            plan_id: "p1".to_string(),
        };
        let err = PipelineError::ApprovalRequired(approval);
        assert!(format!("{}", err).contains("approval"));

        let err = PipelineError::ApprovalMismatch;
        assert!(format!("{}", err).contains("mismatch"));

        let err = PipelineError::PolicyRejected;
        assert!(format!("{}", err).contains("rejected"));
    }

    #[test]
    fn test_pipeline_new_with_scope() {
        let dir = tempdir().unwrap();
        let pipeline = Pipeline::new(dir.path());
        assert_eq!(pipeline.parser.default_scope, dir.path().to_path_buf());
    }

    #[test]
    fn test_pipeline_run_dry_run() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let options = PipelineOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = pipeline
            .run("Organize this folder by category", &options)
            .unwrap();
        assert!(result.apply.is_none());
        assert!(result.plan.dry_run);
        assert!(!result.preview.is_empty());
    }

    #[test]
    fn test_pipeline_run_no_execute() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let options = PipelineOptions {
            dry_run: false,
            execute: false,
            force: false,
            user_intent: None,
        };

        let result = pipeline
            .run("Organize this folder by category", &options)
            .unwrap();
        assert!(result.apply.is_none());
        assert!(!result.plan.dry_run);
    }

    #[test]
    fn test_pipeline_run_execute() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let options = PipelineOptions {
            dry_run: false,
            execute: true,
            force: false,
            user_intent: None,
        };

        let result = pipeline
            .run("Organize this folder by category", &options)
            .unwrap();
        assert!(result.apply.is_some());
    }

    #[test]
    fn test_pipeline_staged_plan_no_mutation() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source_file = scope.join("doc.pdf");
        fs::write(&source_file, "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        assert!(source_file.exists());
        assert!(!plan.dry_run);
    }

    #[test]
    fn test_pipeline_staged_apply_requires_filesystem() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let apply_result = pipeline
            .apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .unwrap();

        assert!(apply_result.is_complete);
    }

    #[test]
    fn test_pipeline_apply_rejects_dry_run_plan() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.dry_run = true;
        let validation = pipeline.validate(&plan);

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    }

    #[test]
    fn test_pipeline_apply_rejects_invalid_even_with_force() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let mut validation = pipeline.validate(&plan);
        validation.has_invalid = true;

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );

        assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    }

    #[test]
    fn test_cli_apply_dry_run_does_not_mutate() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source_file = scope.join("doc.pdf");
        fs::write(&source_file, "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let result = pipeline
            .apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: true,
                },
            )
            .unwrap();

        assert!(!result.is_complete);
        assert!(result.log.entries.is_empty());
        assert!(
            source_file.exists(),
            "dry-run apply must not mutate filesystem"
        );
    }

    #[test]
    fn test_cli_apply_rejects_conflict() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let mut validation = pipeline.validate(&plan);
        validation.has_conflicts = true;

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );

        assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    }

    #[test]
    fn test_cli_apply_force_bypasses_blocked_not_invalid() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let mut validation = pipeline.validate(&plan);

        // With force=true, BLOCKED operations should be allowed through
        validation.has_blocked = true;
        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(result.is_ok(), "force must bypass BLOCKED operations");

        // INVALID operations must still be rejected even with force
        validation.has_invalid = true;
        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    }

    #[test]
    fn test_cli_undo_dry_run_does_not_mutate() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        // Execute a plan to get a log
        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);
        let apply_result = pipeline
            .apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .unwrap();

        let undoable = apply_result.log.undoable_entries();

        if undoable.is_empty() {
            // No undoable operations; skip mutation verification
            let preview = pipeline.preview_undo(&apply_result.log);
            assert_eq!(preview.total_undo_operations, 0);
            return;
        }

        // Snapshot filesystem state before undo dry-run
        let applied_target = &undoable[0].applied_target;
        let target_existed_before = applied_target.exists();

        // Undo dry-run (preview) should not mutate filesystem
        let preview = pipeline.preview_undo(&apply_result.log);
        assert_eq!(preview.applied_undoes.len(), 0);
        assert!(preview.total_undo_operations > 0);
        assert_eq!(applied_target.exists(), target_existed_before);

        // Undo (actual) should produce results
        let undo_result = pipeline.undo(&apply_result.log).unwrap();
        assert!(undo_result.total_undo_operations > 0);
    }

    #[test]
    fn test_cli_clarify_uses_pipeline() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::write(scope.join("img.jpg"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let summary = pipeline.clarify(&recommendation);

        assert!(!summary.is_empty());
    }

    fn create_restart_scope(dir: &tempfile::TempDir) -> PathBuf {
        let scope = dir.path().join("downloads");
        fs::create_dir_all(&scope).unwrap();
        fs::create_dir_all(scope.join("documents")).unwrap();
        fs::create_dir_all(scope.join("images")).unwrap();
        fs::write(scope.join("documents").join("doc1.pdf"), "content").unwrap();
        fs::write(scope.join("documents").join("doc2.docx"), "content").unwrap();
        fs::write(scope.join("images").join("photo1.jpg"), "img").unwrap();
        fs::write(scope.join("images").join("photo2.png"), "img").unwrap();
        fs::write(scope.join("archive.zip"), "data").unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();
        fs::create_dir_all(scope.join("Images")).unwrap();
        scope
    }

    fn create_restart_plan(dir: &tempfile::TempDir) -> (PathBuf, OperationPlan) {
        let scope = create_restart_scope(dir);
        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        (scope, plan)
    }

    #[test]
    fn test_phase6c_restart_simulation() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        assert!(plan.operations.len() > 0, "plan must have operations");
        assert!(
            plan.validation_context.is_some(),
            "plan must have validation context"
        );
        assert!(
            !plan.dry_run,
            "plan must be non-dry-run from Pipeline::plan()"
        );

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        assert!(
            reloaded.validation_context.is_some(),
            "validation_context must survive serialization"
        );

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        assert_eq!(validation.plan_id, reloaded.id);
        assert!(
            validation.summary.total > 0,
            "fresh validation must see operations"
        );
        assert!(
            !validation.has_invalid,
            "should have no invalid operations on fresh restart"
        );

        let apply_result = fresh_pipeline
            .apply(
                &reloaded,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("should apply after restart");
        assert!(
            apply_result.log.total_entries > 0,
            "should have processed operations"
        );
    }

    #[test]
    fn test_phase6c_source_deleted_after_planning() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let move_op = plan
            .operations
            .iter()
            .find_map(|op| {
                if let FileSystemOperation::Move { source, .. } = op {
                    Some(source.clone())
                } else {
                    None
                }
            })
            .expect("plan must have at least one Move operation");

        assert!(move_op.exists());
        std::fs::remove_file(&move_op).unwrap();

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        assert!(validation.has_invalid, "source missing should be INVALID");
        assert!(!reloaded.dry_run, "plan must be executable");
    }

    #[test]
    fn test_phase6c_destination_conflict_after_restart() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("should parse json");

        if let Some(ops) = value.get_mut("operations").and_then(|v| v.as_array_mut()) {
            if let Some(first) = ops.first() {
                let duplicated = first.clone();
                ops.push(duplicated);
            }
        }

        let tampered_json = serde_json::to_string(&value).expect("should re-serialize");
        let conflicted_plan: OperationPlan =
            serde_json::from_str(&tampered_json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&conflicted_plan);

        assert!(
            validation.has_conflicts,
            "overlapping destinations should be CONFLICT"
        );
    }

    #[test]
    fn test_phase6c_source_missing_after_restart() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let move_source = reloaded
            .operations
            .iter()
            .find_map(|op| {
                if let FileSystemOperation::Move { source, .. } = op {
                    Some(source.clone())
                } else {
                    None
                }
            })
            .expect("plan must have Move operations");
        std::fs::remove_file(&move_source).unwrap();

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        assert!(validation.has_invalid, "source missing should be INVALID");

        let result = fresh_pipeline.apply(
            &reloaded,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );
        assert!(
            result.is_err(),
            "should reject apply with invalid operations"
        );
    }

    #[test]
    fn test_phase6c_scope_tampering_rejected() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("should parse json");

        if let Some(ops) = value.get_mut("operations").and_then(|v| v.as_array_mut()) {
            for op in ops.iter_mut() {
                if let Some(move_data) = op.get_mut("move").and_then(|v| v.as_object_mut()) {
                    if let Some(dest) = move_data.get_mut("dest") {
                        *dest = serde_json::json!("/outside/scope/file.txt");
                    }
                }
            }
        }

        let tampered_json = serde_json::to_string(&value).expect("should re-serialize");
        let tampered_plan: OperationPlan =
            serde_json::from_str(&tampered_json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&tampered_plan);

        assert!(
            validation.has_invalid,
            "out-of-scope destinations must be INVALID"
        );
    }

    #[test]
    fn test_phase6c_invalid_plus_force_rejected() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let move_source = plan
            .operations
            .iter()
            .find_map(|op| {
                if let FileSystemOperation::Move { source, .. } = op {
                    Some(source.clone())
                } else {
                    None
                }
            })
            .expect("plan must have Move operations");
        std::fs::remove_file(&move_source).unwrap();

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        assert!(validation.has_invalid);

        let result = fresh_pipeline.apply(
            &reloaded,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(
            matches!(result, Err(PipelineError::PolicyRejected)),
            "INVALID + force must still be rejected"
        );
    }

    #[test]
    fn test_phase6c_conflict_plus_force_rejected() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("should parse json");

        if let Some(ops) = value.get_mut("operations").and_then(|v| v.as_array_mut()) {
            if let Some(first) = ops.first() {
                ops.push(first.clone());
            }
        }

        let tampered_json = serde_json::to_string(&value).expect("should re-serialize");
        let conflicted_plan: OperationPlan =
            serde_json::from_str(&tampered_json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&conflicted_plan);

        assert!(
            validation.has_conflicts,
            "should have conflicts from duplicated operations"
        );

        let result = fresh_pipeline.apply(
            &conflicted_plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(
            matches!(result, Err(PipelineError::PolicyRejected)),
            "CONFLICT + force must still be rejected"
        );
    }

    #[test]
    fn test_phase6c_blocked_plus_force_skips() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        let mut tampered_validation = validation.clone();
        for v_op in &mut tampered_validation.validated_operations {
            if v_op.status.is_valid() {
                v_op.status = ValidationStatus::BlockedByConstraint(
                    "Test: operation blocked by constraint".to_string(),
                );
            }
        }
        tampered_validation.has_blocked = true;
        tampered_validation.has_invalid = false;
        tampered_validation.has_conflicts = false;
        tampered_validation.executable_operations = 0;

        let result = fresh_pipeline.apply(
            &reloaded,
            &tampered_validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(
            result.is_ok(),
            "BLOCKED + force should proceed (skip blocked ops)"
        );

        let apply_result = result.unwrap();
        assert!(
            apply_result.log.skipped_count > 0,
            "should have skipped blocked operations"
        );
    }

    #[test]
    fn test_phase6c_validation_does_not_mutate_filesystem() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let files_before = collect_files(&scope);

        let fresh_pipeline = Pipeline::new(&scope);
        let _ = fresh_pipeline.validate(&plan);

        let files_after = collect_files(&scope);
        assert_eq!(
            files_before, files_after,
            "validation must not mutate filesystem"
        );
    }

    #[test]
    fn test_phase6c_dry_run_after_serialization() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        assert!(
            !plan.dry_run,
            "plan must be non-dry-run from Pipeline::plan()"
        );

        let files_before = collect_files(&scope);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);
        let result = fresh_pipeline
            .apply(
                &reloaded,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: true,
                },
            )
            .expect("dry-run apply should succeed");

        assert!(!result.is_complete, "dry-run must not be complete");
        assert!(
            result.log.entries.is_empty(),
            "dry-run must not log operations"
        );

        let files_after = collect_files(&scope);
        assert_eq!(
            files_before, files_after,
            "dry-run must not mutate filesystem"
        );
    }

    #[test]
    fn test_phase6c_apply_after_serialization() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let source_file = scope.join("documents").join("doc1.pdf");
        assert!(source_file.exists());

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);

        assert!(!validation.has_invalid);
        assert!(!validation.has_conflicts);

        let result = fresh_pipeline
            .apply(
                &reloaded,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("should apply after serialization");

        assert!(
            result.log.total_entries > 0,
            "should have processed operations"
        );
    }

    #[test]
    fn test_phase6c_undo_after_deserialized_apply() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let source_file = scope.join("documents").join("doc1.pdf");
        assert!(source_file.exists());

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);
        let apply_result = fresh_pipeline
            .apply(
                &reloaded,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("should apply");

        let log_json = serde_json::to_string(&apply_result.log).expect("should serialize log");
        let reloaded_log: OperationLog =
            serde_json::from_str(&log_json).expect("should deserialize log");

        let undoable = reloaded_log.undoable_entries();
        if undoable.is_empty() {
            let preview = fresh_pipeline.preview_undo(&reloaded_log);
            assert_eq!(preview.total_undo_operations, 0);
            return;
        }

        assert!(
            reloaded_log.id == apply_result.log.id,
            "log id must survive serialization"
        );

        let undo_result = fresh_pipeline.undo(&reloaded_log).expect("should undo");
        assert!(
            undo_result.total_undo_operations > 0,
            "should have undoable operations"
        );
    }

    #[test]
    fn test_policy_approves_valid_plan_execution_allowed() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let source = scope.join("readme.txt");
        let files_before = collect_files(&scope);

        let result = pipeline
            .apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .unwrap();

        assert!(result.is_complete);
        let files_after = collect_files(&scope);
        assert_ne!(
            files_before, files_after,
            "filesystem must be mutated on approved apply"
        );
        assert!(
            !source.exists(),
            "source must no longer exist at original path"
        );
    }

    #[test]
    fn test_policy_rejected_prevents_filesystem_mutation() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source = scope.join("readme.txt");
        let dest = scope.join("Documents").join("readme.txt");
        fs::write(&source, "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let plan = OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "test-approval-reject".to_string(),
            recommendation_id: "rec".to_string(),
            scope: scope.clone(),
            operations: vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        };

        let mut validation = ValidationResult {
            plan_id: plan.id.clone(),
            scope: scope.clone(),
            validated_operations: vec![ValidatedOperation {
                operation: FileSystemOperation::Move {
                    source: source.clone(),
                    dest: dest.clone(),
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
        };
        validation.summary.valid = 1;
        validation.summary.total = 1;

        let files_before = collect_files(&scope);

        let pipeline =
            Pipeline::new(&scope).with_policy(crate::agent::Policy::default().auto_approve(false));

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        assert!(
            matches!(result, Err(PipelineError::ApprovalRequired(_))),
            "auto-approve=false must require approval for valid plan: {:?}",
            result
        );
        assert!(
            source.exists(),
            "source must still exist after approval requirement"
        );
        let files_after = collect_files(&scope);
        assert_eq!(
            files_before, files_after,
            "no filesystem mutation when policy requires approval"
        );
    }

    #[test]
    fn test_policy_enforced_before_executor() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source = scope.join("doc.pdf");
        fs::write(&source, "test").unwrap();
        let dest = scope.join("Documents").join("doc.pdf");

        let plan = OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "policy-test".to_string(),
            recommendation_id: "rec".to_string(),
            scope: scope.clone(),
            operations: vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        };

        let mut validation = ValidationResult {
            plan_id: plan.id.clone(),
            scope: scope.clone(),
            validated_operations: vec![ValidatedOperation {
                operation: FileSystemOperation::Move {
                    source: source.clone(),
                    dest: dest.clone(),
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
        };
        validation.summary.valid = 1;
        validation.summary.total = 1;

        let pipeline =
            Pipeline::new(&scope).with_policy(crate::agent::Policy::default().auto_approve(false));

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        assert!(
            matches!(result, Err(PipelineError::ApprovalRequired(_))),
            "policy requires-approval must prevent executor from running"
        );
        assert!(source.exists(), "source must not be moved");
        assert!(!dest.exists(), "dest must not be created");
    }

    #[test]
    fn test_policy_evaluation_does_not_mutate_filesystem() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let files_before = collect_files(&scope);
        let _ = pipeline.policy_evaluate(&plan, &validation);
        let files_after = collect_files(&scope);

        assert_eq!(
            files_before, files_after,
            "policy evaluation must not mutate filesystem"
        );
    }

    #[test]
    fn test_policy_bypass_does_not_skip_executor_checks() {
        // Defense-in-depth: even when the policy approves (summary flags
        // clean), the Executor still checks per-operation validation statuses.
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source = scope.join("source.txt");
        let dest = scope.join("Documents").join("source.txt");
        fs::write(&source, "original").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();
        fs::write(&dest, "existing").unwrap();

        let plan = OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "exec-defense".to_string(),
            recommendation_id: "rec".to_string(),
            scope: scope.clone(),
            operations: vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        };

        // Per-operation Conflict status, but summary flag is false (no duplicate dests)
        let validation = ValidationResult {
            plan_id: plan.id.clone(),
            scope: scope.clone(),
            validated_operations: vec![ValidatedOperation {
                operation: FileSystemOperation::Move {
                    source: source.clone(),
                    dest: dest.clone(),
                },
                status: ValidationStatus::Conflict("Destination exists".to_string()),
                warnings: vec![],
                dependencies: vec![],
            }],
            summary: {
                let mut s = crate::agent::ValidationSummary::new();
                s.conflicts = 1;
                s
            },
            has_blocked: false,
            has_conflicts: false,
            has_invalid: false,
            has_warnings: false,
            executable_operations: 0,
        };

        let pipeline = Pipeline::new(&scope);

        // Policy approves (summary flags are clean)
        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert!(
            decision.is_approved(),
            "policy should approve when summary flags are clean"
        );

        // Executor catches per-operation Conflict (defense-in-depth)
        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );
        assert!(
            matches!(
                result,
                Err(PipelineError::Apply(ApplyError::InvalidPlan(msg)))
                    if msg.contains("CONFLICT")
            ),
            "Executor defense-in-depth must reject per-operation CONFLICT after policy approval"
        );
        assert!(
            source.exists(),
            "source must still exist after Executor rejection"
        );
    }

    #[test]
    fn test_existing_phase6c_behavior_unchanged() {
        let dir = tempdir().unwrap();
        let (scope, plan) = create_restart_plan(&dir);

        let json = serde_json::to_string(&plan).expect("should serialize");
        let reloaded: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

        let fresh_pipeline = Pipeline::new(&scope);
        let validation = fresh_pipeline.validate(&reloaded);
        assert!(!validation.has_invalid, "valid plan should not be invalid");
        assert!(
            !validation.has_conflicts,
            "valid plan should not have conflicts"
        );

        let decision = fresh_pipeline.policy_evaluate(&reloaded, &validation);
        assert!(
            decision.is_approved(),
            "default policy must approve a valid plan"
        );
    }

    #[test]
    fn test_approval_required_error_contains_approval() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline =
            Pipeline::new(&scope).with_policy(crate::agent::Policy::default().auto_approve(false));

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        match result {
            Err(PipelineError::ApprovalRequired(approval)) => {
                assert_eq!(approval.plan_id, plan.id);
                assert!(approval.verify(&plan));
            }
            _ => panic!("expected ApprovalRequired, got {:?}", result),
        }
    }

    #[test]
    fn test_create_approval_for_valid_plan() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let approval = pipeline
            .create_approval(&plan, &validation)
            .expect("valid plan should produce approval");
        assert_eq!(approval.plan_id, plan.id);
    }

    #[test]
    fn test_create_approval_rejects_invalid_plan() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        let source = scope.join("doc.pdf");
        let dest = scope.join("Documents").join("doc.pdf");
        fs::write(&source, "test").unwrap();

        let mut validation = ValidationResult {
            plan_id: "test".to_string(),
            scope: scope.clone(),
            validated_operations: vec![ValidatedOperation {
                operation: FileSystemOperation::Move {
                    source: source.clone(),
                    dest: dest.clone(),
                },
                status: ValidationStatus::Invalid("test".to_string()),
                warnings: vec![],
                dependencies: vec![],
            }],
            summary: crate::agent::ValidationSummary::new(),
            has_blocked: false,
            has_conflicts: false,
            has_invalid: true,
            has_warnings: false,
            executable_operations: 0,
        };
        validation.summary.invalid = 1;
        validation.summary.total = 1;

        let plan = OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "test".to_string(),
            recommendation_id: "rec".to_string(),
            scope: scope.clone(),
            operations: vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        };

        let pipeline = Pipeline::new(&scope);
        let result = pipeline.create_approval(&plan, &validation);
        assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    }

    #[test]
    fn test_apply_with_approval_succeeds() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline =
            Pipeline::new(&scope).with_policy(crate::agent::Policy::default().auto_approve(false));

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        // Default policy with auto_approve=false → RequiresApproval
        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );
        assert!(matches!(result, Err(PipelineError::ApprovalRequired(_))));

        // Create approval and apply
        let approval = pipeline
            .create_approval(&plan, &validation)
            .expect("should create approval");

        let source = scope.join("readme.txt");
        let dest = scope.join("Documents").join("readme.txt");

        let apply_result = pipeline
            .apply_with_approval(
                &plan,
                &approval,
                &ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("apply_with_approval should succeed");

        assert!(apply_result.is_complete);
        assert!(!source.exists(), "source should be moved");
        assert!(dest.exists(), "dest should exist at full file path");
    }

    #[test]
    fn test_apply_with_approval_mismatch_rejected() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        // Create approval for plan A
        let approval = pipeline.create_approval(&plan, &validation).unwrap();

        // Tamper: change plan_id
        let mut modified_plan = plan.clone();
        modified_plan.id = "tampered-plan-id".to_string();

        let result = pipeline.apply_with_approval(
            &modified_plan,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );
        assert!(
            matches!(result, Err(PipelineError::ApprovalMismatch)),
            "approval must not match modified plan"
        );
    }

    #[test]
    fn test_apply_with_approval_re_validates_filesystem() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        // Create approval before filesystem changes
        let approval = Approval::for_plan(&plan);

        // Simulate filesystem change: delete the source file
        let move_op = plan.operations.iter().find(|op| {
            matches!(op, FileSystemOperation::Move { source, .. } if *source == scope.join("readme.txt"))
        });
        if let Some(FileSystemOperation::Move { source, .. }) = move_op {
            std::fs::remove_file(source).unwrap();
        }

        // apply_with_approval should re-validate and reject (source now missing)
        let result = pipeline.apply_with_approval(
            &plan,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );
        assert!(
            result.is_err(),
            "fresh validation must catch filesystem changes"
        );
        // Approval is valid but validation now detects INVALID → policy rejects
        assert!(
            matches!(result, Err(PipelineError::PolicyRejected)),
            "stale plan after filesystem change must be rejected by policy: {:?}",
            result
        );
    }

    #[test]
    fn test_approval_force_cannot_bypass_approval_required() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("readme.txt"), "text").unwrap();
        fs::create_dir_all(scope.join("Documents")).unwrap();

        let pipeline =
            Pipeline::new(&scope).with_policy(crate::agent::Policy::default().auto_approve(false));

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        // force=true must NOT bypass RequiresApproval from apply()
        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );
        assert!(
            matches!(result, Err(PipelineError::ApprovalRequired(_))),
            "force cannot bypass RequiresApproval from apply()"
        );
    }

    #[test]
    fn test_approval_serialization_round_trip() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();

        let plan = OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "serial-plan".to_string(),
            recommendation_id: "rec".to_string(),
            scope: scope.clone(),
            operations: vec![FileSystemOperation::Move {
                source: scope.join("doc.pdf"),
                dest: scope.join("Documents").join("doc.pdf"),
            }],
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        };

        let approval = Approval::for_plan(&plan);
        let json = serde_json::to_string(&approval).expect("should serialize");
        let deserialized: Approval = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(approval.plan_id, deserialized.plan_id);
        assert!(
            deserialized.verify(&plan),
            "deserialized approval must verify"
        );
    }

    fn collect_files(scope: &PathBuf) -> std::collections::HashSet<(String, bool)> {
        let mut files = std::collections::HashSet::new();
        let mut dirs = vec![scope.clone()];
        while let Some(dir) = dirs.pop() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let is_file = path.is_file();
                    let rel = path
                        .strip_prefix(scope)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    files.insert((rel, is_file));
                    if path.is_dir() {
                        dirs.push(path);
                    }
                }
            }
        }
        files
    }

    #[derive(Clone)]
    struct TestLlmProvider {
        success_response: Option<String>,
    }

    impl LlmProvider for TestLlmProvider {
        fn chat(&self, _messages: &[ChatMessage]) -> Result<String, LlmError> {
            match &self.success_response {
                Some(s) => Ok(s.clone()),
                None => Err(LlmError::ProviderError("Mock provider error".to_string())),
            }
        }

        fn name(&self) -> &str {
            "test-llm"
        }
    }

    fn make_llm_classifier(json_response: &str) -> LlmClassifier {
        let provider = TestLlmProvider {
            success_response: Some(json_response.to_string()),
        };
        LlmClassifier::new(Arc::new(provider))
    }

    fn make_error_llm_classifier() -> LlmClassifier {
        let provider = TestLlmProvider {
            success_response: None,
        };
        LlmClassifier::new(Arc::new(provider))
    }

    fn create_llm_test_scope(dir: &tempfile::TempDir) -> PathBuf {
        let scope = dir.path().join("downloads");
        fs::create_dir_all(&scope).unwrap();

        fs::write(scope.join("readme.txt"), "content").unwrap();

        fs::create_dir_all(scope.join("Documents")).unwrap();

        scope
    }

    fn pipeline_with_llm(scope: &Path, json_response: &str) -> Pipeline {
        let classifier = make_llm_classifier(json_response);
        Pipeline::new(scope).with_llm_classifier(classifier)
    }

    #[test]
    fn test_llm_organize_dry_run() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let options = PipelineOptions {
            dry_run: true,
            execute: false,
            force: false,
            user_intent: None,
        };

        let result = pipeline
            .run("Organize this folder by category", &options)
            .unwrap();
        assert!(result.apply.is_none());
        assert!(result.plan.dry_run);
        assert!(!result.preview.is_empty());
        assert!(!result.plan.operations.is_empty());
    }

    #[test]
    fn test_llm_organize_full_execution() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let options = PipelineOptions {
            dry_run: false,
            execute: true,
            force: false,
            user_intent: None,
        };

        let result = pipeline
            .run("Organize this folder by category", &options)
            .unwrap();
        assert!(result.apply.is_some());
        assert!(result.apply.as_ref().unwrap().is_complete);
    }

    #[test]
    fn test_llm_classification_move_existing() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();

        let classification = analysis.classification_results.first().unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
        assert!(classification.selected_candidate.is_some());
        assert_eq!(
            classification.proposed_category_name.as_deref(),
            Some("Documents")
        );

        let has_move = recommendation
            .proposed_operations
            .iter()
            .any(|op| matches!(op, ProposedOperation::MoveCategory { to_category, .. } if to_category == "Documents"));
        assert!(
            has_move,
            "recommendation should have MoveCategory to Documents"
        );
    }

    #[test]
    fn test_llm_classification_leave_unclassified() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(&scope, r#"[]"#);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();

        let classification = analysis.classification_results.first().unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::LeaveUnclassified
        );
        assert!(classification.selected_candidate.is_none());

        let has_mutation = recommendation.proposed_operations.iter().any(|op| {
            matches!(
                op,
                ProposedOperation::MoveCategory { .. }
                    | ProposedOperation::CreateCategory { .. }
                    | ProposedOperation::ArchiveFiles { .. }
            )
        });
        assert!(
            !has_mutation,
            "no mutation operations for LeaveUnclassified"
        );
    }

    #[test]
    fn test_llm_provider_error_stops_pipeline() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let classifier = make_error_llm_classifier();
        let pipeline = Pipeline::new(&scope).with_llm_classifier(classifier);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let result = pipeline.analyze(&intent);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::Analysis(AnalyzerError::ClassificationFailed(_))
        ));
    }

    #[test]
    fn test_llm_classifier_injection_via_pipeline() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let classifier = make_llm_classifier(
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.9}]}"#,
        );
        let pipeline = Pipeline::new(&scope).with_llm_classifier(classifier);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        let classification = analysis.classification_results.first().unwrap();
        assert_eq!(
            classification.decision,
            ClassificationDecision::MoveExisting
        );
        assert!(
            classification.confidence >= 0.8,
            "confidence from LLM should be 0.9, got {}",
            classification.confidence
        );
    }

    #[test]
    fn test_llm_classification_integrity_passes() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        let validation = pipeline.validate(&plan);
        assert!(!validation.has_invalid);
        assert!(!validation.has_conflicts);
    }

    #[test]
    fn test_non_llm_regression_rule_based() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = Pipeline::new(&scope);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        let classification = analysis.classification_results.first().unwrap();
        assert!(
            classification.provider.is_none(),
            "rule-based classification should not set provider"
        );
        assert!(
            classification.model.is_none(),
            "rule-based classification should not set model"
        );
    }

    #[test]
    fn test_llm_classification_empty_candidates() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("empty_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "content").unwrap();

        let classifier = make_llm_classifier(
            r#"{"classifications":[{"path":"doc.pdf","category":"Documents","confidence":0.95}]}"#,
        );
        let pipeline = Pipeline::new(&scope).with_llm_classifier(classifier);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        assert!(
            analysis.candidate_categories.is_empty(),
            "no subdirectories means no candidate categories"
        );
        assert!(
            analysis.classification_results.is_empty(),
            "no candidates means no classification results"
        );
    }

    #[test]
    fn test_llm_classification_confidence_preserved() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        let classification = analysis.classification_results.first().unwrap();
        assert_eq!(classification.confidence, 0.95);
        assert_eq!(classification.confidence_band, ConfidenceBand::High);
    }

    #[test]
    fn test_llm_classification_unsupported_category() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"InventedCategory","confidence":0.9}]}"#,
        );

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let result = pipeline.analyze(&intent);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::Analysis(AnalyzerError::ClassificationFailed(_))
        ));
    }

    #[test]
    fn test_llm_classification_malformed_json() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let pipeline = pipeline_with_llm(&scope, r#"this is not valid json"#);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let result = pipeline.analyze(&intent);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PipelineError::Analysis(AnalyzerError::ClassificationFailed(_))
        ));
    }

    #[test]
    fn test_llm_dry_run_no_mutation() {
        let dir = tempdir().unwrap();
        let scope = create_llm_test_scope(&dir);

        let source_file = scope.join("readme.txt");
        assert!(source_file.exists());

        let pipeline = pipeline_with_llm(
            &scope,
            r#"{"classifications":[{"path":"readme.txt","category":"Documents","confidence":0.95}]}"#,
        );

        let files_before = collect_files(&scope);

        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);

        let result = pipeline
            .apply(
                &plan,
                &validation,
                &ApplyOptions {
                    force: false,
                    dry_run: true,
                },
            )
            .unwrap();

        assert!(!result.is_complete, "dry-run must not be complete");
        assert!(
            result.log.entries.is_empty(),
            "dry-run must not log operations"
        );

        let files_after = collect_files(&scope);
        assert_eq!(
            files_before, files_after,
            "dry-run must not mutate filesystem"
        );
        assert!(
            source_file.exists(),
            "source file must still exist after dry-run"
        );
    }
}
