use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const SCHEMA_VERSION: &str = "2.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DirectoryEvidence {
    pub path: PathBuf,
    pub name: String,
    pub parent_path: Option<PathBuf>,
    pub depth: usize,
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
    pub extension_histogram: HashMap<String, u64>,
    pub dominant_extensions: Vec<DominantExtension>,
    pub identifier_summary: IdentifierSummary,
    pub child_directory_names: Vec<String>,
    pub filename_sample: Vec<String>,
    pub notable_filenames: Vec<String>,
    pub syntactic_identifiers: Vec<SyntacticIdentifier>,
    pub text_file_presence: TextFilePresence,
    pub is_empty: bool,
    pub partial_scan: bool,
    pub scanned_at: u64,
    pub scan_duration_ms: u64,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub struct DominantExtension {
    pub extension: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct IdentifierSummary {
    pub total: usize,
    pub by_type: HashMap<IdentifierType, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct SyntacticIdentifier {
    pub value: String,
    pub source_filename: String,
    pub identifier_type: IdentifierType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierType {
    Isbn,
    Doi,
    Uuid,
    Semver,
    Hash,
    Date,
    AlphanumericCode,
    Email,
    Url,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ScanMetadata {
    #[serde(rename = "_type")]
    pub _type: String,
    pub schema_version: String,
    pub scan_batch_id: String,
    pub scan_started_at: u64,
    pub root_path: PathBuf,
    pub limits: ScanLimits,
    pub stats: ScanStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct TextFilePresence {
    pub has_readme: bool,
    pub has_nfo: bool,
    pub has_txt: bool,
    pub has_md: bool,
    pub has_license: bool,
    pub has_changelog: bool,
    pub text_files_found: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanLimits {
    pub max_depth: usize,
    pub max_files_per_dir: usize,
    pub max_total_files: usize,
    pub max_total_dirs: usize,
    pub max_representative_files: usize,
    pub max_child_dirs: usize,
    pub timeout_seconds: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_depth: 50,
            max_files_per_dir: 10_000,
            max_total_files: 1_000_000,
            max_total_dirs: 100_000,
            max_representative_files: 20,
            max_child_dirs: 500,
            timeout_seconds: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub metadata: ScanMetadata,
    pub evidence: Vec<DirectoryEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanStats {
    pub directories_scanned: u64,
    pub files_encountered: u64,
    pub bytes_scanned: u64,
    pub dirs_skipped: u64,
    pub files_skipped: u64,
    pub errors: Vec<ScanError>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
    pub category: ErrorCategory,
}

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
