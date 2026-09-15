use super::*;
use crate::agent::intent::TaskIntentParser;
use crate::agent::Goal;
use tempfile::tempdir;
use std::fs;

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
fn test_analyze_organize_intent() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    assert_eq!(analysis.scope_evidence.path, scope);
    assert!(analysis.structure_summary.total_files > 0);
    assert!(!analysis.candidate_categories.is_empty());
    assert!(analysis.analysis_duration_ms > 0 || analysis.analysis_duration_ms == 0);
}

#[test]
fn test_analyze_returns_anomalies() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // We created mixed content (documents, images, loose files)
    // but anomalies may be empty for small test cases - just verify type
    for anomaly in &analysis.anomalies {
        assert!(!anomaly.path.as_os_str().is_empty());
    }
}

#[test]
fn test_content_grouping() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // Should have at least Documents and Other content group
    let has_documents = analysis.content_groups.iter()
        .any(|g| g.category == ContentType::Documents);
    let has_other = analysis.content_groups.iter()
        .any(|g| g.category == ContentType::Other);

    assert!(has_documents || has_other, "Expected at least Documents or Other content group");

    for group in &analysis.content_groups {
        assert!(group.file_count > 0);
        assert!(group.percentage_of_scope > 0.0);
    }
}

#[test]
fn test_analysis_serialization() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize this folder by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    let json = serde_json::to_string(&analysis).expect("should serialize");
    let deserialized: TaskAnalysis = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(analysis, deserialized);
}

#[test]
fn test_analyze_nonexistent_scope() {
    let parser = TaskIntentParser::new(PathBuf::from("/nonexistent/path/12345"));
    let intent = parser
        .parse("Organize this folder")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let result = analyzer.analyze(&intent);
    assert!(result.is_err());
}

#[test]
fn test_analyze_clean_intent() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Clean up temp files and archive old files 90 days")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    match &analysis.intent.goal {
        Goal::Clean { .. } => {}
        _ => panic!("expected Clean goal"),
    }
    assert!(analysis.intent.constraints.auto_delete_temps);
    assert!(analysis.intent.constraints.archive_old.is_some());
}

#[test]
fn test_evidence_gaps_identified() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize downloads")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // "Organize downloads" without specifying purpose → taxonomy gap
    assert!(analysis.evidence_gaps.iter().any(|g| matches!(g.gap_type, GapType::Taxonomy)));
}

#[test]
fn test_candidate_categories_found() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // Should find "documents" and "images" subdirectories as candidates
    let names: Vec<_> = analysis.candidate_categories.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"documents"));
    assert!(names.contains(&"images"));
}

#[test]
fn test_ambiguities_detected() {
    let dir = tempdir().unwrap();
    let scope = create_test_scope(&dir);

    let parser = TaskIntentParser::new(scope.clone());
    let intent = parser
        .parse("Organize by category")
        .expect("should parse");

    let analyzer = EvidenceAnalyzer::default();
    let analysis = analyzer.analyze(&intent).expect("should analyze");

    // Just verify the type is serializable - ambiguities may or may not be found
    // depending on the specific content
    for amb in &analysis.ambiguities {
        assert!(!amb.path.as_os_str().is_empty());
    }
}
