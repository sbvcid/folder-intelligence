use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Represents evidence collected from a single directory.
/// This is a factual observation of the filesystem state, without interpretation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryEvidence {
    /// Absolute path of the directory
    pub path: PathBuf,
    /// Name of the directory (last component of path)
    pub name: String,
    /// Parent directory path (None for root)
    pub parent_path: Option<PathBuf>,
    /// Number of files directly in this directory
    pub file_count: u64,
    /// Number of subdirectories directly in this directory
    pub directory_count: u64,
    /// Total size of direct files in this directory in bytes (not recursive)
    pub total_size: u64,
    /// Histogram of file extensions (lowercase, without dot)
    pub extension_histogram: HashMap<String, u64>,
    /// Names of immediate child directories
    pub child_directory_names: Vec<String>,
    /// Representative filenames (sample of files in this directory)
    pub representative_filenames: Vec<String>,
    /// Common/special filenames observed (README, LICENSE, etc.)
    pub notable_filenames: Vec<String>,
    /// Potential identifiers extracted from filenames (no semantic meaning assigned)
    pub potential_identifiers: Vec<PotentialIdentifier>,
    /// Presence of common text files
    pub text_file_presence: TextFilePresence,
    /// Scan timestamp (Unix epoch seconds)
    pub scanned_at: u64,
    /// Duration of scan for this directory in milliseconds
    pub scan_duration_ms: u64,
}

/// A potential identifier found in a filename.
/// No semantic meaning is assigned - these are purely observational.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct PotentialIdentifier {
    /// The identifier string found
    pub value: String,
    /// Source filename it was extracted from
    pub source_filename: String,
    /// Type of identifier pattern matched
    pub identifier_type: IdentifierType,
}

/// Types of identifier patterns that can be detected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierType {
    /// ISBN-10 or ISBN-13
    Isbn,
    /// DOI (Digital Object Identifier)
    Doi,
    /// UUID
    Uuid,
    /// Semantic version (e.g., v1.2.3, 2.0.0-beta)
    Semver,
    /// Hash (MD5, SHA1, SHA256, etc.)
    Hash,
    /// Date pattern (YYYY-MM-DD, YYYYMMDD, etc.)
    Date,
    /// Alphanumeric code (e.g., ABC123, SKU-001)
    AlphanumericCode,
    /// Email address
    Email,
    /// URL
    Url,
    /// Custom pattern (extensible for future use)
    Custom(String),
}

/// Presence information for common text files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct TextFilePresence {
    /// Whether a README file exists (any case, any extension)
    pub has_readme: bool,
    /// Whether an NFO file exists
    pub has_nfo: bool,
    /// Whether any .txt files exist
    pub has_txt: bool,
    /// Whether any .md files exist
    pub has_md: bool,
    /// Whether a LICENSE file exists
    pub has_license: bool,
    /// Whether a CHANGELOG file exists
    pub has_changelog: bool,
    /// List of text file names found
    pub text_files_found: Vec<String>,
}

/// Configuration limits for scanning to prevent runaway resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanLimits {
    /// Maximum directory depth to traverse (0 = unlimited)
    pub max_depth: usize,
    /// Maximum number of files to process per directory
    pub max_files_per_dir: usize,
    /// Maximum total files to process across entire scan
    pub max_total_files: usize,
    /// Maximum total directories to process
    pub max_total_dirs: usize,
    /// Maximum size of a single file to read metadata from (bytes)
    pub max_file_size: u64,
    /// Maximum representative filenames to collect per directory
    pub max_representative_files: usize,
    /// Maximum child directory names to collect
    pub max_child_dirs: usize,
    /// Timeout for entire scan in seconds (0 = no timeout)
    pub timeout_seconds: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_depth: 50,
            max_files_per_dir: 10_000,
            max_total_files: 1_000_000,
            max_total_dirs: 100_000,
            max_file_size: 10 * 1024 * 1024 * 1024, // 10 GB
            max_representative_files: 20,
            max_child_dirs: 500,
            timeout_seconds: 3600, // 1 hour
        }
    }
}

/// Result of a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// All directory evidence collected
    pub evidence: Vec<DirectoryEvidence>,
    /// Statistics about the scan
    pub stats: ScanStats,
}

/// Statistics about a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanStats {
    /// Total directories scanned
    pub directories_scanned: u64,
    /// Total files encountered
    pub files_encountered: u64,
    /// Total bytes scanned
    pub bytes_scanned: u64,
    /// Directories skipped due to limits
    pub dirs_skipped: u64,
    /// Files skipped due to limits
    pub files_skipped: u64,
    /// Errors encountered (non-fatal)
    pub errors: Vec<ScanError>,
    /// Total scan duration in milliseconds
    pub duration_ms: u64,
}

/// Non-fatal error during scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanError {
    /// Path that caused the error
    pub path: PathBuf,
    /// Error message
    pub message: String,
    /// Error category
    pub category: ErrorCategory,
}

/// Category of scan error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    PermissionDenied,
    IoError,
    PathTooLong,
    InvalidUtf8,
    SymlinkLoop,
    LimitExceeded,
    Timeout,
    Other(String),
}