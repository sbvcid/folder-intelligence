use folder_intelligence::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

const FIXTURES_DIR: &str = "fixtures";

fn make_target_evidence(name: &str, path: &PathBuf) -> DirectoryEvidence {
    let mut ext_hist = HashMap::new();
    ext_hist.insert("mp3".to_string(), 3);
    ext_hist.insert("jpg".to_string(), 1);
    ext_hist.insert("txt".to_string(), 1);

    DirectoryEvidence {
        path: path.clone(),
        name: name.to_string(),
        parent_path: None,
        depth: 0,
        file_count: 5,
        directory_count: 0,
        total_size: 5120,
        extension_histogram: ext_hist,
        dominant_extensions: vec![
            DominantExtension {
                extension: "mp3".to_string(),
                count: 3,
                percentage: 60.0,
            },
            DominantExtension {
                extension: "jpg".to_string(),
                count: 1,
                percentage: 20.0,
            },
            DominantExtension {
                extension: "txt".to_string(),
                count: 1,
                percentage: 20.0,
            },
        ],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec![
            "001.mp3".to_string(),
            "002.mp3".to_string(),
            "003.mp3".to_string(),
            "cover.jpg".to_string(),
            "README.txt".to_string(),
        ],
        notable_filenames: vec!["README.txt".to_string()],
        syntactic_identifiers: vec![],
        text_file_presence: TextFilePresence {
            has_readme: true,
            has_nfo: false,
            has_txt: true,
            has_md: false,
            has_license: false,
            has_changelog: false,
            text_files_found: vec!["README.txt".to_string()],
        },
        is_empty: false,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 5,
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

fn make_candidate_evidence(name: &str, path: &PathBuf, ext: &str, count: u64) -> DirectoryEvidence {
    let mut ext_hist = HashMap::new();
    ext_hist.insert(ext.to_string(), count);

    let percentage = if count > 0 { 100.0 } else { 0.0 };
    DirectoryEvidence {
        path: path.clone(),
        name: name.to_string(),
        parent_path: Some(PathBuf::from("/test")),
        depth: 1,
        file_count: count,
        directory_count: 0,
        total_size: 1024 * count as u64,
        extension_histogram: ext_hist,
        dominant_extensions: vec![DominantExtension {
            extension: ext.to_string(),
            count,
            percentage,
        }],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec![],
        notable_filenames: vec![],
        syntactic_identifiers: vec![],
        text_file_presence: TextFilePresence::default(),
        is_empty: count == 0,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 1,
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

fn make_candidate_with_children(
    name: &str,
    path: &PathBuf,
    ext: &str,
    child_count: usize,
    files_per_child: u64,
) -> CandidateEvidence {
    // Candidate directory is a category root — its own file_count is 0 (just a container),
    // children hold the actual content
    let main = make_candidate_evidence(name, path, ext, 0);
    let children: Vec<_> = (0..child_count.min(SAMPLE_CHILD_DIRS))
        .map(|i| {
            make_candidate_evidence(
                &format!("child_{}", i),
                &path.join(format!("child_{}", i)),
                ext,
                files_per_child,
            )
        })
        .collect();

    let total_children = children.len();
    let mut ext_counts: HashMap<String, u64> = HashMap::new();
    let mut file_sum: u64 = 0;
    for c in &children {
        file_sum += c.file_count;
        for (e, cnt) in &c.extension_histogram {
            *ext_counts.entry(e.clone()).or_insert(0) += cnt;
        }
    }
    // Include the directory's own extensions too
    for (e, cnt) in &main.extension_histogram {
        *ext_counts.entry(e.clone()).or_insert(0) += cnt;
    }
    let mut top: Vec<_> = ext_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    let top_extensions = top.into_iter().take(5).map(|(e, _)| e).collect();

    CandidateEvidence {
        directory: main,
        children,
        summary: CandidateSummary {
            total_children,
            top_extensions,
            common_identifier_types: vec![],
            avg_file_count: if total_children > 0 {
                file_sum as f64 / total_children as f64
            } else {
                0.0
            },
        },
    }
}

fn make_target_input(name: &str, path: &PathBuf) -> TargetEvidence {
    TargetEvidence {
        evidence: make_target_evidence(name, path),
        user_hints: None,
    }
}

#[test]
fn test_classification_move_existing_for_matching_candidate() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    if !fixtures.join("media").exists() {
        return;
    }

    let candidate = make_candidate_with_children("Audio", &fixtures.join("media"), "mp3", 1, 5);

    // Build target evidence matching mp3 media
    let mut target_ev = make_target_evidence("target", &fixtures.join("media"));
    target_ev.extension_histogram = HashMap::new();
    target_ev.extension_histogram.insert("mp3".to_string(), 5);
    target_ev.file_count = 5;
    target_ev.identifier_summary = IdentifierSummary::default();

    let target_input = TargetEvidence {
        evidence: target_ev,
        user_hints: None,
    };

    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[candidate], &config);

    assert_eq!(result.decision, ClassificationDecision::MoveExisting);
    assert!(result.selected_candidate.is_some());
    assert!(result.confidence >= 0.8);
    assert_eq!(result.candidates_considered, 1);
}

#[test]
fn test_classification_move_existing_comics_fixture() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    if !fixtures.join("comics").exists() {
        return;
    }

    let _target = make_target_input("target_comics", &fixtures.join("comics"));
    let candidate = make_candidate_with_children("Comics", &fixtures.join("comics"), "cbz", 1, 5);

    // Build target evidence matching cbz comics
    let mut ext_hist = HashMap::new();
    ext_hist.insert("cbz".to_string(), 5);

    let target_ev = DirectoryEvidence {
        path: fixtures.join("comics").clone(),
        name: "target_comics".to_string(),
        parent_path: None,
        depth: 0,
        file_count: 5,
        directory_count: 0,
        total_size: 5120,
        extension_histogram: ext_hist,
        dominant_extensions: vec![DominantExtension {
            extension: "cbz".to_string(),
            count: 5,
            percentage: 100.0,
        }],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec![
            "RJ01547914.cbz".to_string(),
            "RJ01547915.cbz".to_string(),
            "RJ01547916.cbz".to_string(),
            "RJ01547917.cbz".to_string(),
            "RJ01547918.cbz".to_string(),
        ],
        notable_filenames: vec![],
        syntactic_identifiers: vec![
            SyntacticIdentifier {
                value: "RJ01547914".to_string(),
                source_filename: "RJ01547914.cbz".to_string(),
                identifier_type: IdentifierType::AlphanumericCode,
            },
            SyntacticIdentifier {
                value: "RJ01547915".to_string(),
                source_filename: "RJ01547915.cbz".to_string(),
                identifier_type: IdentifierType::AlphanumericCode,
            },
        ],
        text_file_presence: TextFilePresence::default(),
        is_empty: false,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 5,
        schema_version: SCHEMA_VERSION.to_string(),
    };

    let target_input = TargetEvidence {
        evidence: target_ev,
        user_hints: None,
    };

    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[candidate], &config);

    assert_eq!(result.decision, ClassificationDecision::MoveExisting);
    assert!(result.selected_candidate.is_some());
    assert!(result.confidence >= 0.8);
}

#[test]
fn test_classification_no_candidates() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    let target_input = make_target_input("target", &fixtures.join("media"));
    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[], &config);

    // Target has content → CREATE_CATEGORY
    assert_eq!(result.decision, ClassificationDecision::CreateCategory);
    assert!(result.selected_candidate.is_none());
    assert!(result.proposed_category_name.is_some());
    assert_eq!(result.candidates_considered, 0);
    assert!(result
        .uncertainty
        .contains(&UncertaintyReason::InsufficientPrecedent));
}

#[test]
fn test_classification_empty_target_no_candidates() {
    let empty_ev = DirectoryEvidence {
        path: PathBuf::from("/test/empty"),
        name: "empty_target".to_string(),
        parent_path: None,
        depth: 0,
        file_count: 0,
        directory_count: 0,
        total_size: 0,
        extension_histogram: HashMap::new(),
        dominant_extensions: vec![],
        identifier_summary: IdentifierSummary::default(),
        child_directory_names: vec![],
        filename_sample: vec![],
        notable_filenames: vec![],
        syntactic_identifiers: vec![],
        text_file_presence: TextFilePresence::default(),
        is_empty: true,
        partial_scan: false,
        scanned_at: 1234567890,
        scan_duration_ms: 0,
        schema_version: SCHEMA_VERSION.to_string(),
    };

    let target_input = TargetEvidence {
        evidence: empty_ev,
        user_hints: None,
    };
    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[], &config);

    assert_eq!(result.decision, ClassificationDecision::LeaveUnclassified);
    assert_eq!(result.candidates_considered, 0);
}

#[test]
fn test_classification_competing_candidates_ask_user() {
    // Two candidates with identical extensions and file counts — very close scores
    let candidate_a = CandidateEvidence {
        directory: make_candidate_evidence("MusicA", &PathBuf::from("/lib/MusicA"), "mp3", 5),
        children: vec![make_candidate_evidence(
            "child1",
            &PathBuf::from("/lib/MusicA/child1"),
            "mp3",
            3,
        )],
        summary: CandidateSummary {
            total_children: 1,
            top_extensions: vec!["mp3".to_string()],
            common_identifier_types: vec![],
            avg_file_count: 3.0,
        },
    };

    let candidate_b = CandidateEvidence {
        directory: make_candidate_evidence("MusicB", &PathBuf::from("/lib/MusicB"), "mp3", 5),
        children: vec![make_candidate_evidence(
            "child1",
            &PathBuf::from("/lib/MusicB/child1"),
            "mp3",
            3,
        )],
        summary: CandidateSummary {
            total_children: 1,
            top_extensions: vec!["mp3".to_string()],
            common_identifier_types: vec![],
            avg_file_count: 3.0,
        },
    };

    let target_ev = make_target_evidence("target", &PathBuf::from("/unclassified/target"));
    let target_input = TargetEvidence {
        evidence: target_ev,
        user_hints: None,
    };

    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[candidate_a, candidate_b], &config);

    // Two nearly identical candidates → ask user
    assert!(matches!(result.decision, ClassificationDecision::AskUser));
    assert!(result
        .uncertainty
        .contains(&UncertaintyReason::AmbiguousCandidates));
}

#[test]
fn test_classification_partial_scan_target_never_move_existing() {
    let mut target_ev = make_target_evidence("target", &PathBuf::from("/unclassified/target"));
    target_ev.partial_scan = true;

    let target_input = TargetEvidence {
        evidence: target_ev,
        user_hints: None,
    };

    // Create a strong candidate that would normally trigger MOVE_EXISTING
    let candidate =
        make_candidate_with_children("Audio", &PathBuf::from("/lib/Audio"), "mp3", 10, 5);

    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[candidate], &config);

    // Partial scan on target → never MOVE_EXISTING
    assert!(!matches!(
        result.decision,
        ClassificationDecision::MoveExisting
    ));
    assert!(result.warnings.contains(&Warning::PartialScanTarget));
}

#[test]
fn test_classification_determinism() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    let target_input = make_target_input("target", &fixtures.join("media"));

    let candidate = make_candidate_with_children("Audio", &fixtures.join("media"), "mp3", 1, 5);

    let config = ClassificationConfig::default();
    let result1 = DecisionEngine::classify(&target_input, &[candidate.clone()], &config);
    let target_clone = make_target_input("target", &fixtures.join("media"));
    let result2 = DecisionEngine::classify(&target_clone, &[candidate], &config);

    assert_eq!(result1.decision, result2.decision);
    assert_eq!(result1.confidence, result2.confidence);
    assert_eq!(
        result1.supporting_evidence.len(),
        result2.supporting_evidence.len()
    );
}

#[test]
fn test_classification_no_filesystem_modification() {
    let temp = tempdir().unwrap();
    let target_dir = temp.path().join("target");
    let category_root = temp.path().join("categories");
    let candidate_dir = category_root.join("Audio");

    fs::create_dir_all(&target_dir).unwrap();
    fs::create_dir_all(&candidate_dir).unwrap();

    // Create target files
    for i in 0..5 {
        fs::write(target_dir.join(format!("song{:03}.mp3", i)), "x").unwrap();
    }

    // Create candidate files (matching extensions)
    for i in 0..5 {
        fs::write(candidate_dir.join(format!("track{:03}.mp3", i)), "x").unwrap();
    }

    // Record file listings before classification
    let target_files_before: Vec<_> = fs::read_dir(&target_dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let candidate_files_before: Vec<_> = fs::read_dir(&candidate_dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    // Scan and classify
    let scanner = Scanner::new(&target_dir);
    let scan = scanner.inspect_single().unwrap();
    let target_evidence = scan.evidence.into_iter().next().unwrap();

    let target_input = TargetEvidence {
        evidence: target_evidence,
        user_hints: None,
    };

    let config = ClassificationConfig::default();

    // Build candidate manually
    let cand_scanner = Scanner::new(&candidate_dir);
    let cand_scan = cand_scanner.scan().unwrap();
    let mut ev_iter = cand_scan.evidence.into_iter();
    let cand_dir_ev = ev_iter.next().unwrap();
    let cand_children = ev_iter.collect::<Vec<_>>();

    let candidate = CandidateEvidence {
        directory: cand_dir_ev,
        children: cand_children,
        summary: CandidateSummary {
            total_children: 0,
            top_extensions: vec!["mp3".to_string()],
            common_identifier_types: vec![],
            avg_file_count: 5.0,
        },
    };

    let _ = DecisionEngine::classify(&target_input, &[candidate], &config);

    // Verify filesystem unchanged
    let target_files_after: Vec<_> = fs::read_dir(&target_dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let candidate_files_after: Vec<_> = fs::read_dir(&candidate_dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(target_files_before.len(), target_files_after.len());
    assert_eq!(candidate_files_before.len(), candidate_files_after.len());
}

#[test]
fn test_classification_result_schema_version() {
    let target_input = make_target_input("target", &PathBuf::from("/test/target"));
    let config = ClassificationConfig::default();
    let result = DecisionEngine::classify(&target_input, &[], &config);

    assert_eq!(result.schema_version, "3.0.0");
    assert_eq!(result.confidence_band, ConfidenceBand::Low);
    assert_eq!(result.provider, None);
    assert_eq!(result.model, None);
}

#[test]
fn test_confidence_band_thresholds() {
    assert_eq!(ConfidenceBand::from_confidence(0.9), ConfidenceBand::High);
    assert_eq!(ConfidenceBand::from_confidence(0.8), ConfidenceBand::High);
    assert_eq!(ConfidenceBand::from_confidence(0.7), ConfidenceBand::Medium);
    assert_eq!(ConfidenceBand::from_confidence(0.4), ConfidenceBand::Medium);
    assert_eq!(ConfidenceBand::from_confidence(0.3), ConfidenceBand::Low);
    assert_eq!(ConfidenceBand::from_confidence(0.0), ConfidenceBand::Low);
}

#[test]
fn test_classification_input_from_directory() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR);
    let media_dir = fixtures.join("media");
    if !media_dir.exists() {
        return;
    }

    let scanner = Scanner::new(&media_dir);
    let scan = scanner.inspect_single().unwrap();
    let target_evidence = scan.evidence.into_iter().next().unwrap();

    let category_root = fixtures.join("media");
    let candidate_dirs: Vec<PathBuf> = if category_root.is_dir() {
        std::fs::read_dir(&category_root)
            .unwrap()
            .filter_map(|e| {
                let e = e.ok()?;
                let p = e.path();
                if p.is_dir() {
                    Some(p)
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]
    };

    let input = ClassificationInput::from_directory_evidence(
        target_evidence.clone(),
        &candidate_dirs,
        scan.metadata,
    )
    .unwrap();

    assert_eq!(input.target.path, target_evidence.path);
    assert!(input.candidates.len() <= MAX_CANDIDATES);
    for c in &input.candidates {
        assert!(c.children.len() <= SAMPLE_CHILD_DIRS);
    }
}
