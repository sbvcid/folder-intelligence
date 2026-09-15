use crate::classification::ai_provider::{AiClassificationError, AiClassificationRequest};
use crate::classification::input::ClassificationInput;
use crate::classification::result::ClassificationResult;
use crate::classification::AiClassifier;
use std::time::Duration;

const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Debug, Clone)]
pub struct OpenAiProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

impl Default for OpenAiProviderConfig {
    fn default() -> Self {
        Self {
            base_url: OPENAI_DEFAULT_BASE_URL.to_string(),
            api_key: String::new(),
            model: "gpt-4o".to_string(),
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    config: OpenAiProviderConfig,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiProviderConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    response_format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiErrorResponse {
    error: Option<OpenAiErrorDetail>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAiErrorDetail {
    message: String,
    #[serde(default)]
    r#type: Option<String>,
}

fn build_system_prompt() -> String {
    r#"You are an evidence-driven filesystem classification assistant.

Your task: classify the target directory based on filesystem evidence compared to candidate directories.

Process:
1. Analyze the target's DirectoryEvidence (extensions, file counts, identifiers, structure)
2. Compare against provided candidate evidence (each candidate's own evidence + sampled children)
3. Review the rule-based baseline decision and confidence
4. Produce a ClassificationResult in JSON

Constraints:
- confidence must be within 0.2 of the baseline confidence
- selected_candidate must be an exact match to one of the provided candidate paths
- If you cannot confidently decide, use leave_unclassified or ask_user
- Never fabricate candidate paths or create fake evidence

If the target has partial_scan=true in evidence, NEVER choose move_existing.

Output ONLY valid JSON matching the ClassificationResult schema. No prose, no markdown fences."#
        .to_string()
}

fn build_user_prompt(request: &AiClassificationRequest) -> String {
    let target_json = serde_json::to_string_pretty(&request.target_evidence).unwrap_or_else(|_| "{}".to_string());
    let candidates_json = serde_json::to_string_pretty(&request.candidates).unwrap_or_else(|_| "[]".to_string());
    let baseline_json = serde_json::to_string_pretty(&request.rule_based_result).unwrap_or_else(|_| "{}".to_string());

    format!(
        r#"Here is the classification input:

TARGET EVIDENCE:
```json
{target_json}
```

CANDIDATE EVIDENCE (each candidate directory + sampled children):
```json
{candidates_json}
```

RULE-BASED BASELINE RESULT:
```json
{baseline_json}
```

CONSTRAINTS:
- max_confidence_delta: {delta}
- allowed_decisions: {decisions}
- If selected_candidate is set, it must exactly match a candidate path
- partial_scan target → never move_existing

Output JSON:"#,
        target_json = target_json,
        candidates_json = candidates_json,
        baseline_json = baseline_json,
        delta = request.constraints.max_confidence_delta,
        decisions = serde_json::to_string(&request.constraints.allowed_decisions).unwrap_or_default(),
    )
}

impl AiClassifier for OpenAiProvider {
    fn classify(
        &self,
        input: &ClassificationInput,
        baseline: &ClassificationResult,
    ) -> Result<ClassificationResult, AiClassificationError> {
        let constraints = crate::classification::AiClassificationConstraints::default();
        let request = AiClassificationRequest {
            target_evidence: input.target.clone(),
            candidates: input.candidates.clone(),
            rule_based_result: baseline.clone(),
            constraints,
        };

        let system_prompt = build_system_prompt();
        let user_prompt = build_user_prompt(&request);

        let http_request = OpenAiChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
            response_format: Some(serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "classification_result",
                    "schema": {
                        "type": "object",
                        "required": ["target_path", "decision", "confidence", "confidence_band", "candidates_considered", "supporting_evidence", "alternatives", "uncertainty", "warnings", "classified_at", "schema_version"],
                        "properties": {
                            "target_path": {"type": "string"},
                            "decision": {"type": "string", "enum": ["move_existing", "create_category", "leave_unclassified", "ask_user"]},
                            "selected_candidate": {"type": ["string", "null"]},
                            "proposed_category_name": {"type": ["string", "null"]},
                            "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                            "confidence_band": {"type": "string", "enum": ["high", "medium", "low"]},
                            "candidates_considered": {"type": "integer", "minimum": 0},
                            "supporting_evidence": {"type": "array"},
                            "alternatives": {"type": "array"},
                            "uncertainty": {"type": "array"},
                            "warnings": {"type": "array"},
                            "classified_at": {"type": "integer"},
                            "schema_version": {"type": "string"},
                            "provider": {"type": ["string", "null"]},
                            "model": {"type": ["string", "null"]}
                        }
                    }
                }
            })),
        };

        let response = self.send_request(http_request)?;
        let result: ClassificationResult = response;

        self.validate_result(&result, &request)?;

        Ok(result)
    }
}

impl OpenAiProvider {
    fn send_request(
        &self,
        request: OpenAiChatRequest,
    ) -> Result<ClassificationResult, AiClassificationError> {
        #[cfg(not(feature = "network"))]
        {
            return Err(AiClassificationError::ProviderError(
                "Network feature not enabled. Install with --features network".to_string(),
            ));
        }

        #[cfg(feature = "network")]
        {
            let http_client = reqwest::blocking::Client::builder()
                .timeout(self.config.timeout)
                .build()
                .map_err(|e| AiClassificationError::ProviderError(format!("HTTP client error: {}", e)))?;

            let response = http_client
                .post(&self.config.base_url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .map_err(|e| {
                    if e.is_timeout() {
                        AiClassificationError::Timeout
                    } else {
                        AiClassificationError::ProviderError(format!("Request error: {}", e))
                    }
                })?;

            let status = response.status();
            let body = response.text().map_err(|e| {
                AiClassificationError::ProviderError(format!("Response read error: {}", e))
            })?;

            if !status.is_success() {
                let error: Option<OpenAiErrorResponse> = serde_json::from_str(&body).ok();
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    return Err(AiClassificationError::Unauthorized);
                }
                if status.as_u16() == 429 {
                    return Err(AiClassificationError::RateLimited);
                }
                let msg = error
                    .and_then(|e| e.error)
                    .map(|d| d.message)
                    .unwrap_or_else(|| format!("HTTP {}: {}", status, body));
                return Err(AiClassificationError::ProviderError(msg));
            }

            let chat_response: OpenAiChatResponse = serde_json::from_str(&body)
                .map_err(|e| AiClassificationError::InvalidResponse(format!("Response parse error: {}", e)))?;

            let content = chat_response
                .choices
                .first()
                .ok_or_else(|| AiClassificationError::InvalidResponse("No choices in response".to_string()))?
                .message
                .content
                .clone();

            let result: ClassificationResult = serde_json::from_str(&content)
                .map_err(|e| AiClassificationError::InvalidResponse(format!("JSON deserialize error: {}", e)))?;

            Ok(result)
        }
    }

    fn validate_result(
        &self,
        result: &ClassificationResult,
        request: &AiClassificationRequest,
    ) -> Result<(), AiClassificationError> {
        crate::classification::validate_ai_result(result, request)?;

        self.validate_no_hallucinated_candidates(result, request)?;

        Ok(())
    }

    fn validate_no_hallucinated_candidates(
        &self,
        result: &ClassificationResult,
        request: &AiClassificationRequest,
    ) -> Result<(), AiClassificationError> {
        let valid_candidate_paths: std::collections::HashSet<std::path::PathBuf> = request
            .candidates
            .iter()
            .map(|c| c.directory.path.clone())
            .collect();

        if let Some(ref selected) = result.selected_candidate {
            if !valid_candidate_paths.contains(selected) {
                return Err(AiClassificationError::InvalidResponse(format!(
                    "Hallucinated candidate path: {} not in provided candidates",
                    selected.display()
                )));
            }
        }

        let valid_candidate_names: std::collections::HashSet<String> = request
            .candidates
            .iter()
            .map(|c| c.directory.name.clone())
            .collect();
        let target_name = &request.target_evidence.name;

        for alt in &result.alternatives {
            if !valid_candidate_paths.contains(&alt.candidate_path) {
                return Err(AiClassificationError::InvalidResponse(format!(
                    "Hallucinated alternative path: {}",
                    alt.candidate_path.display()
                )));
            }
        }

        if let Some(ref proposed) = result.proposed_category_name {
            if !valid_candidate_names.contains(proposed) && proposed != target_name {
                return Err(AiClassificationError::InvalidResponse(format!(
                    "Proposed category name '{}' does not match any candidate or target",
                    proposed
                )));
            }
        }

        Ok(())
    }
}
