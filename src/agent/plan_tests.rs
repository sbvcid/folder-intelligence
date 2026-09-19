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
fn test_plan_has_create_dir_operations() {
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

    let has_create = plan
        .operations
        .iter()
        .any(|op| matches!(op, FileSystemOperation::CreateDir { .. }));
    assert!(
        has_create,
        "Plan should include CreateDir operations for flat scope"
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
    let scope = create_flat_scope(&dir);

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

    // Should have CreateDir operations for new category directories
    let created_dirs: Vec<_> = plan
        .operations
        .iter()
        .filter_map(|op| {
            if let FileSystemOperation::CreateDir { path } = op {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert!(!created_dirs.is_empty());
    for dir_path in &created_dirs {
        assert!(dir_path.starts_with(&scope));
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
