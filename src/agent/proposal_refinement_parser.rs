use crate::agent::proposal_refiner::ProposalRefinement;
use crate::agent::recommendation::OrganizationProposal;
use crate::llm::provider::{ChatMessage, LlmError, LlmProvider};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlmRefinementOutput {
    RenameCategory { from: String, to: String },
    RemoveCategory { name: String },
    AddCategory { name: String, purpose: String },
    MoveFileToCategory { file: String, category: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmRefinementResponse {
    #[serde(default)]
    pub refinement: Option<LlmRefinementOutput>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefinementParseOutput {
    pub refinement: Option<ProposalRefinement>,
    pub confidence: f64,
    pub reason: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RefinementParseError {
    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Invalid structured output: {0}")]
    InvalidStructuredOutput(String),

    #[error("No refinement could be parsed")]
    NoRefinement,

    #[error("Ambiguous refinement: {0}")]
    AmbiguousRefinement(String),

    #[error("Unsupported refinement: {0}")]
    UnsupportedRefinement(String),

    #[error("Refinement error: {0}")]
    RefinementError(String),
}

impl From<LlmError> for RefinementParseError {
    fn from(e: LlmError) -> Self {
        RefinementParseError::ProviderError(e.to_string())
    }
}

pub struct ProposalRefinementParser {
    provider: Arc<dyn LlmProvider>,
}

impl ProposalRefinementParser {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub fn parse(
        &self,
        user_request: &str,
        proposal: &OrganizationProposal,
    ) -> Result<RefinementParseOutput, RefinementParseError> {
        let messages = vec![
            ChatMessage::system(system_prompt()),
            ChatMessage::user(user_prompt(user_request, proposal)),
        ];

        let raw_response = self
            .provider
            .chat(&messages)
            .map_err(RefinementParseError::from)?;

        let json_str = extract_json(&raw_response);

        let llm_response: LlmRefinementResponse = serde_json::from_str(json_str).map_err(|e| {
            RefinementParseError::InvalidStructuredOutput(format!(
                "Failed to parse LLM response as JSON: {}",
                e
            ))
        })?;

        if llm_response.refinement.is_none() {
            let reason = llm_response
                .reason
                .unwrap_or_else(|| "No refinement specified".to_string());
            let reason_lower = reason.to_lowercase();
            if reason_lower.contains("ambiguous") {
                return Err(RefinementParseError::AmbiguousRefinement(reason));
            }
            return Err(RefinementParseError::NoRefinement);
        }

        let llm_refinement = llm_response.refinement.unwrap();
        let refinement = Self::convert_llm_refinement(llm_refinement)?;

        Ok(RefinementParseOutput {
            refinement: Some(refinement),
            confidence: llm_response.confidence,
            reason: llm_response.reason,
        })
    }

    fn convert_llm_refinement(
        llm_refinement: LlmRefinementOutput,
    ) -> Result<ProposalRefinement, RefinementParseError> {
        match llm_refinement {
            LlmRefinementOutput::RenameCategory { from, to } => {
                Ok(ProposalRefinement::RenameCategory { from, to })
            }
            LlmRefinementOutput::RemoveCategory { name } => {
                Ok(ProposalRefinement::RemoveCategory { name })
            }
            LlmRefinementOutput::AddCategory { name, purpose } => {
                Ok(ProposalRefinement::AddCategory { name, purpose })
            }
            LlmRefinementOutput::MoveFileToCategory { file, category } => {
                Ok(ProposalRefinement::MoveFileToCategory {
                    file: PathBuf::from(file),
                    category,
                })
            }
        }
    }
}

fn system_prompt() -> &'static str {
    r#"You are a proposal refinement parser. Your task is to parse a user's natural language request into a single structured refinement of the current OrganizationProposal.

The current proposal (as JSON) will be provided in the user message. Only reference categories and files that actually exist in that proposal. Do NOT hallucinate categories, authors, or files that are not present.

Output exactly ONE refinement per request. If the user asks for multiple changes (compound), or if the request is ambiguous, set "refinement" to null and explain why.

Valid refinement types (choose exactly one, or null):
1. rename_category: {"type":"rename_category", "from":"existing_name", "to":"new_name"}
2. remove_category: {"type":"remove_category", "name":"category_name"}
3. add_category: {"type":"add_category", "name":"new_name", "purpose":"description"}
4. move_file_to_category: {"type":"move_file_to_category", "file":"filename.ext", "category":"target_category_name"}

Output format (JSON only, no prose):
{
  "refinement": { ... } | null,
  "confidence": 0.95,
  "reason": "brief explanation"
}"#
}

fn user_prompt(user_request: &str, proposal: &OrganizationProposal) -> String {
    let proposal_json = serde_json::to_string_pretty(proposal)
        .unwrap_or_else(|_| "{\"proposed_categories\":[]}".to_string());

    format!(
        r#"Current OrganizationProposal:
{}

User request: "{}"

Parse the user request into a single structured refinement. Respond ONLY with the JSON object."#,
        proposal_json, user_request
    )
}

fn extract_json(text: &str) -> &str {
    let start = text.find('{').unwrap_or(0);
    let end = text.rfind('}').map(|i| i + 1).unwrap_or(text.len());
    &text[start..end]
}

#[cfg(test)]
#[path = "proposal_refinement_parser_tests.rs"]
mod tests;
