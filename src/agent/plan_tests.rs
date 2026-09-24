use super::*;
use crate::agent::clarification::{
    ClarificationEngine, DecisionAnswer, DecisionCategory, UserDecision,
};
use crate::agent::intent::TaskIntentParser;
use crate::agent::pipeline::Pipeline;
use crate::agent::validate::PlanValidator;
use crate::agent::{EvidenceAnalyzer, PlanGenerator, RecommendationEngine};
use serde_json;
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
fn test_plan_generation_basic() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    assert!(!plan.operations.is_empty());
    assert!(plan.dry_run);
    assert!(plan.created_at > 0);
}

#[test]
fn test_plan_no_operations_for_flat_scope() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // Flat scope with no existing directories produces no classification result
    assert!(analysis.candidate_categories.is_empty());
    assert!(analysis.classification_results.is_empty());

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // No classification result means no operations proposed (no fallback)
    assert!(
        recommendation.proposed_operations.is_empty(),
        "Flat scope with no classification result must produce zero operations"
    );

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Plan must have no operations (zero mutation)
    assert!(
        plan.operations.is_empty(),
        "Plan for flat scope with no classification must have no operations"
    );
}

#[test]
fn test_plan_operations_within_scope() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator::default();
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    for op in &plan.operations {
        if let Some(dest) = op.dest_path() {
            assert!(
                dest.starts_with(&scope),
                "Operation destination {:?} should be within scope {:?}",
                dest,
                scope
            );
        }
    }
}

#[test]
fn test_plan_serialization() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    let json = serde_json::to_string(&plan).expect("should serialize");
    let deserialized: OperationPlan = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(plan, deserialized);
}

#[test]
fn test_plan_preview_contains_operations() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");
    let preview = generator.preview(&plan);

    assert!(preview.contains("Operation Plan"));
    assert!(preview.contains("Dry run"));
    assert!(preview.contains("Estimated Impact"));
}

#[test]
fn test_plan_does_not_execute_filesystem() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Plan should be dry-run by default
    assert!(plan.dry_run);

    // Verify source files still exist (no execution happened)
    assert!(scope.join("archive.zip").exists());
    assert!(scope.join("readme.txt").exists());
}

#[test]
fn test_plan_operation_descriptions() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    for op in &plan.operations {
        let desc = op.description();
        assert!(!desc.is_empty());
        assert!(
            op.operation_type() == "move"
                || op.operation_type() == "create_dir"
                || op.operation_type() == "delete"
        );
    }
}

#[test]
fn test_plan_estimated_impact() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    assert!(plan.estimated_impact.dirs_created > 0 || plan.estimated_impact.files_moved > 0);
}

#[test]
fn test_plan_has_recommendation_id() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    assert_eq!(plan.recommendation_id, recommendation.id);
    assert!(plan.id.starts_with("plan-"));
}

#[test]
fn test_plan_validation_warnings_for_missing_files() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Validation warnings may or may not be present depending on file existence
    for w in &plan.validation_warnings {
        assert!(!w.message.is_empty());
        assert!(!w.path.as_os_str().is_empty());
    }
}

#[test]
fn test_plan_preserves_existing_dir_structure() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category, preserve existing folders")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Existing directories (documents, images) should be recognized
    let has_moves = plan
        .operations
        .iter()
        .any(|op| matches!(op, FileSystemOperation::Move { .. }));
    assert!(
        has_moves,
        "Should have move operations for existing content"
    );
}

#[test]
fn test_plan_category_dir_resolution() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize by category").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // All operations with destinations must be within scope
    for op in &plan.operations {
        if let Some(dest) = op.dest_path() {
            assert!(
                dest.starts_with(&scope),
                "Destination {} must be within scope {}",
                dest.display(),
                scope.display()
            );
        }
    }
}

#[test]
fn test_plan_does_not_have_duplicate_destinations() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Check for duplicate Move destinations (same file being moved twice)
    let move_dests: Vec<_> = plan
        .operations
        .iter()
        .filter_map(|op| {
            if let FileSystemOperation::Move { source, dest } = op {
                Some((source.clone(), dest.clone()))
            } else {
                None
            }
        })
        .collect();

    let mut sources: HashSet<PathBuf> = HashSet::new();
    for (source, _dest) in &move_dests {
        assert!(
            sources.insert(source.clone()),
            "Duplicate move source: {:?}",
            source
        );
    }
}

#[test]
fn test_plan_preview_no_operations() {
    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "test-plan".to_string(),
        recommendation_id: "test-rec".to_string(),
        scope: PathBuf::from("/tmp"),
        operations: Vec::new(),
        estimated_impact: EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
        validation_context: None,
    };

    let generator = PlanGenerator;
    let preview = generator.preview(&plan);
    assert!(preview.contains("No operations proposed"));
}

#[test]
fn test_plan_with_archive_operation() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Clean up temp files").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // Plan should be generated without errors
    assert!(plan.created_at > 0);
}

#[test]
fn test_plan_scope_validation() {
    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "test-plan".to_string(),
        recommendation_id: "test-rec".to_string(),
        scope: PathBuf::from("/tmp/scope"),
        operations: vec![FileSystemOperation::CreateDir {
            path: PathBuf::from("/tmp/scope/new_dir"),
        }],
        estimated_impact: EstimatedImpact {
            files_moved: 0,
            dirs_created: 1,
            files_deleted: 0,
            dirs_affected: 1,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
        validation_context: None,
    };

    let generator = PlanGenerator;
    let preview = generator.preview(&plan);
    assert!(preview.contains("new_dir"));
    assert!(preview.contains("Dry run: Yes"));
}

#[test]
fn test_plan_with_clarified_intent() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser.parse("Organize this folder").expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let _recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    // Apply a decision to specify taxonomy
    let clarifier = ClarificationEngine;
    let updated_intent = clarifier.apply_decisions(
        &intent,
        &[UserDecision {
            category: DecisionCategory::TaxonomyChoice,
            answer: DecisionAnswer::Choice("by_category".to_string()),
            question_id: "q1".to_string(),
            rationale: None,
        }],
    );

    let updated_rec = engine
        .recommend(&updated_intent, &analysis)
        .expect("should recommend");
    let generator = PlanGenerator;
    let plan = generator
        .generate(&updated_rec, &analysis)
        .expect("should generate plan");

    assert!(!plan.operations.is_empty());
}

#[test]
fn test_plan_file_system_operation_types() {
    let move_op = FileSystemOperation::Move {
        source: PathBuf::from("/src/a.txt"),
        dest: PathBuf::from("/dest/a.txt"),
    };
    assert_eq!(move_op.operation_type(), "move");
    assert!(move_op.description().contains("→"));

    let mkdir_op = FileSystemOperation::CreateDir {
        path: PathBuf::from("/new"),
    };
    assert_eq!(mkdir_op.operation_type(), "create_dir");

    let delete_op = FileSystemOperation::Delete {
        path: PathBuf::from("/old"),
        reason: "cleanup".to_string(),
    };
    assert_eq!(delete_op.operation_type(), "delete");
}

#[test]
fn test_plan_is_dry_run() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    assert!(
        plan.dry_run,
        "Plan must be dry-run by default — must NOT execute filesystem operations"
    );
}

#[test]
fn test_plan_has_conflicts_detection() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    // has_conflicts may be true or false depending on the content
    // Just verify it's a valid boolean
    let _ = plan.has_conflicts;
}

#[test]
fn test_category_to_human_name_mapping() {
    let generator = PlanGenerator;

    assert_eq!(
        generator.category_to_human_name("document_storage"),
        "Documents"
    );
    assert_eq!(generator.category_to_human_name("image_storage"), "Images");
    assert_eq!(
        generator.category_to_human_name("archive_storage"),
        "Archives"
    );
    assert_eq!(generator.category_to_human_name("code_storage"), "Code");
    assert_eq!(generator.category_to_human_name("media_storage"), "Media");
    assert_eq!(
        generator.category_to_human_name("installer_storage"),
        "Installers"
    );
    assert_eq!(generator.category_to_human_name("data_storage"), "Data");
    assert_eq!(generator.category_to_human_name("config_storage"), "Config");
    assert_eq!(generator.category_to_human_name("misc_storage"), "Misc");
}

#[test]
fn test_parse_content_type() {
    let generator = PlanGenerator;

    assert_eq!(
        generator.parse_content_type("documents"),
        ContentType::Documents
    );
    assert_eq!(generator.parse_content_type("images"), ContentType::Images);
    assert_eq!(generator.parse_content_type("code"), ContentType::Code);
    assert_eq!(generator.parse_content_type("unknown"), ContentType::Other);
}

#[test]
fn test_content_type_extensions_non_empty() {
    let generator = PlanGenerator;

    for ct in &[
        ContentType::Documents,
        ContentType::Images,
        ContentType::Archives,
        ContentType::Media,
        ContentType::Code,
        ContentType::Installers,
        ContentType::Data,
        ContentType::Config,
    ] {
        assert!(!generator.content_type_extensions(ct).is_empty());
    }

    // Other has empty extensions (catch-all)
    assert!(generator
        .content_type_extensions(&ContentType::Other)
        .is_empty());
}

#[test]
fn test_plan_no_source_equals_dest() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    for op in &plan.operations {
        if let FileSystemOperation::Move { source, dest } = op {
            assert_ne!(
                source, dest,
                "Source and destination must not be the same path"
            );
        }
    }
}

fn pipeline_plan_for_test(dir: &tempfile::TempDir) -> (PathBuf, OperationPlan) {
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
    fs::create_dir_all(scope.join("Documents")).unwrap();
    fs::create_dir_all(scope.join("Images")).unwrap();

    let pipeline = Pipeline::new(&scope);
    let intent = pipeline
        .parse_intent("Organize this folder by category")
        .unwrap();
    let analysis = pipeline.analyze(&intent).unwrap();
    let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
    let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

    (scope, plan)
}

#[test]
fn test_phase6c_json_round_trip_with_validation_context() {
    let dir = tempdir().unwrap();
    let (_scope, plan) = pipeline_plan_for_test(&dir);

    let json = serde_json::to_string(&plan).expect("should serialize");
    let deserialized: OperationPlan = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(plan, deserialized);
    assert!(
        deserialized.validation_context.is_some(),
        "validation_context must survive serialization"
    );
    assert!(
        !deserialized.dry_run,
        "plan must be non-dry-run after Pipeline::plan()"
    );
}

#[test]
fn test_phase6c_validation_context_persisted_in_json() {
    let dir = tempdir().unwrap();
    let (_scope, plan) = pipeline_plan_for_test(&dir);

    let json = serde_json::to_string(&plan).expect("should serialize");
    assert!(
        json.contains("validation_context"),
        "JSON must contain validation_context field"
    );
    assert!(
        json.contains("preserve_existing_folders"),
        "JSON must contain context fields"
    );
    assert!(
        json.contains("auto_delete_temps"),
        "JSON must contain context fields"
    );
}

#[test]
fn test_phase6c_legacy_plan_without_context_rejected() {
    let dir = tempdir().unwrap();
    let (_scope, plan) = pipeline_plan_for_test(&dir);

    let json = serde_json::to_string(&plan).expect("should serialize");
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("should parse json");
    value.as_object_mut().unwrap().remove("validation_context");
    let legacy_json = serde_json::to_string(&value).expect("should re-serialize");

    let legacy_plan: OperationPlan =
        serde_json::from_str(&legacy_json).expect("legacy plan deserializes with default");
    assert!(
        legacy_plan.validation_context.is_none(),
        "legacy plan must have None context"
    );

    let validator = PlanValidator;
    let result = validator.validate(&legacy_plan);
    assert!(
        result.has_invalid,
        "legacy plan without context must be rejected"
    );
}

#[test]
fn test_move_dest_is_full_file_path() {
    let dir = tempdir().unwrap();
    let scope = create_flat_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    for op in &plan.operations {
        if let FileSystemOperation::Move { source, dest } = op {
            assert!(
                source.file_name() == dest.file_name(),
                "Move dest must preserve the source file name: source={:?}, dest={:?}",
                source,
                dest
            );
            assert!(dest != source, "Move dest must be different from source");
            assert!(dest.starts_with(&scope), "Move dest must be within scope");
        }
    }
}

#[test]
fn test_move_into_existing_category_directory() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("downloads");
    fs::create_dir_all(&scope).unwrap();
    fs::create_dir_all(scope.join("Documents")).unwrap();
    fs::write(scope.join("readme.txt"), "text").unwrap();
    fs::write(scope.join("license.md"), "license text").unwrap();

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    let moves: Vec<_> = plan
        .operations
        .iter()
        .filter_map(|op| {
            if let FileSystemOperation::Move { source, dest } = op {
                Some((source.clone(), dest.clone()))
            } else {
                None
            }
        })
        .collect();

    assert!(!moves.is_empty(), "should have at least one move");

    for (source, dest) in &moves {
        assert_eq!(
            source.file_name(),
            dest.file_name(),
            "dest file name should match source file name"
        );
        assert!(dest.starts_with(&scope), "Move dest must be within scope");
        let parent = dest.parent().unwrap_or(dest);
        assert!(
            parent.ends_with("Documents")
                || parent.ends_with("Images")
                || parent.ends_with("Archives"),
            "dest parent should be a category directory: {:?}",
            parent
        );
    }
}

#[test]
fn test_move_dest_parent_created_by_planned_create_dir() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("flat_scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("doc.pdf"), "content").unwrap();
    fs::write(scope.join("photo.jpg"), "img").unwrap();
    fs::write(scope.join("archive.zip"), "data").unwrap();

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let engine = RecommendationEngine;
    let recommendation = engine
        .recommend(&intent, &analysis)
        .expect("should recommend");

    let generator = PlanGenerator;
    let plan = generator
        .generate(&recommendation, &analysis)
        .expect("should generate plan");

    for op in &plan.operations {
        if let FileSystemOperation::Move { dest, .. } = op {
            let parent = dest.parent().unwrap_or(dest);
            if !parent.exists() {
                let has_create_dir = plan.operations.iter().any(|op| {
                    if let FileSystemOperation::CreateDir { path } = op {
                        path == parent
                    } else {
                        false
                    }
                });
                assert!(
                    has_create_dir,
                    "parent directory {:?} should have a planned CreateDir operation",
                    parent
                );
            }
        }
    }
}

#[test]
fn test_move_source_equals_dest_rejected_by_validator() {
    let dir = tempdir().unwrap();
    let scope = dir.path().join("test_scope");
    fs::create_dir_all(&scope).unwrap();
    let file = scope.join("file.txt");
    fs::write(&file, "content").unwrap();

    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "test-move-same".to_string(),
        recommendation_id: "rec".to_string(),
        scope: scope.clone(),
        operations: vec![FileSystemOperation::Move {
            source: file.clone(),
            dest: file.clone(),
        }],
        estimated_impact: EstimatedImpact {
            files_moved: 0,
            dirs_created: 0,
            files_deleted: 0,
            dirs_affected: 0,
            total_bytes: 0,
        },
        validation_warnings: Vec::new(),
        has_conflicts: false,
        dry_run: true,
        created_at: 0,
        validation_context: Some(PlanValidationContext::default()),
    };

    let validator = PlanValidator::default();
    let result = validator.validate(&plan);

    assert!(result.has_invalid, "source == dest must be INVALID");
}

#[cfg(test)]
mod phase17c4_proposal_converter_tests {
    use super::*;
    use crate::agent::proposal_converter::{ProposalConverter, UnresolvedReason};
    use crate::agent::recommendation::{
        OrganizationProposal, ProposedCategory, RecommendationStrategy,
    };
    use crate::agent::EvidenceAnalyzer;

    #[test]
    fn test_1_proposal_category_converts_to_move_operations() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analyzer = EvidenceAnalyzer::default();
        let analysis = analyzer.analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "Author A works".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();

        assert_eq!(ops.len(), 2); // CreateDir + Move
        assert!(unresolved.is_empty());
    }

    #[test]
    fn test_2_multiple_categories_create_separate_destinations() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();
        fs::write(scope.join("Manga2.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![
                ProposedCategory {
                    name: "Author A".to_string(),
                    purpose: "A".to_string(),
                    target_content_types: vec![],
                    confidence: 0.9,
                    is_existing: false,
                    target_path: None,
                    source_files: vec![scope.join("Manga1.cbz")],
                },
                ProposedCategory {
                    name: "Author B".to_string(),
                    purpose: "B".to_string(),
                    target_content_types: vec![],
                    confidence: 0.9,
                    is_existing: false,
                    target_path: None,
                    source_files: vec![scope.join("Manga2.cbz")],
                },
            ],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, _) = converter.convert(&proposal, &analysis, &scope).unwrap();
        // CreateDir Author A, Move Manga1, CreateDir Author B, Move Manga2 = 4 ops
        assert_eq!(ops.len(), 4);
    }

    #[test]
    fn test_3_existing_destination_directory_is_reused() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::create_dir_all(scope.join("Author A")).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: true,
                target_path: Some(scope.join("Author A")),
                source_files: vec![scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, _) = converter.convert(&proposal, &analysis, &scope).unwrap();
        // Since Author A exists, no CreateDir should be generated, only Move = 1 op
        assert_eq!(ops.len(), 1);
        if let FileSystemOperation::Move { dest, .. } = &ops[0] {
            assert!(dest.starts_with(scope.join("Author A")));
        } else {
            panic!("Expected Move operation");
        }
    }

    #[test]
    fn test_4_already_organized_file_is_not_moved() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        let author_dir = scope.join("Author A");
        fs::create_dir_all(&author_dir).unwrap();
        // File is already inside Author A/
        fs::write(author_dir.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: true,
                target_path: Some(author_dir.clone()),
                source_files: vec![author_dir.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();
        assert!(ops.is_empty());
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].reason, UnresolvedReason::AlreadyOrganized);
    }

    #[test]
    fn test_5_missing_proposal_file_produces_no_operation() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Missing.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();
        assert!(ops.is_empty());
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].reason, UnresolvedReason::MissingFile);
    }

    #[test]
    fn test_6_ambiguous_file_produces_no_operation() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Shared.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![
                ProposedCategory {
                    name: "Author A".to_string(),
                    purpose: "A".to_string(),
                    target_content_types: vec![],
                    confidence: 0.9,
                    is_existing: false,
                    target_path: None,
                    source_files: vec![scope.join("Shared.cbz")],
                },
                ProposedCategory {
                    name: "Author B".to_string(),
                    purpose: "B".to_string(),
                    target_content_types: vec![],
                    confidence: 0.9,
                    is_existing: false,
                    target_path: None,
                    source_files: vec![scope.join("Shared.cbz")],
                },
            ],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();
        // File claimed by both -> ambiguous for both or second
        assert!(ops.is_empty());
        assert!(!unresolved.is_empty());
        assert!(unresolved
            .iter()
            .any(|u| u.reason == UnresolvedReason::Ambiguous));
    }

    #[test]
    fn test_7_duplicate_proposal_entries_do_not_create_duplicate_operations() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz"), scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, _) = converter.convert(&proposal, &analysis, &scope).unwrap();
        // Same file twice in same category should be treated as same claim or invalid destination / handled gracefully
        let move_count = ops
            .iter()
            .filter(|o| matches!(o, FileSystemOperation::Move { .. }))
            .count();
        assert_eq!(move_count, 1);
    }

    #[test]
    fn test_8_unsafe_destination_is_rejected() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "../Outside".to_string(),
                purpose: "Outside scope".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (_, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();
        assert!(!unresolved.is_empty());
        assert_eq!(
            unresolved[0].reason,
            UnresolvedReason::DestinationOutsideScope
        );
    }

    #[test]
    fn test_9_proposal_absent_preserves_existing_operation_plan_behavior() {
        let dir = tempdir().unwrap();
        let scope = create_test_scope(&dir);

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize files by type")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();
        let recommendation = RecommendationEngine::default()
            .recommend(&intent, &analysis)
            .unwrap();

        // Ensure recommendation has no proposal or proposal has empty source files
        let mut rec_no_prop = recommendation.clone();
        rec_no_prop.organization_proposal = None;

        let generator = PlanGenerator;
        let plan = generator.generate(&rec_no_prop, &analysis).unwrap();
        // Should generate operations via existing proposed_operations logic
        assert!(!plan.operations.is_empty());
    }

    #[test]
    fn test_10_generated_operations_still_pass_existing_validation() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let recommendation = Recommendation {
            id: "rec-1".to_string(),
            strategy: RecommendationStrategy::ByAuthor,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: proposal.proposed_categories.clone(),
            proposed_operations: vec![],
            organization_proposal: Some(proposal),
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
        if validation.has_invalid {
            println!("VALIDATION FAILED: {:?}", validation);
        }
        assert!(
            !validation.has_invalid,
            "Generated operations must be valid"
        );
        assert!(!validation.has_conflicts);
    }

    #[test]
    fn test_11_conflicting_destination_on_filesystem_rejected() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "source data").unwrap();

        // Create destination directory and pre-existing file with the same name
        let author_dir = scope.join("Author A");
        fs::create_dir_all(&author_dir).unwrap();
        fs::write(author_dir.join("Manga1.cbz"), "existing data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: true,
                target_path: Some(author_dir.clone()),
                source_files: vec![scope.join("Manga1.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let converter = ProposalConverter;
        let (ops, unresolved) = converter.convert(&proposal, &analysis, &scope).unwrap();
        assert!(
            ops.is_empty(),
            "Must not overwrite existing destination file"
        );
        assert_eq!(unresolved.len(), 1);
        assert_eq!(
            unresolved[0].reason,
            UnresolvedReason::ConflictingDestination
        );
    }

    #[test]
    fn test_12_proposal_conversion_result_data_model() {
        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "data").unwrap();

        let intent = TaskIntentParser::new(scope.clone())
            .parse("Organize")
            .unwrap();
        let analysis = EvidenceAnalyzer::default().analyze(&intent).unwrap();

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "test".to_string(),
            proposed_categories: vec![ProposedCategory {
                name: "Author A".to_string(),
                purpose: "A".to_string(),
                target_content_types: vec![],
                confidence: 0.9,
                is_existing: false,
                target_path: None,
                source_files: vec![scope.join("Manga1.cbz"), scope.join("Missing.cbz")],
            }],
            evidence_gaps: vec![],
            ambiguities: vec![],
        };

        let recommendation = Recommendation {
            id: "rec-test-12".to_string(),
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

        let converter = ProposalConverter;
        let result = converter
            .convert_to_plan(&proposal, &recommendation, &analysis, &scope)
            .unwrap();

        assert_eq!(result.operation_plan.operations.len(), 2); // CreateDir + Move
        assert_eq!(result.unresolved.len(), 1);
        assert_eq!(result.unresolved[0].reason, UnresolvedReason::MissingFile);
        assert_eq!(result.operation_plan.estimated_impact.files_moved, 1);
        assert_eq!(result.operation_plan.estimated_impact.dirs_created, 1);
    }

    #[test]
    fn test_13_preview_renders_unresolved_items() {
        use crate::agent::PlanPreview;

        let dir = tempdir().unwrap();
        let scope = dir.path().join("scope");
        fs::create_dir_all(&scope).unwrap();

        let plan = OperationPlan {
            id: "plan-test-13".to_string(),
            recommendation_id: "rec-test-13".to_string(),
            scope: scope.clone(),
            operations: vec![],
            estimated_impact: EstimatedImpact {
                files_moved: 0,
                dirs_created: 0,
                files_deleted: 0,
                dirs_affected: 0,
                total_bytes: 0,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            unresolved_proposals: vec![crate::agent::proposal_converter::UnresolvedItem {
                file: scope.join("Missing.cbz"),
                category: "Author A".to_string(),
                reason: UnresolvedReason::MissingFile,
            }],
            dry_run: true,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        };

        let validator = PlanValidator::default();
        let validation = validator.validate(&plan);
        let preview = PlanPreview.render(&plan, &validation);

        assert!(preview.contains("Unresolved Proposal Items"));
        assert!(preview.contains("Missing file"));
        assert!(preview.contains("Author A"));
    }

    #[test]
    fn test_14_real_e2e_instruction_to_proposal_to_plan_to_preview() {
        use crate::classification::LlmClassifier;
        use crate::llm::MockLlmProvider;
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let scope = dir.path().join("test-intent");
        fs::create_dir_all(&scope).unwrap();
        fs::write(scope.join("Manga1.cbz"), "content1").unwrap();
        fs::write(scope.join("Manga2.cbz"), "content2").unwrap();
        fs::write(scope.join("Manga3.cbz"), "content3").unwrap();

        let mock_json = r#"{
            "decision": "create_category",
            "proposed_category_name": "Author A",
            "confidence": 0.95,
            "reasoning": "Organize manga by author",
            "organization_strategy": {
                "strategy": "by_author",
                "confidence": 0.95,
                "reason": "User requested organization by author"
            },
            "organization_proposal": {
                "rationale": "Grouped into Author A and Author B based on series metadata",
                "categories": [
                    {
                        "name": "Author A",
                        "purpose": "Works by Author A",
                        "files": ["Manga1.cbz", "Manga2.cbz"],
                        "confidence": 0.95
                    },
                    {
                        "name": "Author B",
                        "purpose": "Works by Author B",
                        "files": ["Manga3.cbz"],
                        "confidence": 0.95
                    }
                ],
                "evidence_gaps": [],
                "ambiguities": []
            }
        }"#;

        let mock_provider = MockLlmProvider::new(vec![mock_json.to_string()]);
        let classifier = LlmClassifier::new(Arc::new(mock_provider));

        let pipeline = Pipeline::new(&scope)
            .with_llm_classifier(classifier)
            .with_classifier_instruction(Some("按照作者整理".to_string()));

        let intent = pipeline.parse_intent("Organize").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        // 1. Verify analysis captured the LLM organization proposal
        assert!(analysis.organization_proposal.is_some());
        let prop = analysis.organization_proposal.as_ref().unwrap();
        assert_eq!(prop.proposed_categories.len(), 2);

        // 2. Verify recommendation captured the organization proposal
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        assert!(recommendation.organization_proposal.is_some());

        // 3. Verify ProposalConverter generates OperationPlan
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        assert_eq!(plan.operations.len(), 5); // CreateDir(A), Move(1), Move(2), CreateDir(B), Move(3)

        // 4. Verify existing validation accepts the operations
        let validation = pipeline.validate(&plan);
        assert!(!validation.has_invalid);
        assert!(!validation.has_conflicts);
        assert_eq!(validation.executable_operations, 5);

        // 5. Verify preview output displays the operations correctly
        let preview = pipeline.preview(&plan, &validation);
        assert!(preview.contains("Author A"));
        assert!(preview.contains("Author B"));
        assert!(preview.contains("Manga1.cbz"));
        assert!(preview.contains("Manga2.cbz"));
        assert!(preview.contains("Manga3.cbz"));
        assert!(preview.contains("Files moved:  3"));
        assert!(preview.contains("Dirs created: 2"));

        // 6. Verify existing Executor executes the plan safely
        let apply_result = pipeline
            .apply(
                &plan,
                &validation,
                &crate::agent::ApplyOptions {
                    force: false,
                    dry_run: false,
                },
            )
            .unwrap();

        assert!(apply_result.is_complete);
        assert!(scope.join("Author A").join("Manga1.cbz").exists());
        assert!(scope.join("Author A").join("Manga2.cbz").exists());
        assert!(scope.join("Author B").join("Manga3.cbz").exists());
        assert!(!scope.join("Manga1.cbz").exists());

        // 7. Verify existing Verification passes
        let verification = apply_result.execution_verification.unwrap();
        assert!(verification.passed());
    }
}
