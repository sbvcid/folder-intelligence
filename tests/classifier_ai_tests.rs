use folder_intelligence::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_test_target() -> DirectoryEvidence {
    let mut ext_hist = HashMap::new();
    ext_hist.insert("mp3".to_string(), 5);

    DirectoryEvidence {
        path: PathBuf::from("/test/target"),
        name: "target".to_string(),
        parent_path: None,
        depth: 0,
        file_count: 5,
        directory_count: 0,
        total_size: 5120,
        extension_histogram: ext_hist,
        dominant_extensions: vec![DominantExtension {
            extension: "mp3".to_string(),
            count: 5,
            percentage: 100.0,
        }],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec![
            "001.mp3".to_string(),
            "002.mp3".to_string(),
            "003.mp3".to_string(),
            "004.mp3".to_string(),
            "005.mp3".to_string(),
        ],
        notable_filenames: vec![],
        syntactic_identifiers: vec![],
        text_file_presence: TextFilePresence::default(),
        is_empty: false,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 5,
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

fn make_test_candidate(name: &str, path: &PathBuf) -> CandidateEvidence {
    let mut dir_ext = HashMap::new();
    dir_ext.insert("mp3".to_string(), 5);
    let dir = DirectoryEvidence {
        path: path.clone(),
        name: name.to_string(),
        parent_path: Some(PathBuf::from("/lib")),
        depth: 1,
        file_count: 5,
        directory_count: 0,
        total_size: 5120,
        extension_histogram: dir_ext,
        dominant_extensions: vec![DominantExtension {
            extension: "mp3".to_string(),
            count: 5,
            percentage: 100.0,
        }],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec!["001.mp3".to_string()],
        notable_filenames: vec![],
        syntactic_identifiers: vec![],
        text_file_presence: TextFilePresence::default(),
        is_empty: false,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 1,
        schema_version: SCHEMA_VERSION.to_string(),
    };

    CandidateEvidence {
        directory: dir,
        children: vec![],
        summary: CandidateSummary {
            total_children: 0,
            top_extensions: vec!["mp3".to_string()],
            common_identifier_types: vec![],
            avg_file_count: 5.0,
        },
    }
}

fn build_input_and_baseline(
    candidates: Vec<CandidateEvidence>,
) -> (ClassificationInput, ClassificationResult) {
    let target = make_test_target();
    let metadata = ScanMetadata {
        _type: "scan_metadata".to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        scan_batch_id: "test-batch".to_string(),
        scan_started_at: 1234567890,
        root_path: PathBuf::from("/test"),
        limits: ScanLimits::default(),
        stats: ScanStats {
            directories_scanned: 0,
            files_encountered: 0,
            bytes_scanned: 0,
            dirs_skipped: 0,
            files_skipped: 0,
            errors: vec![],
            duration_ms: 0,
        },
    };

    let target_evidence = target.clone();
    let input = ClassificationInput {
        target: target_evidence.clone(),
        candidates: candidates.clone(),
        metadata,
    };

    let target_input = TargetEvidence {
        evidence: target_evidence,
        user_hints: None,
    };
    let config = ClassificationConfig::default();
    let baseline = DecisionEngine::classify(&target_input, &candidates, &config);

    (input, baseline)
}

#[test]
fn test_mock_ai_high_confidence_move_existing() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    assert_eq!(baseline.decision, ClassificationDecision::MoveExisting);
    assert!(baseline.confidence >= 0.8);

    let classifier = MockAiClassifier::default();
    let result = classifier.classify(&input, &baseline).unwrap();

    assert_eq!(result.decision, ClassificationDecision::MoveExisting);
    assert!(result.selected_candidate.is_some());
    assert!(result.model == Some("mock-ai-v1.0".to_string()));
    assert!(result.provider == Some("mock".to_string()));
}

#[test]
fn test_mock_ai_no_candidates_create_category() {
    let (input, _) = build_input_and_baseline(vec![]);

    let target = make_test_target();
    let target_input = TargetEvidence {
        evidence: target,
        user_hints: None,
    };
    let config = ClassificationConfig::default();
    let baseline = DecisionEngine::classify(&target_input, &[], &config);

    let classifier = MockAiClassifier::default();
    let result = classifier.classify(&input, &baseline).unwrap();

    assert!(matches!(
        result.decision,
        ClassificationDecision::CreateCategory | ClassificationDecision::LeaveUnclassified
    ));
}

#[test]
fn test_mock_ai_confidence_within_delta() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    let classifier = MockAiClassifier::default();
    let result = classifier.classify(&input, &baseline).unwrap();

    let delta = (result.confidence - baseline.confidence).abs();
    assert!(delta <= 0.2, "Confidence delta {} exceeds max 0.2", delta);
}

#[test]
fn test_mock_ai_deterministic() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    let classifier = MockAiClassifier::default();
    let result1 = classifier.classify(&input, &baseline).unwrap();
    let result2 = classifier.classify(&input, &baseline).unwrap();

    assert_eq!(result1.decision, result2.decision);
    assert_eq!(result1.confidence, result2.confidence);
    assert_eq!(
        result1.supporting_evidence.len(),
        result2.supporting_evidence.len()
    );
}

#[test]
fn test_mock_ai_low_confidence_leave_unclassified() {
    let mut target = make_test_target();
    let mut ext_hist = HashMap::new();
    ext_hist.insert("xyz".to_string(), 3);
    target.extension_histogram = ext_hist;
    target.file_count = 3;
    target.dominant_extensions = vec![DominantExtension {
        extension: "xyz".to_string(),
        count: 3,
        percentage: 100.0,
    }];

    let metadata = ScanMetadata {
        _type: "scan_metadata".to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        scan_batch_id: "test-batch".to_string(),
        scan_started_at: 1234567890,
        root_path: PathBuf::from("/test"),
        limits: ScanLimits::default(),
        stats: ScanStats {
            directories_scanned: 0,
            files_encountered: 0,
            bytes_scanned: 0,
            dirs_skipped: 0,
            files_skipped: 0,
            errors: vec![],
            duration_ms: 0,
        },
    };

    let mut candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let mut cand_ext = HashMap::new();
    cand_ext.insert("mp3".to_string(), 5);
    candidate.directory.extension_histogram = cand_ext;
    candidate.directory.file_count = 5;
    candidate.directory.dominant_extensions = vec![DominantExtension {
        extension: "mp3".to_string(),
        count: 5,
        percentage: 100.0,
    }];

    let input = ClassificationInput {
        target: target.clone(),
        candidates: vec![candidate],
        metadata: metadata.clone(),
    };

    let target_input = TargetEvidence {
        evidence: target,
        user_hints: None,
    };
    let config = ClassificationConfig::default();
    let baseline = DecisionEngine::classify(&target_input, &input.candidates, &config);

    // Low score because extensions don't match
    assert!(
        baseline.confidence < 0.4,
        "Expected low confidence, got {}",
        baseline.confidence
    );

    let classifier = MockAiClassifier::default();
    let result = classifier.classify(&input, &baseline).unwrap();

    // With low confidence, mock AI should not move
    assert!(!matches!(
        result.decision,
        ClassificationDecision::MoveExisting
    ));
}

#[test]
fn test_ai_classification_error_propagation() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    let classifier = MockAiClassifier::default();
    let result = classifier.classify(&input, &baseline);
    assert!(result.is_ok());
}

#[test]
fn test_validate_ai_result_rejects_unchecked_move() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    let request = AiClassificationRequest {
        target_evidence: input.target.clone(),
        candidates: input.candidates.clone(),
        rule_based_result: baseline.clone(),
        constraints: AiClassificationConstraints::default(),
    };

    // Try to create a result that would violate validation
    let mut bad_result = baseline.clone();
    bad_result.warnings.push(Warning::PartialScanTarget);

    let validation = validate_ai_result(&bad_result, &request);
    assert!(validation.is_err());
}

#[test]
fn test_validate_ai_result_accepts_valid() {
    let candidate = make_test_candidate("Audio", &PathBuf::from("/lib/Audio"));
    let (input, baseline) = build_input_and_baseline(vec![candidate]);

    let request = AiClassificationRequest {
        target_evidence: input.target.clone(),
        candidates: input.candidates.clone(),
        rule_based_result: baseline.clone(),
        constraints: AiClassificationConstraints::default(),
    };

    let validation = validate_ai_result(&baseline, &request);
    assert!(validation.is_ok());
}

#[test]
fn test_mock_ai_no_filesystem_modification() {
    let temp = tempdir().unwrap();
    let target_dir = temp.path().join("target");
    let category_root = temp.path().join("categories");
    let candidate_dir = category_root.join("Audio");
    let candidate_child = candidate_dir.join("child_0");

    fs::create_dir_all(&target_dir).unwrap();
    fs::create_dir_all(&candidate_child).unwrap();
    for i in 0..5 {
        fs::write(target_dir.join(format!("song{:03}.mp3", i)), "x").unwrap();
    }
    for i in 0..5 {
        fs::write(candidate_dir.join(format!("track{:03}.mp3", i)), "x").unwrap();
    }

    let scanner = Scanner::new(&target_dir);
    let scan = scanner.inspect_single().unwrap();
    let target_evidence = scan.evidence.into_iter().next().unwrap();

    let candidate_scan = Scanner::new(&candidate_dir).scan().unwrap();
    let mut ev_iter = candidate_scan.evidence.into_iter();
    let cand_dir_ev = ev_iter.next().unwrap();
    let cand_children = ev_iter.collect::<Vec<_>>();

    let input = ClassificationInput {
        target: target_evidence,
        candidates: vec![CandidateEvidence {
            directory: cand_dir_ev,
            children: cand_children,
            summary: CandidateSummary {
                total_children: 0,
                top_extensions: vec!["mp3".to_string()],
                common_identifier_types: vec![],
                avg_file_count: 1.0,
            },
        }],
        metadata: ScanMetadata {
            _type: "scan_metadata".to_string(),
            schema_version: SCHEMA_VERSION.to_string(),
            scan_batch_id: "test".to_string(),
            scan_started_at: 0,
            root_path: target_dir.clone(),
            limits: ScanLimits::default(),
            stats: ScanStats {
                directories_scanned: 0,
                files_encountered: 0,
                bytes_scanned: 0,
                dirs_skipped: 0,
                files_skipped: 0,
                errors: vec![],
                duration_ms: 0,
            },
        },
    };

    let target_input = TargetEvidence {
        evidence: input.target.clone(),
        user_hints: None,
    };
    let config = ClassificationConfig::default();
    let baseline = DecisionEngine::classify(&target_input, &input.candidates, &config);

    let files_before = fs::read_dir(&candidate_dir).unwrap().count();
    let _ = MockAiClassifier::default().classify(&input, &baseline);
    let files_after = fs::read_dir(&candidate_dir).unwrap().count();

    assert_eq!(files_before, files_after);
}
