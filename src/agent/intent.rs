use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    Organize {
        scope: PathBuf,
        purpose: String,
    },
    Reorganize {
        scope: PathBuf,
        strategy: Option<String>,
    },
    Clean {
        scope: PathBuf,
        rules: Vec<CleanRule>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CleanRule {
    DeleteTemps,
    ArchiveOld(Duration),
    DeleteByNamePattern(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub struct ConstraintSet {
    pub preserve_existing_folders: bool,
    pub merge_duplicates: bool,
    pub archive_old: Option<Duration>,
    pub auto_delete_temps: bool,
    pub max_interactive_questions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TaskIntent {
    pub goal: Goal,
    pub constraints: ConstraintSet,
    pub user_hints: HashMap<String, String>,
    pub unknown_factors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IntentParseError {
    ScopeNotSpecified,
    AmbiguousPurpose,
    UnrecognizedIntent,
}

impl std::fmt::Display for IntentParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntentParseError::ScopeNotSpecified => write!(f, "Scope directory not specified"),
            IntentParseError::AmbiguousPurpose => write!(f, "Intent purpose is ambiguous"),
            IntentParseError::UnrecognizedIntent => {
                write!(f, "Could not recognize intent from request")
            }
        }
    }
}

impl std::error::Error for IntentParseError {}

#[derive(Debug, Clone)]
pub struct TaskIntentParser {
    pub default_scope: PathBuf,
}

impl Default for TaskIntentParser {
    fn default() -> Self {
        Self {
            default_scope: PathBuf::from("."),
        }
    }
}

impl TaskIntentParser {
    pub fn new(default_scope: PathBuf) -> Self {
        Self { default_scope }
    }

    /// Parse a natural language request into a structured `TaskIntent`.
    ///
    /// Recognized patterns:
    /// - "organize" / "整理" → Goal::Organize
    /// - "reorganize" / "re-organize" / "重新整理" / "rearrange" → Goal::Reorganize
    /// - "clean" / "清理" / "delete" / "remove" / "archive" → Goal::Clean
    ///
    /// Scope keywords:
    /// - "downloads" → expands to common Downloads paths
    /// - "desktop" → expands to common Desktop paths
    /// - bare path → used directly
    /// - no path → uses default_scope
    ///
    /// Purpose keywords (for Organize/Reorganize):
    /// - "by work", "by project" → "work/project"
    /// - "by work / personal / entertainment" → "work/personal/entertainment"
    /// - "by category" / "by type" → "by_category"
    /// - no specific taxonomy → "general_organization"
    pub fn parse(&self, request: &str) -> Result<TaskIntent, IntentParseError> {
        let request_normalized = request.to_lowercase();
        let request_trimmed = request.trim();

        let goal = self.detect_goal(&request_normalized, request_trimmed)?;
        let scope = self.detect_scope(&request_normalized);
        let purpose = self.detect_purpose(&request_normalized);
        let constraints = self.infer_constraints(&request_normalized);
        let user_hints = self.extract_hints(&request_normalized);
        let unknown_factors = self.identify_unknown_factors(&request_normalized, &goal);

        let goal = match goal {
            DetectedGoal::Organize => Goal::Organize { scope, purpose },
            DetectedGoal::Reorganize => Goal::Reorganize {
                scope,
                strategy: Some(purpose.clone()),
            },
            DetectedGoal::Clean(clean_rules) => Goal::Clean {
                scope,
                rules: clean_rules,
            },
        };

        Ok(TaskIntent {
            goal,
            constraints,
            user_hints,
            unknown_factors,
        })
    }

    fn detect_goal(
        &self,
        request_lower: &str,
        _request_raw: &str,
    ) -> Result<DetectedGoal, IntentParseError> {
        if request_lower.contains("reorganize")
            || request_lower.contains("re-organize")
            || request_lower.contains("重新整理")
            || request_lower.contains("rearrange")
            || request_lower.contains("重新排列")
        {
            return Ok(DetectedGoal::Reorganize);
        }

        if request_lower.contains("clean")
            || request_lower.contains("clean up")
            || request_lower.contains("清理")
            || request_lower.contains("delete")
            || request_lower.contains("remove")
            || request_lower.contains("delete")
            || request_lower.contains("archive")
            || request_lower.contains("歸檔")
            || request_lower.contains("整理乾淨")
        {
            let rules = self.detect_clean_rules(request_lower);
            return Ok(DetectedGoal::Clean(rules));
        }

        if request_lower.contains("organize")
            || request_lower.contains("整治")
            || request_lower.contains("整理")
            || request_lower.contains("歸類")
            || request_lower.contains("sort")
            || request_lower.contains("arrange")
            || request_lower.contains("structure")
        {
            return Ok(DetectedGoal::Organize);
        }

        Err(IntentParseError::UnrecognizedIntent)
    }

    fn detect_clean_rules(&self, request_lower: &str) -> Vec<CleanRule> {
        let mut rules = Vec::new();

        if request_lower.contains("temp")
            || request_lower.contains("temporary")
            || request_lower.contains("暫存")
            || request_lower.contains("暫時")
        {
            rules.push(CleanRule::DeleteTemps);
        }

        if request_lower.contains("old")
            || request_lower.contains("過期")
            || request_lower.contains("久未使用")
        {
            if let Some(days) = Self::extract_archive_days(request_lower) {
                rules.push(CleanRule::ArchiveOld(Duration::from_secs(
                    (days as u64) * 86400,
                )));
            }
        }

        rules
    }

    fn extract_archive_days(request_lower: &str) -> Option<usize> {
        if request_lower.contains("30") {
            Some(30)
        } else if request_lower.contains("90") {
            Some(90)
        } else if request_lower.contains("365") || request_lower.contains("1 year") {
            Some(365)
        } else {
            None
        }
    }

    fn detect_scope(&self, request_lower: &str) -> PathBuf {
        if request_lower.contains("downloads") || request_lower.contains("下載") {
            return Self::expand_special_path("Downloads");
        }
        if request_lower.contains("desktop") || request_lower.contains("桌面") {
            return Self::expand_special_path("Desktop");
        }
        if request_lower.contains("documents") {
            return Self::expand_special_path("Documents");
        }

        let tokens: Vec<&str> = request_lower.split_whitespace().collect();
        for token in &tokens {
            let trimmed = token.trim_start_matches(|c: char| !c.is_alphanumeric());
            if trimmed.starts_with('/') || trimmed.starts_with('\\') || trimmed.starts_with("./") {
                return PathBuf::from(trimmed);
            }
            if PathBuf::from(trimmed).is_dir() {
                return PathBuf::from(trimmed);
            }
        }

        self.default_scope.clone()
    }

    fn detect_purpose(&self, request_lower: &str) -> String {
        if request_lower.contains("work")
            && (request_lower.contains("personal") || request_lower.contains("個人"))
            && (request_lower.contains("entertainment") || request_lower.contains("娛樂"))
        {
            return "work/personal/entertainment".to_string();
        }

        if (request_lower.contains("work") || request_lower.contains("工作"))
            && (request_lower.contains("project") || request_lower.contains("專案"))
        {
            return "work/project".to_string();
        }

        if request_lower.contains("project") || request_lower.contains("專案") {
            return "by_project".to_string();
        }

        if request_lower.contains("year")
            || request_lower.contains("年度")
            || request_lower.contains("date")
            || request_lower.contains("時間")
        {
            return "by_date".to_string();
        }

        if request_lower.contains("type")
            || request_lower.contains("category")
            || request_lower.contains("類別")
            || request_lower.contains("種類")
        {
            return "by_category".to_string();
        }

        if request_lower.contains("size") || request_lower.contains("大小") {
            return "by_size".to_string();
        }

        "general_organization".to_string()
    }

    fn infer_constraints(&self, request_lower: &str) -> ConstraintSet {
        let preserve = request_lower.contains("preserve")
            || request_lower.contains("保留")
            || request_lower.contains("keep existing")
            || request_lower.contains("existing folder");

        let merge = request_lower.contains("merge")
            || request_lower.contains("合併")
            || request_lower.contains("duplicate");

        let archive_old = if request_lower.contains("archive old")
            || request_lower.contains("archive")
            || request_lower.contains("歸檔")
            || request_lower.contains("舊檔")
        {
            let days = Self::extract_archive_days(request_lower).unwrap_or(365);
            Some(Duration::from_secs((days as u64) * 86400))
        } else {
            None
        };

        let auto_delete = request_lower.contains("delete temp")
            || request_lower.contains("auto-delete")
            || request_lower.contains("刪除暫存")
            || request_lower.contains("自動刪除")
            || request_lower.contains("clean up temp")
            || request_lower.contains("clean temp");

        ConstraintSet {
            preserve_existing_folders: preserve,
            merge_duplicates: merge,
            archive_old,
            auto_delete_temps: auto_delete,
            max_interactive_questions: 10,
        }
    }

    fn extract_hints(&self, request_lower: &str) -> HashMap<String, String> {
        let mut hints = HashMap::new();

        if request_lower.contains("dry run")
            || request_lower.contains("preview")
            || request_lower.contains("預覽")
        {
            hints.insert("dry_run".to_string(), "true".to_string());
        }

        if request_lower.contains("verbose") || request_lower.contains("詳細") {
            hints.insert("verbose".to_string(), "true".to_string());
        }

        hints
    }

    fn identify_unknown_factors(&self, request_lower: &str, goal: &DetectedGoal) -> Vec<String> {
        let mut unknowns = Vec::new();

        if matches!(goal, DetectedGoal::Organize) || matches!(goal, DetectedGoal::Reorganize) {
            if self.detect_purpose(request_lower) == "general_organization" {
                unknowns.push("desired_category_structure".to_string());
            }
            if !request_lower.contains("delete")
                && !request_lower.contains("remove")
                && !request_lower.contains("clean")
            {
                unknowns.push("file_disposition_policy".to_string());
            }
            unknowns.push("duplicate_handling_strategy".to_string());
            unknowns.push("ambiguous_file_resolution".to_string());
        }

        if matches!(goal, DetectedGoal::Clean(_))
            && !request_lower.contains("delete temp")
            && !request_lower.contains("archive old")
        {
            unknowns.push("cleanup_scope_definition".to_string());
        }

        unknowns
    }

    fn expand_special_path(folder: &str) -> PathBuf {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            PathBuf::from(profile).join(folder)
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(folder)
        } else {
            PathBuf::from(folder)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DetectedGoal {
    Organize,
    Reorganize,
    Clean(Vec<CleanRule>),
}

#[cfg(test)]
#[path = "intent_tests.rs"]
mod tests;
