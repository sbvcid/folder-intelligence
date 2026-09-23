use super::*;
use crate::agent::intent::TaskIntentParser;
use crate::agent::recommendation::{RecommendationEngine, RecommendationStrategy};
use crate::agent::{
    ClarificationEngine, ClarificationError, DecisionAnswer, DecisionCategory, EvidenceAnalyzer,
    Goal, UserDecision,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;

fn create_test_scope(dir: &tempfile::TempDir) -> PathBuf {
    let scope = dir.path().join("downloads");
    fs::create_dir_all(&scope).unwrap();

    fs::create_dir_all(scope.join("documents")).unwrap();
    fs::create_dir_all(scope.join("images")).unwrap();

    fs::write(scope.join("documents").join("doc1.pdf"), "content").unwrap();
    fs::write(scope.join("documents").join("doc2.docx"), "content").unwrap();
    fs::write(scope.join("images").join("photo1.jpg"), "img").unwrap();
    fs::write(scope.join("images").join("photo2.png"), "img").unwrap();
    fs::write(scope.join("archive.zip"), "data").unwrap();
    fs::write(scope.join("readme.txt"), "text").unwrap();

    scope
}

#[test]
fn test_clarification_start_returns_questions() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize this folder").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let clarifier = ClarificationEngine;
    let questions = clarifier.start(&recommendation);

    // "Organize this folder" without specifying purpose → taxonomy gap
    assert!(!questions.is_empty());
    for q in &questions {
        assert!(!q.id.is_empty());
        assert!(!q.question.is_empty());
        assert!(!q.options.is_empty());
    }
}

#[test]
fn test_clarification_complete_when_no_questions() {
    // Build a recommendation with no unresolved questions manually
    let recommendation = Recommendation {
        id: "test-rec".to_string(),
        strategy: RecommendationStrategy::CategoryBased,
        strategy_info: None,
        rationale: "Test rationale".to_string(),
        proposed_categories: Vec::new(),
        proposed_operations: Vec::new(),
        unresolved_questions: Vec::new(),
        confidence: 0.9,
        constraint_checks: Vec::new(),
        constraint_violation: None,
        warnings: Vec::new(),
        organization_proposal: None,
        generated_at: 0,
    };

    let clarifier = ClarificationEngine;
    assert!(clarifier.is_complete(&recommendation));
}

#[test]
fn test_apply_preserve_existing_folders() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    assert!(!intent.constraints.preserve_existing_folders); // default is false for "by category"

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::PreserveExistingFolders,
        answer: DecisionAnswer::Yes,
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let updated = clarifier.apply_decisions(&intent, &decisions);
    assert!(updated.constraints.preserve_existing_folders);
}

#[test]
fn test_apply_auto_delete_temps() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    assert!(!intent.constraints.auto_delete_temps);

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::AutoDeleteTemps,
        answer: DecisionAnswer::Yes,
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let updated = clarifier.apply_decisions(&intent, &decisions);
    assert!(updated.constraints.auto_delete_temps);
}

#[test]
fn test_apply_no_auto_delete_temps() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::AutoDeleteTemps,
        answer: DecisionAnswer::No,
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let updated = clarifier.apply_decisions(&intent, &decisions);
    assert!(!updated.constraints.auto_delete_temps);
}

#[test]
fn test_apply_archive_old_duration() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    assert!(intent.constraints.archive_old.is_none());

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::ArchiveOld,
        answer: DecisionAnswer::Duration(Duration::from_secs(90 * 86400)),
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let updated = clarifier.apply_decisions(&intent, &decisions);
    assert!(updated.constraints.archive_old.is_some());
    assert_eq!(
        updated.constraints.archive_old.unwrap().as_secs(),
        90 * 86400
    );
}

#[test]
fn test_apply_taxonomy_choice() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize this folder").expect("should parse");

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::TaxonomyChoice,
        answer: DecisionAnswer::Choice("by_project".to_string()),
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let updated = clarifier.apply_decisions(&intent, &decisions);

    match &updated.goal {
        Goal::Organize { purpose, .. } => {
            assert_eq!(purpose, "by_project");
        }
        _ => panic!("expected Organize goal"),
    }
}

#[test]
fn test_resolve_question_valid_choice() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "What category structure do you prefer?".to_string(),
        context: "Taxonomy not specified".to_string(),
        options: vec!["by_category".to_string(), "by_project".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier.resolve_question(
        &[question],
        "q1",
        DecisionAnswer::Choice("by_project".to_string()),
    );

    assert!(decision.is_ok());
    let d = decision.unwrap();
    assert_eq!(d.category, DecisionCategory::TaxonomyChoice);
    assert_eq!(d.answer, DecisionAnswer::Choice("by_project".to_string()));
}

#[test]
fn test_resolve_question_valid_yes_no() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "Preserve existing folders?".to_string(),
        context: "User constraint".to_string(),
        options: vec!["Yes".to_string(), "No".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier.resolve_question(&[question], "q1", DecisionAnswer::Yes);

    assert!(decision.is_ok());
    let d = decision.unwrap();
    assert_eq!(d.category, DecisionCategory::PreserveExistingFolders);
    assert_eq!(d.answer, DecisionAnswer::Yes);
}

#[test]
fn test_resolve_question_invalid_choice() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "What category structure?".to_string(),
        context: "Taxonomy not specified".to_string(),
        options: vec!["by_category".to_string(), "by_project".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier.resolve_question(
        &[question],
        "q1",
        DecisionAnswer::Choice("invalid_option".to_string()),
    );

    assert!(matches!(
        decision,
        Err(ClarificationError::InvalidAnswer(_))
    ));
}

#[test]
fn test_resolve_question_not_found() {
    let clarifier = ClarificationEngine;
    let decision = clarifier.resolve_question(&[], "nonexistent", DecisionAnswer::Yes);

    assert!(matches!(
        decision,
        Err(ClarificationError::QuestionNotFound(_))
    ));
}

#[test]
fn test_resolve_question_yes_no_for_non_constraint() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "What category structure?".to_string(),
        context: "Taxonomy not specified".to_string(),
        options: vec!["by_category".to_string(), "by_project".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier.resolve_question(&[question], "q1", DecisionAnswer::Yes);

    // TaxonomyChoice doesn't map to a constraint, so Yes/No is invalid
    assert!(matches!(
        decision,
        Err(ClarificationError::InvalidAnswer(_))
    ));
}

#[test]
fn test_recompute_recommendation_after_decision() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize this folder").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let initial_rec = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Initially has questions
    assert!(!initial_rec.unresolved_questions.is_empty());

    // Apply a decision to specify taxonomy
    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::TaxonomyChoice,
        answer: DecisionAnswer::Choice("by_category".to_string()),
        question_id: "gap-1".to_string(),
        rationale: None,
    }];

    let updated_intent = clarifier.apply_decisions(&intent, &decisions);
    let updated_rec = clarifier.recompute_recommendation(&updated_intent, &analysis);

    assert!(updated_rec.is_ok());
    let rec = updated_rec.unwrap();

    // After specifying taxonomy, fewer questions should remain
    assert!(rec.unresolved_questions.len() <= initial_rec.unresolved_questions.len());
}

#[test]
fn test_blocked_operations_visible() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    // Create temp files
    fs::write(scope.join("temp1.tmp"), "temp").unwrap();
    fs::write(scope.join("temp2.tmp"), "temp").unwrap();

    let parser = TaskIntentParser::new(scope.clone());
    // Don't allow temp deletion
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let clarifier = ClarificationEngine;
    let blocked = clarifier.blocked_operations(&recommendation);

    // blocked_operations should be empty since no constraint violation
    // (the constraint enforcement blocks temp operations silently)
    // But the constraint violation field tells us what was blocked
    assert!(blocked.is_empty() || recommendation.constraint_violation.is_some());
}

#[test]
fn test_summarize_output() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let clarifier = ClarificationEngine;
    let summary = clarifier.summarize(&recommendation);

    assert!(summary.contains("Strategy"));
    assert!(summary.contains("Rationale"));
    assert!(summary.contains("Confidence"));
}

#[test]
fn test_apply_evidence_gaps_to_questions() {
    let gaps = vec![
        EvidenceGap {
            gap_type: crate::agent::analysis::GapType::Taxonomy,
            description: "General organization requested".to_string(),
            requires_user_input: true,
        },
        EvidenceGap {
            gap_type: crate::agent::analysis::GapType::FileDisposition,
            description: "File disposition policy not specified".to_string(),
            requires_user_input: true,
        },
    ];

    let questions = apply_evidence_gaps_to_questions(&gaps);

    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0].id, "gap-1");
    assert_eq!(questions[1].id, "gap-2");
    assert!(!questions[0].options.is_empty());
    assert!(!questions[1].options.is_empty());
}

#[test]
fn test_apply_evidence_gaps_skip_non_required() {
    let gaps = vec![
        EvidenceGap {
            gap_type: crate::agent::analysis::GapType::Taxonomy,
            description: "General organization requested".to_string(),
            requires_user_input: true,
        },
        EvidenceGap {
            gap_type: crate::agent::analysis::GapType::Taxonomy,
            description: "Another taxonomy gap".to_string(),
            requires_user_input: false,
        },
    ];

    let questions = apply_evidence_gaps_to_questions(&gaps);

    assert_eq!(questions.len(), 1);
}

#[test]
fn test_decision_category_display_name() {
    assert_eq!(
        DecisionCategory::PreserveExistingFolders.display_name(),
        "Preserve existing folders"
    );
    assert_eq!(
        DecisionCategory::AutoDeleteTemps.display_name(),
        "Auto-delete temporary files"
    );
    assert_eq!(
        DecisionCategory::TaxonomyChoice.display_name(),
        "Category structure"
    );
}

#[test]
fn test_decision_category_maps_to_constraint() {
    assert_eq!(
        DecisionCategory::PreserveExistingFolders.maps_to_constraint(),
        Some("preserve_existing_folders")
    );
    assert_eq!(
        DecisionCategory::AutoDeleteTemps.maps_to_constraint(),
        Some("auto_delete_temps")
    );
    assert_eq!(
        DecisionCategory::MergeDuplicates.maps_to_constraint(),
        Some("merge_duplicates")
    );
    assert_eq!(
        DecisionCategory::ArchiveOld.maps_to_constraint(),
        Some("archive_old")
    );
    assert_eq!(DecisionCategory::TaxonomyChoice.maps_to_constraint(), None);
}

#[test]
fn test_apply_multiple_decisions() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let clarifier = ClarificationEngine;
    let decisions = vec![
        UserDecision {
            category: DecisionCategory::PreserveExistingFolders,
            answer: DecisionAnswer::Yes,
            question_id: "q1".to_string(),
            rationale: None,
        },
        UserDecision {
            category: DecisionCategory::AutoDeleteTemps,
            answer: DecisionAnswer::No,
            question_id: "q2".to_string(),
            rationale: None,
        },
        UserDecision {
            category: DecisionCategory::MergeDuplicates,
            answer: DecisionAnswer::Yes,
            question_id: "q3".to_string(),
            rationale: None,
        },
    ];

    let updated = clarifier.apply_decisions(&intent, &decisions);
    assert!(updated.constraints.preserve_existing_folders);
    assert!(!updated.constraints.auto_delete_temps);
    assert!(updated.constraints.merge_duplicates);
}

#[test]
fn test_apply_decisions_does_not_modify_original() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let original_preserve = intent.constraints.preserve_existing_folders;

    let clarifier = ClarificationEngine;
    let decisions = vec![UserDecision {
        category: DecisionCategory::PreserveExistingFolders,
        answer: DecisionAnswer::Yes,
        question_id: "q1".to_string(),
        rationale: None,
    }];

    let _updated = clarifier.apply_decisions(&intent, &decisions);

    // Original intent should not be modified
    assert_eq!(
        intent.constraints.preserve_existing_folders,
        original_preserve
    );
}

#[test]
fn test_user_decision_serialization() {
    let decision = UserDecision {
        category: DecisionCategory::PreserveExistingFolders,
        answer: DecisionAnswer::Yes,
        question_id: "q1".to_string(),
        rationale: Some("User wants to keep existing structure".to_string()),
    };

    let json = serde_json::to_string(&decision).expect("should serialize");
    let deserialized: UserDecision = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(decision, deserialized);
}

#[test]
fn test_decision_answer_as_bool() {
    assert_eq!(DecisionAnswer::Yes.as_bool(), Some(true));
    assert_eq!(DecisionAnswer::No.as_bool(), Some(false));
    assert_eq!(DecisionAnswer::Choice("test".to_string()).as_bool(), None);
}

#[test]
fn test_decision_answer_as_choice() {
    assert_eq!(
        DecisionAnswer::Choice("by_category".to_string()).as_choice(),
        Some("by_category")
    );
    assert_eq!(DecisionAnswer::Yes.as_choice(), None);
}

#[test]
fn test_infer_category_from_question() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "Preserve existing folders?".to_string(),
        context: "User constraint".to_string(),
        options: vec!["Yes".to_string(), "No".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier
        .resolve_question(&[question], "q1", DecisionAnswer::Yes)
        .unwrap();

    assert_eq!(decision.category, DecisionCategory::PreserveExistingFolders);
}

#[test]
fn test_infer_duplicate_category() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "How should duplicate files be handled?".to_string(),
        context: "Duplicate files found".to_string(),
        options: vec!["Keep newer".to_string(), "Merge".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier
        .resolve_question(
            &[question],
            "q1",
            DecisionAnswer::Choice("Merge".to_string()),
        )
        .unwrap();

    assert_eq!(decision.category, DecisionCategory::DuplicateHandling);
}

#[test]
fn test_infer_archive_category() {
    let question = ClarificationQuestion {
        id: "q1".to_string(),
        question: "What archive policy for old files?".to_string(),
        context: "Archive old files".to_string(),
        options: vec!["90 days".to_string(), "1 year".to_string()],
    };

    let clarifier = ClarificationEngine;
    let decision = clarifier
        .resolve_question(
            &[question],
            "q1",
            DecisionAnswer::Choice("90 days".to_string()),
        )
        .unwrap();

    assert_eq!(decision.category, DecisionCategory::ArchiveOld);
}

#[test]
fn test_full_clarification_loop() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize this folder").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let clarifier = ClarificationEngine;
    let questions = clarifier.start(&recommendation);
    assert!(!questions.is_empty());

    // Answer the first question (taxonomy)
    let first_question = &questions[0];
    let decision = clarifier
        .resolve_question(
            &questions,
            &first_question.id,
            DecisionAnswer::Choice(first_question.options[0].clone()),
        )
        .expect("should resolve");

    let decisions = vec![decision];
    let updated_intent = clarifier.apply_decisions(&intent, &decisions);

    // Recompute
    let updated_rec = clarifier.recompute_recommendation(&updated_intent, &analysis);
    let updated_rec = updated_rec.expect("should recompute");

    // After answering, there should be fewer or equal questions
    assert!(updated_rec.unresolved_questions.len() <= questions.len());
}

#[test]
fn test_constraint_violation_displayed_in_summarize() {
    let recommendation = Recommendation {
        id: "test-rec".to_string(),
        strategy: crate::agent::recommendation::RecommendationStrategy::CategoryBased,
        strategy_info: None,
        rationale: "Test rationale".to_string(),
        proposed_categories: Vec::new(),
        proposed_operations: vec![
            crate::agent::recommendation::ProposedOperation::MoveCategory {
                strategy: "category".to_string(),
                content_type: "temp".to_string(),
                file_count: 5,
                to_category: "temp_files".to_string(),
            },
        ],
        unresolved_questions: Vec::new(),
        confidence: 0.8,
        constraint_checks: Vec::new(),
        constraint_violation: Some(ConstraintViolation {
            constraint: "auto_delete_temps".to_string(),
            reason: "Temp deletion not allowed".to_string(),
            blocked_operations: vec!["Move 5 temp files to 'temp_files'".to_string()],
        }),
        warnings: Vec::new(),
        organization_proposal: None,
        generated_at: 0,
    };

    let clarifier = ClarificationEngine;
    let summary = clarifier.summarize(&recommendation);

    assert!(summary.contains("Constraint violation"));
    assert!(summary.contains("Blocked operations"));
    assert!(summary.contains("temp"));
}
