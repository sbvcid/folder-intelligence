use crate::agent::plan::FileSystemOperation;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// File metadata captured for cheap change detection.
///
/// This is NOT a cryptographic identity — it provides metadata-based
/// change detection (size + mtime) to narrow the TOCTOU window between
/// precondition capture and execution.
///
/// Post-execution content verification is handled separately by
/// Phase 12's verification layer (`tests/coverage_verification_tests.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub mtime_secs: u64,
}

impl FileMetadata {
    pub fn capture(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let mtime = metadata.modified().ok()?;
        let mtime_secs = mtime.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some(FileMetadata {
            size: metadata.len(),
            mtime_secs,
        })
    }
}

/// Precondition captures filesystem state expected before an operation is executed.
///
/// For Move: source existence + metadata, dest absence + metadata.
/// For CreateDir: target path absence.
/// For Delete: target existence + metadata.
///
/// This is metadata-based (size + mtime), not content-based.
/// It detects state changes to narrow the TOCTOU window, but does NOT
/// guarantee detection of all external modifications (e.g. same-size
/// content changes may go undetected without a mtime change).
#[derive(Debug, Clone, PartialEq)]
pub struct Precondition {
    pub source_existed: bool,
    pub source_metadata: Option<FileMetadata>,
    pub dest_existed: bool,
    pub dest_metadata: Option<FileMetadata>,
}

impl Precondition {
    pub fn capture(op: &FileSystemOperation) -> Self {
        let (source_path, dest_path) = match op {
            FileSystemOperation::Move { source, dest } => {
                (Some(source.as_path()), Some(dest.as_path()))
            }
            FileSystemOperation::CreateDir { path } => (None, Some(path.as_path())),
            FileSystemOperation::Delete { path, .. } => (Some(path.as_path()), None),
        };

        let source_existed = source_path.map(|p| p.exists()).unwrap_or(false);
        let source_metadata = source_path.and_then(|p| FileMetadata::capture(p));
        let dest_existed = dest_path.map(|p| p.exists()).unwrap_or(false);
        let dest_metadata = dest_path.and_then(|p| FileMetadata::capture(p));

        Precondition {
            source_existed,
            source_metadata,
            dest_existed,
            dest_metadata,
        }
    }

    pub fn check_unchanged(&self, op: &FileSystemOperation) -> bool {
        let (source_path, dest_path) = match op {
            FileSystemOperation::Move { source, dest } => {
                (Some(source.as_path()), Some(dest.as_path()))
            }
            FileSystemOperation::CreateDir { path } => (None, Some(path.as_path())),
            FileSystemOperation::Delete { path, .. } => (Some(path.as_path()), None),
        };

        if let Some(src) = source_path {
            let current_exists = src.exists();
            let current_meta = FileMetadata::capture(src);
            if self.source_existed {
                if !current_exists {
                    return false;
                }
                if current_meta.is_none() || current_meta.as_ref() != self.source_metadata.as_ref()
                {
                    return false;
                }
            } else {
                if current_exists {
                    return false;
                }
            }
        }

        if let Some(dst) = dest_path {
            let current_exists = dst.exists();
            let current_meta = FileMetadata::capture(dst);
            if self.dest_existed {
                if !current_exists {
                    return false;
                }
                if current_meta.is_none() || current_meta.as_ref() != self.dest_metadata.as_ref() {
                    return false;
                }
            } else {
                if current_exists {
                    return false;
                }
            }
        }

        true
    }
}

/// Cooperative process-level file lock to prevent concurrent
/// folder-intelligence executions from operating on the same scope.
///
/// This is NOT a universal filesystem lock — it only coordinates
/// cooperative folder-intelligence processes that also use `ScopeLock`.
/// External programs that do not acquire this lock can still modify
/// files in the scope. Precondition checks and post-execution verification
/// provide defense-in-depth against such external changes.
pub struct ScopeLock {
    _lock_file: File,
    lock_path: PathBuf,
}

impl ScopeLock {
    /// Acquire an exclusive lock for the given scope and plan_id.
    ///
    /// Lock file is created at `{scope}/.{plan_id}.lock`.
    /// The lock is acquired BEFORE any metadata is written to avoid
    /// truncating or corrupting lock state belonging to a waiting process.
    ///
    /// This is a blocking call — it waits until the lock can be acquired.
    pub fn acquire(scope: &Path, plan_id: &str) -> Result<Self, String> {
        let lock_path = scope.join(format!(".{}.lock", plan_id));

        if !scope.exists() {
            fs::create_dir_all(scope)
                .map_err(|e| format!("Failed to create scope for lock: {}", e))?;
        }

        // Open/create lock file WITHOUT truncate. Truncating before lock
        // acquisition would corrupt lock state if another process is
        // waiting for the lock and has written metadata.
        let mut lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(|e| format!("Failed to open lock file: {}", e))?;

        // Acquire exclusive lock BEFORE writing any metadata
        lock_file
            .lock_exclusive()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

        // Only after ownership is acquired, write lock metadata
        let _ = lock_file.write_all(plan_id.as_bytes());

        Ok(ScopeLock {
            _lock_file: lock_file,
            lock_path,
        })
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for ScopeLock {
    fn drop(&mut self) {
        let _ = self._lock_file.unlock();
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Result of a guarded execution attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionResult {
    Success,
    Conflict(String),
    Failed(String),
}

impl ExecutionResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionResult::Success)
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, ExecutionResult::Conflict(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, ExecutionResult::Failed(_))
    }
}

/// OperationGuard performs a single operation with precondition capture,
/// revalidation, and execution.
///
/// Lifecycle:
/// 1. `new()` captures the precondition (filesystem snapshot)
/// 2. `check_and_execute()` revalidates and executes:
///    a. Re-inspect operation state (detect state drift since capture)
///    b. Check precondition unchanged (detect metadata changes)
///    c. Execute if both checks pass; return Conflict otherwise
pub struct OperationGuard {
    precondition: Precondition,
    op: FileSystemOperation,
    _lock: ScopeLock,
}

impl OperationGuard {
    pub fn new(op: FileSystemOperation, lock: ScopeLock) -> Self {
        let precondition = Precondition::capture(&op);
        OperationGuard {
            precondition,
            op,
            _lock: lock,
        }
    }

    /// Revalidate and execute the operation.
    ///
    /// This is the single authoritative guarded execution entry point.
    /// It re-captures the filesystem precondition and compares it with
    /// the precondition captured at construction time. Any discrepancy
    /// indicates a TOCTOU race and the operation is not executed.
    pub fn check_and_execute(&self) -> ExecutionResult {
        let current = Precondition::capture(&self.op);
        if current != self.precondition {
            return ExecutionResult::Conflict(format!(
                "TOCTOU: Filesystem state changed between precondition capture and execution for {} operation",
                self.op.operation_type()
            ));
        }

        self.execute()
    }

    fn execute(&self) -> ExecutionResult {
        match &self.op {
            FileSystemOperation::Move { source, dest } => {
                if !source.exists() {
                    return ExecutionResult::Failed(format!(
                        "Source not found: {}",
                        source.display()
                    ));
                }
                if source == dest {
                    return ExecutionResult::Failed("Source equals destination".to_string());
                }

                let parent = dest.parent().unwrap_or(dest);
                if !parent.exists() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        return ExecutionResult::Failed(format!("Failed to create parent: {}", e));
                    }
                }

                match fs::rename(source, dest) {
                    Ok(()) => ExecutionResult::Success,
                    Err(e) => ExecutionResult::Failed(format!("Rename failed: {}", e)),
                }
            }
            FileSystemOperation::CreateDir { path } => {
                if path.exists() {
                    return ExecutionResult::Success;
                }
                match fs::create_dir_all(path) {
                    Ok(()) => ExecutionResult::Success,
                    Err(e) => ExecutionResult::Failed(format!("CreateDir failed: {}", e)),
                }
            }
            FileSystemOperation::Delete { path, .. } => {
                if !path.exists() {
                    return ExecutionResult::Failed(format!(
                        "Delete target not found: {}",
                        path.display()
                    ));
                }
                match fs::remove_file(path) {
                    Ok(()) => ExecutionResult::Success,
                    Err(e) => ExecutionResult::Failed(format!("Delete failed: {}", e)),
                }
            }
        }
    }

    pub fn precondition(&self) -> &Precondition {
        &self.precondition
    }

    pub fn operation(&self) -> &FileSystemOperation {
        &self.op
    }
}

#[cfg(test)]
#[path = "operation_guard_tests.rs"]
mod tests;
