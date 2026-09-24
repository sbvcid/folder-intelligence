use super::*;
use crate::agent::intent::TaskIntentParser;
use crate::agent::proposal_converter::ProposalConverter;
use crate::agent::proposal_refiner::ProposalRefiner;
use crate::agent::recommendation::{ProposedCategory, Recommendation, RecommendationStrategy};
use crate::agent::validate::PlanValidator;
use crate::agent::EvidenceAnalyzer;
use crate::llm::MockLlmProvider;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn make_test_proposal(scope: &Path) -> OrganizationProposal {
    OrganizationProposal {
        strategy: RecommendationStrategy::ByAuthor,
        rationale: "Group manga by author".to_string(),
        proposed_categories: vec![
            ProposedCategory {
                name: "Author A".to_string(),
                purpose: "Works by Author A".to_string(),
                target_content_types: vec!["cbz".to_string()],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz")],
            },
            ProposedCategory {
                name: "Author B".to_string(),
                purpose: "Works by Author B".to_string(),
                target_content_types: vec!["cbz".to_string()],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga2.cbz")],
            },
        ],
        evidence_gaps: vec![],
        ambiguities: vec![],
    }
}

fn make_parser(responses: Vec<String>) -> (ProposalRefinementParser, Arc<dyn LlmProvider>) {
    let provider = Arc::new(MockLlmProvider::new(responses));
    let parser = ProposalRefinementParser::new(provider.clone());
    (parser, provider)
}

#[test]
fn test_rename_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": {
            "type": "rename_category",
            "from": "Author A",
            "to": "作者A"
        },
        "confidence": 0.95,
        "reason": "User wants to rename Author A to 作者A"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("把 Author A 改成 作者A", &proposal).unwrap();

    assert!(result.refinement.is_some());
    let refinement = result.refinement.unwrap();
    assert!(matches!(
        refinement,
        ProposalRefinement::RenameCategory { from, to }
            if from == "Author A" && to == "作者A"
    ));
    assert_eq!(result.confidence, 0.95);
}

#[test]
fn test_remove_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": {
            "type": "remove_category",
            "name": "Author B"
        },
        "confidence": 0.9,
        "reason": "User wants to remove the Author B category"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("刪除 Author B 分類", &proposal).unwrap();

    assert!(result.refinement.is_some());
    let refinement = result.refinement.unwrap();
    assert!(matches!(
        refinement,
        ProposalRefinement::RemoveCategory { name }
            if name == "Author B"
    ));
}

#[test]
fn test_add_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": {
            "type": "add_category",
            "name": "Author C",
            "purpose": "Works by Author C"
        },
        "confidence": 0.9,
        "reason": "User wants to add a new category for Author C"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("新增一個 Author C 分類", &proposal).unwrap();

    assert!(result.refinement.is_some());
    let refinement = result.refinement.unwrap();
    assert!(matches!(
        refinement,
        ProposalRefinement::AddCategory { name, purpose }
            if name == "Author C" && purpose == "Works by Author C"
    ));
}

#[test]
fn test_move_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": {
            "type": "move_file_to_category",
            "file": "Manga1.cbz",
            "category": "Author B"
        },
        "confidence": 0.95,
        "reason": "User wants to move Manga1.cbz to Author B"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser
        .parse("把 Manga1.cbz 移到 Author B", &proposal)
        .unwrap();

    assert!(result.refinement.is_some());
    let refinement = result.refinement.unwrap();
    assert!(matches!(
        refinement,
        ProposalRefinement::MoveFileToCategory { file, category }
            if file == PathBuf::from("Manga1.cbz") && category == "Author B"
    ));
}

#[test]
fn test_unknown_category_returns_no_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": null,
        "confidence": 0.2,
        "reason": "Category 'Nonexistent' does not exist in the proposal"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("把 Nonexistent 改成 ABC", &proposal);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RefinementParseError::NoRefinement
    ));
}

#[test]
fn test_unknown_file_returns_no_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": null,
        "confidence": 0.2,
        "reason": "File 'DoesNotExist.cbz' does not exist in the proposal"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("把 DoesNotExist.cbz 移到 Author A", &proposal);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RefinementParseError::NoRefinement
    ));
}

#[test]
fn test_ambiguous_request_returns_ambiguous_error() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": null,
        "confidence": 0.1,
        "reason": "The request is ambiguous; multiple categories could be the target"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("把那個分類改掉", &proposal);

    assert!(result.is_err());
    match result.unwrap_err() {
        RefinementParseError::AmbiguousRefinement(reason) => {
            assert!(reason.contains("ambiguous"));
        }
        other => panic!("expected AmbiguousRefinement, got: {:?}", other),
    }
}

#[test]
fn test_compound_request_returns_no_refinement() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": null,
        "confidence": 0.2,
        "reason": "The request contains multiple changes which is not supported in a single refinement"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let result = parser.parse("把 A 改成 B，然後把 C 移到 D", &proposal);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RefinementParseError::NoRefinement
    ));
}

#[test]
fn test_malformed_json_returns_error() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let (parser, _) = make_parser(vec!["not valid json at all".to_string()]);
    let result = parser.parse("把 Author A 改名", &proposal);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RefinementParseError::InvalidStructuredOutput(_)
    ));
}

#[test]
fn test_provider_error_returns_error() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let provider: Arc<dyn LlmProvider> = Arc::new(ErrorTestProvider);
    let parser = ProposalRefinementParser::new(provider);
    let result = parser.parse("把 Author A 改成 作者A", &proposal);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RefinementParseError::ProviderError(_)
    ));
}

struct ErrorTestProvider;

impl LlmProvider for ErrorTestProvider {
    fn chat(&self, _messages: &[ChatMessage]) -> Result<String, LlmError> {
        Err(LlmError::ProviderError("Test provider error".to_string()))
    }

    fn name(&self) -> &str {
        "error-test"
    }
}

#[test]
fn test_integration_natural_language_to_operation_plan() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "data").unwrap();
    fs::write(scope.join("Manga2.cbz"), "data").unwrap();

    let original_proposal = make_test_proposal(&scope);

    let json_response = r#"{
        "refinement": {
            "type": "rename_category",
            "from": "Author A",
            "to": "作者A"
        },
        "confidence": 0.95,
        "reason": "User wants to rename Author A to 作者A"
    }"#;

    let (parser, _) = make_parser(vec![json_response.to_string()]);
    let parse_output = parser
        .parse("把 Author A 改成 作者A", &original_proposal)
        .unwrap();

    assert!(parse_output.refinement.is_some());

    let refinement = parse_output.refinement.unwrap();

    let refiner = ProposalRefiner;
    let refined_proposal = refiner.refine(&original_proposal, &refinement).unwrap();

    assert_ne!(original_proposal, refined_proposal);
    assert!(refined_proposal
        .proposed_categories
        .iter()
        .any(|c| c.name == "作者A"));

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let recommendation = Recommendation {
        id: "rec-integration".to_string(),
        strategy: RecommendationStrategy::ByAuthor,
        strategy_info: None,
        rationale: "test".to_string(),
        proposed_categories: refined_proposal.proposed_categories.clone(),
        proposed_operations: vec![],
        organization_proposal: Some(refined_proposal.clone()),
        unresolved_questions: vec![],
        confidence: 0.9,
        constraint_checks: vec![],
        constraint_violation: None,
        warnings: vec![],
        generated_at: 0,
    };

    let converter = ProposalConverter;
    let result = converter.convert_to_plan(&refined_proposal, &recommendation, &analysis, &scope);
    assert!(result.is_ok());

    let plan_result = result.unwrap();
    assert!(!plan_result.operation_plan.operations.is_empty());

    let validator = PlanValidator::default();
    let validation = validator.validate(&plan_result.operation_plan);
    assert!(!validation.has_invalid);
    assert!(!validation.has_conflicts);
}
