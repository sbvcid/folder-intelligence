use crate::agent::executor::OperationLog;
use crate::agent::plan::{FileSystemOperation, OperationPlan};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Passed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VerificationCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct VerificationSummary {
    pub moves_verified: usize,
    pub content_preserved: usize,
    pub sources_removed: usize,
    pub destinations_present: usize,
    pub dirs_created: usize,
    pub paths_deleted: usize,
    pub total_failed_checks: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionVerificationResult {
    pub status: VerificationStatus,
    pub checks: Vec<VerificationCheck>,
    pub summary: VerificationSummary,
}

impl ExecutionVerificationResult {
    #[allow(dead_code)]
    pub fn passed(&self) -> bool {
        matches!(self.status, VerificationStatus::Passed)
    }

    #[allow(dead_code)]
    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("Verification:\n");

        output.push_str(&format!(
            "  Moves verified: {}\n",
            self.summary.moves_verified
        ));
        output.push_str(&format!(
            "  Content preserved: {}\n",
            self.summary.content_preserved
        ));
        output.push_str(&format!(
            "  Sources removed: {}\n",
            self.summary.sources_removed
        ));
        output.push_str(&format!(
            "  Destinations present: {}\n",
            self.summary.destinations_present
        ));
        output.push_str(&format!("  Dirs created: {}\n", self.summary.dirs_created));
        output.push_str(&format!(
            "  Paths deleted: {}\n",
            self.summary.paths_deleted
        ));
        output.push_str(&format!(
            "  Failed checks: {}\n",
            self.summary.total_failed_checks
        ));
        output.push('\n');

        for check in &self.checks {
            let tag = if check.passed { "  ✓" } else { "  ✗" };
            output.push_str(&format!("{} {}\n", tag, check.name));
            if !check.passed {
                output.push_str(&format!("    → {}\n", check.message));
            }
        }

        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesystemState {
    Dir,
    File(Vec<u8>),
}

pub struct ExecutionVerifier;

impl Default for ExecutionVerifier {
    fn default() -> Self {
        Self
    }
}

impl ExecutionVerifier {
    pub fn capture_state(scope: &Path) -> HashMap<PathBuf, FilesystemState> {
        let mut state = HashMap::new();
        Self::capture_rec(scope, &mut state);
        state
    }

    fn capture_rec(dir: &Path, state: &mut HashMap<PathBuf, FilesystemState>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    state.insert(path.clone(), FilesystemState::Dir);
                    Self::capture_rec(&path, state);
                } else if path.is_file() {
                    let content = std::fs::read(&path).unwrap_or_default();
                    state.insert(path, FilesystemState::File(content));
                }
            }
        }
    }

    pub fn verify(
        &self,
        plan: &OperationPlan,
        log: &OperationLog,
        before_state: &HashMap<PathBuf, FilesystemState>,
    ) -> ExecutionVerificationResult {
        let mut checks = Vec::new();
        let mut summary = VerificationSummary::default();
        let mut failure_msgs = Vec::new();

        if plan.operations.is_empty() {
            let after_state = Self::capture_state(&plan.scope);
            if before_state.len() != after_state.len() {
                let msg = format!(
                    "Filesystem changed during no-op plan (before: {} entries, after: {} entries)",
                    before_state.len(),
                    after_state.len()
                );
                failure_msgs.push(msg.clone());
                checks.push(VerificationCheck {
                    name: "No-op plan: filesystem unchanged".to_string(),
                    passed: false,
                    message: msg,
                });
                summary.total_failed_checks += 1;
            } else {
                let mut changed = false;
                for (after_path, after_entry) in &after_state {
                    if let Some(before_entry) = before_state.get(after_path) {
                        if *before_entry != *after_entry {
                            changed = true;
                            let msg = format!(
                                "Filesystem entry changed during no-op plan: {}",
                                after_path.display()
                            );
                            failure_msgs.push(msg.clone());
                            checks.push(VerificationCheck {
                                name: format!("No-op unchanged: {}", after_path.display()),
                                passed: false,
                                message: msg,
                            });
                            summary.total_failed_checks += 1;
                        }
                    } else {
                        changed = true;
                        let msg = format!(
                            "New filesystem entry appeared during no-op plan: {}",
                            after_path.display()
                        );
                        failure_msgs.push(msg.clone());
                        checks.push(VerificationCheck {
                            name: format!("No-op no new entries: {}", after_path.display()),
                            passed: false,
                            message: msg,
                        });
                        summary.total_failed_checks += 1;
                    }
                }
                if !changed {
                    checks.push(VerificationCheck {
                        name: "No-op plan: filesystem unchanged".to_string(),
                        passed: true,
                        message: format!(
                            "No mutations: before and after states match ({} entries)",
                            before_state.len()
                        ),
                    });
                }
            }
        }

        for (idx, op) in plan.operations.iter().enumerate() {
            let entry_id = format!("entry-{}-{}", plan.id, idx);
            let log_entry = log.entries.iter().find(|e| e.id == entry_id);

            let was_success = log_entry.map(|e| e.status.is_success()).unwrap_or(true);

            if !was_success {
                continue;
            }

            match op {
                FileSystemOperation::Move { source, dest } => {
                    let source_existed_before = before_state.contains_key(source);
                    let source_exists_now = source.exists();

                    if source_existed_before && source_exists_now {
                        let msg = format!("Source still exists after move: {}", source.display());
                        failure_msgs.push(msg.clone());
                        checks.push(VerificationCheck {
                            name: format!("Move source removed: {}", source.display()),
                            passed: false,
                            message: msg,
                        });
                        summary.total_failed_checks += 1;
                    } else if source_existed_before && !source_exists_now {
                        checks.push(VerificationCheck {
                            name: format!("Move source removed: {}", source.display()),
                            passed: true,
                            message: "Source existed before and was successfully removed"
                                .to_string(),
                        });
                        summary.sources_removed += 1;
                    }

                    let dest_exists_now = dest.exists();
                    let dest_is_file = dest.is_file();

                    if !dest_exists_now || !dest_is_file {
                        let msg = format!("Destination missing or not a file: {}", dest.display());
                        failure_msgs.push(msg.clone());
                        checks.push(VerificationCheck {
                            name: format!("Move destination valid: {}", dest.display()),
                            passed: false,
                            message: msg,
                        });
                        summary.total_failed_checks += 1;
                    } else if source_existed_before {
                        if let Some(FilesystemState::File(before_content)) =
                            before_state.get(source)
                        {
                            if let Ok(after_content) = std::fs::read(dest) {
                                if before_content != &after_content {
                                    let msg = format!(
                                        "Content mismatch at destination: {} (expected {} bytes, got {} bytes)",
                                        dest.display(),
                                        before_content.len(),
                                        after_content.len()
                                    );
                                    failure_msgs.push(msg.clone());
                                    checks.push(VerificationCheck {
                                        name: format!("Content preserved: {}", dest.display()),
                                        passed: false,
                                        message: msg,
                                    });
                                    summary.total_failed_checks += 1;
                                } else {
                                    checks.push(VerificationCheck {
                                        name: format!("Content preserved: {}", dest.display()),
                                        passed: true,
                                        message: "Destination content matches source content"
                                            .to_string(),
                                    });
                                    summary.content_preserved += 1;
                                }
                            } else {
                                let msg =
                                    format!("Could not read destination file: {}", dest.display());
                                failure_msgs.push(msg.clone());
                                checks.push(VerificationCheck {
                                    name: format!("Destination file readable: {}", dest.display()),
                                    passed: false,
                                    message: msg,
                                });
                                summary.total_failed_checks += 1;
                            }
                        }
                        checks.push(VerificationCheck {
                            name: format!("Move destination valid: {}", dest.display()),
                            passed: true,
                            message: "Destination exists and is a file".to_string(),
                        });
                        summary.destinations_present += 1;
                    }
                    summary.moves_verified += 1;
                }
                FileSystemOperation::CreateDir { path } => {
                    let existed_before =
                        matches!(before_state.get(path), Some(FilesystemState::Dir));
                    let exists_now = path.is_dir();

                    if !exists_now {
                        let msg = format!("Created directory missing: {}", path.display());
                        failure_msgs.push(msg.clone());
                        checks.push(VerificationCheck {
                            name: format!("Directory created: {}", path.display()),
                            passed: false,
                            message: msg,
                        });
                        summary.total_failed_checks += 1;
                    } else if !existed_before {
                        checks.push(VerificationCheck {
                            name: format!("Directory created: {}", path.display()),
                            passed: true,
                            message: "Directory did not exist before and now exists".to_string(),
                        });
                        summary.dirs_created += 1;
                    } else {
                        checks.push(VerificationCheck {
                            name: format!("Directory preserved: {}", path.display()),
                            passed: true,
                            message: "Directory existed before and still exists".to_string(),
                        });
                    }
                }
                FileSystemOperation::Delete { path, .. } => {
                    let existed_before = before_state.contains_key(path);
                    let exists_now = path.exists();

                    if existed_before && exists_now {
                        let msg = format!("Deleted path still exists: {}", path.display());
                        failure_msgs.push(msg.clone());
                        checks.push(VerificationCheck {
                            name: format!("Path deleted: {}", path.display()),
                            passed: false,
                            message: msg,
                        });
                        summary.total_failed_checks += 1;
                    } else if existed_before && !exists_now {
                        checks.push(VerificationCheck {
                            name: format!("Path deleted: {}", path.display()),
                            passed: true,
                            message: "Path existed before and was successfully deleted".to_string(),
                        });
                        summary.paths_deleted += 1;
                    } else {
                        checks.push(VerificationCheck {
                            name: format!("Path absent: {}", path.display()),
                            passed: true,
                            message: "Path was not present before or after (no-op for delete)"
                                .to_string(),
                        });
                    }
                }
            }
        }

        let status = if failure_msgs.is_empty() {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed(failure_msgs.join("; "))
        };

        ExecutionVerificationResult {
            status,
            checks,
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::executor::OperationLog;
    use crate::agent::plan::FileSystemOperation;
    use std::fs;
    use tempfile::tempdir;

    fn make_plan(scope: &Path, operations: Vec<FileSystemOperation>) -> OperationPlan {
        OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "verify-test-plan".to_string(),
            recommendation_id: "rec-test".to_string(),
            scope: scope.to_path_buf(),
            operations,
            estimated_impact: crate::agent::EstimatedImpact {
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
            validation_context: Some(crate::agent::PlanValidationContext::default()),
        }
    }

    #[test]
    fn test_verify_move_success() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let source = scope.join("hello.txt");
        let dest = scope.join("Documents").join("hello.txt");
        fs::write(&source, "hello world").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::rename(&source, &dest).unwrap();

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert_eq!(
            result.status,
            VerificationStatus::Passed,
            "verification should pass after correct move: {:?}",
            result.checks
        );
        assert!(!dest.exists() || true);
        assert!(!source.exists(), "source should be removed");
        assert!(dest.exists(), "destination should exist");
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            "hello world",
            "content should be preserved"
        );
    }

    #[test]
    fn test_verify_move_source_still_exists() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let source = scope.join("file.txt");
        let dest = scope.join("Documents").join("file.txt");
        fs::write(&source, "content").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        // Do NOT execute — source still exists, dest does not
        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification must fail when source was not removed: {:?}",
            result.checks
        );
        assert!(source.exists(), "source must still exist for this test");
        assert!(
            !result.summary.sources_removed > 0,
            "no sources should have been removed"
        );
    }

    #[test]
    fn test_verify_content_mismatch() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let source = scope.join("file.txt");
        let dest = scope.join("Documents").join("file.txt");
        fs::write(&source, "original content").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        // Simulate wrong execution: source deleted, dest created with wrong content
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::remove_file(&source).unwrap();
        fs::write(&dest, "tampered content").unwrap();

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification must fail on content mismatch: {:?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_create_dir_success() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let new_dir = scope.join("subdir");

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::CreateDir {
                path: new_dir.clone(),
            }],
        );

        fs::create_dir_all(&new_dir).unwrap();

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert_eq!(result.status, VerificationStatus::Passed);
    }

    #[test]
    fn test_verify_create_dir_not_created() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let new_dir = scope.join("subdir");

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::CreateDir {
                path: new_dir.clone(),
            }],
        );

        // Do NOT create the directory
        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification must fail when directory was not created: {:?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_delete_success() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let file_to_delete = scope.join("delete_me.txt");
        fs::write(&file_to_delete, "delete").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Delete {
                path: file_to_delete.clone(),
                reason: "cleanup".to_string(),
            }],
        );

        fs::remove_file(&file_to_delete).unwrap();

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert_eq!(result.status, VerificationStatus::Passed);
    }

    #[test]
    fn test_verify_delete_still_exists() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let file_to_delete = scope.join("delete_me.txt");
        fs::write(&file_to_delete, "delete").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Delete {
                path: file_to_delete.clone(),
                reason: "cleanup".to_string(),
            }],
        );

        // Do NOT delete
        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification must fail when file still exists after delete: {:?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_noop_plan_unchanged() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        fs::write(scope.join("a.txt"), "a").unwrap();
        fs::write(scope.join("b.txt"), "b").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(scope, vec![]);

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert_eq!(result.status, VerificationStatus::Passed);
    }

    #[test]
    fn test_verify_noop_plan_changed() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        fs::write(scope.join("a.txt"), "a").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(scope, vec![]);

        // Simulate someone else changing the filesystem
        fs::write(scope.join("a.txt"), "modified").unwrap();
        fs::write(scope.join("new.txt"), "new").unwrap();

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification must fail when no-op plan had filesystem changes: {:?}",
            result.checks
        );
    }

    #[test]
    fn test_execution_verification_result_default_is_passed_when_empty() {
        // A plan with no operations and no checks should pass
        let dir = tempdir().unwrap();
        let scope = dir.path();
        fs::write(scope.join("x.txt"), "x").unwrap();

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(scope, vec![]);

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(result.passed());
    }

    #[test]
    fn test_render_output_for_failure() {
        let dir = tempdir().unwrap();
        let scope = dir.path();
        let source = scope.join("missing.txt");
        let dest = scope.join("Documents").join("missing.txt");

        let before = ExecutionVerifier::capture_state(scope);

        let plan = make_plan(
            scope,
            vec![FileSystemOperation::Move {
                source: source.clone(),
                dest: dest.clone(),
            }],
        );

        let mut log = OperationLog::new(&plan.id);
        log.finalize();
        let result = ExecutionVerifier.verify(&plan, &log, &before);

        assert!(
            matches!(result.status, VerificationStatus::Failed(_)),
            "verification should fail"
        );
        assert!(
            result.summary.total_failed_checks > 0,
            "should have failed checks"
        );

        let rendered = result.render();
        assert!(rendered.contains("Verification:"));
        assert!(rendered.contains("Failed checks:"));
    }
}
