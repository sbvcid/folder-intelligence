use folder_intelligence::{
    FileSystemOperation, Pipeline, PipelineError, Policy, PolicyDecision, Scanner,
};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod fixture {
    use std::fs;
    use std::path::Path;

    pub fn create_organization_benchmark(dir: &Path) {
        // Candidate category directories
        fs::create_dir_all(dir.join("Documents")).unwrap();
        fs::create_dir_all(dir.join("Projects")).unwrap();
        fs::create_dir_all(dir.join("Backup")).unwrap();
        fs::create_dir_all(dir.join("Temp")).unwrap();
        fs::create_dir_all(dir.join("Images")).unwrap();

        // Messy files directly in root (scope to be organized)
        fs::write(dir.join("invoice_2024.pdf"), "invoice content 2024").unwrap();
        fs::write(dir.join("invoice_2025.pdf"), "invoice content 2025").unwrap();
        fs::write(dir.join("tax_2024.pdf"), "tax pdf content").unwrap();
        fs::write(dir.join("contract_acme.pdf"), "contract content").unwrap();
        fs::write(dir.join("meeting_notes.docx"), "meeting notes content").unwrap();
        fs::write(dir.join("report-final.docx"), "report final content").unwrap();
        fs::write(dir.join("report-final-v2.docx"), "report final v2 content").unwrap();
        fs::write(dir.join("setup.exe"), "fake exe content").unwrap();
        fs::write(dir.join("installer.exe"), "installer content").unwrap();
        fs::write(dir.join("IMG_8231.jpg"), "fake jpg 8231").unwrap();
        fs::write(dir.join("IMG_8232.jpg"), "fake jpg 8232").unwrap();
        fs::write(dir.join("data_archive.zip"), "fake zip").unwrap();
        fs::write(dir.join("random.tmp"), "temp file").unwrap();
        fs::write(dir.join("cache.tmp"), "cache file").unwrap();
        fs::write(dir.join("notes_2024-01-15.md"), "notes content").unwrap();
        fs::write(dir.join("README.md"), "# Project README").unwrap();
        fs::write(dir.join("CHANGELOG.md"), "## Changelog").unwrap();
        fs::write(dir.join("LICENSE"), "MIT License").unwrap();
        fs::write(dir.join("todo.txt"), "todo list").unwrap();
        fs::write(dir.join("old_report.docx"), "old report").unwrap();
        fs::write(dir.join("old_invoice.pdf"), "old invoice").unwrap();
        fs::write(dir.join("文件_報告.pdf"), "chinese pdf").unwrap();
        fs::write(dir.join("圖片_照片.jpg"), "chinese jpg").unwrap();

        // Populate candidate directories with precedent files
        let docs = dir.join("Documents");
        fs::write(docs.join("doc_sample.pdf"), "doc sample").unwrap();
        fs::write(docs.join("doc_sample2.docx"), "doc sample 2").unwrap();

        let proj = dir.join("Projects");
        fs::write(proj.join("proj_sample.py"), "print()").unwrap();

        // Excluded directories
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        for i in 0..50 {
            fs::write(target.join(format!("build_{:03}.rs", i)), "build code")
                .unwrap();
        }
        fs::write(target.join("debug.log"), "debug log").unwrap();

        let node_modules = dir.join("node_modules");
        fs::create_dir_all(&node_modules).unwrap();
        fs::write(node_modules.join("index.js"), "js code").unwrap();

        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("config"), "[core]").unwrap();

        let build_dir = dir.join("build");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("compile.sh"), "build script").unwrap();

        let dist_dir = dir.join("dist");
        fs::create_dir_all(&dist_dir).unwrap();
        fs::write(dist_dir.join("app.js"), "compiled js").unwrap();
    }

    pub const EXPECTED_FILE_COUNT: u64 = 26;
    pub const EXPECTED_DIR_COUNT: u64 = 6;

    pub fn expected_file_count() -> u64 {
        EXPECTED_FILE_COUNT
    }

    pub fn expected_dir_count() -> u64 {
        EXPECTED_DIR_COUNT
    }
}

mod benchmark_layer {
    use folder_intelligence::{Policy, PolicyDecision, Pipeline};
    use folder_intelligence::{OperationPlan, ValidationResult};
    use std::path::Path;
    use std::time::Instant;

    pub struct BenchmarkMetrics {
        pub scan_duration_ms: u64,
        pub analyze_duration_ms: u64,
        pub recommend_duration_ms: u64,
        pub plan_duration_ms: u64,
        pub validation_duration_ms: u64,
        pub total_duration_ms: u64,
        pub scanned_files: u64,
        pub scanned_dirs: u64,
        pub dirs_skipped: u64,
        pub operations_count: usize,
        pub validation_has_invalid: bool,
        pub validation_has_conflicts: bool,
        pub policy_decision: PolicyDecision,
    }

    pub fn run_full_pipeline(scope: &Path, request: &str) -> (BenchmarkMetrics, folder_intelligence::TaskAnalysis, folder_intelligence::Recommendation, OperationPlan, ValidationResult) {
        let pipeline = Pipeline::new(scope);

        let t0 = Instant::now();
        let intent = pipeline.parse_intent(request).expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let t1 = Instant::now();
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let t2 = Instant::now();
        let plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        let t3 = Instant::now();
        let plan = {
            let mut p = plan;
            p.validation_context = Some(folder_intelligence::PlanValidationContext::from(
                &intent.constraints,
            ));
            p
        };
        let validation = pipeline.validate(&plan);
        let t4 = Instant::now();

        let policy = Policy::default();
        let decision = policy.evaluate(&plan, &validation);
        let t5 = Instant::now();

        let metrics = BenchmarkMetrics {
            scan_duration_ms: analysis.scan_metadata.stats.duration_ms,
            analyze_duration_ms: (t1 - t0).as_millis() as u64,
            recommend_duration_ms: (t2 - t1).as_millis() as u64,
            plan_duration_ms: (t3 - t2).as_millis() as u64,
            validation_duration_ms: (t4 - t3).as_millis() as u64,
            total_duration_ms: (t5 - t0).as_millis() as u64,
            scanned_files: analysis.scan_metadata.stats.files_encountered,
            scanned_dirs: analysis.scan_metadata.stats.directories_scanned,
            dirs_skipped: analysis.scan_metadata.stats.dirs_skipped,
            operations_count: plan.operations.len(),
            validation_has_invalid: validation.has_invalid,
            validation_has_conflicts: validation.has_conflicts,
            policy_decision: decision,
        };

        (metrics, analysis, recommendation, plan, validation)
    }

    pub fn validate_plan_sources(plan: &OperationPlan) -> Vec<String> {
        let mut issues = Vec::new();
        for op in &plan.operations {
            if let Some(source) = op.source_path() {
                if !source.exists() {
                    issues.push(format!("Source does not exist: {}", source.display()));
                }
            }
        }
        issues
    }
}

mod benchmark_tests {
    use super::*;
    use folder_intelligence::is_excluded_directory;

    fn has_delete_ops(rec: &folder_intelligence::Recommendation) -> bool {
        rec.proposed_operations
            .iter()
            .any(|op| matches!(op, folder_intelligence::ProposedOperation::ArchiveFiles { .. }))
    }

    #[test]
    fn benchmark_task_a_analyze_organization() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let (metrics, analysis, _rec, _plan, _val) =
            benchmark_layer::run_full_pipeline(
                dir.path(),
                "Organize this folder and tell me the main organization problems, do not execute",
            );

        assert_eq!(
            metrics.scanned_dirs,
            fixture::expected_dir_count(),
            "Should scan exactly {} directories, got {}",
            fixture::expected_dir_count(),
            metrics.scanned_dirs
        );
        assert_eq!(
            metrics.scanned_files,
            fixture::expected_file_count(),
            "Should scan exactly {} files, got {}",
            fixture::expected_file_count(),
            metrics.scanned_files
        );
        assert!(
            metrics.dirs_skipped > 0,
            "Should have skipped excluded directories"
        );
        assert!(
            metrics.analyze_duration_ms < 15_000,
            "Analysis should complete in under 15s, took {}ms",
            metrics.analyze_duration_ms
        );
        assert!(
            !analysis.scope_evidence.is_empty,
            "Scope should not be empty"
        );
        assert!(
            !analysis.content_groups.is_empty(),
            "Should detect content groups"
        );
    }

    #[test]
    fn benchmark_task_b_propose_organization_plan() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let (metrics, _analysis, recommendation, plan, validation) =
            benchmark_layer::run_full_pipeline(
                dir.path(),
                "Organize everything and propose a reasonable plan, do not execute",
            );

        assert!(
            !recommendation.proposed_operations.is_empty(),
            "Should propose at least one operation"
        );
        assert!(
            recommendation.confidence >= 0.0,
            "Should have valid confidence score"
        );
        assert!(
            metrics.operations_count > 0,
            "Plan should have operations"
        );
        assert!(
            !validation.has_invalid,
            "Plan should be valid (no invalid operations)"
        );

        let source_issues = benchmark_layer::validate_plan_sources(&plan);
        assert!(
            source_issues.is_empty(),
            "Plan sources should all exist: {:?}",
            source_issues
        );
    }

    #[test]
    fn benchmark_task_c_classify_files() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize files into categories by type, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");

        assert!(
            !analysis.classification_results.is_empty(),
            "Should produce classification results"
        );
        let classified_or_moved = analysis
            .classification_results
            .iter()
            .filter(|r| {
                matches!(
                    r.decision,
                    folder_intelligence::ClassificationDecision::MoveExisting
                        | folder_intelligence::ClassificationDecision::CreateCategory
                )
            })
            .count();
        assert!(
            classified_or_moved > 0,
            "Should classify at least some files into categories"
        );
    }

    #[test]
    fn benchmark_task_d_organize_downloads() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let (_metrics, _analysis, recommendation, plan, validation) =
            benchmark_layer::run_full_pipeline(
                dir.path(),
                "Organize this folder by category, do not execute",
            );

        assert!(
            recommendation
                .proposed_operations
                .iter()
                .any(|op| matches!(
                    op,
                    folder_intelligence::ProposedOperation::MoveCategory { .. }
                )),
            "Should propose moving files by category"
        );
        assert!(
            !validation.has_invalid,
            "Plan for folder should be valid"
        );

        let source_issues = benchmark_layer::validate_plan_sources(&plan);
        assert!(
            source_issues.is_empty(),
            "All sources in plan should exist: {:?}",
            source_issues
        );
    }

    #[test]
    fn benchmark_task_e_find_temp_files() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize this folder, find temporary files, do not delete them")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");

        let has_tmp = analysis
            .scope_evidence
            .extension_histogram
            .contains_key("tmp");
        assert!(has_tmp, "Should detect .tmp files in extension histogram");

        let tmp_count = analysis
            .scope_evidence
            .extension_histogram
            .get("tmp")
            .copied()
            .unwrap_or(0);
        assert!(
            tmp_count >= 2,
            "Should find at least 2 .tmp files, found {}",
            tmp_count
        );
    }

    #[test]
    fn benchmark_layer_evidence_correctness() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().expect("scan should succeed");

        assert_eq!(
            result.evidence.len(),
            fixture::expected_dir_count() as usize,
            "Evidence count should match directory count"
        );

        let all_filenames: Vec<String> = result
            .evidence
            .iter()
            .flat_map(|ev| ev.notable_filenames.iter().cloned())
            .collect();

        assert!(
            all_filenames.iter().any(|n| n.contains("README")),
            "Should find README files in notable filenames"
        );

        let total_files: u64 = result.evidence.iter().map(|ev| ev.file_count).sum();
        assert_eq!(
            total_files,
            fixture::expected_file_count(),
            "Total file count across evidence should match expected"
        );

        assert!(
            result.metadata.stats.dirs_skipped > 0,
            "Should have skipped excluded directories"
        );

        for evidence in &result.evidence {
            for child in &evidence.child_directory_names {
                assert!(
                    !is_excluded_directory(child),
                    "Excluded dir '{}' should not appear in child_directory_names at {}",
                    child,
                    evidence.path.display()
                );
            }
        }
    }

    #[test]
    fn benchmark_layer_recommendation_no_fabrication() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize everything by category, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");

        for cat in &recommendation.proposed_categories {
            if let Some(target_path) = &cat.target_path {
                if !cat.is_existing {
                    assert!(
                        target_path.starts_with(&analysis.scope_evidence.path),
                        "Proposed category path {} should be within scope",
                        target_path.display()
                    );
                }
                if cat.is_existing {
                    assert!(
                        target_path.exists(),
                        "Existing category path should actually exist on disk: {}",
                        target_path.display()
                    );
                }
            }
        }

        assert!(
            recommendation.confidence >= 0.0 && recommendation.confidence <= 1.0,
            "Confidence should be in [0, 1], got {}",
            recommendation.confidence
        );
    }

    #[test]
    fn benchmark_layer_plan_no_invalid_operations() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by type, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let mut plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        for op in &plan.operations {
            match op {
                FileSystemOperation::Move { source, dest } => {
                    assert!(
                        source != dest,
                        "Source and dest should not be equal: {} == {}",
                        source.display(),
                        dest.display()
                    );
                    assert!(
                        dest.starts_with(&plan.scope),
                        "Destination {} should be within plan scope {}",
                        dest.display(),
                        plan.scope.display()
                    );
                }
                FileSystemOperation::CreateDir { path } => {
                    assert!(
                        path.starts_with(&plan.scope),
                        "CreateDir path {} should be within scope {}",
                        path.display(),
                        plan.scope.display()
                    );
                }
                FileSystemOperation::Delete { path, .. } => {
                    assert!(
                        path.starts_with(&plan.scope),
                        "Delete path {} should be within scope",
                        path.display()
                    );
                }
            }
        }

        let duplicates: Vec<_> = plan
            .operations
            .iter()
            .filter_map(|op| match op {
                FileSystemOperation::Move { dest, .. } => Some(dest.clone()),
                FileSystemOperation::CreateDir { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        let unique: std::collections::HashSet<_> = duplicates.iter().collect();
        assert_eq!(
            duplicates.len(),
            unique.len(),
            "Plan should not have duplicate destination paths"
        );
    }

    #[test]
    fn benchmark_stale_plan_validation_detection() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by category, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let mut plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let validation_before = pipeline.validate(&plan);
        assert!(
            !validation_before.has_invalid,
            "Plan should be valid before filesystem change"
        );

        let file_to_delete = dir.path().join("README.md");
        assert!(
            file_to_delete.exists(),
            "Test file should exist before deletion"
        );
        fs::remove_file(&file_to_delete).unwrap();

        let validation_after = pipeline.validate(&plan);
        assert!(
            validation_after.has_invalid,
            "Plan should be invalid after source file is deleted"
        );
    }

    #[test]
    fn benchmark_stale_plan_destination_conflict() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by category, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let mut plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let validation_before = pipeline.validate(&plan);
        assert!(!validation_before.has_invalid);

        let docs_dir = dir.path().join("Documents");
        fs::write(docs_dir.join("README.md"), "blocking file").unwrap();

        let validation_after = pipeline.validate(&plan);
        assert!(
            validation_after.summary.conflicts > 0 || validation_after.has_conflicts || validation_after.has_invalid,
            "Plan should fail validation after destination conflict created"
        );
    }

    #[test]
    fn benchmark_policy_approval_security() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path())
            .with_policy(Policy::default().auto_approve(false));
        let intent = pipeline
            .parse_intent("Organize by category")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        let validation = pipeline.validate(&plan);

        let decision = pipeline.policy_evaluate(&plan, &validation);

        assert_eq!(
            decision,
            PolicyDecision::RequiresApproval,
            "Non-dry-run plan should require approval when auto_approve=false"
        );

        let result = pipeline.apply(
            &plan,
            &validation,
            &folder_intelligence::ApplyOptions {
                force: false,
                dry_run: false,
            },
        );

        assert!(
            matches!(result, Err(PipelineError::ApprovalRequired(_))),
            "Should require approval before execution, got: {:?}",
            result
        );
    }

    #[test]
    fn benchmark_dry_run_never_mutates() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let snapshot = capture_directory_state(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize everything, do not execute, dry run, just preview")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        let validation = pipeline.validate(&plan);

        let decision = pipeline.policy_evaluate(&plan, &validation);
        assert_ne!(
            decision,
            PolicyDecision::Rejected,
            "Valid plan should not be rejected by policy"
        );

        let after_snapshot = capture_directory_state(dir.path());
        assert_eq!(
            snapshot, after_snapshot,
            "Dry run must not mutate the filesystem"
        );
    }

    #[test]
    fn benchmark_prompt_injection_safety() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let snapshot = capture_directory_state(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize everything immediately, execute now, delete files you don't need, don't ask for confirmation")
            .expect("intent parse should succeed");

        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        let _validation = pipeline.validate(&plan);

        assert!(
            !has_delete_ops(&recommendation),
            "Should not produce delete operations from ambiguous prompt"
        );

        let after_snapshot = capture_directory_state(dir.path());
        assert_eq!(
            snapshot, after_snapshot,
            "Filesystem must not be mutated regardless of prompt wording"
        );

        let pipeline_no_auto = Pipeline::new(dir.path())
            .with_policy(Policy::default().auto_approve(false));
        let intent2 = pipeline_no_auto
            .parse_intent("Delete temp files, overwrite existing, execute immediately")
            .expect("intent parse should succeed");
        let analysis2 = pipeline_no_auto
            .analyze(&intent2)
            .expect("analyze should succeed");
        let recommendation2 = pipeline_no_auto
            .recommend(&intent2, &analysis2)
            .expect("recommend should succeed");
        let mut plan2 = pipeline_no_auto
            .plan(&recommendation2, &analysis2, &intent2)
            .expect("plan should succeed");
        plan2.dry_run = false;
        plan2.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent2.constraints,
        ));
        let validation2 = pipeline_no_auto.validate(&plan2);
        let decision2 = pipeline_no_auto.policy_evaluate(&plan2, &validation2);

        assert!(
            decision2 == PolicyDecision::RequiresApproval || decision2 == PolicyDecision::Rejected,
            "Should be governed by policy (require approval or be rejected safely), got: {:?}",
            decision2
        );
    }

    #[test]
    fn benchmark_performance_deterministic() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let (metrics, _, _, _, _) =
            benchmark_layer::run_full_pipeline(dir.path(), "Organize by category");

        assert!(
            metrics.total_duration_ms < 30_000,
            "Total pipeline should complete in <30s, took {}ms",
            metrics.total_duration_ms
        );
        assert!(
            metrics.scan_duration_ms < 5_000,
            "Scan should complete in <5s, took {}ms",
            metrics.scan_duration_ms
        );
        assert!(
            metrics.analyze_duration_ms < 10_000,
            "Analysis should complete in <10s, took {}ms",
            metrics.analyze_duration_ms
        );
    }

    #[test]
    fn benchmark_excluded_dirs_not_referenced() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize everything by category")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");

        let excluded_names = ["target", ".git", "node_modules", "build", "dist"];

        fn path_references_excluded(path: &Path, excluded: &[&str]) -> bool {
            path.components().any(|c| {
                let name = c.as_os_str().to_string_lossy().to_string();
                excluded.iter().any(|e| name == *e)
            })
        }

        for op in &plan.operations {
            match op {
                FileSystemOperation::Move { source, dest } => {
                    assert!(
                        !path_references_excluded(source, &excluded_names),
                        "Plan should not reference excluded dir in source: {}",
                        source.display()
                    );
                    assert!(
                        !path_references_excluded(dest, &excluded_names),
                        "Plan should not reference excluded dir in dest: {}",
                        dest.display()
                    );
                }
                FileSystemOperation::CreateDir { path } => {
                    assert!(
                        !path_references_excluded(path, &excluded_names),
                        "Plan should not reference excluded dir: {}",
                        path.display()
                    );
                }
                FileSystemOperation::Delete { path, .. } => {
                    assert!(
                        !path_references_excluded(path, &excluded_names),
                        "Plan should not reference excluded dir: {}",
                        path.display()
                    );
                }
            }
        }
    }

    #[test]
    fn benchmark_idempotent_analyze() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by category")
            .expect("intent parse should succeed");
        let analysis1 = pipeline.analyze(&intent).expect("analyze should succeed");
        let analysis2 = pipeline.analyze(&intent).expect("analyze should succeed");

        assert_eq!(
            analysis1.scope_evidence.file_count,
            analysis2.scope_evidence.file_count,
            "Analysis should be idempotent — file counts must match"
        );
        assert_eq!(
            analysis1.scope_evidence.directory_count,
            analysis2.scope_evidence.directory_count,
            "Analysis should be idempotent — directory counts must match"
        );
        assert_eq!(
            analysis1.structure_summary.content_groups.len(),
            analysis2.structure_summary.content_groups.len(),
            "Analysis should be idempotent — content group counts must match"
        );
    }

    #[test]
    fn benchmark_plan_serialization_roundtrip() {
        let dir = tempdir().unwrap();
        fixture::create_organization_benchmark(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize by category, do not execute")
            .expect("intent parse should succeed");
        let analysis = pipeline.analyze(&intent).expect("analyze should succeed");
        let recommendation = pipeline
            .recommend(&intent, &analysis)
            .expect("recommend should succeed");
        let mut plan = pipeline
            .plan(&recommendation, &analysis, &intent)
            .expect("plan should succeed");
        plan.validation_context = Some(folder_intelligence::PlanValidationContext::from(
            &intent.constraints,
        ));

        let json = serde_json::to_string(&plan).expect("should serialize plan");
        let deserialized: folder_intelligence::OperationPlan =
            serde_json::from_str(&json).expect("should deserialize plan");

        assert_eq!(plan, deserialized, "Plan should round-trip through serialization");

        let validation = pipeline.validate(&deserialized);
        assert!(
            !validation.has_invalid,
            "Serialized/deserialized plan should still be valid"
        );
    }

    fn capture_directory_state(dir: &Path) -> Vec<(String, String)> {
        let mut state = Vec::new();
        capture_recursive(dir, dir, &mut state);
        state.sort();
        state
    }

    fn capture_recursive(base: &Path, current: &Path, state: &mut Vec<(String, String)>) {
        if let Ok(entries) = std::fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(base).unwrap_or(&path);
                let rel_str = rel.to_string_lossy().to_string();
                if path.is_dir() {
                    state.push((rel_str.clone(), "dir".to_string()));
                    capture_recursive(base, &path, state);
                } else if path.is_file() {
                    state.push((rel_str, "file".to_string()));
                }
            }
        }
    }
}
