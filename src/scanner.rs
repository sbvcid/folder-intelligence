use crate::evidence::{
    DirectoryEvidence, ErrorCategory, IdentifierType, PotentialIdentifier, ScanError, ScanLimits,
    ScanResult, ScanStats, TextFilePresence,
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Scanner for collecting directory evidence.
pub struct Scanner {
    limits: ScanLimits,
    stats: Arc<ScanStatsInner>,
    start_time: Instant,
    root_path: PathBuf,
}

struct ScanStatsInner {
    directories_scanned: AtomicU64,
    files_encountered: AtomicU64,
    bytes_scanned: AtomicU64,
    dirs_skipped: AtomicU64,
    files_skipped: AtomicU64,
    errors: Mutex<Vec<ScanError>>,
}

impl Default for ScanStatsInner {
    fn default() -> Self {
        Self {
            directories_scanned: AtomicU64::new(0),
            files_encountered: AtomicU64::new(0),
            bytes_scanned: AtomicU64::new(0),
            dirs_skipped: AtomicU64::new(0),
            files_skipped: AtomicU64::new(0),
            errors: Mutex::new(Vec::new()),
        }
    }
}

impl Scanner {
    /// Create a new scanner with default limits.
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self::with_limits(root, ScanLimits::default())
    }

    /// Create a new scanner with custom limits.
    pub fn with_limits<P: AsRef<Path>>(root: P, limits: ScanLimits) -> Self {
        Self {
            limits,
            stats: Arc::new(ScanStatsInner::default()),
            start_time: Instant::now(),
            root_path: root.as_ref().to_path_buf(),
        }
    }

    /// Run the scan and return results.
    pub fn scan(self) -> Result<ScanResult> {
        let limits = self.limits.clone();
        let stats = self.stats.clone();
        let start_time = self.start_time;
        let root = self.root_path.clone();

        // Sequential scan with parallel processing per directory
        let mut all_evidence = Vec::new();
        let mut dirs_to_scan = vec![root.clone()];

        while let Some(dir) = dirs_to_scan.pop() {
            // Check timeout
            if limits.timeout_seconds > 0
                && start_time.elapsed() > Duration::from_secs(limits.timeout_seconds)
            {
                stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Scan timeout exceeded".to_string(),
                    category: ErrorCategory::Timeout,
                });
                break;
            }

            if stats.directories_scanned.load(Ordering::Relaxed) >= limits.max_total_dirs as u64 {
                stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
                break;
            }

            // Scan this directory
            match scan_single_directory(&dir, &root, &limits, &stats) {
                Ok((evidence, subdirs)) => {
                    stats.directories_scanned.fetch_add(1, Ordering::Relaxed);
                    all_evidence.push(evidence);
                    // Add subdirs to queue (reverse for DFS-like order)
                    for subdir in subdirs.into_iter().rev() {
                        dirs_to_scan.push(subdir);
                    }
                }
                Err(e) => {
                    stats.errors.lock().unwrap().push(ScanError {
                        path: dir,
                        message: e.to_string(),
                        category: ErrorCategory::IoError,
                    });
                }
            }
        }

        let errors = stats.errors.lock().unwrap().clone();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(ScanResult {
            evidence: all_evidence,
            stats: ScanStats {
                directories_scanned: stats.directories_scanned.load(Ordering::Relaxed),
                files_encountered: stats.files_encountered.load(Ordering::Relaxed),
                bytes_scanned: stats.bytes_scanned.load(Ordering::Relaxed),
                dirs_skipped: stats.dirs_skipped.load(Ordering::Relaxed),
                files_skipped: stats.files_skipped.load(Ordering::Relaxed),
                errors,
                duration_ms,
            },
        })
    }
}

/// Scan a single directory and collect evidence.
fn scan_single_directory(
    dir: &Path,
    root: &Path,
    limits: &ScanLimits,
    stats: &Arc<ScanStatsInner>,
) -> Result<(DirectoryEvidence, Vec<PathBuf>)> {
    let dir_start = Instant::now();
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut total_size = 0u64;
    let mut extension_histogram = HashMap::new();
    let mut child_directory_names = Vec::new();
    let mut representative_filenames = Vec::new();
    let mut notable_filenames = Vec::new();
    let mut potential_identifiers = Vec::new();
    let mut text_file_presence = TextFilePresence::default();
    let mut subdirs = Vec::new();

    let read_dir = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in read_dir {
        // Check file limit per directory
        if file_count + directory_count >= limits.max_files_per_dir as u64 {
            stats.files_skipped.fetch_add(1, Ordering::Relaxed);
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                stats.errors.lock().unwrap().push(ScanError {
                    path: dir.to_path_buf(),
                    message: format!("Failed to read entry: {}", e),
                    category: ErrorCategory::IoError,
                });
                continue;
            }
        };

        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = match file_name.to_str() {
            Some(s) => s.to_string(),
            None => {
                // Handle non-UTF8 filenames
                stats.errors.lock().unwrap().push(ScanError {
                    path: path.clone(),
                    message: "Invalid UTF-8 in filename".to_string(),
                    category: ErrorCategory::InvalidUtf8,
                });
                continue;
            }
        };

        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                stats.errors.lock().unwrap().push(ScanError {
                    path: path.clone(),
                    message: format!("Failed to get file type: {}", e),
                    category: ErrorCategory::IoError,
                });
                continue;
            }
        };

        if ft.is_dir() {
            directory_count += 1;
            child_directory_names.push(file_name_str.clone());
            if child_directory_names.len() < limits.max_child_dirs {
                subdirs.push(path);
            } else {
                stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
            }
        } else if ft.is_file() {
            file_count += 1;
            stats.files_encountered.fetch_add(1, Ordering::Relaxed);

            // Get file size
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    stats.errors.lock().unwrap().push(ScanError {
                        path: path.clone(),
                        message: format!("Failed to get metadata: {}", e),
                        category: ErrorCategory::IoError,
                    });
                    continue;
                }
            };

            let size = metadata.len();
            if size > limits.max_file_size {
                stats.files_skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            total_size += size;
            stats.bytes_scanned.fetch_add(size, Ordering::Relaxed);

            // Extension histogram
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                *extension_histogram.entry(ext_lower).or_insert(0) += 1;
            } else {
                *extension_histogram.entry("(no extension)".to_string()).or_insert(0) += 1;
            }

            // Representative filenames
            if representative_filenames.len() < limits.max_representative_files {
                representative_filenames.push(file_name_str.clone());
            }

            // Notable filenames
            let lower_name = file_name_str.to_lowercase();
            if is_notable_filename(&lower_name) {
                notable_filenames.push(file_name_str.clone());
            }

            // Text file presence
            update_text_file_presence(&lower_name, &file_name_str, &mut text_file_presence);

            // Potential identifiers
            potential_identifiers.extend(extract_identifiers(&file_name_str));
        }
        // Skip symlinks, special files, etc.
    }

    // Deduplicate notable filenames
    notable_filenames.sort();
    notable_filenames.dedup();

    // Deduplicate potential identifiers
    potential_identifiers.sort_by(|a, b| a.value.cmp(&b.value));
    potential_identifiers.dedup_by(|a, b| a.value == b.value && a.source_filename == b.source_filename);

    // Parent path
    let parent_path = if dir == root {
        None
    } else {
        dir.parent().map(|p| p.to_path_buf())
    };

    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let scanned_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let scan_duration_ms = dir_start.elapsed().as_millis() as u64;

    let evidence = DirectoryEvidence {
        path: dir.to_path_buf(),
        name,
        parent_path,
        file_count,
        directory_count,
        total_size,
        extension_histogram,
        child_directory_names,
        representative_filenames,
        notable_filenames,
        potential_identifiers,
        text_file_presence,
        scanned_at,
        scan_duration_ms,
    };

    Ok((evidence, subdirs))
}

fn is_notable_filename(name: &str) -> bool {
    const NOTABLE_PATTERNS: &[&str] = &[
        "readme",
        "license",
        "changelog",
        "changelog.md",
        "changelog.txt",
        "changes",
        "changes.md",
        "changes.txt",
        "history",
        "history.md",
        "history.txt",
        "news",
        "news.md",
        "news.txt",
        "authors",
        "authors.md",
        "authors.txt",
        "contributors",
        "contributors.md",
        "contributors.txt",
        "copying",
        "copying.txt",
        "copying.md",
        "notice",
        "notice.txt",
        "notice.md",
        "todo",
        "todo.md",
        "todo.txt",
        "version",
        "version.txt",
        "version.md",
    ];

    NOTABLE_PATTERNS.iter().any(|p| name.starts_with(p))
}

fn update_text_file_presence(
    lower_name: &str,
    original_name: &str,
    presence: &mut TextFilePresence,
) {
    if lower_name.starts_with("readme") {
        presence.has_readme = true;
        presence.text_files_found.push(original_name.to_string());
    }
    if lower_name.ends_with(".nfo") || lower_name == "nfo" {
        presence.has_nfo = true;
        presence.text_files_found.push(original_name.to_string());
    }
    if lower_name.ends_with(".txt") {
        presence.has_txt = true;
        presence.text_files_found.push(original_name.to_string());
    }
    if lower_name.ends_with(".md") || lower_name.ends_with(".markdown") {
        presence.has_md = true;
        presence.text_files_found.push(original_name.to_string());
    }
    if lower_name.starts_with("license") {
        presence.has_license = true;
        if !presence.text_files_found.contains(&original_name.to_string()) {
            presence.text_files_found.push(original_name.to_string());
        }
    }
    if lower_name.starts_with("changelog") || lower_name.starts_with("changes") {
        presence.has_changelog = true;
        if !presence.text_files_found.contains(&original_name.to_string()) {
            presence.text_files_found.push(original_name.to_string());
        }
    }
}

fn extract_identifiers(filename: &str) -> Vec<PotentialIdentifier> {
    let mut identifiers = Vec::new();

    // ISBN-10 (10 digits, optionally with hyphens)
    if let Some(isbn) = extract_isbn10(filename) {
        identifiers.push(PotentialIdentifier {
            value: isbn,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Isbn,
        });
    }

    // ISBN-13 (13 digits, optionally with hyphens)
    if let Some(isbn) = extract_isbn13(filename) {
        identifiers.push(PotentialIdentifier {
            value: isbn,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Isbn,
        });
    }

    // DOI
    if let Some(doi) = extract_doi(filename) {
        identifiers.push(PotentialIdentifier {
            value: doi,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Doi,
        });
    }

    // UUID
    if let Some(uuid) = extract_uuid(filename) {
        identifiers.push(PotentialIdentifier {
            value: uuid,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Uuid,
        });
    }

    // Semantic version
    if let Some(ver) = extract_semver(filename) {
        identifiers.push(PotentialIdentifier {
            value: ver,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Semver,
        });
    }

    // Hash (MD5, SHA1, SHA256)
    if let Some(hash) = extract_hash(filename) {
        identifiers.push(PotentialIdentifier {
            value: hash,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Hash,
        });
    }

    // Date patterns
    if let Some(date) = extract_date(filename) {
        identifiers.push(PotentialIdentifier {
            value: date,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Date,
        });
    }

    // Email
    if let Some(email) = extract_email(filename) {
        identifiers.push(PotentialIdentifier {
            value: email,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Email,
        });
    }

    // URL
    if let Some(url) = extract_url(filename) {
        identifiers.push(PotentialIdentifier {
            value: url,
            source_filename: filename.to_string(),
            identifier_type: IdentifierType::Url,
        });
    }

    // Alphanumeric codes (e.g., SKU-001, ABC123)
    identifiers.extend(extract_alphanumeric_codes(filename));

    identifiers
}

// Identifier extraction functions
fn extract_isbn10(s: &str) -> Option<String> {
    // ISBN-10: 9 digits + check digit (0-9 or X), optionally with hyphens
    let re = regex::Regex::new(r"\b\d{1,5}[-\s]?\d{1,7}[-\s]?\d{1,7}[-\s]?[\dX]\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_isbn13(s: &str) -> Option<String> {
    // ISBN-13: 13 digits, optionally with hyphens
    let re = regex::Regex::new(r"\b97[89][-\s]?\d{1,5}[-\s]?\d{1,7}[-\s]?\d{1,7}[-\s]?\d\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_doi(s: &str) -> Option<String> {
    // DOI: 10.xxxx/xxxx
    let re = regex::Regex::new(r"\b10\.\d{4,9}/[-._;()/:A-Z0-9]+\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_uuid(s: &str) -> Option<String> {
    // UUID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    let re = regex::Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_semver(s: &str) -> Option<String> {
    // SemVer: v?major.minor.patch(-prerelease)?(+build)?
    let re = regex::Regex::new(r"\bv?\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_hash(s: &str) -> Option<String> {
    // MD5 (32 hex), SHA1 (40 hex), SHA256 (64 hex)
    let re = regex::Regex::new(r"\b[a-fA-F0-9]{32}\b|\b[a-fA-F0-9]{40}\b|\b[a-fA-F0-9]{64}\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_date(s: &str) -> Option<String> {
    // YYYY-MM-DD, YYYYMMDD, YYYY/MM/DD, DD-MM-YYYY, etc.
    let re = regex::Regex::new(r"\b(?:19|20)\d{2}[-/.](?:0[1-9]|1[0-2])[-/.](?:0[1-9]|[12]\d|3[01])\b|\b(?:0[1-9]|[12]\d|3[01])[-/.](?:0[1-9]|1[0-2])[-/.](?:19|20)\d{2}\b|\b(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_email(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_url(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"\bhttps?://[^\s/$.?#].[^\s]*\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_alphanumeric_codes(s: &str) -> Vec<PotentialIdentifier> {
    // Patterns like SKU-001, ABC123, PROD-2024-001, etc.
    let re = regex::Regex::new(r"\b[A-Z]{2,}[-_]?\d{3,}(?:[-_]\d+)*\b|\b[A-Z]{2,}\d{3,}\b").unwrap();
    re.find_iter(s)
        .map(|m| PotentialIdentifier {
            value: m.as_str().to_string(),
            source_filename: s.to_string(),
            identifier_type: IdentifierType::AlphanumericCode,
        })
        .collect()
}