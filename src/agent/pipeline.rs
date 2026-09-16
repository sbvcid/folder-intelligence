use crate::agent::intent::{TaskIntent, TaskIntentParser, IntentParseError, Goal};
use crate::agent::analysis::{EvidenceAnalyzer, TaskAnalysis, AnalyzerError};
use crate::agent::recommendation::{RecommendationEngine, Recommendation, RecommendationError};
use crate::agent::clarification::ClarificationEngine;
use crate::agent::plan::{OperationPlan, PlanGenerator, PlanError};
use crate::agent::validate::{PlanValidator, ValidationResult, PlanPreview};
use crate::agent::executor::{Executor, ApplyResult, ApplyError, OperationLog, UndoResult};
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
    executor: Executor,
}

impl Default for Pipeline {
    fn default() -> Self {
        Pipeline {
            parser: TaskIntentParser::default(),
            analyzer: EvidenceAnalyzer,
            recommender: RecommendationEngine,
            clarifier: ClarificationEngine,
            planner: PlanGenerator,
            validator: PlanValidator,
            executor: Executor,
        }
    }
}

impl Pipeline {
    pub fn new(scope: &Path) -> Self {
        Pipeline {
            parser: TaskIntentParser::new(scope.to_path_buf()),
            analyzer: EvidenceAnalyzer,
            recommender: RecommendationEngine,
            clarifier: ClarificationEngine,
            planner: PlanGenerator,
            validator: PlanValidator,
            executor: Executor,
        }
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
            return Err(PipelineError::Analysis(
                AnalyzerError::ScopeNotADirectory(scope),
            ));
        }

        let limits = ScanLimits::default();
        let scanner = Scanner::with_limits(&scope, limits);
        let scan_result = scanner
            .scan()
            .map_err(|e| PipelineError::Io(e.to_string()))?;

        if scan_result.evidence.is_empty() {
            return Err(PipelineError::Analysis(
                AnalyzerError::ScopeNotScannable(scope),
            ));
        }

        let scope_evidence = scan_result.evidence.into_iter().next().unwrap();
        let scan_metadata = scan_result.metadata;

        self.analyzer
            .analyze_with_evidence(intent, scope_evidence, scan_metadata, start)
            .map_err(PipelineError::Analysis)
    }

    /// Analyze using pre-scanned evidence (no filesystem re-scan).
    #[allow(dead_code)]
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
    ) -> Result<OperationPlan, PipelineError> {
        let mut plan = self.planner.generate(recommendation, analysis, &[])?;
        plan.dry_run = false;
        Ok(plan)
    }

    /// Validate a plan against intent constraints.
    pub fn validate(
        &self,
        plan: &OperationPlan,
        intent: &TaskIntent,
    ) -> ValidationResult {
        self.validator.validate(plan, intent)
    }

    /// Render a human-readable preview of a plan and its validation.
    pub fn preview(&self, plan: &OperationPlan, validation: &ValidationResult) -> String {
        PlanPreview.render(plan, validation)
    }

    /// Apply a validated plan to the filesystem.
    ///
    /// This is the ONLY method that mutates the filesystem.
    /// Plan must have `dry_run = false`.
    /// INVALID and CONFLICT operations are always rejected.
    /// BLOCKED operations are skipped when `force = true`.
    pub fn apply(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        options: &ApplyOptions,
    ) -> Result<ApplyResult, PipelineError> {
        if plan.dry_run {
            return Err(PipelineError::Apply(ApplyError::DryRunFlagSet));
        }

        if validation.has_invalid {
            return Err(PipelineError::Apply(ApplyError::InvalidPlan(
                "Plan has INVALID operations (missing source, path outside scope, etc.)"
                    .to_string(),
            )));
        }

        if validation.has_conflicts {
            return Err(PipelineError::Apply(ApplyError::InvalidPlan(
                "Plan has CONFLICT operations (overlapping destinations, circular moves).".to_string(),
            )));
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
            });
        }

        self.executor
            .execute_with_options(plan, validation, options.force)
            .map_err(PipelineError::Apply)
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

    /// Convenience method: run the full pipeline in one call.
    ///
    /// By default, this runs to plan generation and validation but does NOT execute.
    /// Set `options.execute = true` to apply (requires explicit approval).
    #[allow(dead_code)]
    pub fn run(
        &self,
        request: &str,
        options: &PipelineOptions,
    ) -> Result<PipelineResult, PipelineError> {
        let intent = self.parse_intent(request)?;
        let analysis = self.analyze(&intent)?;
        let recommendation = self.recommend(&intent, &analysis)?;
        let mut plan = self.plan(&recommendation, &analysis)?;
        let validation = self.validate(&plan, &intent);
        let preview = self.preview(&plan, &validation);

        plan.dry_run = options.dry_run;

        let apply = if options.dry_run {
            None
        } else if options.execute {
            Some(self.apply(&plan, &validation, &ApplyOptions {
                force: options.force,
                dry_run: false,
            })?)
        } else {
            None
        };

        Ok(PipelineResult {
            intent,
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

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    pub force: bool,
    pub dry_run: bool,
    pub execute: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    pub force: bool,
    pub dry_run: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub intent: TaskIntent,
    pub analysis: TaskAnalysis,
    pub recommendation: Recommendation,
    pub plan: OperationPlan,
    pub validation: ValidationResult,
    pub preview: String,
    pub apply: Option<ApplyResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    IntentParse(String),
    Analysis(AnalyzerError),
    Recommendation(RecommendationError),
    Plan(PlanError),
    Apply(ApplyError),
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
    use std::fs;
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

        let result = pipeline.run("Organize this folder by category", &options).unwrap();
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
        };

        let result = pipeline.run("Organize this folder by category", &options).unwrap();
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
        };

        let result = pipeline.run("Organize this folder by category", &options).unwrap();
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();

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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let validation = pipeline.validate(&plan, &intent);

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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis).unwrap();
        plan.dry_run = true;
        let validation = pipeline.validate(&plan, &intent);

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        assert!(matches!(result, Err(PipelineError::Apply(ApplyError::DryRunFlagSet))));
    }

    #[test]
    fn test_pipeline_apply_rejects_invalid_even_with_force() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("doc.pdf"), "test").unwrap();

        let pipeline = Pipeline::new(&scope);
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let mut validation = pipeline.validate(&plan, &intent);
        validation.has_invalid = true;

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );

        assert!(matches!(
            result,
            Err(PipelineError::Apply(ApplyError::InvalidPlan(msg))) if msg.contains("INVALID")
        ));
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let validation = pipeline.validate(&plan, &intent);

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
        assert!(source_file.exists(), "dry-run apply must not mutate filesystem");
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let mut validation = pipeline.validate(&plan, &intent);
        validation.has_conflicts = true;

        let result = pipeline.apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: true,
                dry_run: false,
            },
        );

        assert!(matches!(
            result,
            Err(PipelineError::Apply(ApplyError::InvalidPlan(msg))) if msg.contains("CONFLICT")
        ));
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let mut validation = pipeline.validate(&plan, &intent);

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
        assert!(matches!(
            result,
            Err(PipelineError::Apply(ApplyError::InvalidPlan(msg))) if msg.contains("INVALID")
        ));
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis).unwrap();
        let validation = pipeline.validate(&plan, &intent);
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
        let intent = pipeline.parse_intent("Organize this folder by category").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let summary = pipeline.clarify(&recommendation);

        assert!(!summary.is_empty());
    }
}
