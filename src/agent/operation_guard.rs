use crate::agent::plan::FileSystemOperation;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[allow(dead_code)]
const LOCK_TIMEOUT_SECS: u64 = 10;
#[allow(dead_code)]
const LOCK_POLL_INTERVAL_MILLIS: u64 = 100;

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
            FileSystemOperation::Move { source, dest } => (Some(source.as_path()), Some(dest.as_path())),
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
            FileSystemOperation::Move { source, dest } => (Some(source.as_path()), Some(dest.as_path())),
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
                if current_meta.is_none() || current_meta.as_ref() != self.source_metadata.as_ref() {
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

pub struct ScopeLock {
    _lock_file: File,
    lock_path: PathBuf,
}

impl ScopeLock {
    pub fn acquire(scope: &Path, plan_id: &str) -> Result<Self, String> {
        let lock_path = scope.join(format!(".{}.lock", plan_id));

        if !scope.exists() {
            fs::create_dir_all(scope).map_err(|e| format!("Failed to create scope for lock: {}", e))?;
        }

        let mut lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&lock_path)
            .map_err(|e| format!("Failed to create lock file: {}", e))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionResult {
    Success,
    Conflict(String),
    Failed(String),
}

#[allow(dead_code)]
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

pub struct OperationGuard {
    _lock: ScopeLock,
    precondition: Precondition,
    op: FileSystemOperation,
}

#[allow(dead_code)]
impl OperationGuard {
    pub fn new(op: FileSystemOperation, lock: ScopeLock) -> Self {
        let precondition = Precondition::capture(&op);
        OperationGuard {
            _lock: lock,
            precondition,
            op,
        }
    }

    pub fn check_and_execute(&self) -> ExecutionResult {
        if !self.precondition.check_unchanged(&self.op) {
            let msg = "Precondition violated: filesystem state changed between inspection and execution (TOCTOU)".to_string();
            return ExecutionResult::Conflict(msg);
        }

        self.execute()
    }

    fn execute(&self) -> ExecutionResult {
        match &self.op {
            FileSystemOperation::Move { source, dest } => {
                if !source.exists() {
                    return ExecutionResult::Failed(format!("Source not found: {}", source.display()));
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

    #[allow(dead_code)]
    pub fn precondition(&self) -> &Precondition {
        &self.precondition
    }

    #[allow(dead_code)]
    pub fn operation(&self) -> &FileSystemOperation {
        &self.op
    }
}

#[cfg(test)]
#[path = "operation_guard_tests.rs"]
mod tests;