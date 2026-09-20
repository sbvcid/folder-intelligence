use folder_intelligence::{FileSystemOperation, OperationPlan, Pipeline, Policy, PolicyDecision};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageStatus {
    Planned,
    IntentionalNoOp,
    Omitted,
}

#[derive(Debug, Clone)]
pub struct CoverageReport {
    pub total_recommendations: usize,
    pub planned: usize,
    pub intentional_no_op: usize,
    pub omitted: usize,
    pub items: Vec<(String, CoverageStatus, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateEntry {
    Dir,
    File(Vec<u8>),
}

fn content_type_extensions(ct: &str) -> &'static [&'static str] {
    match ct {
        "documents" => &[
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv", "rtf",
        ],
        "images" => &[
            "jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "svg", "ico",
        ],
        "archives" => &["zip", "rar", "7z", "tar", "gz", "bz2", "xz"],
        "media" => &["mp4", "avi", "mkv", "mov", "mp3", "wav", "flac"],
        "code" => &[
            "py", "js", "ts", "rs", "go", "java", "c", "cpp", "h", "sh", "rb", "php",
        ],
        "installers" => &["exe", "msi", "dmg", "pkg", "deb", "rpm"],
        "data" => &["db", "sqlite", "dat", "bin"],
        "config" => &["conf", "cfg", "ini", "env", "properties"],
        "other" => &[],
        _ => &[],
    }
}

fn category_to_dir_name(name: &str) -> Option<&'static str> {
    match name {
        "document_storage" => Some("Documents"),
        "image_storage" => Some("Images"),
        "archive_storage" => Some("Archives"),
        "media_storage" => Some("Media"),
        "code_storage" => Some("Code"),
        "installer_storage" => Some("Installers"),
        "data_storage" => Some("Data"),
        "config_storage" => Some("Config"),
        "misc_storage" => Some("Misc"),
        _ => None,
    }
}

pub fn evaluate_coverage(
    recommendation: &folder_intelligence::Recommendation,
    plan: &OperationPlan,
    _analysis: &folder_intelligence::TaskAnalysis,
) -> CoverageReport {
    let mut items = Vec::new();
    let mut planned = 0;
    let mut intentional_no_op = 0;
    let mut omitted = 0;

    for op in &recommendation.proposed_operations {
        match op {
            folder_intelligence::ProposedOperation::MoveCategory {
                content_type,
                file_count,
                to_category,
                ..
            } => {
                if *file_count == 0 {
                    intentional_no_op += 1;
                    items.push((
                        format!("MoveCategory({})", content_type),
                        CoverageStatus::IntentionalNoOp,
                        "Zero file count — no files to move".to_string(),
                    ));
                } else {
                    let exts = content_type_extensions(content_type);
                    let expected_dir = category_to_dir_name(to_category);

                    let has_matching_op = plan.operations.iter().any(|plan_op| {
                        if let FileSystemOperation::Move { source, dest } = plan_op {
                            let source_ext_match = source
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| {
                                    let lower = e.to_lowercase();
                                    exts.contains(&lower.as_str())
                                })
                                .unwrap_or(false);

                            if !source_ext_match {
                                return false;
                            }

                            if let Some(dir_name) = expected_dir {
                                dest.parent()
                                    .and_then(|p| p.file_name())
                                    .and_then(|n| n.to_str())
                                    == Some(dir_name)
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    });

                    if has_matching_op {
                        planned += 1;
                        items.push((
                            format!("MoveCategory({})", content_type),
                            CoverageStatus::Planned,
                            "Matching move operation found in plan".to_string(),
                        ));
                    } else {
                        omitted += 1;
                        items.push((
                            format!("MoveCategory({})", content_type),
                            CoverageStatus::Omitted,
                            "Recommended category move has no corresponding plan operation"
                                .to_string(),
                        ));
                    }
                }
            }
            folder_intelligence::ProposedOperation::CreateCategory { name, .. } => {
                let expected_dir = category_to_dir_name(name);

                let has_create = expected_dir.is_some_and(|dir_name| {
                    plan.operations.iter().any(|op| {
                        if let FileSystemOperation::CreateDir { path } = op {
                            path.file_name().and_then(|n| n.to_str()) == Some(dir_name)
                        } else {
                            false
                        }
                    })
                });

                if has_create {
                    planned += 1;
                    items.push((
                        format!("CreateCategory({})", name),
                        CoverageStatus::Planned,
                        "Category directory created in plan".to_string(),
                    ));
                } else {
                    omitted += 1;
                    items.push((
                        format!("CreateCategory({})", name),
                        CoverageStatus::Omitted,
                        "Category creation has no corresponding plan operation".to_string(),
                    ));
                }
            }
            folder_intelligence::ProposedOperation::PreserveDirectory { path } => {
                intentional_no_op += 1;
                items.push((
                    format!("PreserveDirectory({})", path.display()),
                    CoverageStatus::IntentionalNoOp,
                    "Directory preservation requested — no mutation".to_string(),
                ));
            }
            folder_intelligence::ProposedOperation::ArchiveFiles { category, .. } => {
                let expected_source = plan.scope.join(category);
                let has_archive = plan.operations.iter().any(|op| {
                    if let FileSystemOperation::Move { source, .. } = op {
                        source == &expected_source
                    } else {
                        false
                    }
                });

                if has_archive {
                    planned += 1;
                    items.push((
                        format!("ArchiveFiles({})", category),
                        CoverageStatus::Planned,
                        "Archive move operation found in plan".to_string(),
                    ));
                } else {
                    omitted += 1;
                    items.push((
                        format!("ArchiveFiles({})", category),
                        CoverageStatus::Omitted,
                        "Archive has no corresponding plan operation".to_string(),
                    ));
                }
            }
            folder_intelligence::ProposedOperation::LeaveUnclassified { file_count, reason } => {
                if *file_count > 0 {
                    intentional_no_op += 1;
                    items.push((
                        format!("LeaveUnclassified({})", reason),
                        CoverageStatus::IntentionalNoOp,
                        format!(
                            "Intentional unclassified: {} files left in place",
                            file_count
                        ),
                    ));
                }
            }
        }
    }

    let total_recommendations = items.len();

    CoverageReport {
        total_recommendations,
        planned,
        intentional_no_op,
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
    before_state: &[(PathBuf, StateEntry)],
) -> ExecutionVerificationReport {
    let mut checks = Vec::new();
    let mut failed = false;
    let mut failure_msgs = Vec::new();

    let before_map: HashMap<&Path, &StateEntry> =
        before_state.iter().map(|(p, s)| (p.as_path(), s)).collect();

    for op in &plan.operations {
        match op {
            FileSystemOperation::Move { source, dest } => {
                let source_existed_before = before_map.get(source.as_path()).is_some();

                let source_exists_now = source.exists();
                if source_exists_now {
                    if !failed {
                        failed = true;
                    }
                    let msg = format!("Source still exists after move: {}", source.display());
                    failure_msgs.push(msg.clone());
                    checks.push((
                        format!("Move source removed: {}", source.display()),
                        false,
                        msg,
                    ));
                } else if source_existed_before {
                    checks.push((
                        format!("Move source removed: {}", source.display()),
                        true,
                        "Source existed before and was successfully removed".to_string(),
                    ));
                } else {
                    failed = true;
                    let msg = format!(
                        "Source did not exist before move (plan may be stale): {}",
                        source.display()
                    );
                    failure_msgs.push(msg.clone());
                    checks.push((
                        format!("Move source existed before: {}", source.display()),
                        false,
                        msg,
                    ));
                }

                let dest_exists_now = dest.exists();
                let dest_is_file = dest.is_file();
                if !dest_exists_now || !dest_is_file {
                    failed = true;
                    let msg = format!("Destination missing or not a file: {}", dest.display());
                    failure_msgs.push(msg.clone());
                    checks.push((
                        format!("Move destination valid: {}", dest.display()),
                        false,
                        msg,
                    ));
                } else if let Some(StateEntry::File(before_content)) =
                    before_map.get(source.as_path())
                {
                    if let Ok(after_content) = std::fs::read(dest) {
                        if &after_content != before_content {
                            failed = true;
                            let msg = format!(
                                "Content mismatch at destination: {} (expected {} bytes, got {} bytes)",
                                dest.display(),
                                before_content.len(),
                                after_content.len()
                            );
                            failure_msgs.push(msg.clone());
                            checks.push((
                                format!("Content preserved: {}", dest.display()),
                                false,
                                msg,
                            ));
                        } else {
                            checks.push((
                                format!("Content preserved: {}", dest.display()),
                                true,
                                "Destination content matches source content".to_string(),
                            ));
                        }
                    } else {
                        failed = true;
                        let msg = format!("Could not read destination file: {}", dest.display());
                        failure_msgs.push(msg.clone());
                        checks.push((
                            format!("Destination file readable: {}", dest.display()),
                            false,
                            msg,
                        ));
                    }
                } else {
                    checks.push((
                        format!("Destination valid: {}", dest.display()),
                        true,
                        "Destination exists and is a file".to_string(),
                    ));
                }
            }
            FileSystemOperation::CreateDir { path } => {
                let existed_before = before_map.get(path.as_path()).is_some();
                let exists_now = path.is_dir();

                if !exists_now {
                    failed = true;
                    let msg = format!("Created directory missing: {}", path.display());
                    failure_msgs.push(msg.clone());
                    checks.push((format!("Directory created: {}", path.display()), false, msg));
                } else if !existed_before {
                    checks.push((
                        format!("Directory created: {}", path.display()),
                        true,
                        "Directory did not exist before and now exists".to_string(),
                    ));
                } else {
                    checks.push((
                        format!("Directory preserved: {}", path.display()),
                        true,
                        "Directory existed before and still exists".to_string(),
                    ));
                }
            }
            FileSystemOperation::Delete { path, .. } => {
                let existed_before = before_map.get(path.as_path()).is_some();
                let exists_now = path.exists();

                if exists_now {
                    failed = true;
                    let msg = format!("Deleted path still exists: {}", path.display());
                    failure_msgs.push(msg.clone());
                    checks.push((format!("Path deleted: {}", path.display()), false, msg));
                } else if existed_before {
                    checks.push((
                        format!("Path deleted: {}", path.display()),
                        true,
                        "Path existed before and was successfully deleted".to_string(),
                    ));
                } else {
                    checks.push((
                        format!("Path absent: {}", path.display()),
                        true,
                        "Path was not present before or after (no-op for delete)".to_string(),
                    ));
                }
            }
        }
    }

    if plan.operations.is_empty() {
        let after_state = capture_state(&plan.scope);
        if before_state.len() != after_state.len() {
            failed = true;
            let msg = format!(
                "Filesystem changed during no-op plan (before: {} entries, after: {} entries)",
                before_state.len(),
                after_state.len()
            );
            failure_msgs.push(msg.clone());
            checks.push(("No-op plan: filesystem unchanged".to_string(), false, msg));
        } else {
            let mut changed = false;
            for (after_path, after_entry) in &after_state {
                if let Some(before_entry) = before_map.get(after_path.as_path()) {
                    if **before_entry != *after_entry {
                        changed = true;
                        let msg = format!(
                            "Filesystem entry changed during no-op plan: {}",
                            after_path.display()
                        );
                        failure_msgs.push(msg.clone());
                        checks.push((
                            format!("No-op unchanged: {}", after_path.display()),
                            false,
                            msg,
                        ));
                    }
                } else {
                    changed = true;
                    let msg = format!(
                        "New filesystem entry appeared during no-op plan: {}",
                        after_path.display()
                    );
                    failure_msgs.push(msg.clone());
                    checks.push((
                        format!("No-op no new entries: {}", after_path.display()),
                        false,
                        msg,
                    ));
                }
            }
            if !changed {
                checks.push((
                    "No-op plan: filesystem unchanged".to_string(),
                    true,
                    format!(
                        "No mutations: before and after states match ({} entries)",
                        before_state.len()
                    ),
                ));
            }
        }
    }

    let status = if failed {
        VerificationStatus::Failed(failure_msgs.join("; "))
    } else {
        VerificationStatus::Passed
    };

    ExecutionVerificationReport { status, checks }
}

fn capture_state(dir: &Path) -> Vec<(PathBuf, StateEntry)> {
    let mut state = Vec::new();
    capture_rec(dir, &mut state);
    state.sort_by(|a, b| a.0.cmp(&b.0));
    state
}

fn capture_rec(dir: &Path, state: &mut Vec<(PathBuf, StateEntry)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                state.push((path.clone(), StateEntry::Dir));
                capture_rec(&path, state);
            } else if path.is_file() {
                let content = std::fs::read(&path).unwrap_or_default();
                state.push((path, StateEntry::File(content)));
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
        fs::write(dir.join("readme.png"), "png").unwrap();
    }

    fn make_test_plan(scope: &Path, operations: Vec<FileSystemOperation>) -> OperationPlan {
        OperationPlan {
            id: "test-plan".to_string(),
            recommendation_id: "rec-test".to_string(),
            scope: scope.to_path_buf(),
            operations,
            estimated_impact: folder_intelligence::EstimatedImpact {
                files_moved: 0,
                dirs_created: 0,
                files_deleted: 0,
                dirs_affected: 0,
                total_bytes: 0,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(folder_intelligence::PlanValidationContext::default()),
        }
    }

    #[test]
    fn test_coverage_complete() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let report = evaluate_coverage(&recommendation, &plan, &analysis);
        assert_eq!(
            report.omitted, 0,
            "Should have 0 omitted items, got {:?}",
            report.items
        );
        assert!(report.planned > 0, "Should have planned operations");
    }

    #[test]
    fn test_coverage_intentional_no_op() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        let report = evaluate_coverage(&recommendation, &plan, &analysis);

        assert!(
            report.intentional_no_op > 0,
            "PreserveDirectory operations should be classified as IntentionalNoOp, got {} items",
            report.items.len()
        );

        let no_op_items: Vec<_> = report
            .items
            .iter()
            .filter(|(_, status, _)| *status == CoverageStatus::IntentionalNoOp)
            .collect();

        for (name, _, _) in &no_op_items {
            assert!(
                name.starts_with("PreserveDirectory"),
                "IntentionalNoOp item should be PreserveDirectory, got: {}",
                name
            );
        }

        assert!(
            !plan
                .operations
                .iter()
                .any(|op| matches!(op, FileSystemOperation::Delete { .. })),
            "Plan must not contain Delete operations for preserved directories"
        );
    }

    #[test]
    fn test_coverage_omitted_when_move_operations_removed() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        let no_op_count_before = report_intentional_no_op_count(&recommendation, &plan, &analysis);

        let mut tampered_plan = plan.clone();
        tampered_plan
            .operations
            .retain(|op| !matches!(op, FileSystemOperation::Move { .. }));

        let report = evaluate_coverage(&recommendation, &tampered_plan, &analysis);

        let move_category_count = recommendation
            .proposed_operations
            .iter()
            .filter(|op| matches!(op, folder_intelligence::ProposedOperation::MoveCategory { file_count, .. } if *file_count > 0))
            .count();

        assert!(
            move_category_count > 0,
            "Fixture should produce MoveCategory operations with files"
        );

        assert!(
            report.omitted == move_category_count,
            "All MoveCategory(items with files should be Omitted when Move operations are removed, got {} omitted vs {} move categories",
            report.omitted,
            move_category_count
        );

        assert!(
            report.planned == 0,
            "No planned items expected when all operations are removed, got {}",
            report.planned
        );

        assert!(
            report.intentional_no_op == no_op_count_before,
            "IntentionalNoOp count should be unchanged, got {} before and {} after",
            no_op_count_before,
            report.intentional_no_op
        );
    }

    fn report_intentional_no_op_count(
        recommendation: &folder_intelligence::Recommendation,
        plan: &OperationPlan,
        analysis: &folder_intelligence::TaskAnalysis,
    ) -> usize {
        evaluate_coverage(recommendation, plan, analysis).intentional_no_op
    }

    #[test]
    fn test_coverage_wrong_extension_not_matched() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        for op in &mut plan.operations {
            if let FileSystemOperation::Move { source, .. } = op {
                if source.extension().map(|e| e == "txt").unwrap_or(false) {
                    *source = dir.path().join("readme.png");
                }
            }
        }

        let report = evaluate_coverage(&recommendation, &plan, &analysis);

        let docs_move_count = recommendation
            .proposed_operations
            .iter()
            .filter(|op| matches!(op, folder_intelligence::ProposedOperation::MoveCategory { content_type, file_count, .. }
                if content_type == "documents" && *file_count > 0))
            .count();

        assert!(
            docs_move_count > 0,
            "Fixture should produce MoveCategory for documents"
        );

        assert!(
            report.omitted >= docs_move_count,
            "MoveCategory(documents) should be Omitted when source extension is wrong (png instead of txt), got {} omitted",
            report.omitted
        );
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
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));
        let validation = pipeline.validate(&plan);

        let apply_result = pipeline
            .apply(
                &plan,
                &validation,
                &folder_intelligence::ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("apply should succeed");

        assert!(apply_result.log.success_count > 0);

        let verification = verify_execution(&plan, &snapshot);
        assert_eq!(
            verification.status,
            VerificationStatus::Passed,
            "Execution verification failed: {:?}",
            verification.checks
        );
    }

    #[test]
    fn test_verification_false_success() {
        let dir = tempdir().unwrap();
        let scope = dir.path().to_path_buf();

        let source = scope.join("readme.txt");
        let dest = scope.join("Documents").join("readme.txt");
        fs::create_dir_all(scope.join("Documents")).unwrap();
        fs::write(&source, "content").unwrap();

        let before = capture_state(&scope);

        let plan = make_test_plan(
            &scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        let verification = verify_execution(&plan, &before);

        assert!(
            matches!(verification.status, VerificationStatus::Failed(_)),
            "Verification must fail when filesystem unchanged despite claimed success: {:?}",
            verification.checks
        );

        assert!(
            source.exists(),
            "Source must still exist (no execution occurred)"
        );
    }

    #[test]
    fn test_verification_wrong_result() {
        let dir = tempdir().unwrap();
        let scope = dir.path().to_path_buf();

        let source = scope.join("readme.txt");
        let dest = scope.join("Documents").join("readme.txt");
        fs::create_dir_all(scope.join("Documents")).unwrap();
        fs::write(&source, "original").unwrap();

        let before = capture_state(&scope);

        // Simulate wrong execution: source deleted, dest created with wrong content
        fs::remove_file(&source).unwrap();
        fs::write(&dest, "wrong content").unwrap();

        let plan = make_test_plan(
            &scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        let verification = verify_execution(&plan, &before);

        assert!(
            matches!(verification.status, VerificationStatus::Failed(_)),
            "Verification must fail when destination content does not match source: {:?}",
            verification.checks
        );
    }

    #[test]
    fn test_verification_intentional_no_op() {
        let dir = tempdir().unwrap();
        let scope = dir.path().to_path_buf();

        fs::write(scope.join("file1.txt"), "content1").unwrap();
        fs::write(scope.join("file2.png"), "content2").unwrap();

        let before = capture_state(&scope);

        let plan = make_test_plan(&scope, vec![]);

        let verification = verify_execution(&plan, &before);

        assert_eq!(
            verification.status,
            VerificationStatus::Passed,
            "No-op plan with unchanged filesystem should pass verification: {:?}",
            verification.checks
        );

        let after = capture_state(&scope);
        assert_eq!(
            before.len(),
            after.len(),
            "Filesystem must be unchanged for no-op plan"
        );
    }

    #[test]
    fn test_stale_plan_source_disappeared_blocked() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let a_pdf = dir.path().join("readme.txt");
        assert!(a_pdf.exists());
        fs::remove_file(&a_pdf).unwrap();

        let validation = pipeline.validate(&plan);
        assert!(
            validation.has_invalid,
            "Plan validation must catch missing source file"
        );

        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert_eq!(
            decision,
            PolicyDecision::Rejected,
            "Policy must reject invalid plan"
        );
    }

    #[test]
    fn test_stale_plan_destination_conflict_blocked() {
        let dir = tempdir().unwrap();
        create_benchmark_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let mut plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let docs_a = dir.path().join("Documents").join("readme.txt");
        fs::write(&docs_a, "blocking conflict").unwrap();

        plan.operations.push(FileSystemOperation::Move {
            source: dir.path().join("readme.png"),
            dest: docs_a.clone(),
        });

        let validation = pipeline.validate(&plan);
        assert!(
            validation.summary.conflicts > 0 || validation.has_conflicts || validation.has_invalid,
            "Plan validation must catch destination conflict"
        );

        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert_eq!(
            decision,
            PolicyDecision::Rejected,
            "Policy must reject plan with conflict"
        );
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
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));
        let validation = pipeline.validate(&plan);

        let apply_result = pipeline
            .apply(
                &plan,
                &validation,
                &folder_intelligence::ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .expect("apply should succeed");

        let log = &apply_result.log;
        assert_eq!(log.plan_id, plan.id);
        assert_eq!(log.total_entries, log.entries.len());
        assert_eq!(
            log.success_count + log.failure_count + log.skipped_count,
            log.total_entries
        );

        let log_path = dir.path().join("operation-log.json");
        log.save(&log_path).expect("should save log");
        let reloaded =
            folder_intelligence::OperationLog::load(&log_path).expect("should reload log");
        assert_eq!(log.plan_id, reloaded.plan_id);
        assert_eq!(log.success_count, reloaded.success_count);
    }
}
