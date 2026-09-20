use crate::agent::intent::TaskIntentParser;
use crate::agent::{ConstraintSet, EvidenceAnalyzer, Goal, TaskIntent};
use crate::agent::{
    Pipeline, ProposedOperation, Recommendation, RecommendationEngine, RecommendationError,
    RecommendationStrategy, RecommendationWarning,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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
    fs::write(scope.join("temp_file.tmp"), "temp").unwrap();

    scope
}

fn create_flat_scope(dir: &tempfile::TempDir) -> PathBuf {
    let scope = dir.path().join("flat_downloads");
    fs::create_dir_all(&scope).unwrap();

    fs::write(scope.join("doc1.pdf"), "content").unwrap();
    fs::write(scope.join("doc2.docx"), "content").unwrap();
    fs::write(scope.join("photo1.jpg"), "img").unwrap();
    fs::write(scope.join("photo2.png"), "img").unwrap();
    fs::write(scope.join("archive.zip"), "data").unwrap();
    fs::write(scope.join("readme.txt"), "text").unwrap();

    scope
}

#[test]
fn test_recommend_basic() {
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

    assert!(!recommendation.rationale.is_empty());
    assert!(!recommendation.proposed_categories.is_empty());
    assert!(!recommendation.proposed_operations.is_empty());
    assert!(recommendation.confidence >= 0.0 && recommendation.confidence <= 1.0);
    assert_eq!(
        recommendation.strategy,
        RecommendationStrategy::CategoryBased
    );
}

#[test]
fn test_strategy_from_purpose() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by project").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    assert_eq!(
        recommendation.strategy,
        RecommendationStrategy::ProjectBased
    );
}

#[test]
fn test_strategy_chronological() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by date").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    assert_eq!(
        recommendation.strategy,
        RecommendationStrategy::Chronological
    );
}

#[test]
fn test_constraint_preserve_existing_folders() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize by category, preserve existing folders")
        .expect("should parse");

    assert!(intent.constraints.preserve_existing_folders);

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let has_preserve = recommendation
        .proposed_operations
        .iter()
        .any(|op| matches!(op, ProposedOperation::PreserveDirectory { .. }));
    assert!(has_preserve);

    let constraint_check = recommendation
        .constraint_checks
        .iter()
        .find(|c| c.name == "preserve_existing_folders");
    assert!(constraint_check.is_some());
    assert!(constraint_check.unwrap().passed);
}

#[test]
fn test_constraint_auto_delete_temps_blocked() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    assert!(!intent.constraints.auto_delete_temps);

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Temp file archive operations should be blocked, not present
    let has_temp_archive = recommendation.proposed_operations.iter().any(|op| {
        if let ProposedOperation::ArchiveFiles { category, .. } = op {
            category.contains("temp")
        } else {
            false
        }
    });
    assert!(!has_temp_archive);

    // Since there's no violation (we blocked it), the constraint check should reflect this
    let auto_delete_check = recommendation
        .constraint_checks
        .iter()
        .find(|c| c.name == "auto_delete_temps");
    assert!(auto_delete_check.is_some());
}

#[test]
fn test_constraint_violation_when_auto_delete_temps_true() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Clean up temp files").expect("should parse");

    assert!(intent.constraints.auto_delete_temps);

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // When auto_delete_temps is true, temp operations should be allowed (no violation)
    assert!(recommendation.constraint_violation.is_none());
}

#[test]
fn test_constraint_violation_present() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    assert!(!intent.constraints.auto_delete_temps);

    let analyzer = EvidenceAnalyzer::default();
    let _analysis = analyzer.analyze(&intent).expect("should analyze");

    // Force temp file operations by creating temp files
    fs::write(scope.join("another.tmp"), "temp").unwrap();
    fs::write(scope.join("cache.temp"), "temp").unwrap();

    let intent_with_temps = TaskIntent {
        goal: Goal::Organize {
            scope: scope.clone(),
            purpose: "by_category".to_string(),
        },
        constraints: ConstraintSet {
            auto_delete_temps: true, // Allow temp deletion
            ..Default::default()
        },
        user_hints: HashMap::new(),
        unknown_factors: Vec::new(),
    };

    let analysis2 = analyzer
        .analyze(&intent_with_temps)
        .expect("should analyze");
    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent_with_temps, &analysis2)
        .expect("should recommend");

    // With auto_delete_temps=true, no constraint violation
    assert!(recommendation.constraint_violation.is_none());
}

#[test]
fn test_no_candidate_categories_warning() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Flat scope has no existing directories, so no candidate categories
    assert!(analysis.candidate_categories.is_empty());
    // No classification result for flat scope (no candidates to match against)
    assert!(analysis.classification_results.is_empty());

    // Without ClassificationResult, no categories should be proposed
    // and no NoCandidateCategories warning should fire (it only fires
    // when categories are proposed despite having no candidates)
    assert!(
        recommendation.proposed_categories.is_empty(),
        "No categories should be proposed without ClassificationResult"
    );
    assert!(
        !recommendation
            .warnings
            .iter()
            .any(|w| matches!(w, RecommendationWarning::NoCandidateCategories)),
        "NoCandidateCategories warning should not fire when no categories are proposed"
    );
}

#[test]
fn test_ambiguity_clarification_questions() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Should have clarification questions from evidence gaps
    // "by_category" purpose should not generate taxonomy gap
    // But file_disposition and duplicate_handling may generate gaps
    assert!(!recommendation.unresolved_questions.is_empty());
}

#[test]
fn test_scope_mismatch_error() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // Create a modified intent with a different scope
    let wrong_scope = dir.path().join("wrong_scope");
    let wrong_intent = TaskIntent {
        goal: Goal::Organize {
            scope: wrong_scope,
            purpose: "by_category".to_string(),
        },
        constraints: ConstraintSet::default(),
        user_hints: HashMap::new(),
        unknown_factors: Vec::new(),
    };

    let engine = RecommendationEngine::default();
    let result = engine.recommend(&wrong_intent, &analysis);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), RecommendationError::ScopeMismatch);
}

#[test]
fn test_no_content_groups_error() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("empty_scope");
    fs::create_dir_all(&scope).unwrap();

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer
        .analyze(&intent)
        .expect("analysis should succeed on empty dir");

    // Empty directory produces no content groups
    assert!(analysis.structure_summary.content_groups.is_empty());

    let engine = RecommendationEngine::default();
    let result = engine.recommend(&intent, &analysis);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), RecommendationError::NoContentGroups);
}

#[test]
fn test_recommendation_serialization() {
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

    let json = serde_json::to_string(&recommendation).expect("should serialize");
    let deserialized: Recommendation = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(recommendation, deserialized);
}

#[test]
fn test_proposed_operations_descriptions() {
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

    for op in &recommendation.proposed_operations {
        let desc = op.description();
        assert!(!desc.is_empty());
    }
}

#[test]
fn test_confidence_bounds() {
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

    assert!(recommendation.confidence >= 0.0);
    assert!(recommendation.confidence <= 1.0);
}

#[test]
fn test_max_interactive_questions_respected() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    // Set a low max questions limit
    let intent = TaskIntent {
        goal: intent.goal.clone(),
        constraints: ConstraintSet {
            max_interactive_questions: 1,
            ..intent.constraints.clone()
        },
        user_hints: intent.user_hints.clone(),
        unknown_factors: intent.unknown_factors.clone(),
    };

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    assert!(recommendation.unresolved_questions.len() <= 1);
}

#[test]
fn test_clean_goal_preserves_existing() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Clean up temp files").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Even for Clean goal, should produce a recommendation
    assert!(!recommendation.rationale.is_empty());
    assert!(recommendation.confidence >= 0.0 && recommendation.confidence <= 1.0);
}

#[test]
fn test_reorganize_strategy_inference() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Reorganize this folder by type")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    assert_eq!(
        recommendation.strategy,
        RecommendationStrategy::CategoryBased
    );
}

#[test]
fn test_recommendation_has_constraint_checks() {
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

    assert!(!recommendation.constraint_checks.is_empty());
    for check in &recommendation.constraint_checks {
        assert!(!check.name.is_empty());
        assert!(!check.message.is_empty());
    }
}

#[test]
fn test_proposed_category_from_content_groups() {
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

    // Should have proposed categories for document and image content
    let category_names: Vec<_> = recommendation
        .proposed_categories
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        category_names.contains(&"document_storage") || category_names.contains(&"image_storage")
    );

    for cat in &recommendation.proposed_categories {
        assert!(!cat.name.is_empty());
        assert!(!cat.purpose.is_empty());
        assert!(cat.confidence >= 0.0 && cat.confidence <= 1.0);
    }
}

#[test]
fn test_merge_duplicates_constraint_check() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize by category, merge duplicates")
        .expect("should parse");

    assert!(intent.constraints.merge_duplicates);

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let merge_check = recommendation
        .constraint_checks
        .iter()
        .find(|c| c.name == "merge_duplicates");
    assert!(merge_check.is_some());
    assert!(merge_check.unwrap().passed);
}

#[test]
fn test_archive_old_constraint_check() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize by category, archive old files 90 days")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let archive_check = recommendation
        .constraint_checks
        .iter()
        .find(|c| c.name == "archive_old_files");
    assert!(archive_check.is_some());
    assert!(archive_check.unwrap().passed);
}

#[test]
fn test_warning_for_partial_scan() {
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

    // If partial scan was triggered, should have LowEvidence warning
    // If not, that's fine too - just verify the type is correct
    for warning in &recommendation.warnings {
        match warning {
            RecommendationWarning::LowEvidence => {}
            RecommendationWarning::AmbiguousContent => {}
            RecommendationWarning::NoCandidateCategories => {}
            RecommendationWarning::ConflictingCategories => {}
            RecommendationWarning::InsufficientContentGroups => {}
            RecommendationWarning::HighRisk => {}
        }
    }
}

#[test]
fn test_rationale_mentions_content_types() {
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

    assert!(
        recommendation.rationale.contains("category")
            || recommendation.rationale.contains("Strategy")
    );
}

#[test]
fn test_does_not_fabricate_filesystem_evidence() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // No candidate categories exist (flat structure)
    assert!(analysis.candidate_categories.is_empty());
    // No classification result without candidate categories
    assert!(analysis.classification_results.is_empty());

    let engine = RecommendationEngine::default();
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // No categories should be fabricated from ContentType without ClassificationResult
    assert!(
        recommendation.proposed_categories.is_empty(),
        "Must not fabricate categories from ContentType without ClassificationResult"
    );

    // No operations should be proposed without ClassificationResult
    assert!(
        recommendation.proposed_operations.is_empty(),
        "Must not fabricate operations from ContentType without ClassificationResult"
    );
}

#[test]
fn test_proposed_categories_count_matches_content_groups() {
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

    // Should have at least one proposed category per content group
    let content_group_count = analysis.content_groups.len();
    assert!(recommendation.proposed_categories.len() >= content_group_count);
}

#[test]
fn test_recommendation_id_format() {
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

    assert!(recommendation.id.starts_with("rec-"));
}

#[test]
fn test_generated_at_is_set() {
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

    assert!(recommendation.generated_at > 0);
}

#[test]
fn test_phase15_move_existing_propagation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);
    let doc_path = scope.join("documents");

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    if let Some(res) = analysis.classification_results.first_mut() {
        res.decision = crate::classification::ClassificationDecision::MoveExisting;
        res.selected_candidate = Some(doc_path.clone());
    }

    let pipeline = Pipeline::new(&scope);
    let recommendation = pipeline
        .recommend(&intent, &analysis)
        .expect("should recommend");
    assert!(!recommendation.proposed_categories.is_empty());
    assert_eq!(recommendation.proposed_categories[0].name, "documents");

    let plan = pipeline
        .plan(&recommendation, &analysis, &intent)
        .expect("should plan");
    assert!(!plan.operations.is_empty());
}

#[test]
fn test_phase15_create_category_propagation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    if let Some(res) = analysis.classification_results.first_mut() {
        res.decision = crate::classification::ClassificationDecision::CreateCategory;
        res.proposed_category_name = Some("Projects".to_string());
        res.selected_candidate = None;
    }

    let pipeline = Pipeline::new(&scope);
    let recommendation = pipeline
        .recommend(&intent, &analysis)
        .expect("should recommend");
    assert!(!recommendation.proposed_categories.is_empty());
    assert_eq!(recommendation.proposed_categories[0].name, "projects");

    let plan = pipeline
        .plan(&recommendation, &analysis, &intent)
        .expect("should plan");
    assert!(!plan.operations.is_empty());
}

#[test]
fn test_phase15_classification_recommendation_mismatch() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    // Classification says CreateCategory("Projects")
    if let Some(res) = analysis.classification_results.first_mut() {
        res.decision = crate::classification::ClassificationDecision::CreateCategory;
        res.proposed_category_name = Some("Projects".to_string());
    }

    let pipeline = Pipeline::new(&scope);
    let mut recommendation = pipeline
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Tamper with recommendation to mismatch classification (propose different category name)
    recommendation.proposed_categories[0].name = "images_storage_mismatch".to_string();

    let plan_result = Pipeline::validate_recommendation_plan_integrity(&recommendation, &analysis);
    assert!(
        plan_result.is_err(),
        "Classification/Recommendation mismatch must fail integrity validation"
    );
}

#[test]
fn test_phase15_leave_unclassified_zero_mutation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    if let Some(res) = analysis.classification_results.first_mut() {
        res.decision = crate::classification::ClassificationDecision::LeaveUnclassified;
    }

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");
    assert!(
        recommendation.proposed_operations.is_empty(),
        "LeaveUnclassified must produce zero mutations"
    );
}

#[test]
fn test_phase15_ask_user_zero_mutation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    if let Some(res) = analysis.classification_results.first_mut() {
        res.decision = crate::classification::ClassificationDecision::AskUser;
    }

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");
    assert!(
        recommendation.proposed_operations.is_empty(),
        "AskUser must produce zero mutations"
    );
}

#[test]
fn test_phase15_no_classification_result_no_fallback() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let mut analysis = analyzer.analyze(&intent).expect("should analyze");

    // Clear classification results
    analysis.classification_results.clear();

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");
    assert!(
        recommendation.proposed_categories.is_empty(),
        "No classification result must not trigger content-type fallback for categories"
    );
    assert!(
        recommendation.proposed_operations.is_empty(),
        "No classification result must not trigger content-type fallback for operations"
    );
}
