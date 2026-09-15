use super::*;
use crate::agent::intent::TaskIntentParser;
use crate::agent::{EvidenceAnalyzer, RecommendationEngine, PlanGenerator, TaskIntent, Goal, ConstraintSet};
use crate::agent::plan::WarningType;
use tempfile::tempdir;
use std::fs;
use std::path::PathBuf;

fn create_test_scope(dir: &tempfile::TempDir) -> PathBuf {
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

    scope
}

fn run_pipeline(scope: &PathBuf, request: &str) -> (TaskIntent, OperationPlan) {
    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse(request)
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine.recommend(&intent, &analysis).expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis, &[]).expect("should generate plan");

    (intent, plan)
}

#[test]
fn test_validate_basic_plan() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    assert_eq!(result.plan_id, plan.id);
    assert!(result.summary.total > 0);
    assert!(result.executable_operations > 0);
}

#[test]
fn test_validate_status_types() {
    assert!(ValidationStatus::Valid.is_valid());
    assert!(!ValidationStatus::Valid.is_blocked());
    assert!(!ValidationStatus::Valid.is_conflict());
    assert!(!ValidationStatus::Valid.is_invalid());
    assert!(!ValidationStatus::Valid.is_warning());

    assert!(ValidationStatus::BlockedByConstraint("test".to_string()).is_blocked());
    assert!(ValidationStatus::Conflict("test".to_string()).is_conflict());
    assert!(ValidationStatus::Invalid("test".to_string()).is_invalid());
    assert!(ValidationStatus::Warning("test".to_string()).is_warning());
}

#[test]
fn test_validate_source_not_found() {
    let plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: PathBuf::from("/tmp/test"),
        operations: vec![
            FileSystemOperation::Move {
                source: PathBuf::from("/nonexistent/file.txt"),
                dest: PathBuf::from("/tmp/test/dest"),
            },
        ],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let intent = TaskIntent {
        goal: crate::agent::Goal::Organize {
            scope: PathBuf::from("/tmp/test"),
            purpose: "by_category".to_string(),
        },
        constraints: crate::agent::ConstraintSet::default(),
        user_hints: std::collections::HashMap::new(),
        unknown_factors: Vec::new(),
    };

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    assert!(result.summary.invalid > 0);
    for v in &result.validated_operations {
        if matches!(v.status, ValidationStatus::Invalid(_)) {
            return;
        }
    }
    panic!("Expected an invalid operation");
}

#[test]
fn test_validate_source_equals_dest() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("test_eq");
    fs::create_dir_all(&scope).unwrap();

    let same_path = scope.join("file.txt");
    fs::write(same_path.clone(), "content").unwrap();

    let plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: scope.clone(),
        operations: vec![
            FileSystemOperation::Move {
                source: same_path.clone(),
                dest: same_path.clone(),
            },
        ],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let intent = TaskIntent {
        goal: crate::agent::Goal::Organize {
            scope: scope.clone(),
            purpose: "by_category".to_string(),
        },
        constraints: crate::agent::ConstraintSet::default(),
        user_hints: std::collections::HashMap::new(),
        unknown_factors: Vec::new(),
    };

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    let has_invalid = result.validated_operations.iter().any(|v| {
        matches!(v.status, ValidationStatus::Invalid(ref msg) if msg.contains("equals destination"))
    });
    assert!(has_invalid);
}

#[test]
fn test_validate_destination_outside_scope() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let mut bad_plan = plan.clone();
    if let Some(FileSystemOperation::Move { dest, .. }) = bad_plan.operations.first() {
        // We need to use the actual scope for this to work with the validator
    }

    // Create a plan with a destination outside scope
    let outside_plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: scope.clone(),
        operations: vec![
            FileSystemOperation::CreateDir {
                path: PathBuf::from("/outside/scope/dir"),
            },
        ],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let validator = PlanValidator::default();
    let result = validator.validate(&outside_plan, &intent);

    assert!(result.summary.invalid > 0);
    assert!(result.has_invalid);
}

#[test]
fn test_validation_result_serialization() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    let json = serde_json::to_string(&result).expect("should serialize");
    let deserialized: ValidationResult = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(result, deserialized);
}

#[test]
fn test_preview_render() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);
    let preview = PlanPreview::default().render(&plan, &result);

    assert!(preview.contains("Operation Plan"));
    assert!(preview.contains("Validation Summary"));
    assert!(preview.contains("VALID"));
    assert!(preview.contains("Estimated Impact"));
}

#[test]
fn test_preview_empty_plan() {
    let plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: PathBuf::from("/tmp"),
        operations: Vec::new(),
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let intent = TaskIntent {
        goal: crate::agent::Goal::Organize {
            scope: PathBuf::from("/tmp"),
            purpose: "by_category".to_string(),
        },
        constraints: crate::agent::ConstraintSet::default(),
        user_hints: std::collections::HashMap::new(),
        unknown_factors: Vec::new(),
    };

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);
    let preview = PlanPreview::default().render(&plan, &result);

    assert!(preview.contains("No operations proposed"));
    assert!(preview.contains("0 total"));
}

#[test]
fn test_filter_executable() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let exec_plan = validator.filter_executable(&plan, &intent);

    // Executable plan should only have valid/warning operations
    for op in &exec_plan.operations {
        let validated = validator.validate(&plan, &intent);
        let op_str = format!("{:?}", op);
        let _ = op_str;
    }

    assert!(exec_plan.operations.len() <= plan.operations.len());
    assert!(exec_plan.id.starts_with("plan-exec-"));
}

#[test]
fn test_validate_plan_consistency_duplicate_sources() {
    let scope = PathBuf::from("/tmp/test_consistency");

    let plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: scope.clone(),
        operations: vec![
            FileSystemOperation::Move {
                source: scope.join("file.txt"),
                dest: scope.join("dir1"),
            },
            FileSystemOperation::Move {
                source: scope.join("file.txt"),
                dest: scope.join("dir2"),
            },
        ],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let validator = PlanValidator::default();
    let issues = validator.validate_plan_consistency(&plan);

    assert!(!issues.is_empty());
    assert!(issues.iter().any(|i| i.contains("Duplicate source")));
}

#[test]
fn test_validate_plan_consistency_no_issues() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let issues = validator.validate_plan_consistency(&plan);

    // May or may not have issues depending on generated plan
    // Just verify the method runs without panic
    assert!(issues.len() <= plan.operations.len());
}

#[test]
fn test_validate_create_dir_idempotent() {
    let plan = OperationPlan {
        id: "test".to_string(),
        recommendation_id: "rec".to_string(),
        scope: PathBuf::from("/tmp/test"),
        operations: vec![
            FileSystemOperation::CreateDir {
                path: PathBuf::from("/tmp/new_dir"),
            },
            FileSystemOperation::CreateDir {
                path: PathBuf::from("/tmp/new_dir"),
            },
        ],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
    };

    let validator = PlanValidator::default();
    let issues = validator.validate_plan_consistency(&plan);

    assert!(issues.iter().any(|i| i.contains("Duplicate CreateDir")));
}

#[test]
fn test_validate_with_constraint_preserved() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category, preserve existing folders")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine.recommend(&intent, &analysis).expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis, &[]).expect("should generate plan");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    // Plan should validate successfully with preserved folders
    assert!(result.summary.total > 0);
}

#[test]
fn test_validate_warning_type_display() {
    assert_eq!(format!("{}", WarningType::PartialScanScope), "Partial scan scope");
    assert_eq!(format!("{}", WarningType::FileCountEstimate), "File count estimate");
    assert_eq!(format!("{}", WarningType::MissingSourceFile), "Missing source file");
    assert_eq!(format!("{}", WarningType::PathTooDeep), "Path too deep");
}

#[test]
fn test_validate_operation_type_and_description() {
    let move_op = FileSystemOperation::Move {
        source: PathBuf::from("/src/a.txt"),
        dest: PathBuf::from("/dest/a.txt"),
    };
    assert_eq!(move_op.operation_type(), "move");
    assert_eq!(move_op.source_path(), Some(&PathBuf::from("/src/a.txt")));
    assert_eq!(move_op.dest_path(), Some(&PathBuf::from("/dest/a.txt")));

    let mkdir_op = FileSystemOperation::CreateDir {
        path: PathBuf::from("/new"),
    };
    assert_eq!(mkdir_op.operation_type(), "create_dir");
    assert_eq!(mkdir_op.source_path(), None);
    assert_eq!(mkdir_op.dest_path(), Some(&PathBuf::from("/new")));

    let delete_op = FileSystemOperation::Delete {
        path: PathBuf::from("/old"),
        reason: "cleanup".to_string(),
    };
    assert_eq!(delete_op.operation_type(), "delete");
    assert_eq!(delete_op.source_path(), Some(&PathBuf::from("/old")));
    assert_eq!(delete_op.dest_path(), None);
}

#[test]
fn test_validate_summary_default() {
    let summary = ValidationSummary::default();
    assert_eq!(summary.total, 0);
    assert_eq!(summary.valid, 0);
    assert_eq!(summary.blocked, 0);
    assert_eq!(summary.conflicts, 0);
    assert_eq!(summary.invalid, 0);
    assert_eq!(summary.warnings, 0);
}

#[test]
fn test_validate_has_warnings_flag() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let (intent, plan) = run_pipeline(&scope, "Organize this folder by category");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    // has_warnings flag should be consistent
    assert_eq!(result.has_warnings, result.validated_operations.iter().any(|v| v.status.is_warning()));
}

#[test]
fn test_full_pipeline_to_validation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine.recommend(&intent, &analysis).expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis, &[]).expect("should generate plan");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    // Full pipeline should produce a validatable plan
    assert_eq!(result.plan_id, plan.id);
    assert_eq!(result.scope, scope);
    assert!(result.summary.total > 0);
    assert!(result.executable_operations > 0);
}

#[test]
fn test_validate_blocked_by_constraint() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    // Create temp files
    fs::write(scope.join("cache.tmp"), "temp").unwrap();

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    assert!(!intent.constraints.auto_delete_temps);

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine.recommend(&intent, &analysis).expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis, &[]).expect("should generate plan");

    let validator = PlanValidator::default();
    let result = validator.validate(&plan, &intent);

    // Should not have blocked operations since we didn't propose temp deletion
    // (recommendation engine handles this already)
    assert!(!result.operations_contain_blocked());
}

trait ValidationResultExt {
    fn operations_contain_blocked(&self) -> bool;
}

impl ValidationResultExt for ValidationResult {
    fn operations_contain_blocked(&self) -> bool {
        self.validated_operations.iter().any(|v| v.status.is_blocked())
    }
}