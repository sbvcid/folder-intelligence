use crate::agent::proposal_refinement_parser::{ProposalRefinementParser, RefinementParseError};
use crate::agent::proposal_refiner::{ProposalRefinementError, ProposalRefiner};
use crate::agent::recommendation::OrganizationProposal;
use crate::llm::provider::LlmProvider;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RefinementServiceError {
    #[error("Parse error: {0}")]
    Parse(#[from] RefinementParseError),
    #[error("Refinement error: {0}")]
    Refinement(#[from] ProposalRefinementError),
}

pub struct ProposalRefinementService {
    parser: ProposalRefinementParser,
    refiner: ProposalRefiner,
}

impl ProposalRefinementService {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            parser: ProposalRefinementParser::new(provider),
            refiner: ProposalRefiner,
        }
    }

    pub fn refine(
        &self,
        instruction: &str,
        proposal: &OrganizationProposal,
    ) -> Result<OrganizationProposal, RefinementServiceError> {
        let parse_output = self.parser.parse(instruction, proposal)?;
        let refinement = parse_output
            .refinement
            .ok_or(RefinementParseError::NoRefinement)?;
        let revised = self.refiner.refine(proposal, &refinement)?;
        Ok(revised)
    }
}

#[cfg(test)]
#[path = "proposal_refinement_service_tests.rs"]
mod tests;
