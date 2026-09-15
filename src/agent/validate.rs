use crate::agent::intent::TaskIntent;
use crate::agent::plan::{FileSystemOperation, OperationPlan};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Valid,
    BlockedByConstraint(String),
    Conflict(String),
    Invalid(String),
    Warning(String),
}

impl std::fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationStatus::Valid => write!(f, "VALID"),
            ValidationStatus::BlockedByConstraint(msg) => write!(f, "BLOCKED: {}", msg),
            ValidationStatus::Conflict(msg) => write!(f, "CONFLICT: {}", msg),
            ValidationStatus::Invalid(msg) => write!(f, "INVALID: {}", msg),
            ValidationStatus::Warning(msg) => write!(f, "WARNING: {}", msg),
        }
    }
}

impl ValidationStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationStatus::Valid)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, ValidationStatus::BlockedByConstraint(_))
    }

    #[allow(dead_code)]
    pub fn is_conflict(&self) -> bool {
        matches!(self, ValidationStatus::Conflict(_))
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, ValidationStatus::Invalid(_))
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, ValidationStatus::Warning(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatedOperation {
    pub operation: FileSystemOperation,
    pub status: ValidationStatus,
    pub warnings: Vec<String>,
    pub dependencies: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidationSummary {
    pub total: usize,
    pub valid: usize,
    pub blocked: usize,
    pub conflicts: usize,
    pub invalid: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidationResult {
    pub plan_id: String,
    pub scope: PathBuf,
    pub validated_operations: Vec<ValidatedOperation>,
    pub summary: ValidationSummary,
    pub has_blocked: bool,
    pub has_conflicts: bool,
    pub has_invalid: bool,
    pub has_warnings: bool,
    pub executable_operations: usize,
}

impl ValidationSummary {
    pub fn new() -> Self {
        ValidationSummary {
            total: 0,
            valid: 0,
            blocked: 0,
            conflicts: 0,
            invalid: 0,
            warnings: 0,
        }
    }
}

impl Default for ValidationSummary {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PlanValidator;

impl Default for PlanValidator {
    fn default() -> Self {
        Self
    }
}

impl PlanValidator {
    pub fn validate(
        &self,
        plan: &OperationPlan,
        intent: &TaskIntent,
    ) -> ValidationResult {
        let mut validated = Vec::new();
        let mut summary = ValidationSummary::new();
        let scope = &plan.scope;

        let mut source_map: HashMap<PathBuf, usize> = HashMap::new();
        let mut dest_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        let mut create_dir_indices: HashMap<PathBuf, usize> = HashMap::new();

        for op in &plan.operations {
            let mut deps = Vec::new();
            let mut warnings = Vec::new();
            let mut status = ValidationStatus::Valid;

            match op {
                FileSystemOperation::Move { source, dest } => {
                    if !source.exists() {
                        status = ValidationStatus::Invalid(format!(
                            "Source not found: {}", source.display()
                        ));
                        summary.invalid += 1;
                    } else if source == dest {
                        status = ValidationStatus::Invalid(format!(
                            "Source equals destination: {}", source.display()
                        ));
                        summary.invalid += 1;
                    } else if !Self::is_within_scope(dest, scope) {
                        status = ValidationStatus::Invalid(format!(
                            "Destination outside scope: {}", dest.display()
                        ));
                        summary.invalid += 1;
                    } else if dest.is_file() || dest.is_symlink() {
                        status = ValidationStatus::Conflict(format!(
                            "Destination file exists: {}", dest.display()
                        ));
                        summary.conflicts += 1;
                    } else if dest.is_dir() && source.is_file() {
                        let parent_dest = dest.parent().unwrap_or(dest);
                        if !parent_dest.exists() {
                            if let Some(&idx) = create_dir_indices.get(parent_dest) {
                                deps.push(idx);
                            }
                        }
                    }

                    if let Some(existing_idx) = source_map.get(source) {
                        let conflicting = format!(
                            "Duplicate source: {} (also moved in operation #{})",
                            source.display(), existing_idx
                        );
                        status = ValidationStatus::Conflict(conflicting);
                        summary.conflicts += 1;
                    } else {
                        source_map.insert(source.clone(), validated.len());
                    }

                    dest_map.entry(dest.clone()).or_default().push(validated.len());

                    if source.is_dir() && !Self::check_path_depth(source) {
                        warnings.push(format!("Source path is very deep: {}", source.display()));
                    }
                }
                FileSystemOperation::CreateDir { path } => {
                    if path.exists() {
                        status = ValidationStatus::Warning(format!(
                            "Directory already exists: {}", path.display()
                        ));
                        summary.warnings += 1;
                    } else if !Self::is_within_scope(path, scope) {
                        status = ValidationStatus::Invalid(format!(
                            "CreateDir path outside scope: {}", path.display()
                        ));
                        summary.invalid += 1;
                    } else {
                        create_dir_indices.insert(path.clone(), validated.len());
                    }
                }
                FileSystemOperation::Delete { path, .. } => {
                    if !path.exists() {
                        status = ValidationStatus::Invalid(format!(
                            "Delete target not found: {}", path.display()
                        ));
                        summary.invalid += 1;
                    } else if !Self::is_within_scope(path, scope) {
                        status = ValidationStatus::Invalid(format!(
                            "Delete target outside scope: {}", path.display()
                        ));
                        summary.invalid += 1;
                    }
                }
            }

            if status.is_valid() && !warnings.is_empty() {
                status = ValidationStatus::Warning(warnings.remove(0).clone());
                summary.warnings += 1;
            }

            let blocked_reason = Self::check_constraint_blocked(op, intent);
            if let Some(reason) = blocked_reason {
                status = ValidationStatus::BlockedByConstraint(reason);
                summary.blocked += 1;
            }

            if status.is_valid() {
                summary.valid += 1;
            }

            validated.push(ValidatedOperation {
                operation: op.clone(),
                status,
                warnings,
                dependencies: deps,
            });
        }

        let has_conflicts = Self::detect_op_conflicts(&plan.operations);
        let has_blocked = validated.iter().any(|v| v.status.is_blocked());
        let has_invalid = validated.iter().any(|v| v.status.is_invalid());
        let has_warnings = validated.iter().any(|v| v.status.is_warning());

        let executable_operations = validated.iter().filter(|v| v.status.is_valid()).count();

        summary.total = plan.operations.len();

        ValidationResult {
            plan_id: plan.id.clone(),
            scope: scope.clone(),
            validated_operations: validated,
            summary,
            has_blocked,
            has_conflicts,
            has_invalid,
            has_warnings,
            executable_operations,
        }
    }

    fn check_constraint_blocked(
        op: &FileSystemOperation,
        intent: &TaskIntent,
    ) -> Option<String> {
        match op {
            FileSystemOperation::Delete { path, .. } => {
                let path_str = path.to_string_lossy().to_lowercase();
                if path_str.contains("temp") && !intent.constraints.auto_delete_temps {
                    return Some("auto_delete_temps is false".to_string());
                }
                if path_str.contains("tmp") && !intent.constraints.auto_delete_temps {
                    return Some("auto_delete_temps is false".to_string());
                }
                if intent.constraints.preserve_existing_folders {
                    return Some("preserve_existing_folders is true".to_string());
                }
                None
            }
            FileSystemOperation::Move { dest, .. } => {
                if !intent.constraints.preserve_existing_folders {
                    // Not a constraint violation, just a note
                }
                let dest_ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("");
                if (dest_ext == "tmp" || dest_ext == "temp" || dest_ext == "crdownload")
                    && !intent.constraints.auto_delete_temps {
                    return Some("auto_delete_temps is false".to_string());
                }
                None
            }
            _ => None,
        }
    }

    fn detect_op_conflicts(operations: &[FileSystemOperation]) -> bool {
        let mut create_dirs: HashSet<&PathBuf> = HashSet::new();
        let mut destinations: Vec<&PathBuf> = Vec::new();

        for op in operations {
            match op {
                FileSystemOperation::CreateDir { path } => {
                    if !create_dirs.insert(path) {
                        return true;
                    }
                }
                FileSystemOperation::Move { dest, .. } => {
                    destinations.push(dest);
                }
                _ => {}
            }
        }

        let dest_counts: HashMap<&PathBuf, usize> = destinations.iter().fold(HashMap::new(), |mut map, d| {
            *map.entry(*d).or_insert(0) += 1;
            map
        });

        destinations.iter().any(|d| {
            dest_counts.get(d).map(|c| *c > 1).unwrap_or(false)
        })
    }

    fn is_within_scope(path: &Path, scope: &Path) -> bool {
        if path == scope {
            return true;
        }

        let path_components: Vec<_> = path.components().collect();
        let scope_components: Vec<_> = scope.components().collect();

        if path_components.len() <= scope_components.len() {
            return false;
        }

        path_components.starts_with(&scope_components[..])
    }

    fn check_path_depth(path: &Path) -> bool {
        path.components().count() <= 10
    }

    #[allow(dead_code)]
    pub fn validate_plan_consistency(
        &self,
        plan: &OperationPlan,
    ) -> Vec<String> {
        let mut issues = Vec::new();

        let mut sources: HashSet<&PathBuf> = HashSet::new();
        for op in &plan.operations {
            if let FileSystemOperation::Move { source, .. } = op {
                if !sources.insert(source) {
                    issues.push(format!("Duplicate source: {}", source.display()));
                }
            }
        }

        let mut create_dir_paths: HashSet<&PathBuf> = HashSet::new();
        for op in &plan.operations {
            if let FileSystemOperation::CreateDir { path } = op {
                if !create_dir_paths.insert(path) {
                    issues.push(format!("Duplicate CreateDir: {}", path.display()));
                }
            }
        }

        let mut move_destinations: HashMap<&PathBuf, Vec<&PathBuf>> = HashMap::new();
        for op in &plan.operations {
            if let FileSystemOperation::Move { dest, source } = op {
                move_destinations.entry(dest).or_default().push(source);
            }
        }

        for (dest, sources) in &move_destinations {
            if sources.len() > 1 {
                issues.push(format!(
                    "Destination {} is target of {} move operations (potential collision)",
                    dest.display(), sources.len()
                ));
            }
        }

        issues
    }

    #[allow(dead_code)]
    pub fn filter_executable(
        &self,
        plan: &OperationPlan,
        intent: &TaskIntent,
    ) -> OperationPlan {
        let result = self.validate(plan, intent);

        let executable: Vec<FileSystemOperation> = result
            .validated_operations
            .iter()
            .filter(|v| v.status.is_valid() || v.status.is_warning())
            .map(|v| v.operation.clone())
            .collect();

        let mut new_plan = plan.clone();
        new_plan.operations = executable;
        new_plan.id = format!("plan-exec-{}", plan.created_at);
        new_plan

    }
}

pub struct PlanPreview;

impl Default for PlanPreview {
    fn default() -> Self {
        Self
    }
}

impl PlanPreview {
    pub fn render(
        &self,
        plan: &OperationPlan,
        validation: &ValidationResult,
    ) -> String {
        let mut output = String::new();

        output.push_str(&format!("Operation Plan: {}\n", plan.id));
        output.push_str(&format!("Scope: {}\n", plan.scope.display()));
        output.push_str(&format!("Dry run: {}\n", if plan.dry_run { "Yes" } else { "No" }));
        output.push_str(&format!("Status: {} total, {} executable\n",
            validation.summary.total,
            validation.executable_operations));
        output.push('\n');

        output.push_str("=== Validation Summary ===\n");
        output.push_str(&format!("  VALID:     {}\n", validation.summary.valid));
        output.push_str(&format!("  BLOCKED:   {}\n", validation.summary.blocked));
        output.push_str(&format!("  CONFLICT:  {}\n", validation.summary.conflicts));
        output.push_str(&format!("  INVALID:   {}\n", validation.summary.invalid));
        output.push_str(&format!("  WARNING:   {}\n", validation.summary.warnings));
        output.push('\n');

        if plan.operations.is_empty() {
            output.push_str("No operations proposed.\n");
            return output;
        }

        output.push_str("=== Operations ===\n");
        for validated in validation.validated_operations.iter() {
            let status_tag = match &validated.status {
                ValidationStatus::Valid => "  ✓",
                ValidationStatus::BlockedByConstraint(_) => "  ✗",
                ValidationStatus::Conflict(_) => "  ⚠",
                ValidationStatus::Invalid(_) => "  ✗",
                ValidationStatus::Warning(_) => "  ⚠",
            };
            output.push_str(&format!("{} [{}] {}\n", status_tag, validated.status, validated.operation.description()));
            for dep in &validated.dependencies {
                output.push_str(&format!("    depends on operation #{}\n", dep));
            }
        }

        output.push('\n');
        output.push_str("=== Estimated Impact ===\n");
        output.push_str(&format!("  Files moved:  {}\n", plan.estimated_impact.files_moved));
        output.push_str(&format!("  Dirs created: {}\n", plan.estimated_impact.dirs_created));
        output.push_str(&format!("  Dirs affected: {}\n", plan.estimated_impact.dirs_affected));
        output.push_str(&format!("  Total bytes:  {}\n", plan.estimated_impact.total_bytes));

        if plan.has_conflicts {
            output.push_str("\n⚠️  Plan has detected conflicts\n");
        }

        output
    }
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;