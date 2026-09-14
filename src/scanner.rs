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

/// Internal scan state passed through the traversal.
struct ScanState {
    limits: ScanLimits,
    stats: Arc<ScanStatsInner>,
    start_time: Instant,
    root_path: PathBuf,
}

impl Scanner {
    /// Create a new scanner with default limits.
    #[allow(dead_code)]
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
        if !self.root_path.exists() {
            return Err(anyhow::anyhow!("Root path does not exist: {}", self.root_path.display()));
        }

        let state = ScanState {
            limits: self.limits.clone(),
            stats: self.stats.clone(),
            start_time: self.start_time,
            root_path: self.root_path.clone(),
        };

        let mut all_evidence = Vec::new();
        // Stack of (directory_path, depth)
        let mut dirs_to_scan: Vec<(PathBuf, usize)> = vec![(state.root_path.clone(), 0)];

        while let Some((dir, depth)) = dirs_to_scan.pop() {
            // Check timeout
            if state.limits.timeout_seconds > 0
                && state.start_time.elapsed() > Duration::from_secs(state.limits.timeout_seconds)
            {
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Scan timeout exceeded".to_string(),
                    category: ErrorCategory::Timeout,
                });
                // Record partial evidence with limit reached flag
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            // Check max_total_dirs
            let dirs_scanned = state.stats.directories_scanned.load(Ordering::Relaxed);
            if dirs_scanned >= state.limits.max_total_dirs as u64 {
                state.stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Max total directories limit reached".to_string(),
                    category: ErrorCategory::LimitExceeded,
                });
                // Record partial evidence for this directory
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            // Check max_depth
            if state.limits.max_depth > 0 && depth >= state.limits.max_depth {
                state.stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            // Check max_total_files
            let files_encountered = state.stats.files_encountered.load(Ordering::Relaxed);
            if files_encountered >= state.limits.max_total_files as u64 {
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Max total files limit reached".to_string(),
                    category: ErrorCategory::LimitExceeded,
                });
                // Record partial evidence for this directory
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            // Scan this directory
            match scan_single_directory(&dir, &state.root_path, &state.limits, &state.stats, depth) {
                Ok((evidence, subdirs)) => {
                    state.stats.directories_scanned.fetch_add(1, Ordering::Relaxed);
                    all_evidence.push(evidence);
                    // Add subdirs to queue with depth (reverse for DFS-like order)
                    for subdir in subdirs.into_iter().rev() {
                        dirs_to_scan.push((subdir, depth + 1));
                    }
                }
                Err(e) => {
                    state.stats.errors.lock().unwrap().push(ScanError {
                        path: dir,
                        message: e.to_string(),
                        category: ErrorCategory::IoError,
                    });
                }
            }
        }

        let errors = state.stats.errors.lock().unwrap().clone();
        let duration_ms = state.start_time.elapsed().as_millis() as u64;

        Ok(ScanResult {
            evidence: all_evidence,
            stats: ScanStats {
                directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
                files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
                bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
                dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
                files_skipped: state.stats.files_skipped.load(Ordering::Relaxed),
                errors,
                duration_ms,
            },
        })
    }

    /// Scan only a single directory without recursing into subdirectories.
    /// Used by the `inspect` command for efficient single-directory evidence.
    pub fn inspect_single(self) -> Result<ScanResult> {
        let state = ScanState {
            limits: self.limits.clone(),
            stats: self.stats.clone(),
            start_time: self.start_time,
            root_path: self.root_path.clone(),
        };

        let mut all_evidence = Vec::new();
        let root = &state.root_path;

        // Check timeout
        if state.limits.timeout_seconds > 0
            && state.start_time.elapsed() > Duration::from_secs(state.limits.timeout_seconds)
        {
            state.stats.errors.lock().unwrap().push(ScanError {
                path: root.clone(),
                message: "Scan timeout exceeded".to_string(),
                category: ErrorCategory::Timeout,
            });
            return Ok(ScanResult {
                evidence: all_evidence,
                stats: ScanStats {
                    directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
                    files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
                    bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
                    dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
                    files_skipped: state.stats.files_skipped.load(Ordering::Relaxed),
                    errors: state.stats.errors.lock().unwrap().clone(),
                    duration_ms: state.start_time.elapsed().as_millis() as u64,
                },
            });
        }

        match scan_single_directory(root, root, &state.limits, &state.stats, 0) {
            Ok((evidence, _subdirs)) => {
                state.stats.directories_scanned.fetch_add(1, Ordering::Relaxed);
                all_evidence.push(evidence);
            }
            Err(e) => {
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: root.clone(),
                    message: e.to_string(),
                    category: ErrorCategory::IoError,
                });
            }
        }

        let errors = state.stats.errors.lock().unwrap().clone();
        let duration_ms = state.start_time.elapsed().as_millis() as u64;

        Ok(ScanResult {
            evidence: all_evidence,
            stats: ScanStats {
                directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
                files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
                bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
                dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
                files_skipped: state.stats.files_skipped.load(Ordering::Relaxed),
                errors,
                duration_ms,
            },
        })
    }
}

/// Scan a single directory and collect evidence.
/// Returns the evidence and a list of subdirectory paths to scan.
fn scan_single_directory(
    dir: &Path,
    root: &Path,
    limits: &ScanLimits,
    stats: &Arc<ScanStatsInner>,
    _depth: usize,
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
        // Check per-directory file limit (files only, not directories)
        if file_count >= limits.max_files_per_dir as u64 {
            stats.files_skipped.fetch_add(1, Ordering::Relaxed);
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                stats.errors.lock().unwrap().push(ScanError {
                    path: dir.to_path_buf(),
                    message: format!("Failed to read entry: {}", e),
                    category: if is_permission_error(&e) {
                        ErrorCategory::PermissionDenied
                    } else {
                        ErrorCategory::IoError
                    },
                });
                continue;
            }
        };

        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = match file_name.to_str() {
            Some(s) => s.to_string(),
            None => {
                // Handle non-UTF8 filenames by using lossy conversion
                let lossy_name = file_name.to_string_lossy().to_string();
                stats.errors.lock().unwrap().push(ScanError {
                    path: path.clone(),
                    message: "Invalid UTF-8 in filename".to_string(),
                    category: ErrorCategory::InvalidUtf8,
                });
                // Continue processing with lossy name
                lossy_name
            }
        };

        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                stats.errors.lock().unwrap().push(ScanError {
                    path: path.clone(),
                    message: format!("Failed to get file type: {}", e),
                    category: if is_permission_error(&e) {
                        ErrorCategory::PermissionDenied
                    } else {
                        ErrorCategory::IoError
                    },
                });
                continue;
            }
        };

        // Skip symlinks and reparse points to avoid loops and infinite traversal
        if ft.is_symlink() {
            continue;
        }

        if ft.is_dir() {
            directory_count += 1;
            // Always record the directory name in child_directory_names
            child_directory_names.push(file_name_str.clone());
            // Always add subdirectory to scan (max_child_dirs only limits output, not traversal)
            subdirs.push(path.clone());
        } else if ft.is_file() {
            // Check max_total_files (across entire scan)
            let files_encountered = stats.files_encountered.load(Ordering::Relaxed);
            if files_encountered >= limits.max_total_files as u64 {
                stats.files_skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            file_count += 1;
            stats.files_encountered.fetch_add(1, Ordering::Relaxed);

            // Get file size
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    stats.errors.lock().unwrap().push(ScanError {
                        path: path.clone(),
                        message: format!("Failed to get metadata: {}", e),
                        category: if is_permission_error(&e) {
                            ErrorCategory::PermissionDenied
                        } else {
                            ErrorCategory::IoError
                        },
                    });
                    // Still count the file but without size
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

    // Limit child_directory_names output to max_child_dirs
    let child_directory_names: Vec<String> = child_directory_names
        .into_iter()
        .take(limits.max_child_dirs)
        .collect();

    // Deduplicate notable filenames
    notable_filenames.sort();
    notable_filenames.dedup();

    // Deduplicate potential identifiers (by value + source filename)
    potential_identifiers.sort_by(|a, b| {
        a.value.cmp(&b.value).then_with(|| a.source_filename.cmp(&b.source_filename))
    });
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

/// Scan a directory partially (for evidence when limits are reached).
/// This produces evidence with what can be collected before the scan stopped.
fn scan_directory_partial(
    dir: &Path,
    state: &ScanState,
    _depth: usize,
) -> Option<DirectoryEvidence> {
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut total_size = 0u64;
    let mut extension_histogram = HashMap::new();
    let mut child_directory_names = Vec::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return None,
    };

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if ft.is_dir() {
            directory_count += 1;
            if let Some(name) = entry.file_name().to_str() {
                child_directory_names.push(name.to_string());
            }
        } else if ft.is_file() {
            file_count += 1;
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    *extension_histogram.entry(ext_lower).or_insert(0) += 1;
                } else {
                    *extension_histogram.entry("(no extension)".to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let parent_path = if dir == state.root_path.as_path() {
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

    Some(DirectoryEvidence {
        path: dir.to_path_buf(),
        name,
        parent_path,
        file_count,
        directory_count,
        total_size,
        extension_histogram,
        child_directory_names: child_directory_names
            .into_iter()
            .take(state.limits.max_child_dirs)
            .collect(),
        representative_filenames: Vec::new(),
        notable_filenames: Vec::new(),
        potential_identifiers: Vec::new(),
        text_file_presence: TextFilePresence::default(),
        scanned_at,
        scan_duration_ms: 0,
    })
}

/// Check if an error is a permission error (cross-platform).
fn is_permission_error(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        Some(13) => true,  // EACCES (Unix)
        Some(5) => true,   // ERROR_ACCESS_DENIED (Windows)
        Some(1) => true,   // EPERM (Unix)
        Some(1314) => true, // ERROR_PRIVILEGE_NOT_HELD (Windows)
        _ => error.to_string().to_lowercase().contains("permission"),
    }
}

fn is_notable_filename(name: &str) -> bool {
    const NOTABLE_PATTERNS: &[&str] = &[
        "readme",
        "license",
        "licence",
        "changelog",
        "changes",
        "history",
        "news",
        "authors",
        "contributors",
        "copying",
        "notice",
        "todo",
        "version",
        "release",
        "install",
        "package",
        "makefile",
        "nfo",
        "info",
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

/// Maximum number of identifiers to extract per file to prevent noise.
const MAX_IDENTIFIERS_PER_FILE: usize = 10;

fn extract_identifiers(filename: &str) -> Vec<PotentialIdentifier> {
    let mut identifiers = Vec::new();

    // ISBN-10 (10 digits, optionally with hyphens)
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(isbn) = extract_isbn10(filename) {
            identifiers.push(PotentialIdentifier {
                value: isbn,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Isbn,
            });
        }
    }

    // ISBN-13 (13 digits, optionally with hyphens)
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(isbn) = extract_isbn13(filename) {
            identifiers.push(PotentialIdentifier {
                value: isbn,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Isbn,
            });
        }
    }

    // DOI
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(doi) = extract_doi(filename) {
            identifiers.push(PotentialIdentifier {
                value: doi,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Doi,
            });
        }
    }

    // UUID
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(uuid) = extract_uuid(filename) {
            identifiers.push(PotentialIdentifier {
                value: uuid,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Uuid,
            });
        }
    }

    // Semantic version
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(ver) = extract_semver(filename) {
            identifiers.push(PotentialIdentifier {
                value: ver,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Semver,
            });
        }
    }

    // Hash (MD5, SHA1, SHA256)
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(hash) = extract_hash(filename) {
            identifiers.push(PotentialIdentifier {
                value: hash,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Hash,
            });
        }
    }

    // Date patterns
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(date) = extract_date(filename) {
            identifiers.push(PotentialIdentifier {
                value: date,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Date,
            });
        }
    }

    // Email
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(email) = extract_email(filename) {
            identifiers.push(PotentialIdentifier {
                value: email,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Email,
            });
        }
    }

    // URL
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(url) = extract_url(filename) {
            identifiers.push(PotentialIdentifier {
                value: url,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Url,
            });
        }
    }

    // Alphanumeric codes (e.g., SKU-001, ABC123, limited to avoid noise)
    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        identifiers.extend(extract_alphanumeric_codes(filename));
    }

    identifiers
}

// Identifier extraction functions
fn extract_isbn10(s: &str) -> Option<String> {
    // ISBN-10: 10 digits (last can be X), optionally with hyphens/space
    let re = regex::Regex::new(r"(?:\d[-\s]?){9}[\dX]").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_isbn13(s: &str) -> Option<String> {
    // ISBN-13: starts with 978 or 979, 13 digits, optionally with hyphens
    let re = regex::Regex::new(r"(?:97[89][-\s]?(?:\d[-\s]?){1,5}\d[-\s]?(?:\d[-\s]?){1,7}\d[-\s]?(?:\d[-\s]?){1,7}\d)").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_doi(s: &str) -> Option<String> {
    // DOI: 10.xxxx/xxxx
    let re = regex::Regex::new(r"10\.\d{4,9}[/_][-_/;:A-Za-z0-9]+").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_uuid(s: &str) -> Option<String> {
    // UUID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    let re = regex::Regex::new(
        r"(?:^|[^0-9a-fA-F])([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})(?:[^0-9a-fA-F]|$)"
    ).ok()?;
    re.captures(s).and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn extract_semver(s: &str) -> Option<String> {
    // SemVer: v?major.minor.patch(-prerelease)?(+build)?
    let re = regex::Regex::new(
        r"v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?)"
    ).ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_hash(s: &str) -> Option<String> {
    // MD5 (32 hex), SHA1 (40 hex), SHA256 (64 hex)
    let re = regex::Regex::new(r"(?:^|[^0-9a-fA-F])([a-fA-F0-9]{32})(?:[^0-9a-fA-F]|$)|(?:^|[^0-9a-fA-F])([a-fA-F0-9]{40})(?:[^0-9a-fA-F]|$)|(?:^|[^0-9a-fA-F])([a-fA-F0-9]{64})(?:[^0-9a-fA-F]|$)").ok()?;
    re.captures(s).and_then(|caps| {
        caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3))
            .map(|m| m.as_str().to_string())
    })
}

fn extract_date(s: &str) -> Option<String> {
    // YYYY-MM-DD, YYYYMMDD, YYYY/MM/DD, DD-MM-YYYY, etc.
    let re = regex::Regex::new(
        r"(?:^|[^0-9A-Za-z.-])(19|20)\d{2}[-/.](0[1-9]|1[0-2])[-/.](0[1-9]|[12]\d|3[01])(?:[^0-9A-Za-z]|$)|(?:^|[^0-9A-Za-z.-])(0[1-9]|[12]\d|3[01])[-/.](0[1-9]|1[0-2])[-/.](19|20)\d{2}(?:[^0-9A-Za-z]|$)|(?:^|[^0-9A-Za-z.-])(19|20)\d{2}(0[1-9]|1[0-2])(0[1-9]|[12]\d|3[01])(?:[^0-9A-Za-z]|$)"
    ).ok()?;
    re.find(s).map(|m| m.as_str().trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_string())
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
    // Require at least 2 uppercase letters followed by digits, or digits after hyphen/underscore
    let re = regex::Regex::new(r"[A-Z]{2,}[-_]?\d{3,}(?:[-_]\d+)*|[A-Z]{2,}[-_]?[A-Z]+\d{2,}").unwrap();
    re.find_iter(s)
        .take(MAX_IDENTIFIERS_PER_FILE)
        .map(|m| PotentialIdentifier {
            value: m.as_str().to_string(),
            source_filename: s.to_string(),
            identifier_type: IdentifierType::AlphanumericCode,
        })
        .collect()
}