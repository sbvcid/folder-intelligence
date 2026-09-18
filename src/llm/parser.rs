use crate::agent::{CleanRule, ConstraintSet, Goal, IntentParseError, TaskIntent};
use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct LlmIntentResponse {
    #[serde(default)]
    goal: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    strategy: Option<String>,
    #[serde(default)]
    clean_rules: Option<Vec<String>>,
    #[serde(default)]
    constraints: LlmConstraints,
    #[serde(default)]
    user_hints: HashMap<String, String>,
    #[serde(default)]
    unknown_factors: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct LlmConstraints {
    #[serde(default)]
    preserve_existing_folders: bool,
    #[serde(default)]
    merge_duplicates: bool,
    #[serde(default)]
    archive_old: Option<u64>,
    #[serde(default)]
    auto_delete_temps: bool,
    #[serde(default)]
    max_interactive_questions: usize,
}

fn system_prompt() -> String {
    r#"You are a filesystem intent parser. Parse natural language requests into structured JSON.

Available goals:
- "organize": Group files into category directories (e.g., "organize by file type", "organize documents")
- "reorganize": Rearrange existing files into a new structure (e.g., "rearrange by date")
- "clean": Delete or archive files based on rules (e.g., "clean up temp files")

Available constraint keywords in user requests:
- "don't delete" / "no delete" / "no deletion" → auto_delete_temps=false
- "preserve" / "keep existing folders" → preserve_existing_folders=true
- "merge duplicates" → merge_duplicates=true
- "archive old" / "archive files older than" → archive_old
- "max questions" / "ask at most N questions" → max_interactive_questions

Output ONLY valid JSON. No prose, no markdown fences.

JSON schema:
{
  "goal": "organize" | "reorganize" | "clean",
  "scope": "/absolute/path/to/directory" | null,
  "purpose": "general_organization" | "by_type" | "by_category" | null,
  "strategy": null | "string describing reorganization strategy",
  "clean_rules": null | ["delete_temps", "archive_old:30" (days), "delete_pattern:*.tmp"],
  "constraints": {
    "preserve_existing_folders": true,
    "merge_duplicates": true,
    "archive_old": null | <seconds>,
    "auto_delete_temps": false,
    "max_interactive_questions": 3
  },
  "user_hints": {},
  "unknown_factors": []
}"#
        .to_string()
}

fn user_prompt(request: &str, default_scope: &str) -> String {
    format!(
        r#"Parse this request into structured JSON.
If the request mentions a directory path, use it as "scope". Otherwise set scope to null.
Working directory: {}

Request: "{}""#,
        default_scope, request
    )
}

fn extract_json(text: &str) -> &str {
    let start = text.find('{').unwrap_or(0);
    let end = text.rfind('}').map(|i| i + 1).unwrap_or(text.len());
    &text[start..end]
}

fn parse_clean_rules(rules: &[String]) -> Result<Vec<CleanRule>, LlmError> {
    let mut parsed = Vec::new();
    for rule in rules {
        let lower = rule.to_lowercase();
        if lower == "delete_temps" {
            parsed.push(CleanRule::DeleteTemps);
        } else if lower.starts_with("archive_old:") {
            let duration_str = &rule["archive_old:".len()..];
            let days: u64 = duration_str.parse().map_err(|_| {
                LlmError::InvalidResponse(format!("Invalid archive_old duration: {}", rule))
            })?;
            parsed.push(CleanRule::ArchiveOld(Duration::from_secs(days * 86400)));
        } else if lower.starts_with("delete_pattern:") {
            let pattern = &rule["delete_pattern:".len()..];
            parsed.push(CleanRule::DeleteByNamePattern(pattern.to_string()));
        } else {
            return Err(LlmError::InvalidResponse(format!(
                "Unknown clean rule: {}",
                rule
            )));
        }
    }
    Ok(parsed)
}

pub struct LlmIntentParser {
    default_scope: PathBuf,
    provider: Arc<dyn LlmProvider>,
}

impl LlmIntentParser {
    pub fn new(default_scope: PathBuf, provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            default_scope,
            provider,
        }
    }

    pub fn parse(&self, request: &str) -> Result<TaskIntent, LlmError> {
        let messages = vec![
            ChatMessage::system(system_prompt()),
            ChatMessage::user(user_prompt(
                request,
                &self.default_scope.display().to_string(),
            )),
        ];

        let raw_response = self.provider.chat(&messages)?;

        let json_str = extract_json(&raw_response);

        let response: LlmIntentResponse = serde_json::from_str(json_str).map_err(|e| {
            LlmError::InvalidResponse(format!(
                "LLM response does not match expected schema: {}",
                e
            ))
        })?;

        if response.goal.is_empty() {
            return Err(LlmError::InvalidResponse(format!(
                "{}",
                IntentParseError::UnrecognizedIntent
            )));
        }

        let scope = response
            .scope
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_scope.clone());

        let constraints = ConstraintSet {
            preserve_existing_folders: response.constraints.preserve_existing_folders,
            merge_duplicates: response.constraints.merge_duplicates,
            archive_old: response.constraints.archive_old.map(Duration::from_secs),
            auto_delete_temps: response.constraints.auto_delete_temps,
            max_interactive_questions: response.constraints.max_interactive_questions,
        };

        let goal = match response.goal.as_str() {
            "organize" => Goal::Organize {
                scope,
                purpose: response
                    .purpose
                    .unwrap_or_else(|| "general_organization".to_string()),
            },
            "reorganize" => Goal::Reorganize {
                scope,
                strategy: response.strategy,
            },
            "clean" => {
                let rules = response.clean_rules.as_deref().unwrap_or(&[]);
                let parsed_rules = parse_clean_rules(rules)?;
                Goal::Clean {
                    scope,
                    rules: parsed_rules,
                }
            }
            _ => {
                let mut unknown = response.unknown_factors;
                unknown.push(format!("Unknown goal: {}", response.goal));
                return Err(LlmError::InvalidResponse(format!(
                    "Unknown goal '{}'. Unknown factors: {:?}",
                    response.goal, unknown
                )));
            }
        };

        Ok(TaskIntent {
            goal,
            constraints,
            user_hints: response.user_hints,
            unknown_factors: response.unknown_factors,
        })
    }

    #[allow(dead_code)]
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_partial_json_missing_constraints() {
        let partial_response = r#"{"goal": "organize"}"#;
        let response: LlmIntentResponse =
            serde_json::from_str(partial_response).expect("should parse partial JSON");
        assert_eq!(response.goal, "organize");
        assert!(response.constraints.preserve_existing_folders == false);
        assert!(response.unknown_factors.is_empty());
    }

    #[test]
    fn test_parse_partial_json_missing_all_optional() {
        let partial_response = r#"{}"#;
        let response: LlmIntentResponse =
            serde_json::from_str(partial_response).expect("should parse empty JSON");
        assert_eq!(response.goal, "");
    }

    #[test]
    fn test_parse_full_response_still_works() {
        let full_response = r#"{"goal":"organize","scope":null,"purpose":"general_organization","strategy":null,"clean_rules":null,"constraints":{"preserve_existing_folders":true,"merge_duplicates":true,"auto_delete_temps":false,"max_interactive_questions":3},"user_hints":{},"unknown_factors":[]}"#;
        let response: LlmIntentResponse =
            serde_json::from_str(full_response).expect("should parse full JSON");
        assert_eq!(response.goal, "organize");
        assert!(response.constraints.preserve_existing_folders);
        assert!(response.constraints.merge_duplicates);
        assert_eq!(response.constraints.max_interactive_questions, 3);
    }
}
