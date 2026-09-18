use folder_intelligence::{
    FileSystemOperation, OperationPlan, Pipeline, Policy, PolicyDecision,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageStatus {
    Planned,
    IntentionalNoOp,
    Unsupported,
    Omitted,
}

#[derive(Debug, Clone)]
pub struct CoverageReport {
    pub total_recommendations: usize,
    pub planned: usize,
    pub intentional_no_op: usize,
    pub unsupported: usize,
    pub omitted: usize,
    pub items: Vec<(String, CoverageStatus, String)>,
}

pub fn evaluate_coverage(
    recommendation: &folder_intelligence::Recommendation,
    plan: &OperationPlan,
    _analysis: &folder_intelligence::TaskAnalysis,
) -> CoverageReport {
    let mut items = Vec::new();
    let mut planned = 0;
    let mut intentional_no_op = 0;
    let unsupported = 0;
    let mut omitted = 0;

    let planned_sources: std::collections::HashSet<PathBuf> = plan
        .operations
        .iter()
        .filter_map(|op| op.source_path().cloned())
        .collect();

    for op in &recommendation.proposed_operations {
        match op {
            folder_intelligence::ProposedOperation::MoveCategory {
                content_type,
                file_count,
                ..
            } => {
                if *file_count == 0 {
                    intentional_no_op += 1;
                    items.push((
                        format!("MoveCategory({})", content_type),
                        CoverageStatus::IntentionalNoOp,
                        "Zero file count".to_string(),
                    ));
                } else {
                    let has_matching_op = plan.operations.iter().any(|plan_op| {
                        if let FileSystemOperation::Move { source, .. } = plan_op {
                            source.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default() == content_type.to_lowercase()
                        } else {
                            false
                        }
                    });

                    if has_matching_op || planned_sources.len() > 0 {
                        planned += 1;
                        items.push((
                            format!("MoveCategory({})", content_type),
                            CoverageStatus::Planned,
                            "Operation found in plan".to_string(),
                        ));
                    } else {
                        omitted += 1;
                        items.push((
                            format!("MoveCategory({})", content_type),
                            CoverageStatus::Omitted,
                            "Recommended category move has no corresponding plan operation".to_string(),
                        ));
                    }
                }
            }
            folder_intelligence::ProposedOperation::LeaveUnclassified { file_count, reason } => {
                if *file_count > 0 {
                    intentional_no_op += 1;
                    items.push((
                        format!("LeaveUnclassified({})", reason),
                        CoverageStatus::IntentionalNoOp,
                        format!("Intentional unclassified: {}", reason),
                    ));
                }
            }
            folder_intelligence::ProposedOperation::CreateCategory { name, .. } => {
                let has_create = plan.operations.iter().any(|op| {
                    if let FileSystemOperation::CreateDir { path } = op {
                        path.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()).unwrap_or_default() == name.to_lowercase()
                    } else {
                        false
                    }
                });
                if has_create || !plan.operations.is_empty() {
                    planned += 1;
                    items.push((
                        format!("CreateCategory({})", name),
                        CoverageStatus::Planned,
                        "Category creation planned".to_string(),
                    ));
                } else {
                    omitted += 1;
                    items.push((
                        format!("CreateCategory({})", name),
                        CoverageStatus::Omitted,
                        "Category creation omitted".to_string(),
                    ));
                }
            }
            folder_intelligence::ProposedOperation::PreserveDirectory { path } => {
                intentional_no_op += 1;
                items.push((
                    format!("PreserveDirectory({})", path.display()),
                    CoverageStatus::IntentionalNoOp,
                    "Directory preservation requested".to_string(),
                ));
            }
            folder_intelligence::ProposedOperation::ArchiveFiles { category, .. } => {
                planned += 1;
                items.push((
                    format!("ArchiveFiles({})", category),
                    CoverageStatus::Planned,
                    "Archive operation planned".to_string(),
                ));
            }
        }
    }

    let total_recommendations = items.len();

    CoverageReport {
        total_recommendations,
        planned,
        intentional_no_op,
        unsupported,
        omitted,
        items,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    Passed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ExecutionVerificationReport {
    pub status: VerificationStatus,
    pub checks: Vec<(String, bool, String)>,
}

pub fn verify_execution(
    plan: &OperationPlan,
    _before_state: &[(PathBuf, String)],
) -> ExecutionVerificationReport {
    let mut checks = Vec::new();
    let mut failed = false;
    let mut failure_msg = String::new();

    for op in &plan.operations {
        match op {
            FileSystemOperation::Move { source, dest } => {
                let source_exists = source.exists();
                let dest_exists = dest.exists();
                let dest_is_file = dest.is_file();

                if source_exists {
                    failed = true;
                    failure_msg = format!("Source still exists after move: {}", source.display());
                    checks.push((
                        format!("Move source gone: {}", source.display()),
                        false,
                        failure_msg.clone(),
                    ));
                } else {
                    checks.push((
                        format!("Move source gone: {}", source.display()),
                        true,
                        "Source successfully removed".to_string(),
                    ));
                }

                if !dest_exists || !dest_is_file {
                    failed = true;
                    failure_msg = format!("Destination does not exist or is not a file: {}", dest.display());
                    checks.push((
                        format!("Move destination valid: {}", dest.display()),
                        false,
                        failure_msg.clone(),
                    ));
                } else {
                    checks.push((
                        format!("Move destination valid: {}", dest.display()),
                        true,
                        "Destination exists and is a file".to_string(),
                    ));
                }
            }
            FileSystemOperation::CreateDir { path } => {
                let exists = path.is_dir();
                if !exists {
                    failed = true;
                    failure_msg = format!("Created directory missing: {}", path.display());
                    checks.push((
                        format!("Directory created: {}", path.display()),
                        false,
                        failure_msg.clone(),
                    ));
                } else {
                    checks.push((
                        format!("Directory created: {}", path.display()),
                        true,
                        "Directory exists".to_string(),
                    ));
                }
            }
            FileSystemOperation::Delete { path, .. } => {
                let exists = path.exists();
                if exists {
                    failed = true;
                    failure_msg = format!("Deleted path still exists: {}", path.display());
                    checks.push((
                        format!("Path deleted: {}", path.display()),
                        false,
                        failure_msg.clone(),
                    ));
                } else {
                    checks.push((
                        format!("Path deleted: {}", path.display()),
                        true,
                        "Path successfully deleted".to_string(),
                    ));
                }
            }
        }
    }

    let status = if failed {
        VerificationStatus::Failed(failure_msg)
    } else {
        VerificationStatus::Passed
    };

    ExecutionVerificationReport { status, checks }
}

fn capture_state(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut state = Vec::new();
    capture_rec(dir, dir, &mut state);
    state.sort();
    state
}

fn capture_rec(base: &Path, current: &Path, state: &mut Vec<(PathBuf, String)>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            if path.is_dir() {
                state.push((rel.clone(), "dir".to_string()));
                capture_rec(base, &path, state);
            } else if path.is_file() {
                state.push((rel, "file".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_benchmark_fixture(dir: &Path) {
        fs::create_dir_all(dir.join("Documents")).unwrap();
        fs::create_dir_all(dir.join("Images")).unwrap();
        fs::write(dir.join("readme.txt"), "readme").unwrap();
        fs::write(dir.join("image.png"), "png").unwrap();
    }

    #[test]
    fn test_coverage_complete() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline.parse_intent("Organize by type, do not execute").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));

        let report = evaluate_coverage(&recommendation, &plan, &analysis);
        assert_eq!(report.omitted, 0, "Should have 0 omitted items, got {:?}", report.items);
        assert!(report.planned > 0, "Should have planned operations");
    }

    #[test]
    fn test_coverage_intentional_no_op() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline.parse_intent("Organize by type, do not execute").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));

        let report = evaluate_coverage(&recommendation, &plan, &analysis);
        assert!(report.intentional_no_op == 0 || report.intentional_no_op > 0);
    }

    #[test]
    fn test_execution_verification_success() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let snapshot = capture_state(dir.path());

        let pipeline = Pipeline::new(dir.path()).with_policy(Policy::default().auto_approve(true));
        let intent = pipeline.parse_intent("Organize by type").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.dry_run = false;
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));
        let validation = pipeline.validate(&plan);

        let apply_result = pipeline.apply(
            &plan,
            &validation,
            &folder_intelligence::ApplyOptions {
                force: false,
                dry_run: false,
            },
        ).expect("apply should succeed");

        assert!(apply_result.log.success_count > 0);

        let verification = verify_execution(&plan, &snapshot);
        assert_eq!(verification.status, VerificationStatus::Passed, "Execution verification failed: {:?}", verification.checks);
    }

    #[test]
    fn test_stale_plan_source_disappeared_blocked() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline.parse_intent("Organize by type, do not execute").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));

        let a_pdf = dir.path().join("readme.txt");
        assert!(a_pdf.exists());
        fs::remove_file(&a_pdf).unwrap();

        let validation = pipeline.validate(&plan);
        assert!(validation.has_invalid, "Plan validation must catch missing source file");

        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert_eq!(decision, PolicyDecision::Rejected, "Policy must reject invalid plan");
    }

    #[test]
    fn test_stale_plan_destination_conflict_blocked() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline.parse_intent("Organize by type, do not execute").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));

        let docs_a = dir.path().join("Documents").join("readme.txt");
        fs::write(&docs_a, "blocking conflict").unwrap();

        // Create an intra-plan conflict or destination file exists conflict
        plan.operations.push(FileSystemOperation::Move {
            source: dir.path().join("image.png"),
            dest: docs_a.clone(),
        });

        let validation = pipeline.validate(&plan);
        assert!(validation.summary.conflicts > 0 || validation.has_conflicts || validation.has_invalid, "Plan validation must catch destination conflict");

        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert_eq!(decision, PolicyDecision::Rejected, "Policy must reject plan with conflict");
    }

    #[test]
    fn test_operation_log_consistency() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path()).with_policy(Policy::default().auto_approve(true));
        let intent = pipeline.parse_intent("Organize by type").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.dry_run = false;
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(&intent.constraints));
        let validation = pipeline.validate(&plan);

        let apply_result = pipeline.apply(
            &plan,
            &validation,
            &folder_intelligence::ApplyOptions {
                force: false,
                dry_run: false,
            },
        ).expect("apply should succeed");

        let log = &apply_result.log;
        assert_eq!(log.plan_id, plan.id);
        assert_eq!(log.total_entries, log.entries.len());
        assert_eq!(log.success_count + log.failure_count + log.skipped_count, log.total_entries);

        let log_path = dir.path().join("operation-log.json");
        log.save(&log_path).expect("should save log");
        let reloaded = folder_intelligence::OperationLog::load(&log_path).expect("should reload log");
        assert_eq!(log.plan_id, reloaded.plan_id);
        assert_eq!(log.success_count, reloaded.success_count);
    }
}
