use super::*;
use crate::agent::plan::{FileSystemOperation, OperationPlan};
use crate::agent::validate::{
    ValidatedOperation, ValidationResult, ValidationStatus, ValidationSummary,
};
use std::fs;
use tempfile::tempdir;

fn create_full_plan(dir: &std::path::Path, dry_run: bool) -> (OperationPlan, ValidationResult) {
    let source = dir.join("source.txt");
    let dest = dir.join("dest.txt");
    fs::write(&source, "test content").unwrap();

    let plan = OperationPlan {
        id: "test-plan-1".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: dir.to_path_buf(),
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
        dry_run,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    (plan, validation)
}

#[test]
fn test_execute_move_operation() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);
    let source = dir.path().join("source.txt");
    assert!(source.exists());

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(result.is_complete);
    assert!(!result.log.entries.is_empty());
    assert!(result.log.entries[0].status.is_success());
    assert!(!source.exists());
    assert!(dir.path().join("dest.txt").exists());
}

#[test]
fn test_execute_create_dir_operation() {
    let dir = tempdir().unwrap();
    let new_dir = dir.path().join("new_dir");

    let plan = OperationPlan {
        id: "test-plan-2".to_string(),
        recommendation_id: "rec-2".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::CreateDir {
            path: new_dir.clone(),
        }],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 1,
            files_deleted: 0,
            dirs_affected: 1,
            total_bytes: 0,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::CreateDir {
                path: new_dir.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(result.is_complete);
    assert!(new_dir.exists());
    assert!(result.log.entries[0].status.is_success());
}

#[test]
fn test_execute_delete_operation() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("delete_me.txt");
    fs::write(&file, "delete me").unwrap();

    let plan = OperationPlan {
        id: "test-plan-3".to_string(),
        recommendation_id: "rec-3".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::Delete {
            path: file.clone(),
            reason: "cleanup".to_string(),
        }],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 1,
            dirs_affected: 1,
            total_bytes: 1024,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Delete {
                path: file.clone(),
                reason: "cleanup".to_string(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(!file.exists());
    assert_eq!(
        result.log.entries[0].status,
        ExecutionStatus::UndoNotSupported
    );
    assert!(!result.log.entries[0].undo_supported);
}

#[test]
fn test_dry_run_rejected() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), true);

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ApplyError::DryRunFlagSet);
}

#[test]
fn test_invalid_plan_rejected() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let mut validation = validation;
    validation.has_invalid = true;
    validation.validated_operations = vec![ValidatedOperation {
        operation: FileSystemOperation::Move {
            source: dir.path().join("nonexistent.txt"),
            dest: dir.path().join("dest.txt"),
        },
        status: ValidationStatus::Invalid("not found".to_string()),
        warnings: vec![],
        dependencies: vec![],
    }];

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false);

    assert!(result.is_err());
    match result.unwrap_err() {
        ApplyError::InvalidPlan(msg) => assert!(msg.contains("INVALID")),
        _ => panic!("Expected InvalidPlan error"),
    }
}

#[test]
fn test_execute_move_failure() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("nonexistent.txt");
    let dest = dir.path().join("dest.txt");

    let plan = OperationPlan {
        id: "test-plan-fail".to_string(),
        recommendation_id: "rec-fail".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::Move {
            source: source.clone(),
            dest: dest.clone(),
        }],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 1,
            total_bytes: 0,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(!result.is_complete);
    assert!(result.log.entries[0].status.is_failed());
    assert!(result.log.failure_count == 1);
}

#[test]
fn test_undo_move_operation() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);
    let source = dir.path().join("source.txt");

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    let undo_result = executor.undo(&apply_result.log).unwrap();
    assert_eq!(undo_result.total_undo_operations, 1);
    assert!(undo_result.applied_undoes.len() == 1);
    assert!(undo_result.applied_undoes[0].status.is_success());
    assert!(source.exists());
}

#[test]
fn test_undo_with_source_missing() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    let mut log = apply_result.log.clone();
    let entry = &mut log.entries[0];
    entry.original_source = dir.path().join("gone_source.txt");
    entry.applied_target = dir.path().join("gone_dest.txt");

    let undo_result = executor.undo(&log).unwrap();
    assert!(!undo_result.conflicts.is_empty());
    match &undo_result.conflicts[0] {
        UndoConflict::SourceMissing { .. } => {}
        _ => panic!("Expected SourceMissing conflict"),
    }
}

#[test]
fn test_undo_with_target_exists() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let dest = dir.path().join("dest.txt");
    fs::write(&source, "test").unwrap();

    let plan = OperationPlan {
        id: "test-undo-blocked".to_string(),
        recommendation_id: "rec-blocked".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    // After executing, source was moved to dest. Now create a file at source
    // (the original source path) so that undo's destination already exists.
    fs::write(&source, "blocker").unwrap();

    let undo_result = executor.undo(&apply_result.log).unwrap();
    assert!(!undo_result.conflicts.is_empty());
    match &undo_result.conflicts[0] {
        UndoConflict::TargetExists { .. } => {}
        _ => panic!("Expected TargetExists conflict"),
    }
}

#[test]
fn test_undo_delete_is_not_undoable() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("delete_me.txt");
    fs::write(&file, "delete me").unwrap();

    let plan = OperationPlan {
        id: "test-undo-delete".to_string(),
        recommendation_id: "rec-delete".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::Delete {
            path: file.clone(),
            reason: "cleanup".to_string(),
        }],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 1,
            dirs_affected: 1,
            total_bytes: 1024,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Delete {
                path: file.clone(),
                reason: "cleanup".to_string(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    let undo_result = executor.undo(&apply_result.log).unwrap();
    assert!(!undo_result.conflicts.is_empty());
    assert!(undo_result
        .conflicts
        .iter()
        .any(|c| matches!(c, UndoConflict::NotUndoable { .. })));
}

#[test]
fn test_log_entry_structure() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    let entry = &result.log.entries[0];
    assert!(!entry.id.is_empty());
    assert_eq!(entry.plan_id, "test-plan-1");
    assert_eq!(entry.operation_type, "move");
    assert!(entry.started_at > 0);
    assert!(entry.completed_at.is_some());
}

#[test]
fn test_apply_result_summary() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(result.can_undo);
    assert_eq!(result.undo_supported_count, 1);
    assert_eq!(result.undo_unsupported_count, 0);
}

#[test]
fn test_dry_run_flag_on_plan() {
    let dir = tempdir().unwrap();

    let plan = OperationPlan {
        id: "test-dry-run-plan".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: true,
        created_at: 1234567890,
        validation_context: None,
    };

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![],
        summary: ValidationSummary::new(),
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 0,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false);

    assert!(matches!(result, Err(ApplyError::DryRunFlagSet)));
}

#[test]
fn test_execution_status_helpers() {
    assert!(ExecutionStatus::Success.is_success());
    assert!(ExecutionStatus::Failed("err".to_string()).is_failed());
    assert!(ExecutionStatus::Skipped("skip".to_string()).is_skipped());
    assert!(ExecutionStatus::Pending.is_pending());
}

#[test]
fn test_operation_log_counts() {
    let mut log = OperationLog::new("test-log");

    log.add_entry(LogEntry {
        id: "e1".to_string(),
        plan_id: "p1".to_string(),
        operation_type: "move".to_string(),
        original_source: PathBuf::from("/a"),
        applied_target: PathBuf::from("/b"),
        status: ExecutionStatus::Success,
        started_at: 100,
        completed_at: Some(101),
        error: None,
        undo_supported: true,
    });

    log.add_entry(LogEntry {
        id: "e2".to_string(),
        plan_id: "p1".to_string(),
        operation_type: "delete".to_string(),
        original_source: PathBuf::from("/c"),
        applied_target: PathBuf::from("/c"),
        status: ExecutionStatus::Failed("err".to_string()),
        started_at: 102,
        completed_at: Some(103),
        error: Some("err".to_string()),
        undo_supported: false,
    });

    log.add_entry(LogEntry {
        id: "e3".to_string(),
        plan_id: "p1".to_string(),
        operation_type: "create_dir".to_string(),
        original_source: PathBuf::from("/d"),
        applied_target: PathBuf::from("/d"),
        status: ExecutionStatus::Skipped("exists".to_string()),
        started_at: 104,
        completed_at: Some(105),
        error: Some("exists".to_string()),
        undo_supported: false,
    });

    log.finalize();

    assert_eq!(log.success_count, 1);
    assert_eq!(log.failure_count, 1);
    assert_eq!(log.skipped_count, 1);
    assert_eq!(log.total_entries, 3);
    assert_eq!(log.undoable_entries().len(), 1);
    assert_eq!(log.undo_unsupported_entries().len(), 0);
}

#[test]
fn test_undo_conflict_description() {
    let conflict = UndoConflict::SourceMissing {
        path: PathBuf::from("/tmp/test"),
        message: "Source no longer exists".to_string(),
    };
    let desc = conflict.description();
    assert!(desc.contains("Source no longer exists"));
    assert!(desc.contains("/tmp/test"));
}

#[test]
fn test_undo_result_serialization() {
    let undo_log = UndoLogEntry {
        entry_id: "e1".to_string(),
        operation_type: "move".to_string(),
        source: PathBuf::from("/a"),
        dest: PathBuf::from("/b"),
        status: ExecutionStatus::Success,
        error: None,
    };

    let undo_result = UndoResult {
        log_id: "log-1".to_string(),
        applied_undoes: vec![undo_log],
        conflicts: vec![],
        total_undo_operations: 1,
        completed_at: 123456,
    };

    let json = serde_json::to_string(&undo_result).unwrap();
    let deserialized: UndoResult = serde_json::from_str(&json).unwrap();
    assert_eq!(undo_result, deserialized);
}

#[test]
fn test_apply_result_serialization() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: ApplyResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

#[test]
fn test_operation_log_serialization() {
    let mut log = OperationLog::new("log-1");
    log.add_entry(LogEntry {
        id: "e1".to_string(),
        plan_id: "log-1".to_string(),
        operation_type: "move".to_string(),
        original_source: PathBuf::from("/a"),
        applied_target: PathBuf::from("/b"),
        status: ExecutionStatus::Success,
        started_at: 100,
        completed_at: Some(101),
        error: None,
        undo_supported: true,
    });
    log.finalize();

    let json = serde_json::to_string(&log).unwrap();
    let deserialized: OperationLog = serde_json::from_str(&json).unwrap();
    assert_eq!(log, deserialized);
}

#[test]
fn test_undo_empty_log() {
    let log = OperationLog::new("empty-log");
    let executor = Executor::default();
    let result = executor.undo(&log).unwrap();

    assert_eq!(result.total_undo_operations, 0);
    assert!(result.applied_undoes.is_empty());
    assert!(result.conflicts.is_empty());
}

#[test]
fn test_undo_with_failed_entry_is_not_undone() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    let mut log = apply_result.log.clone();

    let source = dir.path().join("source.txt");
    fs::write(&source, "test").unwrap();

    log.entries[0].status = ExecutionStatus::Failed("manually failed".to_string());
    log.entries[0].error = Some("manually failed".to_string());
    log.entries[0].undo_supported = false;
    log.entries[0].completed_at = Some(now_secs());

    let undo_result = executor.undo(&log).unwrap();
    assert_eq!(undo_result.total_undo_operations, 0);
    assert!(undo_result.applied_undoes.is_empty());
}

#[test]
fn test_undo_with_blocked_plan_rejected() {
    let dir = tempdir().unwrap();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    let mut log = apply_result.log.clone();
    log.entries[0].status = ExecutionStatus::Success;
    log.entries[0].operation_type = "unknown_type".to_string();
    log.entries[0].undo_supported = true;

    let undo_result = executor.undo(&log).unwrap();
    assert_eq!(undo_result.total_undo_operations, 0);
}

#[test]
fn test_dry_run_flag_prevents_execution() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let dest = dir.path().join("dest.txt");
    fs::write(&source, "test").unwrap();

    let plan = OperationPlan {
        id: "test-dry-run-exec".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: dir.path().to_path_buf(),
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
        dry_run: true,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false);

    assert!(matches!(result, Err(ApplyError::DryRunFlagSet)));
    assert!(source.exists());
    let _ = dest;
}

#[test]
fn test_force_bypasses_blocked_not_invalid() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let _dest = dir.path().join("dest.txt");
    fs::write(&source, "test").unwrap();

    let plan = OperationPlan {
        id: "test-force-blocked".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::Delete {
            path: source.clone(),
            reason: "test".to_string(),
        }],
        estimated_impact: crate::agent::EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 1,
            dirs_affected: 1,
            total_bytes: 1024,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 0;
    summary.blocked = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Delete {
                path: source.clone(),
                reason: "test".to_string(),
            },
            status: ValidationStatus::BlockedByConstraint("preserve_existing_folders".to_string()),
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: true,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 0,
    };

    let executor = Executor::default();

    let result_no_force = executor.execute_with_options(&plan, &validation, false);
    assert!(result_no_force.is_err());
    assert!(source.exists());

    let result_force = executor
        .execute_with_options(&plan, &validation, true)
        .unwrap();
    assert!(!result_force.log.entries.is_empty());
    assert!(result_force.log.entries[0].status.is_skipped());
    assert!(source.exists());
    assert_eq!(result_force.log.skipped_count, 1);
}

#[test]
fn test_force_does_not_bypass_invalid() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("nonexistent.txt");
    let dest = dir.path().join("dest.txt");

    let plan = OperationPlan {
        id: "test-force-invalid".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: None,
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 0;
    summary.invalid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Invalid("Source not found".to_string()),
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: true,
        has_warnings: false,
        executable_operations: 0,
    };

    let executor = Executor::default();
    let result = executor.execute_with_options(&plan, &validation, true);

    assert!(matches!(result, Err(ApplyError::InvalidPlan(msg)) if msg.contains("INVALID")));
}

#[test]
fn test_phase6c_executor_conflict_plus_force_rejected() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let dest = dir.path().join("dest.txt");
    fs::write(&source, "test").unwrap();
    fs::write(&dest, "conflict").unwrap();

    let plan = OperationPlan {
        id: "test-conflict-force".to_string(),
        recommendation_id: "rec-conflict".to_string(),
        scope: dir.path().to_path_buf(),
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
        has_conflicts: true,
        dry_run: false,
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.conflicts = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Conflict("Destination file exists".to_string()),
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: true,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 0,
    };

    let executor = Executor::default();
    let result = executor.execute_with_options(&plan, &validation, true);

    assert!(
        matches!(
            result,
            Err(ApplyError::InvalidPlan(msg)) if msg.contains("CONFLICT")
        ),
        "Executor must reject CONFLICT even with force"
    );

    assert!(
        source.exists(),
        "source must still exist (executor defense-in-depth)"
    );
}

#[test]
fn test_phase6c_executor_dry_run_rejected_on_execute() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("src.txt");
    let dest = dir.path().join("dest.txt");
    fs::write(&source, "test").unwrap();

    let plan = OperationPlan {
        id: "test-dry-run-exec".to_string(),
        recommendation_id: "rec-dry".to_string(),
        scope: dir.path().to_path_buf(),
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
        dry_run: true,
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute_with_options(&plan, &validation, false);

    assert!(matches!(result, Err(ApplyError::DryRunFlagSet)));
    assert!(source.exists(), "source must still exist");
    assert!(!dest.exists(), "dest must not be created for dry-run plan");
}

fn create_move_plan_with_category_dir(
    dir: &std::path::Path,
    source_name: &str,
    category_subdir: &str,
    file_content: &str,
) -> (OperationPlan, ValidationResult) {
    let source = dir.join(source_name);
    fs::write(&source, file_content).unwrap();

    let dest_dir = dir.join(category_subdir);
    fs::create_dir_all(&dest_dir).unwrap();
    let dest = dest_dir.join(source_name);

    let plan = OperationPlan {
        id: "test-move-category".to_string(),
        recommendation_id: "rec-move".to_string(),
        scope: dir.to_path_buf(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    (plan, validation)
}

#[test]
fn test_execute_move_into_existing_directory() {
    let dir = tempdir().unwrap();
    let (plan, validation) =
        create_move_plan_with_category_dir(dir.path(), "doc.pdf", "Documents", "content");

    let source = dir.path().join("doc.pdf");
    let dest = dir.path().join("Documents").join("doc.pdf");
    assert!(source.exists());
    assert!(dir.path().join("Documents").is_dir());

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(
        result.is_complete,
        "move into existing directory should succeed"
    );
    assert!(
        result.log.entries[0].status.is_success(),
        "entry should be Success"
    );
    assert!(!source.exists(), "source should no longer exist");
    assert!(dest.exists(), "dest should now exist at full file path");
}

#[test]
fn test_execute_move_creates_parent_directory() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("photo.jpg");
    fs::write(&source, "img").unwrap();
    let dest = dir.path().join("Images").join("photo.jpg");

    assert!(
        !dir.path().join("Images").exists(),
        "Images directory should not exist yet"
    );

    let plan = OperationPlan {
        id: "test-move-parent".to_string(),
        recommendation_id: "rec".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor.execute(&plan, &validation, false).unwrap();

    assert!(result.is_complete, "move should succeed");
    assert!(
        result.log.entries[0].status.is_success(),
        "entry should be Success"
    );
    assert!(!source.exists(), "source should be gone");
    assert!(dest.exists(), "dest should exist");
    assert!(
        dir.path().join("Images").is_dir(),
        "parent directory should be created"
    );
}

#[test]
fn test_execute_move_missing_source() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("missing.txt");
    let dest = dir.path().join("Documents").join("missing.txt");

    let plan = OperationPlan {
        id: "test-move-missing".to_string(),
        recommendation_id: "rec".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor
        .execute_with_options(&plan, &validation, false)
        .unwrap();

    assert!(!result.is_complete, "move with missing source should fail");
    assert!(
        result.log.entries[0].status.is_failed(),
        "entry should be Failed due to missing source"
    );
}

#[test]
fn test_execute_move_source_equals_dest() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("same.txt");
    fs::write(&path, "content").unwrap();

    let plan = OperationPlan {
        id: "test-move-same".to_string(),
        recommendation_id: "rec".to_string(),
        scope: dir.path().to_path_buf(),
        operations: vec![FileSystemOperation::Move {
            source: path.clone(),
            dest: path.clone(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: path.clone(),
                dest: path.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let result = executor
        .execute_with_options(&plan, &validation, false)
        .unwrap();

    assert!(
        result.log.entries[0].status.is_failed(),
        "source == dest should fail at execution"
    );
    assert!(path.exists(), "file should still exist");
}

#[test]
fn test_execute_move_dest_conflict_rejected() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let dest = dir.path().join("Documents").join("source.txt");
    fs::write(&source, "new").unwrap();
    fs::create_dir_all(dir.path().join("Documents")).unwrap();
    fs::write(&dest, "existing").unwrap();

    let plan = OperationPlan {
        id: "test-move-conflict".to_string(),
        recommendation_id: "rec".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.conflicts = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Conflict("Destination file exists".to_string()),
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: true,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 0,
    };

    let executor = Executor::default();
    let result = executor.execute_with_options(&plan, &validation, false);

    assert!(
        matches!(
            result,
            Err(ApplyError::InvalidPlan(msg)) if msg.contains("CONFLICT")
        ),
        "executor must reject CONFLICT"
    );
    assert!(
        source.exists(),
        "source must still exist after rejected conflict"
    );
}

#[test]
fn test_undo_move_into_directory() {
    let dir = tempdir().unwrap();
    let (plan, validation) =
        create_move_plan_with_category_dir(dir.path(), "doc.pdf", "Documents", "undo me");

    let source = dir.path().join("doc.pdf");
    let dest = dir.path().join("Documents").join("doc.pdf");
    assert!(source.exists());

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();

    assert!(
        apply_result.log.entries[0].status.is_success(),
        "move should succeed"
    );
    assert!(!source.exists(), "source should be gone after apply");
    assert!(dest.exists(), "dest should exist after apply");

    let undo_result = executor.undo(&apply_result.log).unwrap();

    assert_eq!(
        undo_result.total_undo_operations, 1,
        "should undo 1 operation"
    );
    assert_eq!(undo_result.applied_undoes.len(), 1);
    assert!(
        undo_result.applied_undoes[0].status.is_success(),
        "undo should succeed"
    );
    assert!(source.exists(), "source should be restored after undo");
    assert!(!dest.exists(), "dest should be gone after undo");
}

#[test]
fn test_undo_move_into_directory_no_parent() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("photo.jpg");
    let dest = dir.path().join("Images").join("photo.jpg");
    fs::write(&source, "img").unwrap();

    let plan = OperationPlan {
        id: "test-undo-new-parent".to_string(),
        recommendation_id: "rec".to_string(),
        scope: dir.path().to_path_buf(),
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
        created_at: 1234567890,
        validation_context: Some(crate::agent::PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: dir.path().to_path_buf(),
        validated_operations: vec![ValidatedOperation {
            operation: FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            },
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    let executor = Executor::default();
    let apply_result = executor.execute(&plan, &validation, false).unwrap();
    assert!(apply_result.is_complete, "move should succeed");

    assert!(!source.exists(), "source should be gone");
    assert!(dest.exists(), "dest should exist");

    let undo_result = executor.undo(&apply_result.log).unwrap();
    assert_eq!(undo_result.total_undo_operations, 1);
    assert!(source.exists(), "source should be restored after undo");
}

#[test]
fn test_operation_log_save_load_round_trip() {
    let dir = tempdir().unwrap();
    let (plan, _validation) = create_full_plan(dir.path(), false);

    let log_path = dir.path().join("operation-log.json");
    let log = OperationLog::new(&plan.id);
    let log_with_entry = {
        let mut l = log.clone();
        l.add_entry(LogEntry {
            id: "entry-1".to_string(),
            plan_id: plan.id.clone(),
            operation_type: "move".to_string(),
            original_source: dir.path().join("source.txt"),
            applied_target: dir.path().join("dest.txt"),
            status: ExecutionStatus::Success,
            started_at: 1000,
            completed_at: Some(1001),
            error: None,
            undo_supported: true,
        });
        l.finalize();
        l
    };

    log_with_entry.save(&log_path).expect("save should succeed");

    let loaded = OperationLog::load(&log_path).expect("load should succeed");
    assert_eq!(loaded.plan_id, plan.id);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.success_count, 1);
    assert_eq!(loaded.total_entries, 1);
    assert_eq!(loaded.entries[0].operation_type, "move");
    assert_eq!(loaded.entries[0].status, ExecutionStatus::Success);
}

#[test]
fn test_cross_process_undo_simulation() {
    let dir = tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();
    let (plan, validation) = create_full_plan(dir.path(), false);

    let source = dir.path().join("source.txt");
    let dest = dir.path().join("dest.txt");
    assert!(source.exists());
    assert!(!dest.exists());

    let executor = Executor::default();
    let apply_result = executor
        .execute(&plan, &validation, false)
        .expect("should execute");

    assert!(apply_result.is_complete);
    assert!(!source.exists(), "source should be moved");
    assert!(dest.exists(), "dest should exist");

    let log_path = dir.path().join("operation-log.json");
    apply_result.log.save(&log_path).expect("save log");

    drop(apply_result);

    let reloaded = Executor::default();
    let loaded_log = OperationLog::load(&log_path).expect("should load log");

    let undo_result = reloaded.undo(&loaded_log).expect("should undo");
    assert_eq!(undo_result.total_undo_operations, 1);
    assert!(source.exists(), "source should be restored after undo");

    let _ = dir_path;
}

#[test]
fn test_load_malformed_json_rejected() {
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("bad-log.json");
    std::fs::write(&log_path, "{ this is not valid json").unwrap();

    let result = OperationLog::load(&log_path);
    assert!(result.is_err(), "malformed JSON should be rejected");
}

#[test]
fn test_load_missing_file_rejected() {
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("nonexistent-log.json");

    let result = OperationLog::load(&log_path);
    assert!(result.is_err(), "missing log file should be rejected");
    let err = result.unwrap_err();
    assert!(
        matches!(err, ApplyError::IoError(_)),
        "missing file should produce IoError, got {:?}",
        err
    );
}

#[test]
fn test_load_log_with_empty_plan_id_rejected() {
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("bad-log.json");

    let bad_log = serde_json::json!({
        "id": "log-123",
        "plan_id": "",
        "entries": [],
        "started_at": 1000,
        "completed_at": null,
        "success_count": 0,
        "failure_count": 0,
        "skipped_count": 0,
        "total_entries": 0
    });
    std::fs::write(&log_path, bad_log.to_string()).unwrap();

    let result = OperationLog::load(&log_path);
    assert!(result.is_err(), "log with empty plan_id should be rejected");
}

#[test]
fn test_load_log_with_mismatched_entry_plan_id_rejected() {
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("bad-log.json");

    let bad_log = serde_json::json!({
        "id": "log-123",
        "plan_id": "plan-A",
        "entries": [
            {
                "id": "entry-1",
                "plan_id": "plan-B",
                "operation_type": "move",
                "original_source": "/tmp/source.txt",
                "applied_target": "/tmp/dest.txt",
                "status": "success",
                "started_at": 1000,
                "completed_at": null,
                "error": null,
                "undo_supported": true
            }
        ],
        "started_at": 1000,
        "completed_at": null,
        "success_count": 1,
        "failure_count": 0,
        "skipped_count": 0,
        "total_entries": 1
    });
    std::fs::write(&log_path, bad_log.to_string()).unwrap();

    let result = OperationLog::load(&log_path);
    assert!(
        result.is_err(),
        "log with mismatched entry plan_id should be rejected"
    );
}
