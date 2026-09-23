use crate::agent::intent::{Goal, TaskIntent};
use crate::agent::recommendation::OrganizationProposal;
use crate::classification::{
    build_classification_request, ClassificationDecision, ClassificationInput,
    ClassificationProcessor, ClassificationResult, LlmClassifier, LlmStrategyInfo,
    RuleBasedProcessor,
};
use crate::evidence::{DirectoryEvidence, ScanLimits, ScanResult};
use crate::scanner::Scanner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContentGroup {
    pub extension: String,
    pub file_count: u64,
    pub total_size: u64,
    pub percentage_of_scope: f64,
    pub category: ContentType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Documents,
    Images,
    Archives,
    Media,
    Code,
    Installers,
    Data,
    Config,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StructureSummary {
    pub total_files: u64,
    pub total_directories: u64,
    pub total_size: u64,
    pub content_groups: Vec<ContentGroup>,
    pub depth_levels: usize,
    pub has_mixed_content_dirs: bool,
    pub partial_scan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Anomaly {
    pub path: PathBuf,
    pub anomaly_type: AnomalyType,
    pub description: String,
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    DuplicateContent,
    OrphanedDirectory,
    MixedContent,
    NamingInconsistency,
    LargeFile,
    EmptyDirectory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Ambiguity {
    pub path: PathBuf,
    pub reason: AmbiguityReason,
    pub confidence_low: f64,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityReason {
    UnclearCategory,
    ConflictingEvidence,
    InsufficientPrecedent,
    GenericFilename,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceGap {
    pub gap_type: GapType,
    pub description: String,
    pub requires_user_input: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GapType {
    Taxonomy,
    FileDisposition,
    DuplicateHandling,
    ArchivePolicy,
    ScopeBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CandidateCategory {
    pub name: String,
    pub path: PathBuf,
    pub file_count: u64,
    pub is_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TaskAnalysis {
    pub intent: TaskIntent,
    pub scope_evidence: DirectoryEvidence,
    pub scan_metadata: crate::evidence::ScanMetadata,
    pub structure_summary: StructureSummary,
    pub content_groups: Vec<ContentGroup>,
    pub candidate_categories: Vec<CandidateCategory>,
    pub classification_results: Vec<ClassificationResult>,
    pub organization_strategy: Option<LlmStrategyInfo>,
    pub organization_proposal: Option<OrganizationProposal>,
    pub anomalies: Vec<Anomaly>,
    pub ambiguities: Vec<Ambiguity>,
    pub evidence_gaps: Vec<EvidenceGap>,
    pub analyzed_at: u64,
    pub analysis_duration_ms: u64,
}

#[derive(Default)]
pub struct EvidenceAnalyzer {
    llm_classifier: Option<LlmClassifier>,
    classifier_instruction: Option<String>,
}

const DOCUMENT_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv", "rtf", "odt", "odp",
    "ods",
];
const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "svg", "ico", "raw",
];
const ARCHIVE_EXTS: &[&str] = &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "tbz2"];
const MEDIA_EXTS: &[&str] = &[
    "mp4", "avi", "mkv", "mov", "wmv", "flv", "webm", "mp3", "wav", "flac", "aac", "ogg",
];
const CODE_EXTS: &[&str] = &[
    "py", "js", "ts", "rs", "go", "java", "c", "cpp", "h", "hpp", "sh", "rb", "php", "html", "css",
    "json", "xml", "yaml", "yml", "toml",
];
const INSTALLER_EXTS: &[&str] = &["exe", "msi", "dmg", "pkg", "deb", "rpm", "appimage"];
const DATA_EXTS: &[&str] = &["db", "sqlite", "sqlite3", "dat", "bin"];
const CONFIG_EXTS: &[&str] = &["conf", "cfg", "ini", "env", "properties"];

impl EvidenceAnalyzer {
    pub fn with_llm_classifier(mut self, classifier: LlmClassifier) -> Self {
        self.llm_classifier = Some(classifier);
        self
    }

    pub fn with_classifier_instruction(mut self, instruction: Option<String>) -> Self {
        self.classifier_instruction = instruction.filter(|s| !s.trim().is_empty());
        self
    }

    #[allow(dead_code)]
    pub fn analyze(&self, intent: &TaskIntent) -> Result<TaskAnalysis, AnalyzerError> {
        let start = std::time::Instant::now();
        let scope = self.extract_scope(intent);

        if !scope.is_dir() {
            return Err(AnalyzerError::ScopeNotADirectory(scope.clone()));
        }

        let limits = ScanLimits::default();
        let scanner = Scanner::with_limits(&scope, limits);
        let scan_result: ScanResult = scanner.scan()?;

        if scan_result.evidence.is_empty() {
            return Err(AnalyzerError::ScopeNotScannable(scope.clone()));
        }

        let canonical_scope = std::fs::canonicalize(&scope).unwrap_or_else(|_| scope.clone());
        let scope_evidence = scan_result
            .evidence
            .into_iter()
            .find(|e| {
                let canonical_e_path =
                    std::fs::canonicalize(&e.path).unwrap_or_else(|_| e.path.clone());
                canonical_e_path == canonical_scope || e.path == scope
            })
            .ok_or_else(|| AnalyzerError::ScopeNotScannable(scope.clone()))?;
        let scan_metadata = scan_result.metadata;

        self.analyze_with_evidence(intent, scope_evidence, scan_metadata, start)
    }

    pub fn analyze_with_evidence(
        &self,
        intent: &TaskIntent,
        scope_evidence: DirectoryEvidence,
        scan_metadata: crate::evidence::ScanMetadata,
        start: std::time::Instant,
    ) -> Result<TaskAnalysis, AnalyzerError> {
        let scope = &scope_evidence.path;
        let content_groups = Self::group_content(&scope_evidence);
        let structure_summary = Self::build_structure_summary(&scope_evidence, &content_groups);
        let candidate_categories = Self::find_candidate_categories(scope, &scan_metadata);
        let (classification_results, organization_strategy, organization_proposal) = self
            .run_classification(
                &scope_evidence,
                &candidate_categories,
                scan_metadata.clone(),
                self.classifier_instruction.as_deref(),
            )?;
        let anomalies = Self::detect_anomalies(&scope_evidence);
        let ambiguities = Self::detect_ambiguities(&scope_evidence, &classification_results);
        let evidence_gaps = self.identify_gaps(intent, &content_groups, &classification_results);

        let duration = start.elapsed();

        Ok(TaskAnalysis {
            intent: intent.clone(),
            scope_evidence,
            scan_metadata,
            structure_summary,
            content_groups,
            candidate_categories,
            classification_results,
            organization_strategy,
            organization_proposal,
            anomalies,
            ambiguities,
            evidence_gaps,
            analyzed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            analysis_duration_ms: duration.as_millis() as u64,
        })
    }

    fn extract_scope(&self, intent: &TaskIntent) -> PathBuf {
        match &intent.goal {
            Goal::Organize { scope, .. }
            | Goal::Reorganize { scope, .. }
            | Goal::Clean { scope, .. } => scope.clone(),
        }
    }

    fn group_content(evidence: &DirectoryEvidence) -> Vec<ContentGroup> {
        let total_files = evidence.file_count;
        if total_files == 0 {
            return Vec::new();
        }

        let mut type_map: HashMap<ContentType, (u64, u64)> = HashMap::new();

        for (ext, count) in &evidence.extension_histogram {
            let ct = Self::classify_extension(ext);
            let size_estimate = count * 1024;
            let entry = type_map.entry(ct).or_insert((0, 0));
            entry.0 += count;
            entry.1 += size_estimate;
        }

        let mut groups: Vec<ContentGroup> = type_map
            .into_iter()
            .map(|(ct, (count, size))| ContentGroup {
                extension: format!("{:?}", ct).to_lowercase(),
                file_count: count,
                total_size: size,
                percentage_of_scope: (count as f64 / total_files as f64) * 100.0,
                category: ct,
            })
            .collect();

        groups.sort_by_key(|g| std::cmp::Reverse(g.file_count));
        groups
    }

    fn classify_extension(ext: &str) -> ContentType {
        let ext = ext.to_lowercase();
        let ext = ext.trim_start_matches('.');

        if DOCUMENT_EXTS.contains(&ext) {
            ContentType::Documents
        } else if IMAGE_EXTS.contains(&ext) {
            ContentType::Images
        } else if ARCHIVE_EXTS.contains(&ext) {
            ContentType::Archives
        } else if MEDIA_EXTS.contains(&ext) {
            ContentType::Media
        } else if CODE_EXTS.contains(&ext) {
            ContentType::Code
        } else if INSTALLER_EXTS.contains(&ext) {
            ContentType::Installers
        } else if DATA_EXTS.contains(&ext) {
            ContentType::Data
        } else if CONFIG_EXTS.contains(&ext) {
            ContentType::Config
        } else {
            ContentType::Other
        }
    }

    fn build_structure_summary(
        evidence: &DirectoryEvidence,
        content_groups: &[ContentGroup],
    ) -> StructureSummary {
        let mut mixed_content_dirs = false;
        for ext in evidence.extension_histogram.keys() {
            let ct = Self::classify_extension(ext);
            if content_groups.iter().filter(|g| g.category == ct).count() > 1 {
                mixed_content_dirs = true;
                break;
            }
        }

        let depth = evidence.depth;
        let dominant_ext_count = evidence.dominant_extensions.len();

        StructureSummary {
            total_files: evidence.file_count,
            total_directories: evidence.directory_count,
            total_size: evidence.total_size,
            content_groups: content_groups.to_vec(),
            depth_levels: depth.max(dominant_ext_count),
            has_mixed_content_dirs: mixed_content_dirs,
            partial_scan: evidence.partial_scan,
        }
    }

    fn find_candidate_categories(
        scope: &PathBuf,
        metadata: &crate::evidence::ScanMetadata,
    ) -> Vec<CandidateCategory> {
        let mut categories = Vec::new();

        if let Ok(entries) = std::fs::read_dir(scope) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.is_symlink() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if crate::scanner::is_excluded_directory(name) {
                            continue;
                        }
                    }
                    let child_meta = metadata.limits.clone();
                    if let Ok(child_scan) = Scanner::with_limits(&path, child_meta).inspect_single()
                    {
                        if let Some(ev) = child_scan.evidence.into_iter().next() {
                            categories.push(CandidateCategory {
                                name: ev.name.clone(),
                                path: path.clone(),
                                file_count: ev.file_count,
                                is_existing: true,
                            });
                            continue;
                        }
                    }
                    categories.push(CandidateCategory {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.clone(),
                        file_count: 0,
                        is_existing: true,
                    });
                }
            }
        }

        categories
    }

    fn run_classification(
        &self,
        scope_evidence: &DirectoryEvidence,
        candidates: &[CandidateCategory],
        scan_metadata: crate::evidence::ScanMetadata,
        instruction: Option<&str>,
    ) -> Result<
        (
            Vec<ClassificationResult>,
            Option<LlmStrategyInfo>,
            Option<OrganizationProposal>,
        ),
        AnalyzerError,
    > {
        if candidates.is_empty() && instruction.is_none() {
            return Ok((Vec::new(), None, None));
        }

        if let Some(ref classifier) = self.llm_classifier {
            let (results, strategy, proposal) =
                self.run_llm_classification(scope_evidence, candidates, classifier, instruction)?;
            return Ok((results, strategy, proposal));
        }

        let results =
            self.run_rule_based_classification(scope_evidence, candidates, scan_metadata)?;
        Ok((results, None, None))
    }

    fn run_rule_based_classification(
        &self,
        scope_evidence: &DirectoryEvidence,
        candidates: &[CandidateCategory],
        scan_metadata: crate::evidence::ScanMetadata,
    ) -> Result<Vec<ClassificationResult>, AnalyzerError> {
        let candidate_paths: Vec<PathBuf> = candidates.iter().map(|c| c.path.clone()).collect();

        let mut candidates_with_prefix: Vec<crate::classification::CandidateEvidence> = Vec::new();
        for candidate_path in &candidate_paths {
            if !candidate_path.is_dir() {
                continue;
            }
            let scanner = crate::scanner::Scanner::new(candidate_path);
            if let Ok(scan_result) = scanner.scan() {
                if let Some(ev) = scan_result.evidence.into_iter().next() {
                    candidates_with_prefix.push(crate::classification::CandidateEvidence {
                        directory: ev,
                        children: Vec::new(),
                        summary: crate::classification::CandidateSummary {
                            total_children: 0,
                            top_extensions: Vec::new(),
                            common_identifier_types: Vec::new(),
                            avg_file_count: 0.0,
                        },
                    });
                }
            }
        }

        let input = ClassificationInput {
            target: scope_evidence.clone(),
            candidates: candidates_with_prefix,
            metadata: scan_metadata,
        };

        let processor = RuleBasedProcessor;
        let result = processor.classify(&input)?;

        Ok(vec![result])
    }

    fn run_llm_classification(
        &self,
        scope_evidence: &DirectoryEvidence,
        candidates: &[CandidateCategory],
        classifier: &LlmClassifier,
        instruction: Option<&str>,
    ) -> Result<
        (
            Vec<ClassificationResult>,
            Option<LlmStrategyInfo>,
            Option<OrganizationProposal>,
        ),
        AnalyzerError,
    > {
        let allowed_categories: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        let allowed_category_paths: Vec<(String, PathBuf)> = candidates
            .iter()
            .map(|c| (c.name.clone(), c.path.clone()))
            .collect();

        let request =
            build_classification_request(scope_evidence, &allowed_categories, instruction);

        let result = classifier
            .classify(&request, &scope_evidence.path, &allowed_category_paths)
            .map_err(|e| AnalyzerError::ClassificationFailed(e.to_string()))?;

        Ok((
            vec![result.classification],
            result.strategy,
            result.proposal,
        ))
    }

    fn detect_anomalies(evidence: &DirectoryEvidence) -> Vec<Anomaly> {
        let mut anomalies = Vec::new();

        if evidence.directory_count == 0 && evidence.file_count > 50 {
            anomalies.push(Anomaly {
                path: evidence.path.clone(),
                anomaly_type: AnomalyType::MixedContent,
                description:
                    "Large number of files in a single directory (possible flat structure)"
                        .to_string(),
                evidence: serde_json::json!({
                    "file_count": evidence.file_count,
                    "directory_count": evidence.directory_count,
                }),
            });
        }

        if evidence.file_count == 0 && evidence.directory_count > 0 {
            anomalies.push(Anomaly {
                path: evidence.path.clone(),
                anomaly_type: AnomalyType::OrphanedDirectory,
                description: "Directory exists but contains no files".to_string(),
                evidence: serde_json::json!({
                    "file_count": evidence.file_count,
                    "directory_count": evidence.directory_count,
                }),
            });
        }

        if evidence.total_size > 100 * 1024 * 1024 * 1024 {
            anomalies.push(Anomaly {
                path: evidence.path.clone(),
                anomaly_type: AnomalyType::LargeFile,
                description: "Scope directory is very large (>100GB)".to_string(),
                evidence: serde_json::json!({
                    "total_size": evidence.total_size,
                }),
            });
        }

        if evidence.is_empty {
            anomalies.push(Anomaly {
                path: evidence.path.clone(),
                anomaly_type: AnomalyType::EmptyDirectory,
                description: "Scope directory is empty".to_string(),
                evidence: serde_json::json!({}),
            });
        }

        if evidence.extension_histogram.len() > 20 {
            anomalies.push(Anomaly {
                path: evidence.path.clone(),
                anomaly_type: AnomalyType::MixedContent,
                description: "Many different file types detected (possible lack of organization)"
                    .to_string(),
                evidence: serde_json::json!({
                    "unique_extensions": evidence.extension_histogram.len(),
                }),
            });
        }

        anomalies
    }

    fn detect_ambiguities(
        evidence: &DirectoryEvidence,
        classification_results: &[ClassificationResult],
    ) -> Vec<Ambiguity> {
        let mut ambiguities = Vec::new();

        if evidence.extension_histogram.len() > 1 {
            let ext_count = evidence.extension_histogram.len();
            let max_ext_count = evidence
                .extension_histogram
                .values()
                .copied()
                .max()
                .unwrap_or(0);
            if (max_ext_count as f64 / evidence.file_count as f64) < 0.5 && ext_count > 5 {
                ambiguities.push(Ambiguity {
                    path: evidence.path.clone(),
                    reason: AmbiguityReason::UnclearCategory,
                    confidence_low: 0.3,
                    alternatives: evidence
                        .extension_histogram
                        .keys()
                        .take(5)
                        .cloned()
                        .collect(),
                });
            }
        }

        for result in classification_results {
            if matches!(result.decision, ClassificationDecision::AskUser) {
                ambiguities.push(Ambiguity {
                    path: result.target_path.clone(),
                    reason: AmbiguityReason::ConflictingEvidence,
                    confidence_low: result.confidence,
                    alternatives: result
                        .alternatives
                        .iter()
                        .map(|a| a.candidate_name.clone())
                        .collect(),
                });
            }
            if result.decision == ClassificationDecision::LeaveUnclassified {
                ambiguities.push(Ambiguity {
                    path: result.target_path.clone(),
                    reason: AmbiguityReason::InsufficientPrecedent,
                    confidence_low: result.confidence,
                    alternatives: Vec::new(),
                });
            }
        }

        ambiguities
    }

    fn identify_gaps(
        &self,
        intent: &TaskIntent,
        content_groups: &[ContentGroup],
        classification_results: &[ClassificationResult],
    ) -> Vec<EvidenceGap> {
        let mut gaps = Vec::new();
        let request_lower = serde_json::to_string(&intent.goal)
            .unwrap_or_default()
            .to_lowercase();

        if content_groups.is_empty()
            || content_groups
                .iter()
                .all(|g| g.category == ContentType::Other)
        {
            gaps.push(EvidenceGap {
                gap_type: GapType::Taxonomy,
                description: "No recognizable file categories found — user-defined taxonomy needed"
                    .to_string(),
                requires_user_input: true,
            });
        }

        if let Goal::Organize { purpose, .. } = &intent.goal {
            if purpose == "general_organization" {
                gaps.push(EvidenceGap {
                    gap_type: GapType::Taxonomy,
                    description: "General organization requested — specific taxonomy not specified"
                        .to_string(),
                    requires_user_input: true,
                });
            }
        }

        if !request_lower.contains("delete") && !request_lower.contains("keep") {
            gaps.push(EvidenceGap {
                gap_type: GapType::FileDisposition,
                description: "File disposition policy not specified".to_string(),
                requires_user_input: true,
            });
        }

        if classification_results
            .iter()
            .any(|r| matches!(r.decision, ClassificationDecision::AskUser))
        {
            gaps.push(EvidenceGap {
                gap_type: GapType::DuplicateHandling,
                description: "Some files have conflicting classifications requiring user input"
                    .to_string(),
                requires_user_input: true,
            });
        }

        gaps
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyzerError {
    ScopeNotADirectory(PathBuf),
    ScopeNotScannable(PathBuf),
    ScanFailed(String),
    ClassificationFailed(String),
}

impl std::fmt::Display for AnalyzerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyzerError::ScopeNotADirectory(path) => {
                write!(f, "Scope is not a directory: {}", path.display())
            }
            AnalyzerError::ScopeNotScannable(path) => {
                write!(f, "Scope directory is not scannable: {}", path.display())
            }
            AnalyzerError::ScanFailed(msg) => write!(f, "Scan failed: {}", msg),
            AnalyzerError::ClassificationFailed(msg) => write!(f, "Classification failed: {}", msg),
        }
    }
}

impl std::error::Error for AnalyzerError {}

impl From<crate::evidence::ScanError> for AnalyzerError {
    fn from(e: crate::evidence::ScanError) -> Self {
        AnalyzerError::ScanFailed(format!("{:?}", e))
    }
}

impl From<anyhow::Error> for AnalyzerError {
    fn from(e: anyhow::Error) -> Self {
        AnalyzerError::ClassificationFailed(e.to_string())
    }
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
