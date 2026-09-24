use super::*;
use crate::agent::intent::TaskIntentParser;
use crate::agent::validate::PlanValidator;
use crate::agent::proposal_converter::ProposalConverter;
use crate::agent::recommendation::{Recommendation, RecommendationStrategy};
use crate::agent::{EvidenceAnalyzer, PlanGenerator};
use std::fs;
use tempfile::tempdir;

fn make_test_proposal(scope: &Path) -> OrganizationProposal {
    OrganizationProposal {
        strategy: RecommendationStrategy::ByAuthor,
        rationale: "test rationale".to_string(),
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
fn test_1_rename_category() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let refinement = ProposalRefinement::RenameCategory {
        from: "Author A".to_string(),
        to: "作者A".to_string(),
    };

    let refiner = ProposalRefiner;
    let revised = refiner.refine(&proposal, &refinement).unwrap();

    assert_eq!(revised.proposed_categories.len(), 2);
    assert!(revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "作者A"));
    assert!(revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author B"));
    assert!(!revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author A"));
}

#[test]
fn test_2_remove_category() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let refinement = ProposalRefinement::RemoveCategory {
        name: "Author B".to_string(),
    };

    let refiner = ProposalRefiner;
    let revised = refiner.refine(&proposal, &refinement).unwrap();

    assert_eq!(revised.proposed_categories.len(), 1);
    assert!(revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author A"));
    assert!(!revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author B"));
    let author_a = revised
        .proposed_categories
        .iter()
        .find(|c| c.name == "Author A")
        .unwrap();
    assert!(!author_a.source_files.contains(&scope.join("Manga2.cbz")));
}

#[test]
fn test_3_add_category() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let refinement = ProposalRefinement::AddCategory {
        name: "Author C".to_string(),
        purpose: "Works by Author C".to_string(),
    };

    let refiner = ProposalRefiner;
    let revised = refiner.refine(&proposal, &refinement).unwrap();

    assert_eq!(revised.proposed_categories.len(), 3);
    assert!(revised
        .proposed_categories
        .iter()
        .any(|c| c.name == "Author C"));
    let author_c = revised
        .proposed_categories
        .iter()
        .find(|c| c.name == "Author C")
        .unwrap();
    assert!(author_c.source_files.is_empty());
}

#[test]
fn test_4_move_file_between_categories() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);
    let manga1 = scope.join("Manga1.cbz");

    let refinement = ProposalRefinement::MoveFileToCategory {
        file: manga1.clone(),
        category: "Author B".to_string(),
    };

    let refiner = ProposalRefiner;
    let revised = refiner.refine(&proposal, &refinement).unwrap();

    let author_a = revised
        .proposed_categories
        .iter()
        .find(|c| c.name == "Author A")
        .unwrap();
    let author_b = revised
        .proposed_categories
        .iter()
        .find(|c| c.name == "Author B")
        .unwrap();

    assert!(!author_a.source_files.contains(&manga1));
    assert!(author_b.source_files.contains(&manga1));

    let count = revised
        .proposed_categories
        .iter()
        .filter(|c| c.source_files.contains(&manga1))
        .count();
    assert_eq!(count, 1);
}

#[test]
fn test_5_unknown_category_rejected() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);
    let refiner = ProposalRefiner;

    let refinement = ProposalRefinement::RenameCategory {
        from: "NonExistent".to_string(),
        to: "NewName".to_string(),
    };
    let result = refiner.refine(&proposal, &refinement);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        ProposalRefinementError::CategoryNotFound("NonExistent".to_string())
    );

    let refinement = ProposalRefinement::RemoveCategory {
        name: "NonExistent".to_string(),
    };
    let result = refiner.refine(&proposal, &refinement);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        ProposalRefinementError::CategoryNotFound("NonExistent".to_string())
    );
}

#[test]
fn test_6_unknown_file_rejected() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);

    let refinement = ProposalRefinement::MoveFileToCategory {
        file: PathBuf::from("MissingFile.cbz"),
        category: "Author B".to_string(),
    };

    let refiner = ProposalRefiner;
    let result = refiner.refine(&proposal, &refinement);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ProposalRefinementError::FileNotFound(_)
    ));
}

#[test]
fn test_7_duplicate_file_assignment_rejected() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let proposal = make_test_proposal(&scope);
    let manga2 = scope.join("Manga2.cbz");

    let refinement = ProposalRefinement::MoveFileToCategory {
        file: manga2.clone(),
        category: "Author B".to_string(),
    };

    let refiner = ProposalRefiner;
    let result = refiner.refine(&proposal, &refinement);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ProposalRefinementError::FileAlreadyInCategory { .. }
    ));
}

#[test]
fn test_8_original_proposal_remains_unchanged() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();

    let original = make_test_proposal(&scope);
    let original_clone = original.clone();

    let refiner = ProposalRefiner;
    let refined = refiner
        .refine(
            &original,
            &ProposalRefinement::RenameCategory {
                from: "Author A".to_string(),
                to: "作者A".to_string(),
            },
        )
        .unwrap();

    assert_eq!(original, original_clone);
    assert_ne!(original, refined);
    assert!(refined
        .proposed_categories
        .iter()
        .any(|c| c.name == "作者A"));
}

#[test]
fn test_9_revised_proposal_passes_proposal_converter() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "data").unwrap();
    fs::write(scope.join("Manga2.cbz"), "data").unwrap();

    let proposal = make_test_proposal(&scope);

    let refiner = ProposalRefiner;
    let refined = refiner
        .refine(
            &proposal,
            &ProposalRefinement::RenameCategory {
                from: "Author A".to_string(),
                to: "作者A".to_string(),
            },
        )
        .unwrap();

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let converter = ProposalConverter;
    let result = converter.convert(&refined, &analysis, &scope);
    assert!(result.is_ok());
    let (ops, unresolved) = result.unwrap();
    assert!(ops
        .iter()
        .any(|op| matches!(op, crate::agent::FileSystemOperation::CreateDir { .. })));
    assert!(unresolved.is_empty());
}

#[test]
fn test_10_revised_proposal_produces_valid_operation_plan() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "data").unwrap();
    fs::write(scope.join("Manga2.cbz"), "data").unwrap();

    let proposal = make_test_proposal(&scope);

    let refiner = ProposalRefiner;
    let refined = refiner
        .refine(
            &proposal,
            &ProposalRefinement::RenameCategory {
                from: "Author A".to_string(),
                to: "作者A".to_string(),
            },
        )
        .unwrap();

    let recommendation = Recommendation {
        id: "rec-refined".to_string(),
        strategy: RecommendationStrategy::ByAuthor,
        strategy_info: None,
        rationale: "test".to_string(),
        proposed_categories: refined.proposed_categories.clone(),
        proposed_operations: vec![],
        organization_proposal: Some(refined.clone()),
        unresolved_questions: vec![],
        confidence: 0.9,
        constraint_checks: vec![],
        constraint_violation: None,
        warnings: vec![],
        generated_at: 0,
    };

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis).unwrap();

    let validator = PlanValidator::default();
    let validation = validator.validate(&plan);
    assert!(!validation.has_invalid);
    assert!(!validation.has_conflicts);
}

#[test]
fn test_11_existing_validation_still_works() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "data").unwrap();
    fs::write(scope.join("Manga2.cbz"), "data").unwrap();

    let proposal = make_test_proposal(&scope);

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let recommendation = Recommendation {
        id: "rec-original".to_string(),
        strategy: RecommendationStrategy::ByAuthor,
        strategy_info: None,
        rationale: "test".to_string(),
        proposed_categories: proposal.proposed_categories.clone(),
        proposed_operations: vec![],
        organization_proposal: Some(proposal.clone()),
        unresolved_questions: vec![],
        confidence: 0.9,
        constraint_checks: vec![],
        constraint_violation: None,
        warnings: vec![],
        generated_at: 0,
    };

    let generator = PlanGenerator;
    let plan = generator.generate(&recommendation, &analysis).unwrap();

    let validator = PlanValidator::default();
    let validation = validator.validate(&plan);
    assert!(!validation.has_invalid);
}

#[test]
fn test_integration_refinement_to_operation_plan() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Manga1.cbz"), "data").unwrap();
    fs::write(scope.join("Manga2.cbz"), "data").unwrap();

    let original = make_test_proposal(&scope);
    let original_clone = original.clone();

    let refiner = ProposalRefiner;

    let refined = refiner
        .refine(
            &original,
            &ProposalRefinement::RenameCategory {
                from: "Author A".to_string(),
                to: "作者A".to_string(),
            },
        )
        .unwrap();

    let refined = refiner
        .refine(
            &refined,
            &ProposalRefinement::MoveFileToCategory {
                file: scope.join("Manga1.cbz"),
                category: "Author B".to_string(),
            },
        )
        .unwrap();

    let refined = refiner
        .refine(
            &refined,
            &ProposalRefinement::AddCategory {
                name: "Author C".to_string(),
                purpose: "Works by Author C".to_string(),
            },
        )
        .unwrap();

    assert_eq!(original, original_clone);
    assert_ne!(original, refined);

    let intent = TaskIntentParser::new(scope.clone())
        .parse("Organize")
        .unwrap();
    let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

    let recommendation = Recommendation {
        id: "rec-integration".to_string(),
        strategy: RecommendationStrategy::ByAuthor,
        strategy_info: None,
        rationale: "test".to_string(),
        proposed_categories: refined.proposed_categories.clone(),
        proposed_operations: vec![],
        organization_proposal: Some(refined.clone()),
        unresolved_questions: vec![],
        confidence: 0.9,
        constraint_checks: vec![],
        constraint_violation: None,
        warnings: vec![],
        generated_at: 0,
    };

    let converter = ProposalConverter;
    let result = converter.convert_to_plan(&refined, &recommendation, &analysis, &scope);
    assert!(result.is_ok());

    let plan_result = result.unwrap();
    assert!(plan_result.operation_plan.operations.len() >= 2);

    let validator = PlanValidator::default();
    let validation = validator.validate(&plan_result.operation_plan);
    assert!(!validation.has_invalid);
    assert!(!validation.has_conflicts);
}
