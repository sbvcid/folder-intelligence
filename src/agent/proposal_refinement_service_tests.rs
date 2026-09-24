use super::*;
use crate::agent::proposal_converter::ProposalConverter;
use crate::agent::recommendation::{ProposedCategory, RecommendationStrategy};
use crate::agent::{EvidenceAnalyzer, TaskIntentParser};
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

#[test]
fn test_service_rename_refinement_succeeds() {
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
        "reason": "Rename Author A to 作者A"
    }"#;

    let provider = Arc::new(MockLlmProvider::new(vec![json_response.to_string()]));
    let service = ProposalRefinementService::new(provider);

    let revised = service.refine("把 Author A 改成 作者A", &proposal).unwrap();

    assert_eq!(revised.proposed_categories.len(), 2);
    assert!(revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "作者A"));
    assert!(!revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author A"));
}

#[test]
fn test_service_move_file_refinement_succeeds() {
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
        "reason": "Move Manga1.cbz to Author B"
    }"#;

    let provider = Arc::new(MockLlmProvider::new(vec![json_response.to_string()]));
    let service = ProposalRefinementService::new(provider);

    let revised = service
        .refine("把 Manga1.cbz 移到 Author B", &proposal)
        .unwrap();

    let author_b = revised
        .proposed_categories
        .iter()
        .find(|c| c.name == "Author B")
        .unwrap();
    assert_eq!(author_b.source_files.len(), 2);
}

#[test]
fn test_service_ambiguous_refinement_fails_and_leaves_proposal_intact() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);
    let json_response = r#"{
        "refinement": null,
        "confidence": 0.2,
        "reason": "ambiguous request"
    }"#;

    let provider = Arc::new(MockLlmProvider::new(vec![json_response.to_string()]));
    let service = ProposalRefinementService::new(provider);

    let res = service.refine("把那個分類改掉", &proposal);
    assert!(res.is_err());

    // Original proposal is intact (passed by reference, never mutated)
    assert_eq!(proposal.proposed_categories[0].name, "Author A");
}

#[test]
fn test_service_invalid_refinement_fails() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);
    // Trying to rename a non-existent category
    let json_response = r#"{
        "refinement": {
            "type": "rename_category",
            "from": "NonExistent",
            "to": "NewName"
        },
        "confidence": 0.95,
        "reason": "rename non-existent"
    }"#;

    let provider = Arc::new(MockLlmProvider::new(vec![json_response.to_string()]));
    let service = ProposalRefinementService::new(provider);

    let res = service.refine("把 NonExistent 改成 NewName", &proposal);
    assert!(res.is_err());
}

#[test]
fn test_service_revised_proposal_to_converter() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "test").unwrap();
    fs::write(scope.join("Manga2.cbz"), "test").unwrap();

    let proposal = make_test_proposal(&scope);
    let json_response = r#"{
        "refinement": {
            "type": "rename_category",
            "from": "Author A",
            "to": "作者A"
        },
        "confidence": 0.95,
        "reason": "rename Author A"
    }"#;

    let provider = Arc::new(MockLlmProvider::new(vec![json_response.to_string()]));
    let service = ProposalRefinementService::new(provider);

    let revised = service.refine("把 Author A 改成 作者A", &proposal).unwrap();

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let converter = ProposalConverter;
    let recommendation = crate::agent::Recommendation {
        id: "rec-1".to_string(),
        strategy: RecommendationStrategy::ByAuthor,
        strategy_info: None,
        rationale: "test".to_string(),
        proposed_categories: revised.proposed_categories.clone(),
        proposed_operations: vec![],
        organization_proposal: Some(revised.clone()),
        unresolved_questions: vec![],
        confidence: 0.9,
        constraint_checks: vec![],
        constraint_violation: None,
        warnings: vec![],
        generated_at: 0,
    };

    let conversion = converter.convert_to_plan(&revised, &recommendation, &analysis, &scope);
    assert!(
        conversion.is_ok(),
        "Revised proposal should successfully convert to plan: {:?}",
        conversion.err()
    );
}
