use crate::agent::analysis::{ContentType, TaskAnalysis};
use crate::agent::clarification::UserDecision;
use crate::agent::intent::ConstraintSet;
use crate::agent::proposal_converter::ProposalConverter;
use crate::agent::recommendation::{ProposedOperation, Recommendation};
use crate::evidence::DirectoryEvidence;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemOperation {
    Move { source: PathBuf, dest: PathBuf },
    CreateDir { path: PathBuf },
    Delete { path: PathBuf, reason: String },
}

impl FileSystemOperation {
    #[allow(dead_code)]
    pub fn operation_type(&self) -> &'static str {
        match self {
            FileSystemOperation::Move { .. } => "move",
            FileSystemOperation::CreateDir { .. } => "create_dir",
            FileSystemOperation::Delete { .. } => "delete",
        }
    }

    #[allow(dead_code)]
    pub fn description(&self) -> String {
        match self {
            FileSystemOperation::Move { source, dest } => {
                format!("Move {} → {}", source.display(), dest.display())
            }
            FileSystemOperation::CreateDir { path } => {
                format!("Create directory: {}", path.display())
            }
            FileSystemOperation::Delete { path, reason } => {
                format!("Delete {} ({})", path.display(), reason)
            }
        }
    }

    pub fn source_path(&self) -> Option<&PathBuf> {
        match self {
            FileSystemOperation::Move { source, .. } => Some(source),
            FileSystemOperation::Delete { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn dest_path(&self) -> Option<&PathBuf> {
        match self {
            FileSystemOperation::Move { dest, .. } => Some(dest),
            FileSystemOperation::CreateDir { path } => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct EstimatedImpact {
    pub files_moved: u64,
    pub dirs_created: u64,
    pub files_deleted: u64,
    pub dirs_affected: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidationWarning {
    pub path: PathBuf,
    pub warning_type: WarningType,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningType {
    PartialScanScope,
    FileCountEstimate,
    MissingSourceFile,
    PathTooDeep,
}

impl std::fmt::Display for WarningType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningType::PartialScanScope => write!(f, "Partial scan scope"),
            WarningType::FileCountEstimate => write!(f, "File count estimate"),
            WarningType::MissingSourceFile => write!(f, "Missing source file"),
            WarningType::PathTooDeep => write!(f, "Path too deep"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct PlanValidationContext {
    pub preserve_existing_folders: bool,
    pub auto_delete_temps: bool,
}

impl From<&ConstraintSet> for PlanValidationContext {
    fn from(constraints: &ConstraintSet) -> Self {
        PlanValidationContext {
            preserve_existing_folders: constraints.preserve_existing_folders,
            auto_delete_temps: constraints.auto_delete_temps,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OperationPlan {
    pub id: String,
    pub recommendation_id: String,
    pub scope: PathBuf,
    pub operations: Vec<FileSystemOperation>,
    pub estimated_impact: EstimatedImpact,
    pub validation_warnings: Vec<ValidationWarning>,
    pub has_conflicts: bool,
    #[serde(default)]
    pub unresolved_proposals: Vec<crate::agent::proposal_converter::UnresolvedItem>,
    pub dry_run: bool,
    pub created_at: u64,
    #[serde(default)]
    pub validation_context: Option<PlanValidationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlanError {
    SourcePathNotFound(PathBuf),
    DestinationOutsideScope(PathBuf),
    SourceEqualsDestination { source: PathBuf, dest: PathBuf },
    ConflictingOperations(String),
    InvalidCategory(String),
    EmptySourceDir(PathBuf),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::SourcePathNotFound(p) => write!(f, "Source path not found: {}", p.display()),
            PlanError::DestinationOutsideScope(p) => {
                write!(f, "Destination outside scope: {}", p.display())
            }
            PlanError::SourceEqualsDestination { source, dest } => {
                write!(
                    f,
                    "Source equals destination: {} == {}",
                    source.display(),
                    dest.display()
                )
            }
            PlanError::ConflictingOperations(msg) => write!(f, "Conflicting operations: {}", msg),
            PlanError::InvalidCategory(name) => write!(f, "Invalid category: {}", name),
            PlanError::EmptySourceDir(p) => write!(f, "Source directory is empty: {}", p.display()),
        }
    }
}

impl std::error::Error for PlanError {}

pub struct PlanGenerator;

impl Default for PlanGenerator {
    fn default() -> Self {
        Self
    }
}

impl PlanGenerator {
    pub fn generate(
        &self,
        recommendation: &Recommendation,
        analysis: &TaskAnalysis,
    ) -> Result<OperationPlan, PlanError> {
        let scope = analysis.scope_evidence.path.clone();
        let mut operations: Vec<FileSystemOperation> = Vec::new();
        let mut warnings: Vec<ValidationWarning> = Vec::new();
        let mut estimated = EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        };

        let mut created_dirs: HashSet<PathBuf> = self.collect_existing_dirs(&scope, analysis);
        let mut unresolved_proposals = Vec::new();

        let use_proposal_converter = recommendation
            .organization_proposal
            .as_ref()
            .map(|p| {
                p.proposed_categories
                    .iter()
                    .any(|c| !c.source_files.is_empty())
            })
            .unwrap_or(false);

        if use_proposal_converter {
            let proposal = recommendation.organization_proposal.as_ref().unwrap();
            let converter = ProposalConverter;
            let (prop_ops, prop_unresolved) = converter.convert(proposal, analysis, &scope)?;
            operations = prop_ops;
            unresolved_proposals = prop_unresolved;
        } else {
            for op in &recommendation.proposed_operations {
                match op {
                    ProposedOperation::CreateCategory { name, .. } => {
                        let dest =
                            self.resolve_category_dir(&scope, &analysis.candidate_categories, name);
                        if !created_dirs.contains(&dest) && !dest.exists() {
                            operations.push(FileSystemOperation::CreateDir { path: dest.clone() });
                            estimated.dirs_created += 1;
                            created_dirs.insert(dest.clone());
                        }
                    }
                    ProposedOperation::MoveCategory {
                        to_category,
                        content_type,
                        file_count,
                        ..
                    } => {
                        let dest = self.resolve_category_dir(
                            &scope,
                            &analysis.candidate_categories,
                            to_category,
                        );
                        if !created_dirs.contains(&dest) && !dest.exists() {
                            operations.push(FileSystemOperation::CreateDir { path: dest.clone() });
                            estimated.dirs_created += 1;
                            created_dirs.insert(dest.clone());
                        }

                        let ct = self.parse_content_type(content_type);
                        let moved = self.resolve_file_moves(
                            &scope,
                            &analysis.scope_evidence,
                            &ct,
                            *file_count,
                            &mut warnings,
                        );

                        for (source, _) in &moved {
                            if source == &dest {
                                return Err(PlanError::SourceEqualsDestination {
                                    source: source.clone(),
                                    dest: dest.clone(),
                                });
                            }
                        }

                        let moved_len = moved.len();

                        operations.extend(moved.into_iter().map(|(source, _size)| {
                            let file_name =
                                source.file_name().map(PathBuf::from).unwrap_or_default();
                            FileSystemOperation::Move {
                                source,
                                dest: dest.join(file_name),
                            }
                        }));

                        estimated.files_moved += moved_len as u64;
                    }
                    ProposedOperation::PreserveDirectory { path } => {
                        created_dirs.insert(path.clone());
                    }
                    ProposedOperation::ArchiveFiles {
                        category,
                        file_count: _,
                    } => {
                        let archive_dir = scope.join("archive");
                        if !created_dirs.contains(&archive_dir) && !archive_dir.exists() {
                            operations.push(FileSystemOperation::CreateDir {
                                path: archive_dir.clone(),
                            });
                            estimated.dirs_created += 1;
                            created_dirs.insert(archive_dir.clone());
                        }

                        let archive_pattern = category;
                        let archive_dest = scope.join("archive").join(archive_pattern);
                        if !archive_dest.exists() {
                            operations.push(FileSystemOperation::CreateDir {
                                path: archive_dest.clone(),
                            });
                            estimated.dirs_created += 1;
                            created_dirs.insert(archive_dest.clone());
                        }

                        operations.push(FileSystemOperation::Move {
                            source: scope.join(category),
                            dest: archive_dest,
                        });
                    }
                    ProposedOperation::LeaveUnclassified { .. } => {}
                }
            }
        }

        let has_conflicts = self.validate_operations(&operations, &scope)?;

        for op in &operations {
            if let Some(path) = op.source_path() {
                if !path.exists() {
                    if let FileSystemOperation::Move { source, .. } = op {
                        if source.is_dir() {
                            return Err(PlanError::SourcePathNotFound(source.clone()));
                        }
                    }
                }
            }
        }

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let plan_id = format!("plan-{}", created_at);

        Ok(OperationPlan {
            id: plan_id,
            recommendation_id: recommendation.id.clone(),
            scope,
            operations,
            estimated_impact: estimated,
            validation_warnings: warnings,
            has_conflicts,
            unresolved_proposals,
            dry_run: true,
            created_at,
            validation_context: Some(PlanValidationContext::default()),
        })
    }

    fn collect_existing_dirs(&self, scope: &Path, analysis: &TaskAnalysis) -> HashSet<PathBuf> {
        let mut dirs = HashSet::new();
        dirs.insert(scope.to_path_buf());
        for candidate in &analysis.candidate_categories {
            dirs.insert(candidate.path.clone());
        }
        dirs
    }

    fn resolve_category_dir(
        &self,
        scope: &Path,
        candidates: &[crate::agent::analysis::CandidateCategory],
        category_name: &str,
    ) -> PathBuf {
        if let Some(candidate) = candidates.iter().find(|c| c.name == category_name) {
            return candidate.path.clone();
        }

        let human_name = self.category_to_human_name(category_name);
        scope.join(human_name)
    }

    fn category_to_human_name(&self, category: &str) -> String {
        match category {
            "document_storage" => "Documents",
            "image_storage" => "Images",
            "archive_storage" => "Archives",
            "media_storage" => "Media",
            "code_storage" => "Code",
            "installer_storage" => "Installers",
            "data_storage" => "Data",
            "config_storage" => "Config",
            "misc_storage" => "Misc",
            "temp_files" => "Temp",
            _ => category,
        }
        .to_string()
    }

    fn parse_content_type(&self, content_type: &str) -> ContentType {
        match content_type {
            "documents" => ContentType::Documents,
            "images" => ContentType::Images,
            "archives" => ContentType::Archives,
            "media" => ContentType::Media,
            "code" => ContentType::Code,
            "installers" => ContentType::Installers,
            "data" => ContentType::Data,
            "config" => ContentType::Config,
            _ => ContentType::Other,
        }
    }

    fn resolve_file_moves(
        &self,
        scope: &Path,
        evidence: &DirectoryEvidence,
        content_type: &ContentType,
        expected_count: u64,
        warnings: &mut Vec<ValidationWarning>,
    ) -> Vec<(PathBuf, u64)> {
        let mut results = Vec::new();
        let target_exts = self.content_type_extensions(content_type);

        for filename in &evidence.filename_sample {
            let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();

            if target_exts.contains(&ext.as_str()) {
                let source = scope.join(filename);
                if source.exists() {
                    results.push((source, 1024));
                } else {
                    warnings.push(ValidationWarning {
                        path: source.clone(),
                        warning_type: WarningType::MissingSourceFile,
                        message: format!("File not found: {}", filename),
                    });
                }
            }
        }

        if results.len() < expected_count as usize && evidence.partial_scan {
            warnings.push(ValidationWarning {
                path: scope.to_path_buf(),
                warning_type: WarningType::FileCountEstimate,
                message: format!(
                    "Only {} of {} expected files found (partial scan)",
                    results.len(),
                    expected_count
                ),
            });
        }

        results
    }

    fn content_type_extensions(&self, ct: &ContentType) -> Vec<&'static str> {
        match ct {
            ContentType::Documents => vec![
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "csv", "rtf",
            ],
            ContentType::Images => vec![
                "jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "svg", "ico",
            ],
            ContentType::Archives => vec!["zip", "rar", "7z", "tar", "gz", "bz2", "xz"],
            ContentType::Media => vec!["mp4", "avi", "mkv", "mov", "mp3", "wav", "flac"],
            ContentType::Code => vec![
                "py", "js", "ts", "rs", "go", "java", "c", "cpp", "h", "sh", "rb", "php",
            ],
            ContentType::Installers => vec!["exe", "msi", "dmg", "pkg", "deb", "rpm"],
            ContentType::Data => vec!["db", "sqlite", "dat", "bin"],
            ContentType::Config => vec!["conf", "cfg", "ini", "env", "properties"],
            ContentType::Other => vec![],
        }
    }

    fn validate_operations(
        &self,
        operations: &[FileSystemOperation],
        scope: &Path,
    ) -> Result<bool, PlanError> {
        let mut conflicts = false;

        for op in operations {
            if let Some(dest) = op.dest_path() {
                if !Self::is_within_scope(dest, scope) {
                    return Err(PlanError::DestinationOutsideScope(dest.clone()));
                }
            }

            if let FileSystemOperation::Move { source, dest } = op {
                if source == dest {
                    return Err(PlanError::SourceEqualsDestination {
                        source: source.clone(),
                        dest: dest.clone(),
                    });
                }
            }
        }

        let mut all_paths: HashSet<&Path> = HashSet::new();
        for op in operations {
            if let Some(dest) = op.dest_path() {
                if !all_paths.insert(dest.as_path()) {
                    conflicts = true;
                }
            }
        }

        Ok(conflicts)
    }

    fn is_within_scope(path: &Path, scope: &Path) -> bool {
        path.starts_with(scope)
    }

    #[allow(dead_code)]
    pub fn preview(&self, plan: &OperationPlan) -> String {
        let mut preview = String::new();
        preview.push_str(&format!("Operation Plan: {}\n", plan.id));
        preview.push_str(&format!("Scope: {}\n", plan.scope.display()));
        preview.push_str(&format!(
            "Dry run: {}\n",
            if plan.dry_run { "Yes" } else { "No" }
        ));
        preview.push('\n');

        if plan.operations.is_empty() {
            preview.push_str("No operations proposed.\n");
            if !plan.unresolved_proposals.is_empty() {
                preview.push_str("\n=== Unresolved Proposals ===\n");
                for item in &plan.unresolved_proposals {
                    preview.push_str(&format!(
                        "  ⚠️  {}: {:?} ({})\n",
                        item.file.display(),
                        item.reason,
                        item.category
                    ));
                }
            }
            return preview;
        }

        preview.push_str(&format!("Total operations: {}\n", plan.operations.len()));
        preview.push('\n');

        preview.push_str("=== Create Directories ===\n");
        for op in &plan.operations {
            if let FileSystemOperation::CreateDir { path } = op {
                preview.push_str(&format!("  + {}\n", path.display()));
            }
        }

        preview.push_str("\n=== File/Directory Moves ===\n");
        for op in &plan.operations {
            if let FileSystemOperation::Move { source, dest } = op {
                preview.push_str(&format!("  {} → {}\n", source.display(), dest.display()));
            }
        }

        preview.push_str("\n=== Deletions ===\n");
        for op in &plan.operations {
            if let FileSystemOperation::Delete { path, reason } = op {
                preview.push_str(&format!("  × {} ({})\n", path.display(), reason));
            }
        }

        if !plan.unresolved_proposals.is_empty() {
            preview.push_str("\n=== Unresolved Proposals ===\n");
            for item in &plan.unresolved_proposals {
                preview.push_str(&format!(
                    "  ⚠️  {}: {:?} ({})\n",
                    item.file.display(),
                    item.reason,
                    item.category
                ));
            }
        }

        preview.push('\n');
        preview.push_str("=== Estimated Impact ===\n");
        preview.push_str(&format!(
            "  Files moved: {}\n",
            plan.estimated_impact.files_moved
        ));
        preview.push_str(&format!(
            "  Dirs created: {}\n",
            plan.estimated_impact.dirs_created
        ));
        preview.push_str(&format!(
            "  Dirs affected: {}\n",
            plan.estimated_impact.dirs_affected
        ));
        preview.push_str(&format!(
            "  Total bytes: {}\n",
            plan.estimated_impact.total_bytes
        ));

        if plan.has_conflicts {
            preview.push_str("\n⚠️  Warning: Potential conflicts detected\n");
        }

        if !plan.validation_warnings.is_empty() {
            preview.push_str("\n=== Validation Warnings ===\n");
            for w in &plan.validation_warnings {
                preview.push_str(&format!("  ⚠️  {}: {}\n", w.warning_type, w.message));
            }
        }

        preview
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
