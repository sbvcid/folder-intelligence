use crate::evidence::{
    DirectoryEvidence, DominantExtension, ErrorCategory, IdentifierSummary, IdentifierType,
    ScanError, ScanLimits, ScanMetadata, ScanResult, ScanStats, SyntacticIdentifier,
    TextFilePresence, SCHEMA_VERSION,
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
struct FileCandidate {
    name: String,
    extension: String,
    has_identifier: bool,
    is_notable: bool,
    position: usize,
}

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

struct ScanState {
    limits: ScanLimits,
    stats: Arc<ScanStatsInner>,
    start_time: Instant,
    root_path: PathBuf,
}

impl Scanner {
    #[allow(dead_code)]
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self::with_limits(root, ScanLimits::default())
    }

    pub fn with_limits<P: AsRef<Path>>(root: P, limits: ScanLimits) -> Self {
        Self {
            limits,
            stats: Arc::new(ScanStatsInner::default()),
            start_time: Instant::now(),
            root_path: root.as_ref().to_path_buf(),
        }
    }

    pub fn scan(self) -> Result<ScanResult> {
        if !self.root_path.exists() {
            return Err(anyhow::anyhow!(
                "Root path does not exist: {}",
                self.root_path.display()
            ));
        }
        if !self.root_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Root path is not a directory: {}",
                self.root_path.display()
            ));
        }

        let state = ScanState {
            limits: self.limits.clone(),
            stats: self.stats.clone(),
            start_time: self.start_time,
            root_path: self.root_path.clone(),
        };

        let scan_batch_id = generate_batch_id();
        let scan_started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut all_evidence = Vec::new();
        let mut dirs_to_scan: Vec<(PathBuf, usize)> = vec![(state.root_path.clone(), 0)];

        while let Some((dir, depth)) = dirs_to_scan.pop() {
            if state.limits.timeout_seconds > 0
                && state.start_time.elapsed() > Duration::from_secs(state.limits.timeout_seconds)
            {
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Scan timeout exceeded".to_string(),
                    category: ErrorCategory::Timeout,
                });
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            let dirs_scanned = state.stats.directories_scanned.load(Ordering::Relaxed);
            if dirs_scanned >= state.limits.max_total_dirs as u64 {
                state.stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Max total directories limit reached".to_string(),
                    category: ErrorCategory::LimitExceeded,
                });
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            if state.limits.max_depth > 0 && depth >= state.limits.max_depth {
                state.stats.dirs_skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            let files_encountered = state.stats.files_encountered.load(Ordering::Relaxed);
            if files_encountered >= state.limits.max_total_files as u64 {
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: dir.clone(),
                    message: "Max total files limit reached".to_string(),
                    category: ErrorCategory::LimitExceeded,
                });
                if let Some(ev) = scan_directory_partial(&dir, &state, depth) {
                    all_evidence.push(ev);
                }
                break;
            }

            match scan_single_directory(&dir, &state.root_path, &state, depth) {
                Ok((evidence, subdirs)) => {
                    state
                        .stats
                        .directories_scanned
                        .fetch_add(1, Ordering::Relaxed);
                    all_evidence.push(evidence);
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

        let stats = ScanStats {
            directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
            files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
            bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
            dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
            files_skipped: state.stats.files_skipped.swap(0, Ordering::Relaxed),
            errors,
            duration_ms,
        };

        Ok(ScanResult {
            metadata: ScanMetadata {
                _type: "scan_metadata".to_string(),
                schema_version: SCHEMA_VERSION.to_string(),
                scan_batch_id,
                scan_started_at,
                root_path: state.root_path.clone(),
                limits: state.limits.clone(),
                stats,
            },
            evidence: all_evidence,
        })
    }

    pub fn inspect_single(self) -> Result<ScanResult> {
        if !self.root_path.exists() {
            return Err(anyhow::anyhow!(
                "Root path does not exist: {}",
                self.root_path.display()
            ));
        }
        if !self.root_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Root path is not a directory: {}",
                self.root_path.display()
            ));
        }

        let state = ScanState {
            limits: self.limits.clone(),
            stats: self.stats.clone(),
            start_time: self.start_time,
            root_path: self.root_path.clone(),
        };

        let scan_batch_id = generate_batch_id();
        let scan_started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut all_evidence = Vec::new();
        let root = &state.root_path;

        if state.limits.timeout_seconds > 0
            && state.start_time.elapsed() > Duration::from_secs(state.limits.timeout_seconds)
        {
            state.stats.errors.lock().unwrap().push(ScanError {
                path: root.clone(),
                message: "Scan timeout exceeded".to_string(),
                category: ErrorCategory::Timeout,
            });
            let errors = state.stats.errors.lock().unwrap().clone();
            let duration_ms = state.start_time.elapsed().as_millis() as u64;
            let stats = ScanStats {
                directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
                files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
                bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
                dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
                files_skipped: state.stats.files_skipped.swap(0, Ordering::Relaxed),
                errors,
                duration_ms,
            };
            return Ok(ScanResult {
                metadata: ScanMetadata {
                    _type: "scan_metadata".to_string(),
                    schema_version: SCHEMA_VERSION.to_string(),
                    scan_batch_id,
                    scan_started_at,
                    root_path: state.root_path.clone(),
                    limits: state.limits.clone(),
                    stats,
                },
                evidence: all_evidence,
            });
        }

        match scan_single_directory(root, root, &state, 0) {
            Ok((evidence, _subdirs)) => {
                state
                    .stats
                    .directories_scanned
                    .fetch_add(1, Ordering::Relaxed);
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

        let stats = ScanStats {
            directories_scanned: state.stats.directories_scanned.load(Ordering::Relaxed),
            files_encountered: state.stats.files_encountered.load(Ordering::Relaxed),
            bytes_scanned: state.stats.bytes_scanned.load(Ordering::Relaxed),
            dirs_skipped: state.stats.dirs_skipped.load(Ordering::Relaxed),
            files_skipped: state.stats.files_skipped.swap(0, Ordering::Relaxed),
            errors,
            duration_ms,
        };

        Ok(ScanResult {
            metadata: ScanMetadata {
                _type: "scan_metadata".to_string(),
                schema_version: SCHEMA_VERSION.to_string(),
                scan_batch_id,
                scan_started_at,
                root_path: state.root_path.clone(),
                limits: state.limits.clone(),
                stats,
            },
            evidence: all_evidence,
        })
    }
}

fn generate_batch_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}", nanos)
}

fn compute_dominant_extensions(
    histogram: &HashMap<String, u64>,
    file_count: u64,
    max_entries: usize,
) -> Vec<DominantExtension> {
    let mut entries: Vec<(String, u64)> = histogram.iter().map(|(k, &v)| (k.clone(), v)).collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries.truncate(max_entries);

    entries
        .into_iter()
        .map(|(ext, count)| {
            let percentage = if file_count > 0 {
                (count as f64 / file_count as f64) * 100.0
            } else {
                0.0
            };
            DominantExtension {
                extension: ext,
                count,
                percentage,
            }
        })
        .collect()
}

fn compute_identifier_summary(identifiers: &[SyntacticIdentifier]) -> IdentifierSummary {
    let total = identifiers.len();
    let mut by_type: HashMap<IdentifierType, usize> = HashMap::new();
    for id in identifiers {
        *by_type.entry(id.identifier_type.clone()).or_insert(0) += 1;
    }
    IdentifierSummary { total, by_type }
}

fn scan_single_directory(
    dir: &Path,
    root: &Path,
    state: &ScanState,
    depth: usize,
) -> Result<(DirectoryEvidence, Vec<PathBuf>)> {
    let dir_start = Instant::now();
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut total_size = 0u64;
    let mut extension_histogram = HashMap::new();
    let mut child_directory_names = Vec::new();
    let mut notable_filenames = Vec::new();
    let mut syntactic_identifiers = Vec::new();
    let mut text_file_presence = TextFilePresence::default();
    let mut subdirs = Vec::new();
    let mut partial = false;

    let mut file_candidates: Vec<FileCandidate> = Vec::new();

    let read_dir = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in read_dir {
        if state.limits.timeout_seconds > 0
            && state.start_time.elapsed() > Duration::from_secs(state.limits.timeout_seconds)
        {
            state.stats.errors.lock().unwrap().push(ScanError {
                path: dir.to_path_buf(),
                message: "Scan timeout exceeded during directory iteration".to_string(),
                category: ErrorCategory::Timeout,
            });
            partial = true;
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                state.stats.errors.lock().unwrap().push(ScanError {
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
                let lossy_name = file_name.to_string_lossy().to_string();
                state.stats.errors.lock().unwrap().push(ScanError {
                    path: path.clone(),
                    message: "Invalid UTF-8 in filename".to_string(),
                    category: ErrorCategory::InvalidUtf8,
                });
                lossy_name
            }
        };

        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                state.stats.errors.lock().unwrap().push(ScanError {
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

        if ft.is_symlink() {
            continue;
        }

        if ft.is_dir() {
            directory_count += 1;
            child_directory_names.push(file_name_str.clone());
            subdirs.push(path.clone());
        } else if ft.is_file() {
            if file_count >= state.limits.max_files_per_dir as u64 {
                state.stats.files_skipped.fetch_add(1, Ordering::Relaxed);
                partial = true;
                continue;
            }

            let files_encountered = state.stats.files_encountered.load(Ordering::Relaxed);
            if files_encountered >= state.limits.max_total_files as u64 {
                state.stats.files_skipped.fetch_add(1, Ordering::Relaxed);
                partial = true;
                continue;
            }
            file_count += 1;
            state
                .stats
                .files_encountered
                .fetch_add(1, Ordering::Relaxed);

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    state.stats.errors.lock().unwrap().push(ScanError {
                        path: path.clone(),
                        message: format!("Failed to get metadata: {}", e),
                        category: if is_permission_error(&e) {
                            ErrorCategory::PermissionDenied
                        } else {
                            ErrorCategory::IoError
                        },
                    });
                    continue;
                }
            };

            let size = metadata.len();
            total_size += size;
            state.stats.bytes_scanned.fetch_add(size, Ordering::Relaxed);

            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_else(|| "(no extension)".to_string());
            *extension_histogram.entry(extension.clone()).or_insert(0) += 1;

            let lower_name = file_name_str.to_lowercase();
            let is_notable = is_notable_filename(&lower_name);
            if is_notable {
                notable_filenames.push(file_name_str.clone());
            }

            let idents = extract_identifiers(&file_name_str);
            let has_identifier = !idents.is_empty();
            syntactic_identifiers.extend(idents);

            update_text_file_presence(&lower_name, &file_name_str, &mut text_file_presence);

            file_candidates.push(FileCandidate {
                name: file_name_str.clone(),
                extension,
                has_identifier,
                is_notable,
                position: file_candidates.len(),
            });
        }
    }

    let filename_sample = select_representative_filenames(
        &file_candidates,
        &extension_histogram,
        state.limits.max_representative_files,
    );

    let child_directory_names: Vec<String> = child_directory_names
        .into_iter()
        .take(state.limits.max_child_dirs)
        .collect();

    notable_filenames.sort();
    notable_filenames.dedup();

    syntactic_identifiers.sort_by(|a, b| {
        a.value
            .cmp(&b.value)
            .then_with(|| a.source_filename.cmp(&b.source_filename))
    });
    syntactic_identifiers
        .dedup_by(|a, b| a.value == b.value && a.source_filename == b.source_filename);

    let dominant_extensions = compute_dominant_extensions(&extension_histogram, file_count, 10);
    let identifier_summary = compute_identifier_summary(&syntactic_identifiers);
    let is_empty = file_count == 0 && directory_count == 0;

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
        depth,
        file_count,
        directory_count,
        total_size,
        extension_histogram,
        dominant_extensions,
        identifier_summary,
        child_directory_names,
        filename_sample,
        notable_filenames,
        syntactic_identifiers,
        text_file_presence,
        is_empty,
        partial_scan: partial,
        scanned_at,
        scan_duration_ms,
        schema_version: SCHEMA_VERSION.to_string(),
    };

    Ok((evidence, subdirs))
}

fn scan_directory_partial(
    dir: &Path,
    state: &ScanState,
    depth: usize,
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

        if ft.is_symlink() {
            continue;
        }

        if ft.is_dir() {
            directory_count += 1;
            if let Some(name) = entry.file_name().to_str() {
                child_directory_names.push(name.to_string());
            }
        } else if ft.is_file() {
            if file_count >= state.limits.max_files_per_dir as u64 {
                break;
            }
            file_count += 1;
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    *extension_histogram.entry(ext_lower).or_insert(0) += 1;
                } else {
                    *extension_histogram
                        .entry("(no extension)".to_string())
                        .or_insert(0) += 1;
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

    let dominant_extensions = compute_dominant_extensions(&extension_histogram, file_count, 10);
    let identifier_summary = IdentifierSummary::default();
    let is_empty = file_count == 0 && directory_count == 0;

    Some(DirectoryEvidence {
        path: dir.to_path_buf(),
        name,
        parent_path,
        depth,
        file_count,
        directory_count,
        total_size,
        extension_histogram,
        dominant_extensions,
        identifier_summary,
        child_directory_names: child_directory_names
            .into_iter()
            .take(state.limits.max_child_dirs)
            .collect(),
        filename_sample: Vec::new(),
        notable_filenames: Vec::new(),
        syntactic_identifiers: Vec::new(),
        text_file_presence: TextFilePresence::default(),
        is_empty,
        partial_scan: true,
        scanned_at,
        scan_duration_ms: 0,
        schema_version: SCHEMA_VERSION.to_string(),
    })
}

fn is_permission_error(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        Some(13) => true,
        Some(5) => true,
        Some(1) => true,
        Some(1314) => true,
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
        if !presence
            .text_files_found
            .contains(&original_name.to_string())
        {
            presence.text_files_found.push(original_name.to_string());
        }
    }
    if lower_name.starts_with("changelog") || lower_name.starts_with("changes") {
        presence.has_changelog = true;
        if !presence
            .text_files_found
            .contains(&original_name.to_string())
        {
            presence.text_files_found.push(original_name.to_string());
        }
    }
}

const MAX_IDENTIFIERS_PER_FILE: usize = 10;

fn extract_identifiers(filename: &str) -> Vec<SyntacticIdentifier> {
    let mut identifiers = Vec::new();

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(isbn) = extract_isbn10(filename) {
            identifiers.push(SyntacticIdentifier {
                value: isbn,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Isbn,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(isbn) = extract_isbn13(filename) {
            identifiers.push(SyntacticIdentifier {
                value: isbn,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Isbn,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(doi) = extract_doi(filename) {
            identifiers.push(SyntacticIdentifier {
                value: doi,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Doi,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(uuid) = extract_uuid(filename) {
            identifiers.push(SyntacticIdentifier {
                value: uuid,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Uuid,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(ver) = extract_semver(filename) {
            identifiers.push(SyntacticIdentifier {
                value: ver,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Semver,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(hash) = extract_hash(filename) {
            identifiers.push(SyntacticIdentifier {
                value: hash,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Hash,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(date) = extract_date(filename) {
            identifiers.push(SyntacticIdentifier {
                value: date,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Date,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(email) = extract_email(filename) {
            identifiers.push(SyntacticIdentifier {
                value: email,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Email,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        if let Some(url) = extract_url(filename) {
            identifiers.push(SyntacticIdentifier {
                value: url,
                source_filename: filename.to_string(),
                identifier_type: IdentifierType::Url,
            });
        }
    }

    if identifiers.len() < MAX_IDENTIFIERS_PER_FILE {
        identifiers.extend(extract_alphanumeric_codes(filename));
    }

    identifiers
}

fn extract_isbn10(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?:\d[-\s]?){9}[\dX]").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_isbn13(s: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"(?:97[89][-\s]?(?:\d[-\s]?){1,5}\d[-\s]?(?:\d[-\s]?){1,7}\d[-\s]?(?:\d[-\s]?){1,7}\d)",
    )
    .ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_doi(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"10\.\d{4,9}[/_][-_/;:A-Za-z0-9]+").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_uuid(s: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"(?:^|[^0-9a-fA-F])([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})(?:[^0-9a-fA-F]|$)"
    ).ok()?;
    re.captures(s)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn extract_semver(s: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?)"
    ).ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_hash(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?:^|[^0-9a-fA-F])([a-fA-F0-9]{32})(?:[^0-9a-fA-F]|$)|(?:^|[^0-9a-fA-F])([a-fA-F0-9]{40})(?:[^0-9a-fA-F]|$)|(?:^|[^0-9a-fA-F])([a-fA-F0-9]{64})(?:[^0-9a-fA-F]|$)").ok()?;
    re.captures(s).and_then(|caps| {
        caps.get(1)
            .or_else(|| caps.get(2))
            .or_else(|| caps.get(3))
            .map(|m| m.as_str().to_string())
    })
}

fn extract_date(s: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"(?:^|[^0-9A-Za-z.-])(19|20)\d{2}[-/.](0[1-9]|1[0-2])[-/.](0[1-9]|[12]\d|3[01])(?:[^0-9A-Za-z]|$)|(?:^|[^0-9A-Za-z.-])(0[1-9]|[12]\d|3[01])[-/.](0[1-9]|1[0-2])[-/.](19|20)\d{2}(?:[^0-9A-Za-z]|$)|(?:^|[^0-9A-Za-z.-])(19|20)\d{2}(0[1-9]|1[0-2])(0[1-9]|[12]\d|3[01])(?:[^0-9A-Za-z]|$)"
    ).ok()?;
    re.find(s).map(|m| {
        m.as_str()
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
            .to_string()
    })
}

fn extract_email(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_url(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"\bhttps?://[^\s/$.?#].[^\s]*\b").ok()?;
    re.find(s).map(|m| m.as_str().to_string())
}

fn extract_alphanumeric_codes(s: &str) -> Vec<SyntacticIdentifier> {
    let re =
        regex::Regex::new(r"[A-Z]{2,}[-_]?\d{3,}(?:[-_]\d+)*|[A-Z]{2,}[-_]?[A-Z]+\d{2,}").unwrap();
    re.find_iter(s)
        .take(MAX_IDENTIFIERS_PER_FILE)
        .map(|m| SyntacticIdentifier {
            value: m.as_str().to_string(),
            source_filename: s.to_string(),
            identifier_type: IdentifierType::AlphanumericCode,
        })
        .collect()
}

fn select_representative_filenames(
    candidates: &[FileCandidate],
    extension_histogram: &HashMap<String, u64>,
    budget: usize,
) -> Vec<String> {
    if candidates.is_empty() || budget == 0 {
        return Vec::new();
    }

    let budget = budget.min(candidates.len());

    let unique_ext_count = extension_histogram.len();
    let max_per_ext = if unique_ext_count <= 2 {
        budget
    } else {
        (budget / 2).max(2)
    };

    let mut ext_count: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<String> = Vec::new();

    let id_budget = if budget <= 4 { 1 } else { (budget / 4).max(1) };
    for c in candidates.iter() {
        if result.len() >= id_budget {
            break;
        }
        if c.has_identifier && try_add(c, &mut result, &mut ext_count, budget, max_per_ext) {}
    }

    for c in candidates.iter() {
        if result.len() >= budget {
            break;
        }
        if c.is_notable && try_add(c, &mut result, &mut ext_count, budget, max_per_ext) {}
    }

    let mut rare_candidates: Vec<&FileCandidate> = candidates
        .iter()
        .filter(|c| {
            let freq = extension_histogram.get(&c.extension).copied().unwrap_or(0);
            freq <= 5 && !result.contains(&c.name)
        })
        .collect();
    rare_candidates.sort_by(|a, b| {
        let fa = extension_histogram.get(&a.extension).copied().unwrap_or(0);
        let fb = extension_histogram.get(&b.extension).copied().unwrap_or(0);
        fa.cmp(&fb).then_with(|| a.name.cmp(&b.name))
    });
    for c in &rare_candidates {
        if result.len() >= budget {
            break;
        }
        try_add(c, &mut result, &mut ext_count, budget, max_per_ext);
    }

    if candidates.len() > budget * 2 {
        let positions = compute_structural_positions(candidates.len(), budget);
        for &pos in &positions {
            if result.len() >= budget {
                break;
            }
            let c = &candidates[pos];
            try_add(c, &mut result, &mut ext_count, budget, max_per_ext);
        }
    }

    let mut remaining: Vec<&FileCandidate> = candidates
        .iter()
        .filter(|c| !result.contains(&c.name))
        .collect();
    remaining.sort_by(|a, b| a.name.cmp(&b.name));
    for c in remaining {
        if result.len() >= budget {
            break;
        }
        try_add(c, &mut result, &mut ext_count, budget, max_per_ext);
    }

    result
}

fn try_add(
    c: &FileCandidate,
    result: &mut Vec<String>,
    ext_count: &mut HashMap<String, usize>,
    budget: usize,
    max_per_ext: usize,
) -> bool {
    if result.len() >= budget {
        return false;
    }
    if result.contains(&c.name) {
        return false;
    }
    let count = ext_count.entry(c.extension.clone()).or_insert(0);
    if *count < max_per_ext {
        result.push(c.name.clone());
        *count += 1;
        true
    } else {
        false
    }
}

fn compute_structural_positions(total: usize, budget: usize) -> Vec<usize> {
    if total == 0 || budget == 0 {
        return Vec::new();
    }

    let mut positions = Vec::with_capacity(budget.min(total));

    if total <= budget {
        return positions;
    }

    let first = 0usize;
    let last = total - 1;

    if budget == 1 {
        positions.push(first);
        return positions;
    }

    if budget == 2 {
        positions.push(first);
        positions.push(last);
        return positions;
    }

    let middle_count = budget - 2;
    positions.push(first);
    positions.push(last);

    let step = if middle_count > 0 {
        (total - 1) as f64 / (middle_count + 1) as f64
    } else {
        1.0
    };
    let mut current = step;
    for _ in 0..middle_count {
        let pos = current.round() as usize;
        if pos < total && pos != first && pos != last && !positions.contains(&pos) {
            positions.push(pos);
        }
        current += step;
    }

    if positions.len() < budget {
        let mut candidate_positions: Vec<usize> = (0..total).collect();
        candidate_positions.retain(|p| !positions.contains(p));
        for p in candidate_positions {
            if positions.len() >= budget {
                break;
            }
            positions.push(p);
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    fn make_candidates_single_ext(names: &[&str], extension: &str) -> Vec<FileCandidate> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| FileCandidate {
                name: name.to_string(),
                extension: extension.to_string(),
                has_identifier: false,
                is_notable: false,
                position: i,
            })
            .collect()
    }

    fn make_candidates_multi(
        names: &[&str],
        extensions: &[&str],
        identifiers: &[&str],
        notable: &[&str],
    ) -> Vec<FileCandidate> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let ext = extensions.get(i).map(|s| s.to_string()).unwrap_or_default();
                let has_id = identifiers.contains(name);
                let is_notable = notable.contains(name);
                FileCandidate {
                    name: name.to_string(),
                    extension: ext,
                    has_identifier: has_id,
                    is_notable: is_notable,
                    position: i,
                }
            })
            .collect()
    }

    fn make_ext_hist_from_candidates(candidates: &[FileCandidate]) -> HashMap<String, u64> {
        let mut hist = HashMap::new();
        for c in candidates {
            *hist.entry(c.extension.clone()).or_insert(0) += 1;
        }
        hist
    }

    #[test]
    fn test_homogeneous_sequential_directory() {
        let names: Vec<String> = (1..=2981).map(|i| format!("{:03}.mp3", i)).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let candidates = make_candidates_single_ext(&name_refs, "mp3");
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        for budget in [1, 2, 3, 5, 10, 20] {
            let result = select_representative_filenames(&candidates, &ext_hist, budget);
            assert_eq!(result.len(), budget.min(candidates.len()));

            if budget >= 2 {
                let first_in_result = result.iter().any(|n| n.starts_with("001.mp3"));
                let last_in_result = result.iter().any(|n| n.starts_with("2981.mp3"));
                assert!(
                    first_in_result,
                    "Budget {} should include first file",
                    budget
                );
                assert!(last_in_result, "Budget {} should include last file", budget);
            }

            if budget >= 5 {
                let has_mid = result.iter().any(|n| {
                    let num: u32 = n.trim_end_matches(".mp3").parse().unwrap();
                    num >= 900 && num <= 2100
                });
                assert!(
                    has_mid,
                    "Budget {} should include a file from the middle region",
                    budget
                );
            }
        }
    }

    #[test]
    fn test_mixed_extensions_rare_preserved() {
        let names = vec![
            "001.mp3",
            "002.mp3",
            "003.mp3",
            "004.mp3",
            "005.mp3",
            "006.mp3",
            "007.mp3",
            "008.mp3",
            "009.mp3",
            "010.mp3",
            "cover.jpg",
            "README.txt",
            "config.unknown",
        ];
        let extensions = vec![
            "mp3", "mp3", "mp3", "mp3", "mp3", "mp3", "mp3", "mp3", "mp3", "mp3", "jpg", "txt",
            "unknown",
        ];
        let candidates = make_candidates_multi(&names, &extensions, &[], &[]);
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        let result = select_representative_filenames(&candidates, &ext_hist, 10);

        assert!(
            result.contains(&"cover.jpg".to_string()),
            "Should include rare .jpg"
        );
        assert!(
            result.contains(&"README.txt".to_string()),
            "Should include rare .txt"
        );
        assert!(
            result.contains(&"config.unknown".to_string()),
            "Should include rare .unknown"
        );
        assert!(
            result.contains(&"001.mp3".to_string()),
            "Should include first .mp3"
        );
    }

    #[test]
    fn test_notable_files_priority() {
        let mut names: Vec<String> = (1..=50).map(|i| format!("{:03}.mp3", i)).collect();
        names.extend_from_slice(&[
            "README.md".to_string(),
            "LICENSE".to_string(),
            "cover.jpg".to_string(),
        ]);
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

        let mut extensions: Vec<String> = (0..50).map(|_| "mp3".to_string()).collect();
        extensions.extend_from_slice(&[
            "md".to_string(),
            "(no extension)".to_string(),
            "jpg".to_string(),
        ]);

        let ext_vec: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();

        let candidates =
            make_candidates_multi(&name_refs, &ext_vec, &[], &["README.md", "LICENSE"]);
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        let result = select_representative_filenames(&candidates, &ext_hist, 10);

        assert!(
            result.contains(&"README.md".to_string()),
            "Should include README.md"
        );
        assert!(
            result.contains(&"LICENSE".to_string()),
            "Should include LICENSE"
        );
        assert!(
            result.contains(&"cover.jpg".to_string()),
            "Should include cover.jpg"
        );
    }

    #[test]
    fn test_identifier_bearing_priority() {
        let mut names: Vec<String> = (1..=20).map(|i| format!("file{:03}.txt", i)).collect();
        names.push("song_550e8400-e29b-41d4-a716-446655440000.mp3".to_string());
        names.push("doc_10.1038_nature12373_2024-01-15.pdf".to_string());
        names.push("data_978-0-306-40615-7_v1.2.3.bin".to_string());

        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

        let mut extensions: Vec<&str> = vec!["txt"; 20];
        extensions.extend_from_slice(&["mp3", "pdf", "bin"]);

        let idents = vec![
            "song_550e8400-e29b-41d4-a716-446655440000.mp3",
            "doc_10.1038_nature12373_2024-01-15.pdf",
            "data_978-0-306-40615-7_v1.2.3.bin",
        ];

        let candidates = make_candidates_multi(&name_refs, &extensions, &idents, &[]);
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        let result = select_representative_filenames(&candidates, &ext_hist, 10);

        assert!(result.contains(&"song_550e8400-e29b-41d4-a716-446655440000.mp3".to_string()));
        assert!(result.contains(&"doc_10.1038_nature12373_2024-01-15.pdf".to_string()));
        assert!(result.contains(&"data_978-0-306-40615-7_v1.2.3.bin".to_string()));
        assert!(
            result.iter().any(|n| n.starts_with("file0")),
            "Should include at least one regular file"
        );
    }

    #[test]
    fn test_small_budget_deterministic() {
        let names: Vec<String> = (1..=100).map(|i| format!("{:03}.mp3", i)).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let candidates = make_candidates_single_ext(&name_refs, "mp3");
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        for budget in [1, 2, 3, 5, 10] {
            let result1 = select_representative_filenames(&candidates, &ext_hist, budget);
            let result2 = select_representative_filenames(&candidates, &ext_hist, budget);
            assert_eq!(
                result1, result2,
                "Budget {} should be deterministic",
                budget
            );
            assert_eq!(
                result1.len(),
                budget,
                "Budget {} should return {} items",
                budget,
                budget
            );

            if budget == 1 {
                assert_eq!(result1, vec!["001.mp3"]);
            }

            if budget == 2 {
                assert!(result1.contains(&"001.mp3".to_string()));
                assert!(result1.contains(&"100.mp3".to_string()));
            }
        }
    }

    #[test]
    fn test_determinism_across_scans() {
        let dir = tempdir().unwrap();
        for i in 0..50 {
            fs::write(dir.path().join(format!("file_{:03}.txt", i)), "x").unwrap();
        }
        fs::write(dir.path().join("README.md"), "readme").unwrap();
        fs::write(dir.path().join("song_550e8400.mp3"), "song").unwrap();

        let limits = ScanLimits {
            max_representative_files: 10,
            ..Default::default()
        };

        let scanner1 = Scanner::with_limits(dir.path(), limits.clone());
        let result1 = scanner1.scan().unwrap();

        let scanner2 = Scanner::with_limits(dir.path(), limits.clone());
        let result2 = scanner2.scan().unwrap();

        assert_eq!(
            result1.evidence[0].filename_sample, result2.evidence[0].filename_sample,
            "Repeated scans must produce identical filename_sample"
        );
    }

    #[test]
    fn test_extension_diversity_homogeneous() {
        let names: Vec<String> = (1..=100).map(|i| format!("file{:03}.mp3", i)).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let candidates = make_candidates_single_ext(&name_refs, "mp3");
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        let result = select_representative_filenames(&candidates, &ext_hist, 10);

        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_partial_scan_bounded_and_deterministic() {
        let dir = tempdir().unwrap();
        for i in 0..300 {
            fs::write(dir.path().join(format!("file{:03}.txt", i)), "x").unwrap();
        }

        let limits = ScanLimits {
            max_files_per_dir: 50,
            max_representative_files: 10,
            ..Default::default()
        };

        let scanner = Scanner::with_limits(dir.path(), limits);
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.partial_scan, "Should be marked as partial");
        assert!(
            evidence.filename_sample.len() <= 10,
            "filename_sample should be bounded"
        );
        assert_eq!(
            evidence.file_count, 50,
            "File count should be limited by max_files_per_dir"
        );
    }

    #[test]
    fn test_unicode_filenames_deterministic() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("文件_001.txt"), "1").unwrap();
        fs::write(dir.path().join("文件_002.txt"), "2").unwrap();
        fs::write(dir.path().join("файл_003.txt"), "3").unwrap();
        fs::write(dir.path().join("αρχείο_004.txt"), "4").unwrap();

        let limits = ScanLimits {
            max_representative_files: 5,
            ..Default::default()
        };

        let scanner1 = Scanner::with_limits(dir.path(), limits.clone());
        let result1 = scanner1.scan().unwrap();

        let scanner2 = Scanner::with_limits(dir.path(), limits.clone());
        let result2 = scanner2.scan().unwrap();

        assert_eq!(
            result1.evidence[0].filename_sample, result2.evidence[0].filename_sample,
            "Unicode filenames should be handled deterministically"
        );
    }

    #[test]
    fn test_extension_diversity_mixed() {
        let mut names: Vec<String> = (1..=10).map(|i| format!("{:03}.mp3", i)).collect();
        names.extend((1..=10).map(|i| format!("file{:03}.txt", i)));
        names.extend_from_slice(&["cover.jpg".to_string(), "data.bin".to_string()]);
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

        let mut extensions: Vec<&str> = vec!["mp3"; 10];
        extensions.extend_from_slice(&["txt"; 10]);
        extensions.extend_from_slice(&["jpg", "bin"]);

        let candidates = make_candidates_multi(&name_refs, &extensions, &[], &[]);
        let ext_hist = make_ext_hist_from_candidates(&candidates);

        let result = select_representative_filenames(&candidates, &ext_hist, 10);

        assert!(
            result.contains(&"cover.jpg".to_string()),
            "Should include rare .jpg"
        );
        assert!(
            result.contains(&"data.bin".to_string()),
            "Should include rare .bin"
        );
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_compute_structural_positions() {
        assert_eq!(compute_structural_positions(1000, 1), vec![0]);
        assert_eq!(compute_structural_positions(1000, 2), vec![0, 999]);

        let result = compute_structural_positions(1000, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0);
        assert_eq!(result[1], 999);
        assert!(
            result[2] >= 400 && result[2] <= 600,
            "Middle position should be around 500, got {}",
            result[2]
        );

        assert!(compute_structural_positions(5, 10).is_empty());

        let result = compute_structural_positions(1000, 20);
        let unique: std::collections::HashSet<usize> = result.iter().copied().collect();
        assert_eq!(unique.len(), result.len(), "Positions must be unique");
        assert!(
            result.iter().all(|&p| p < 1000),
            "All positions must be within bounds"
        );
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn test_no_duplicate_filenames_in_result() {
        let dir = tempdir().unwrap();
        for i in 0..100 {
            fs::write(dir.path().join(format!("file{:03}.txt", i)), "x").unwrap();
        }

        let limits = ScanLimits {
            max_representative_files: 20,
            ..Default::default()
        };

        for _ in 0..5 {
            let scanner = Scanner::with_limits(dir.path(), limits.clone());
            let result = scanner.scan().unwrap();
            let filenames = &result.evidence[0].filename_sample;

            let unique: std::collections::HashSet<_> = filenames.iter().collect();
            assert_eq!(
                unique.len(),
                filenames.len(),
                "No duplicate filenames in result"
            );
            assert_eq!(filenames.len(), 20);
        }
    }

    #[test]
    fn test_depth_field_populated() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("level1").join("level2");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(dir.path().join("root.txt"), "x").unwrap();
        fs::write(subdir.join("deep.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let root_ev = result
            .evidence
            .iter()
            .find(|e| e.parent_path.is_none())
            .unwrap();
        assert_eq!(root_ev.depth, 0);

        let level1 = result.evidence.iter().find(|e| e.name == "level1").unwrap();
        assert_eq!(level1.depth, 1);

        let level2 = result.evidence.iter().find(|e| e.name == "level2").unwrap();
        assert_eq!(level2.depth, 2);
    }

    #[test]
    fn test_is_empty_flag() {
        let dir = tempdir().unwrap();
        let empty_subdir = dir.path().join("empty");
        fs::create_dir(&empty_subdir).unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let empty_ev = result.evidence.iter().find(|e| e.name == "empty").unwrap();
        assert!(empty_ev.is_empty);
        assert_eq!(empty_ev.file_count, 0);
        assert_eq!(empty_ev.directory_count, 0);

        let root_ev = result
            .evidence
            .iter()
            .find(|e| e.parent_path.is_none())
            .unwrap();
        assert!(!root_ev.is_empty);
    }

    #[test]
    fn test_dominant_extensions_computed() {
        let dir = tempdir().unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("file{:02}.mp3", i)), "x").unwrap();
        }
        fs::write(dir.path().join("readme.md"), "x").unwrap();
        fs::write(dir.path().join("cover.jpg"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(!evidence.dominant_extensions.is_empty());
        let top = &evidence.dominant_extensions[0];
        assert_eq!(top.extension, "mp3");
        assert_eq!(top.count, 10);
        assert!((top.percentage - 83.33).abs() < 0.1);
    }

    #[test]
    fn test_identifier_summary_computed() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("book_978-0-306-40615-7.pdf"), "x").unwrap();
        fs::write(
            dir.path()
                .join("song_550e8400-e29b-41d4-a716-446655440000.mp3"),
            "x",
        )
        .unwrap();
        fs::write(dir.path().join("data_v1.2.3.bin"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.identifier_summary.total >= 3);
        assert!(evidence
            .identifier_summary
            .by_type
            .contains_key(&IdentifierType::Isbn));
        assert!(evidence
            .identifier_summary
            .by_type
            .contains_key(&IdentifierType::Uuid));
        assert!(evidence
            .identifier_summary
            .by_type
            .contains_key(&IdentifierType::Semver));
    }

    #[test]
    fn test_schema_version_in_evidence() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence[0].schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_scan_batch_id_generated() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert!(!result.metadata.scan_batch_id.is_empty());
        assert_eq!(result.evidence[0].schema_version, SCHEMA_VERSION);
    }
}
