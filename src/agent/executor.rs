use crate::agent::plan::{FileSystemOperation, OperationPlan};
use crate::agent::validate::ValidationResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    Success,
    Failed(String),
    Skipped(String),
    UndoNotSupported,
}

impl ExecutionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionStatus::Success)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, ExecutionStatus::Failed(_))
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, ExecutionStatus::Skipped(_))
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        matches!(self, ExecutionStatus::Pending)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UndoConflict {
    SourceMissing {
        path: PathBuf,
        message: String,
    },
    TargetExists {
        path: PathBuf,
        message: String,
    },
    NotUndoable {
        operation_type: String,
        message: String,
    },
}

impl UndoConflict {
    #[allow(dead_code)]
    pub fn description(&self) -> String {
        match self {
            UndoConflict::SourceMissing { path, message } => {
                format!("{} — {}", message, path.display())
            }
            UndoConflict::TargetExists { path, message } => {
                format!("{} — {}", message, path.display())
            }
            UndoConflict::NotUndoable { operation_type, message } => {
                format!("{} — {}", message, operation_type)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LogEntry {
    pub id: String,
    pub plan_id: String,
    pub operation_type: String,
    pub original_source: PathBuf,
    pub applied_target: PathBuf,
    pub status: ExecutionStatus,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
    pub undo_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OperationLog {
    pub id: String,
    pub plan_id: String,
    pub entries: Vec<LogEntry>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub success_count: usize,
    pub failure_count: usize,
    pub skipped_count: usize,
    pub total_entries: usize,
}

impl OperationLog {
    pub fn new(plan_id: &str) -> Self {
        let now = now_secs();
        OperationLog {
            id: format!("log-{}", now),
            plan_id: plan_id.to_string(),
            entries: Vec::new(),
            started_at: now,
            completed_at: None,
            success_count: 0,
            failure_count: 0,
            skipped_count: 0,
            total_entries: 0,
        }
    }

    pub fn add_entry(&mut self, entry: LogEntry) {
        if entry.status.is_success() {
            self.success_count += 1;
        } else if entry.status.is_failed() {
            self.failure_count += 1;
        } else if entry.status.is_skipped() {
            self.skipped_count += 1;
        }

        self.entries.push(entry);
        self.total_entries = self.entries.len();
    }

    pub fn finalize(&mut self) {
        self.completed_at = Some(now_secs());
        self.total_entries = self.entries.len();
    }

    pub fn undoable_entries(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.status.is_success() && e.undo_supported)
            .collect()
    }

    pub fn undo_unsupported_entries(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| {
                (e.status.is_success() && !e.undo_supported)
                    || matches!(e.status, ExecutionStatus::UndoNotSupported)
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn failed_entries(&self) -> Vec<&LogEntry> {
        self.entries.iter().filter(|e| e.status.is_failed()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct UndoLogEntry {
    pub entry_id: String,
    pub operation_type: String,
    pub source: PathBuf,
    pub dest: PathBuf,
    pub status: ExecutionStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct UndoResult {
    pub log_id: String,
    pub applied_undoes: Vec<UndoLogEntry>,
    pub conflicts: Vec<UndoConflict>,
    pub total_undo_operations: usize,
    pub completed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApplyError {
    PlanNotValidated,
    DryRunFlagSet,
    InvalidPlan(String),
    ExecutionError(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::PlanNotValidated => write!(f, "Plan has not been validated"),
            ApplyError::DryRunFlagSet => write!(f, "Cannot apply a dry-run plan"),
            ApplyError::InvalidPlan(msg) => write!(f, "Invalid plan: {}", msg),
            ApplyError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
        }
    }
}

impl std::error::Error for ApplyError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ApplyResult {
    pub plan_id: String,
    pub log: OperationLog,
    pub is_complete: bool,
    pub can_undo: bool,
    pub undo_supported_count: usize,
    pub undo_unsupported_count: usize,
}

pub struct Executor;

impl Default for Executor {
    fn default() -> Self {
        Self
    }
}

impl Executor {
    #[allow(dead_code)]
    pub fn execute(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        _dry_run: bool,
    ) -> Result<ApplyResult, ApplyError> {
        self.execute_with_options(plan, validation, false)
    }

    pub fn execute_with_options(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
        force: bool,
    ) -> Result<ApplyResult, ApplyError> {
        if plan.dry_run {
            return Err(ApplyError::DryRunFlagSet);
        }

        let has_invalid = validation
            .validated_operations
            .iter()
            .any(|v| v.status.is_invalid());
        let has_blocked = validation
            .validated_operations
            .iter()
            .any(|v| v.status.is_blocked());

        if has_invalid {
            return Err(ApplyError::InvalidPlan(
                "Plan has INVALID operations (missing source, path outside scope, etc.)".to_string(),
            ));
        }

        if has_blocked && !force {
            return Err(ApplyError::InvalidPlan(
                "Plan has BLOCKED operations (constraint violations). Use --force to override.".to_string(),
            ));
        }

        let mut log = OperationLog::new(&plan.id);
        let mut undo_supported_count = 0;
        let mut undo_unsupported_count = 0;

        for (idx, op) in plan.operations.iter().enumerate() {
            let entry_id = format!("entry-{}-{}", plan.id, idx);
            let started_at = now_secs();

            let (status, completed_at, error, can_undo) = if force {
                let validated = validation
                    .validated_operations
                    .get(idx)
                    .map(|v| v.status.is_blocked())
                    .unwrap_or(false);
                if validated {
                    (
                        ExecutionStatus::Skipped("Blocked by constraint (force override)".to_string()),
                        Some(now_secs()),
                        Some("Skipped due to constraint block with --force".to_string()),
                        false,
                    )
                } else {
                    self.execute_operation(op, idx)
                }
            } else {
                self.execute_operation(op, idx)
            };

            if can_undo {
                undo_supported_count += 1;
            } else {
                undo_unsupported_count += 1;
            }

            let entry = LogEntry {
                id: entry_id.clone(),
                plan_id: plan.id.clone(),
                operation_type: op.operation_type().to_string(),
                original_source: Self::extract_source(op),
                applied_target: Self::extract_target(op),
                status,
                started_at,
                completed_at,
                error,
                undo_supported: can_undo,
            };

            log.add_entry(entry);
        }

        log.finalize();

        let is_complete = log.failure_count == 0 && log.skipped_count == 0;
        let can_undo = undo_supported_count > 0;

        Ok(ApplyResult {
            plan_id: plan.id.clone(),
            log,
            is_complete,
            can_undo,
            undo_supported_count,
            undo_unsupported_count,
        })
    }

    fn execute_operation(
        &self,
        op: &FileSystemOperation,
        _idx: usize,
    ) -> (ExecutionStatus, Option<u64>, Option<String>, bool) {
        match op {
            FileSystemOperation::Move { source, dest } => {
                if !source.exists() {
                    let msg = format!("Source not found: {}", source.display());
                    return (
                        ExecutionStatus::Failed(msg.clone()),
                        Some(now_secs()),
                        Some(msg),
                        true,
                    );
                }

                if source == dest {
                    let msg = "Source equals destination".to_string();
                    return (
                        ExecutionStatus::Failed(msg.clone()),
                        Some(now_secs()),
                        Some(msg),
                        true,
                    );
                }

                let parent = dest.parent().unwrap_or(dest);
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        let msg = format!("Failed to create parent: {}", e);
                        return (
                            ExecutionStatus::Failed(msg.clone()),
                            Some(now_secs()),
                            Some(msg),
                            true,
                        );
                    }
                }

                match std::fs::rename(source, dest) {
                    Ok(()) => (
                        ExecutionStatus::Success,
                        Some(now_secs()),
                        None,
                        true,
                    ),
                    Err(e) => {
                        let msg = format!("Rename failed: {}", e);
                        (
                            ExecutionStatus::Failed(msg.clone()),
                            Some(now_secs()),
                            Some(msg),
                            true,
                        )
                    }
                }
            }
            FileSystemOperation::CreateDir { path } => {
                if path.exists() {
                    let msg = format!("Directory already exists: {}", path.display());
                    return (
                        ExecutionStatus::Skipped(msg.clone()),
                        Some(now_secs()),
                        Some(msg),
                        false,
                    );
                }

                match std::fs::create_dir_all(path) {
                    Ok(()) => (
                        ExecutionStatus::Success,
                        Some(now_secs()),
                        None,
                        true,
                    ),
                    Err(e) => {
                        let msg = format!("CreateDir failed: {}", e);
                        (
                            ExecutionStatus::Failed(msg.clone()),
                            Some(now_secs()),
                            Some(msg),
                            true,
                        )
                    }
                }
            }
            FileSystemOperation::Delete { path, .. } => {
                if !path.exists() {
                    let msg = format!("Delete target not found: {}", path.display());
                    return (
                        ExecutionStatus::Failed(msg.clone()),
                        Some(now_secs()),
                        Some(msg),
                        false,
                    );
                }

                match std::fs::remove_file(path) {
                    Ok(()) => (
                        ExecutionStatus::UndoNotSupported,
                        Some(now_secs()),
                        None,
                        false,
                    ),
                    Err(e) => {
                        let msg = format!("Delete failed: {}", e);
                        (
                            ExecutionStatus::Failed(msg.clone()),
                            Some(now_secs()),
                            Some(msg),
                            false,
                        )
                    }
                }
            }
        }
    }

    fn extract_source(op: &FileSystemOperation) -> PathBuf {
        match op {
            FileSystemOperation::Move { source, .. } => source.clone(),
            FileSystemOperation::Delete { path, .. } => path.clone(),
            FileSystemOperation::CreateDir { path } => path.clone(),
        }
    }

    fn extract_target(op: &FileSystemOperation) -> PathBuf {
        match op {
            FileSystemOperation::Move { dest, .. } => dest.clone(),
            FileSystemOperation::CreateDir { path } => path.clone(),
            FileSystemOperation::Delete { path, .. } => path.clone(),
        }
    }

    pub fn undo(&self, log: &OperationLog) -> Result<UndoResult, ApplyError> {
        let undoable = log.undoable_entries();
        let mut applied_undoes = Vec::new();
        let mut conflicts = Vec::new();
        let mut undo_count = 0;

        for entry in &undoable {
            match self.generate_undo_operation(entry) {
                Some((operation, operation_type)) => {
                    undo_count += 1;
                    let source = match &operation {
                        FileSystemOperation::Move { source, .. } => source,
                        FileSystemOperation::Delete { path, .. } => path,
                        FileSystemOperation::CreateDir { path } => path,
                    };
                    let dest = match &operation {
                        FileSystemOperation::Move { dest, .. } => dest,
                        FileSystemOperation::Delete { path, .. } => path,
                        FileSystemOperation::CreateDir { path } => path,
                    };

                    let undo_result = if !source.exists() {
                        let conflict = UndoConflict::SourceMissing {
                            path: source.clone(),
                            message: format!(
                                "Cannot undo: source {} no longer exists",
                                source.display()
                            ),
                        };
                        conflicts.push(conflict);
                        None
                    } else if dest.exists() {
                        let conflict = UndoConflict::TargetExists {
                            path: dest.clone(),
                            message: format!(
                                "Cannot undo: target {} already exists",
                                dest.display()
                            ),
                        };
                        conflicts.push(conflict);
                        None
                    } else {
                        let parent = dest.parent().unwrap_or(dest);
                        if !parent.exists() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                let msg =
                                    format!("Failed to create parent for undo: {}", e);
                                applied_undoes.push(UndoLogEntry {
                                    entry_id: entry.id.clone(),
                                    operation_type,
                                    source: source.clone(),
                                    dest: dest.clone(),
                                    status: ExecutionStatus::Failed(msg.clone()),
                                    error: Some(msg),
                                });
                                continue;
                            }
                        }

                        match std::fs::rename(source, dest) {
                            Ok(()) => Some(UndoLogEntry {
                                entry_id: entry.id.clone(),
                                operation_type,
                                source: source.clone(),
                                dest: dest.clone(),
                                status: ExecutionStatus::Success,
                                error: None,
                            }),
                            Err(e) => {
                                let msg = format!("Undo rename failed: {}", e);
                                Some(UndoLogEntry {
                                    entry_id: entry.id.clone(),
                                    operation_type,
                                    source: source.clone(),
                                    dest: dest.clone(),
                                    status: ExecutionStatus::Failed(msg.clone()),
                                    error: Some(msg),
                                })
                            }
                        }
                    };

                    if let Some(log_entry) = undo_result {
                        applied_undoes.push(log_entry);
                    }
                }
                None => {
                    conflicts.push(UndoConflict::NotUndoable {
                        operation_type: entry.operation_type.clone(),
                        message: format!(
                            "Operation {} has unsupported type for undo",
                            entry.id
                        ),
                    });
                }
            }
        }

        for entry in log.undo_unsupported_entries() {
            conflicts.push(UndoConflict::NotUndoable {
                operation_type: entry.operation_type.clone(),
                message: format!(
                    "Operation {} cannot be undone (delete operation)",
                    entry.id
                ),
            });
        }

        Ok(UndoResult {
            log_id: log.id.clone(),
            applied_undoes,
            conflicts,
            total_undo_operations: undo_count,
            completed_at: now_secs(),
        })
    }

    fn generate_undo_operation(
        &self,
        entry: &LogEntry,
    ) -> Option<(FileSystemOperation, String)> {
        match entry.operation_type.as_str() {
            "move" => Some((
                FileSystemOperation::Move {
                    source: entry.applied_target.clone(),
                    dest: entry.original_source.clone(),
                },
                "move".to_string(),
            )),
            "create_dir" => Some((
                FileSystemOperation::Delete {
                    path: entry.original_source.clone(),
                    reason: "undo create_dir".to_string(),
                },
                "delete".to_string(),
            )),
            "delete" => Some((
                FileSystemOperation::Move {
                    source: entry.original_source.clone(),
                    dest: entry.applied_target.clone(),
                },
                "move".to_string(),
            )),
            _ => None,
        }
    }
}

#[allow(dead_code)]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;