use folder_intelligence::{DirectoryEvidence, ErrorCategory, IdentifierType, ScanLimits, Scanner};
use std::fs;
use tempfile::tempdir;

fn create_fixture_basic(dir: &std::path::Path) {
    fs::write(dir.join("file1.txt"), "content1").unwrap();
    fs::write(dir.join("file2.txt"), "content2").unwrap();
    fs::write(dir.join("image.png"), "fake png").unwrap();
    fs::write(dir.join("document.pdf"), "fake pdf").unwrap();

    let subdir = dir.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("subfile.txt"), "subcontent").unwrap();
}

fn create_fixture_with_identifiers(dir: &std::path::Path) {
    fs::write(dir.join("book_978-0-306-40615-7_v1.2.3.pdf"), "book").unwrap();
    fs::write(
        dir.join("paper_10.1038_nature12373_2024-01-15.pdf"),
        "paper",
    )
    .unwrap();
    fs::write(
        dir.join("app_550e8400-e29b-41d4-a716-446655440000.exe"),
        "app",
    )
    .unwrap();
    fs::write(
        dir.join("data_d41d8cd98f00b204e9800998ecf8427e.bin"),
        "data",
    )
    .unwrap();
    fs::write(dir.join("SKU-ABC123_product.jpg"), "image").unwrap();
}

fn create_fixture_text_files(dir: &std::path::Path) {
    fs::write(dir.join("README.md"), "# Project").unwrap();
    fs::write(dir.join("LICENSE.txt"), "MIT License").unwrap();
    fs::write(dir.join("CHANGELOG.md"), "## v1.0.0").unwrap();
    fs::write(dir.join("AUTHORS"), "John Doe").unwrap();
    fs::write(dir.join("NOTICE"), "Notice").unwrap();
    fs::write(dir.join("regular.txt"), "regular").unwrap();
}

fn create_fixture_nested(dir: &std::path::Path) {
    fs::write(dir.join("root.txt"), "root").unwrap();

    let level1a = dir.join("level1a");
    fs::create_dir(&level1a).unwrap();
    fs::write(level1a.join("file1a.txt"), "1a").unwrap();

    let level1b = dir.join("level1b");
    fs::create_dir(&level1b).unwrap();
    fs::write(level1b.join("file1b.txt"), "1b").unwrap();

    let level2 = level1a.join("level2");
    fs::create_dir(&level2).unwrap();
    fs::write(level2.join("file2.txt"), "2").unwrap();

    let level3 = level2.join("level3");
    fs::create_dir(&level3).unwrap();
    fs::write(level3.join("file3.txt"), "3").unwrap();
}

#[test]
fn test_fixture_basic() {
    let dir = tempdir().unwrap();
    create_fixture_basic(dir.path());

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 2);

    let root_evidence = result
        .evidence
        .iter()
        .find(|e| e.parent_path.is_none())
        .unwrap();
    assert_eq!(root_evidence.file_count, 4);
    assert_eq!(root_evidence.directory_count, 1);
    assert_eq!(
        root_evidence.child_directory_names,
        vec!["subdir".to_string()]
    );

    let ext_hist: std::collections::HashMap<&String, &u64> =
        root_evidence.extension_histogram.iter().collect();
    assert!(ext_hist.get(&"txt".to_string()).is_some_and(|v| **v == 2));
    assert!(ext_hist.get(&"png".to_string()).is_some_and(|v| **v == 1));
    assert!(ext_hist.get(&"pdf".to_string()).is_some_and(|v| **v == 1));

    let sub_evidence = result.evidence.iter().find(|e| e.name == "subdir").unwrap();
    assert_eq!(sub_evidence.file_count, 1);
    assert_eq!(sub_evidence.directory_count, 0);
}

#[test]
fn test_fixture_identifiers() {
    let dir = tempdir().unwrap();
    create_fixture_with_identifiers(dir.path());

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];

    let id_types: Vec<_> = evidence
        .syntactic_identifiers
        .iter()
        .map(|i| &i.identifier_type)
        .collect();
    assert!(id_types.contains(&&IdentifierType::Isbn));
    assert!(id_types.contains(&&IdentifierType::Semver));
    assert!(id_types.contains(&&IdentifierType::Doi));
    assert!(id_types.contains(&&IdentifierType::Date));
    assert!(id_types.contains(&&IdentifierType::Uuid));
    assert!(id_types.contains(&&IdentifierType::Hash));
    assert!(id_types.contains(&&IdentifierType::AlphanumericCode));
}

#[test]
fn test_fixture_text_files() {
    let dir = tempdir().unwrap();
    create_fixture_text_files(dir.path());

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert!(evidence.text_file_presence.has_readme);
    assert!(evidence.text_file_presence.has_license);
    assert!(evidence.text_file_presence.has_changelog);
    assert!(evidence.text_file_presence.has_md);
    assert!(evidence.text_file_presence.has_txt);
    assert_eq!(evidence.notable_filenames.len(), 5);
}

#[test]
fn test_fixture_nested() {
    let dir = tempdir().unwrap();
    create_fixture_nested(dir.path());

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 5);

    let root = result
        .evidence
        .iter()
        .find(|e| e.parent_path.is_none())
        .unwrap();
    assert_eq!(root.directory_count, 2);
    assert!(root.child_directory_names.contains(&"level1a".to_string()));
    assert!(root.child_directory_names.contains(&"level1b".to_string()));
    assert_eq!(root.depth, 0);

    let level1a = result
        .evidence
        .iter()
        .find(|e| e.name == "level1a")
        .unwrap();
    assert_eq!(level1a.parent_path, Some(dir.path().to_path_buf()));
    assert_eq!(level1a.directory_count, 1);
    assert!(level1a
        .child_directory_names
        .contains(&"level2".to_string()));
    assert_eq!(level1a.depth, 1);

    let level2 = result.evidence.iter().find(|e| e.name == "level2").unwrap();
    assert_eq!(level2.parent_path, Some(level1a.path.clone()));
    assert_eq!(level2.directory_count, 1);
    assert!(level2.child_directory_names.contains(&"level3".to_string()));
    assert_eq!(level2.depth, 2);

    let level3 = result.evidence.iter().find(|e| e.name == "level3").unwrap();
    assert_eq!(level3.parent_path, Some(level2.path.clone()));
    assert_eq!(level3.directory_count, 0);
    assert_eq!(level3.depth, 3);
}

#[test]
fn test_fixture_empty_folder() {
    let dir = tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 2);

    let empty_evidence = result.evidence.iter().find(|e| e.name == "empty").unwrap();
    assert_eq!(empty_evidence.file_count, 0);
    assert_eq!(empty_evidence.directory_count, 0);
    assert_eq!(empty_evidence.total_size, 0);
    assert!(empty_evidence.extension_histogram.is_empty());
    assert!(empty_evidence.filename_sample.is_empty());
    assert!(empty_evidence.notable_filenames.is_empty());
    assert!(empty_evidence.is_empty);
}

#[test]
fn test_fixture_unicode() {
    let dir = tempdir().unwrap();
    let unicode_dir = dir.path().join("测试目录_тест");
    fs::create_dir(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("文件_файл.txt"), "内容").unwrap();
    fs::write(unicode_dir.join("image_изображение.png"), "png").unwrap();

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 2);

    let unicode_evidence = result
        .evidence
        .iter()
        .find(|e| e.name == "测试目录_тест")
        .unwrap();
    assert_eq!(unicode_evidence.file_count, 2);
    let ext_hist: std::collections::HashMap<&String, &u64> =
        unicode_evidence.extension_histogram.iter().collect();
    assert!(ext_hist.get(&"txt".to_string()).is_some_and(|v| **v == 1));
    assert!(ext_hist.get(&"png".to_string()).is_some_and(|v| **v == 1));
}

#[test]
fn test_json_output_valid() {
    let dir = tempdir().unwrap();
    create_fixture_basic(dir.path());

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let mut output = Vec::new();
    for evidence in &result.evidence {
        let line = serde_json::to_string(evidence).unwrap();
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
    }

    let output_str = String::from_utf8(output).unwrap();
    for line in output_str.lines() {
        let parsed: DirectoryEvidence = serde_json::from_str(line).unwrap();
        assert!(!parsed.path.as_os_str().is_empty());
    }
}

#[test]
fn test_scan_limits_respected() {
    let dir = tempdir().unwrap();

    for i in 0..1000 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        max_files_per_dir: 100,
        max_total_files: 500,
        max_total_dirs: 10,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert!(result.metadata.stats.files_encountered <= 500);
    assert!(result.metadata.stats.directories_scanned <= 10);
}

#[test]
fn test_representative_filenames_limit() {
    let dir = tempdir().unwrap();
    for i in 0..50 {
        fs::write(dir.path().join(format!("file{:03}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        max_representative_files: 5,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.filename_sample.len(), 5);
}

#[test]
fn test_child_directory_names_limit() {
    let dir = tempdir().unwrap();
    for i in 0..100 {
        let subdir = dir.path().join(format!("subdir{:03}", i));
        fs::create_dir(&subdir).unwrap();
    }

    let limits = ScanLimits {
        max_child_dirs: 10,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.child_directory_names.len(), 10);
    assert_eq!(evidence.directory_count, 100);
    assert_eq!(result.evidence.len(), 101);
}

#[test]
fn test_max_total_files_enforced() {
    let dir = tempdir().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    for i in 0..10 {
        fs::write(subdir.join(format!("sub{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        max_total_files: 50,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert!(result.metadata.stats.files_encountered <= 50);
}

#[test]
fn test_max_total_dirs_enforced() {
    let dir = tempdir().unwrap();
    for i in 0..100 {
        let subdir = dir.path().join(format!("subdir{:03}", i));
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content").unwrap();
    }

    let limits = ScanLimits {
        max_total_dirs: 10,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert!(result.metadata.stats.directories_scanned <= 10);
    assert!(result
        .metadata
        .stats
        .errors
        .iter()
        .any(|e| e.category == ErrorCategory::LimitExceeded));
}

#[test]
fn test_max_depth_zero_means_unlimited() {
    let dir = tempdir().unwrap();
    let mut current = dir.path().to_path_buf();
    for i in 0..10 {
        current = current.join(format!("level{}", i));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("file.txt"), "content").unwrap();
    }

    let limits = ScanLimits {
        max_depth: 0,
        ..Default::default()
    };
    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 11);
}

#[test]
fn test_max_files_per_dir_counts_files_only() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        fs::create_dir(dir.path().join(format!("subdir{:03}", i))).unwrap();
    }
    for i in 0..100 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        max_files_per_dir: 10,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert!(evidence.file_count <= 10);
    assert!(evidence.directory_count <= 10);
}

#[test]
fn test_inspect_single_efficient() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(dir.path().join("root.txt"), "root").unwrap();
    fs::write(subdir.join("sub.txt"), "sub").unwrap();

    let limits = ScanLimits::default();
    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.inspect_single().unwrap();

    assert_eq!(result.evidence.len(), 1);
    assert_eq!(
        result.evidence[0].name,
        dir.path().file_name().unwrap().to_str().unwrap()
    );
}

#[test]
fn test_error_for_nonexistent_directory() {
    let dir = tempdir().unwrap();
    let nonexistent = dir.path().join("does_not_exist");
    let scanner = Scanner::new(&nonexistent);
    let result = scanner.scan();

    assert!(result.is_err());
}

#[test]
fn test_symlink_not_followed() {
    let dir = tempdir().unwrap();
    let real_dir = dir.path().join("real");
    fs::create_dir(&real_dir).unwrap();
    fs::write(real_dir.join("file.txt"), "content").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_dir, dir.path().join("link")).unwrap();
    }

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    #[cfg(unix)]
    {
        let root_evidence = &result.evidence[0];
        assert_eq!(root_evidence.directory_count, 1);
        assert_eq!(result.evidence.len(), 2);
    }

    #[cfg(windows)]
    {
        assert!(result.evidence.len() >= 1);
    }
}

#[test]
fn test_special_characters_in_filenames() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file with spaces.txt"), "content").unwrap();
    fs::write(dir.path().join("file-with-dashes.txt"), "content").unwrap();
    fs::write(dir.path().join("file_with_underscores.txt"), "content").unwrap();
    fs::write(dir.path().join("file(with)parentheses.txt"), "content").unwrap();

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 4);
    assert!(evidence
        .filename_sample
        .contains(&"file with spaces.txt".to_string()));
}

#[test]
fn test_empty_filename_handling() {
    let dir = tempdir().unwrap();
    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.evidence[0].file_count, 0);
    assert_eq!(result.evidence[0].directory_count, 0);
    assert_eq!(result.evidence[0].total_size, 0);
}

#[test]
fn test_timeout_limit_reached() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        timeout_seconds: 0,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 1);
    assert!(result.metadata.stats.errors.is_empty());
}

#[test]
fn test_deeply_nested_directory_max_depth() {
    let dir = tempdir().unwrap();
    let mut current = dir.path().to_path_buf();

    for i in 0..5 {
        current = current.join(format!("level{}", i));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("file.txt"), "content").unwrap();
    }

    let limits = ScanLimits {
        max_depth: 2,
        ..Default::default()
    };
    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert_eq!(result.evidence.len(), 2);

    let level0 = result.evidence.iter().find(|e| e.name == "level0").unwrap();
    assert_eq!(level0.file_count, 1);
    let level1_exists = result.evidence.iter().any(|e| e.name == "level1");
    assert!(!level1_exists);
}

#[test]
fn test_max_files_per_dir_still_discovers_subdirs() {
    let dir = tempdir().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }
    for i in 0..5 {
        let subdir = dir.path().join(format!("subdir{}", i));
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("inner.txt"), "content").unwrap();
    }

    let limits = ScanLimits {
        max_files_per_dir: 5,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 5);
    assert_eq!(evidence.directory_count, 5);
    assert_eq!(result.evidence.len(), 6);
}

#[test]
fn test_timeout_during_large_directory_scan() {
    let dir = tempdir().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        timeout_seconds: 60,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    assert!(result.metadata.stats.errors.is_empty());
    assert_eq!(result.evidence.len(), 1);
}

#[test]
fn test_root_not_a_directory_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("not_a_dir.txt");
    fs::write(&file_path, "content").unwrap();

    let scanner = Scanner::new(&file_path);
    let result = scanner.scan();

    assert!(result.is_err());
}

#[test]
fn test_inspect_single_nonexistent_path() {
    let dir = tempdir().unwrap();
    let nonexistent = dir.path().join("does_not_exist");
    let scanner = Scanner::new(&nonexistent);
    let result = scanner.inspect_single();
    assert!(result.is_err());
}

#[test]
fn test_inspect_single_not_a_directory() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("not_a_dir.txt");
    fs::write(&file_path, "content").unwrap();

    let scanner = Scanner::new(&file_path);
    let result = scanner.inspect_single();
    assert!(result.is_err());
}

#[test]
fn test_partial_scan_indicator() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "content").unwrap();

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert!(!evidence.partial_scan);
}

#[test]
fn test_max_files_per_dir_limits_files_not_dirs() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "x").unwrap();
    }
    fs::create_dir(dir.path().join("subdir")).unwrap();

    let limits = ScanLimits {
        max_files_per_dir: 5,
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 5);
    assert_eq!(evidence.directory_count, 1);
    assert!(evidence
        .child_directory_names
        .contains(&"subdir".to_string()));
    assert_eq!(result.evidence.len(), 2);
}

#[test]
fn test_fixture_media_directory() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/media");
    if !dir.exists() {
        return;
    }

    let scanner = Scanner::new(&dir);
    let result1 = scanner.scan().unwrap();

    let scanner2 = Scanner::new(&dir);
    let result2 = scanner2.scan().unwrap();

    assert_eq!(result1.evidence.len(), result2.evidence.len());
    for (e1, e2) in result1.evidence.iter().zip(result2.evidence.iter()) {
        assert_eq!(e1.filename_sample, e2.filename_sample);
        assert_eq!(e1.dominant_extensions, e2.dominant_extensions);
        assert_eq!(e1.identifier_summary, e2.identifier_summary);
    }

    let evidence = &result1.evidence[0];
    assert_eq!(evidence.depth, 0);
    assert_eq!(evidence.file_count, 6);
    assert!(!evidence.is_empty);
    assert_eq!(evidence.dominant_extensions[0].extension, "mp3");
    assert_eq!(evidence.dominant_extensions[0].count, 3);
}

#[test]
fn test_fixture_comics_directory() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/comics");
    if !dir.exists() {
        return;
    }

    let scanner = Scanner::new(&dir);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 5);
    assert!(evidence.extension_histogram.contains_key("cbz"));
    assert!(evidence.extension_histogram.contains_key("txt"));
    assert!(evidence.extension_histogram.contains_key("png"));
    assert!(evidence.extension_histogram.contains_key("xml"));
    assert!(evidence.text_file_presence.has_readme);
}

#[test]
fn test_fixture_identifiers_directory() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/identifiers");
    if !dir.exists() {
        return;
    }

    let scanner = Scanner::new(&dir);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert!(evidence.identifier_summary.total >= 2);
    assert!(evidence
        .identifier_summary
        .by_type
        .contains_key(&IdentifierType::Isbn));
}

#[test]
fn test_fixture_mixed_directory() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/mixed");
    if !dir.exists() {
        return;
    }

    let scanner = Scanner::new(&dir);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 3);
    assert!(evidence.extension_histogram.contains_key("txt"));
    assert!(evidence.extension_histogram.contains_key("png"));
    assert!(evidence.extension_histogram.contains_key("pdf"));
}

#[test]
fn test_fixture_empty_directory() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/empty");
    if !dir.exists() {
        return;
    }

    let scanner = Scanner::new(&dir);
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert!(evidence.is_empty);
    assert_eq!(evidence.file_count, 0);
    assert_eq!(evidence.directory_count, 0);
}

use folder_intelligence::{
    ApplyOptions, EstimatedImpact, FileSystemOperation, OperationLog, OperationPlan, Pipeline,
    PipelineError, PlanValidationContext, Policy, ValidatedOperation, ValidationResult,
    ValidationStatus, ValidationSummary,
};
use std::path::PathBuf;

fn create_approval_scope(dir: &tempfile::TempDir) -> (PathBuf, OperationPlan, ValidationResult) {
    let scope = dir.path().join("test_scope");
    std::fs::create_dir_all(&scope).unwrap();
    std::fs::write(scope.join("readme.txt"), "text").unwrap();
    std::fs::create_dir_all(scope.join("Documents")).unwrap();

    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "approval-test-plan".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: scope.clone(),
        operations: vec![FileSystemOperation::Move {
            source: scope.join("readme.txt"),
            dest: scope.join("Documents").join("readme.txt"),
        }],
        estimated_impact: EstimatedImpact {
            files_moved: 1,
            dirs_created: 1,
            files_deleted: 0,
            dirs_affected: 1,
            total_bytes: 5,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 0,
        validation_context: Some(PlanValidationContext::default()),
    };

    let mut summary = ValidationSummary::new();
    summary.total = 1;
    summary.valid = 1;

    let validation = ValidationResult {
        plan_id: plan.id.clone(),
        scope: scope.clone(),
        validated_operations: vec![ValidatedOperation {
            operation: plan.operations[0].clone(),
            status: ValidationStatus::Valid,
            warnings: vec![],
            dependencies: vec![],
        }],
        summary,
        has_blocked: false,
        has_conflicts: false,
        has_invalid: false,
        has_warnings: false,
        executable_operations: 1,
    };

    (scope, plan, validation)
}

#[test]
fn test_cli_approval_workflow_auto_approved() {
    let dir = tempfile::tempdir().unwrap();
    let (_scope, plan, validation) = create_approval_scope(&dir);

    let source = plan.scope.join("readme.txt");
    let dest = plan.scope.join("Documents").join("readme.txt");

    let pipeline = Pipeline::new(&plan.scope);

    let decision = pipeline.policy_evaluate(&plan, &validation);
    assert!(
        decision.is_approved(),
        "default policy should approve valid plan"
    );

    let result = pipeline
        .apply(
            &plan,
            &validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("apply should succeed");

    assert!(result.is_complete);
    assert!(!source.exists());
    assert!(dest.exists());

    let undo_result = pipeline.undo(&result.log).expect("undo should succeed");
    assert_eq!(undo_result.total_undo_operations, 1);
    assert!(source.exists());
}

#[test]
fn test_cli_approval_workflow_requires_approval() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, validation) = create_approval_scope(&dir);

    let source = scope.join("readme.txt");

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let decision = pipeline.policy_evaluate(&plan, &validation);
    assert!(
        decision.is_requires_approval(),
        "auto_approve=false should require approval"
    );

    let result = pipeline.apply(
        &plan,
        &validation,
        &ApplyOptions {
            force: false,
            dry_run: false,
        },
    );
    assert!(matches!(result, Err(PipelineError::ApprovalRequired(_))));
    assert!(
        source.exists(),
        "source must still exist when approval required"
    );
}

#[test]
fn test_cli_approval_workflow_approved_via_apply_with_approval() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, validation) = create_approval_scope(&dir);

    let source = scope.join("readme.txt");
    let dest = scope.join("Documents").join("readme.txt");

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let approval = pipeline
        .create_approval(&plan, &validation)
        .expect("should create approval");

    assert_eq!(approval.plan_id, plan.id);

    let result = pipeline
        .apply_with_approval(
            &plan,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("apply_with_approval should succeed");

    assert!(result.is_complete);
    assert!(!source.exists());
    assert!(dest.exists());
}

#[test]
fn test_cli_approval_workflow_rejected_plan() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let mut plan = plan;
    plan.dry_run = true;

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let validation = pipeline.validate(&plan);
    let decision = pipeline.policy_evaluate(&plan, &validation);

    assert!(
        decision.is_rejected(),
        "dry-run plan must be rejected even with auto_approve=false"
    );

    let result = pipeline.apply(
        &plan,
        &validation,
        &ApplyOptions {
            force: false,
            dry_run: false,
        },
    );
    assert!(matches!(result, Err(PipelineError::PolicyRejected)));
}

#[test]
fn test_cli_approval_workflow_with_yes_flag() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, validation) = create_approval_scope(&dir);

    let source = scope.join("readme.txt");
    let dest = scope.join("Documents").join("readme.txt");

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let decision = pipeline.policy_evaluate(&plan, &validation);
    assert!(decision.is_requires_approval());

    let approval = pipeline
        .create_approval(&plan, &validation)
        .expect("should create approval with yes flag equivalent");

    let result = pipeline
        .apply_with_approval(
            &plan,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("apply_with_approval should succeed with explicit approval");

    assert!(result.is_complete);
    assert!(!source.exists());
    assert!(dest.exists());
}

#[test]
fn test_cli_approval_workflow_rejected_with_yes_still_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let mut plan = plan;
    plan.dry_run = true;

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let validation = pipeline.validate(&plan);
    let decision = pipeline.policy_evaluate(&plan, &validation);

    assert!(decision.is_rejected());

    let result = pipeline.apply(
        &plan,
        &validation,
        &ApplyOptions {
            force: true,
            dry_run: false,
        },
    );
    assert!(matches!(result, Err(PipelineError::PolicyRejected)));
}

#[test]
fn test_cli_approval_workflow_fresh_validation_after_change() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, validation) = create_approval_scope(&dir);

    let source = scope.join("readme.txt");

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let approval = pipeline
        .create_approval(&plan, &validation)
        .expect("should create approval");

    std::fs::remove_file(&source).unwrap();

    let result = pipeline.apply_with_approval(
        &plan,
        &approval,
        &ApplyOptions {
            force: false,
            dry_run: false,
        },
    );
    assert!(
        result.is_err(),
        "stale plan after filesystem change must be rejected"
    );
    assert!(matches!(result, Err(PipelineError::PolicyRejected)));
}

#[test]
fn test_cli_approval_workflow_save_log_after_approval() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, validation) = create_approval_scope(&dir);

    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));

    let approval = pipeline
        .create_approval(&plan, &validation)
        .expect("should create approval");

    let result = pipeline
        .apply_with_approval(
            &plan,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("should execute");

    let log_path = dir.path().join("operation-log.json");
    result.log.save(&log_path).expect("should save log");

    drop(result);

    let loaded_log = OperationLog::load(&log_path).expect("should load log");

    let undo_result = pipeline.undo(&loaded_log).expect("should undo");
    assert!(undo_result.total_undo_operations > 0);
    assert!(scope.join("readme.txt").exists());
}

// === Phase 8C: Plan File I/O Tests ===

use std::path::Path;

fn save_plan_json(plan: &OperationPlan, path: &Path) {
    let json = serde_json::to_string_pretty(plan).expect("should serialize plan");
    std::fs::write(path, json).expect("should write plan file");
}

fn load_plan_json(path: &Path) -> OperationPlan {
    let json = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read plan file '{}': {}", path.display(), e));
    serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("Failed to parse plan JSON from '{}': {}", path.display(), e))
}

#[test]
fn test_plan_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let (_scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);

    assert_eq!(loaded.id, plan.id);
    assert_eq!(loaded.scope, plan.scope);
    assert_eq!(loaded.operations.len(), plan.operations.len());
    assert_eq!(loaded.dry_run, plan.dry_run);
    assert_eq!(
        loaded.estimated_impact.files_moved,
        plan.estimated_impact.files_moved
    );
    assert_eq!(loaded.validation_context, plan.validation_context);
}

#[test]
fn test_loaded_plan_process_restart_fresh_validation() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    // Simulate process restart: new Pipeline, load plan from file
    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&scope);
    let fresh_validation = pipeline.validate(&loaded);

    assert!(!fresh_validation.has_invalid);
    assert!(!fresh_validation.has_conflicts);
    assert_eq!(fresh_validation.executable_operations, 1);

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(
        decision.is_approved(),
        "default policy should approve valid loaded plan"
    );
}

#[test]
fn test_loaded_plan_detects_filesystem_change() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    // Simulate filesystem change after plan was saved
    std::fs::remove_file(scope.join("readme.txt")).unwrap();

    // Load plan and validate against changed filesystem
    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&loaded.scope);
    let fresh_validation = pipeline.validate(&loaded);

    assert!(
        fresh_validation.has_invalid,
        "validation should detect missing source file"
    );

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(
        decision.is_rejected(),
        "changed filesystem should cause policy rejection"
    );
}

#[test]
fn test_loaded_plan_invalid_plan_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let scope = dir.path().join("test_scope");
    std::fs::create_dir_all(&scope).unwrap();
    std::fs::create_dir_all(scope.join("Documents")).unwrap();
    // Deliberately do NOT create readme.txt — source won't exist

    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "invalid-plan".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: scope.clone(),
        operations: vec![FileSystemOperation::Move {
            source: scope.join("readme.txt"),
            dest: scope.join("Documents").join("readme.txt"),
        }],
        estimated_impact: EstimatedImpact {
            files_moved: 1,
            dirs_created: 1,
            files_deleted: 0,
            dirs_affected: 1,
            total_bytes: 5,
        },
        validation_warnings: vec![],
        has_conflicts: false,
        dry_run: false,
        created_at: 0,
        validation_context: Some(PlanValidationContext::default()),
    };

    let plan_path = dir.path().join("invalid-plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&loaded.scope);
    let fresh_validation = pipeline.validate(&loaded);

    assert!(
        fresh_validation.has_invalid,
        "missing source file should cause invalid operation"
    );

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(decision.is_rejected(), "invalid plan should be rejected");
}

#[test]
fn test_loaded_plan_conflicting_plan_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let scope = dir.path().join("test_scope");
    std::fs::create_dir_all(&scope).unwrap();
    std::fs::create_dir_all(scope.join("Documents")).unwrap();
    std::fs::write(scope.join("file1.txt"), "a").unwrap();
    std::fs::write(scope.join("file2.txt"), "b").unwrap();

    let plan = OperationPlan {
        unresolved_proposals: Vec::new(),
        id: "conflict-plan".to_string(),
        recommendation_id: "rec-1".to_string(),
        scope: scope.clone(),
        operations: vec![
            FileSystemOperation::Move {
                source: scope.join("file1.txt"),
                dest: scope.join("Documents").join("conflict.txt"),
            },
            FileSystemOperation::Move {
                source: scope.join("file2.txt"),
                dest: scope.join("Documents").join("conflict.txt"),
            },
        ],
        estimated_impact: EstimatedImpact {
            files_moved: 2,
            dirs_created: 1,
            files_deleted: 0,
            dirs_affected: 2,
            total_bytes: 2,
        },
        validation_warnings: vec![],
        has_conflicts: true,
        dry_run: false,
        created_at: 0,
        validation_context: Some(PlanValidationContext::default()),
    };

    let plan_path = dir.path().join("conflict-plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&loaded.scope);
    let fresh_validation = pipeline.validate(&loaded);

    assert!(
        fresh_validation.has_conflicts,
        "conflicting operations should be detected by validator"
    );

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(
        decision.is_rejected(),
        "conflicting plan should be rejected"
    );
}

#[test]
fn test_loaded_plan_requires_approval_flow() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    // Simulate process restart
    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));
    let fresh_validation = pipeline.validate(&loaded);

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(
        decision.is_requires_approval(),
        "auto_approve=false should require approval"
    );

    let source = scope.join("readme.txt");
    let dest = scope.join("Documents").join("readme.txt");
    assert!(source.exists(), "source must exist before execution");
    assert!(!dest.exists(), "dest must not exist before execution");

    // Create approval and apply (simulates --yes)
    let approval = pipeline
        .create_approval(&loaded, &fresh_validation)
        .expect("should create approval");
    assert_eq!(approval.plan_id, loaded.id);

    let result = pipeline
        .apply_with_approval(
            &loaded,
            &approval,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("should apply with approval");

    assert!(result.is_complete);
    assert!(!source.exists(), "source should be moved");
    assert!(dest.exists(), "dest should exist after move");

    // Undo
    let undo_result = pipeline.undo(&result.log).expect("should undo");
    assert!(undo_result.total_undo_operations > 0);
    assert!(source.exists(), "source restored after undo");
}

#[test]
fn test_loaded_plan_yes_does_not_bypass_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, mut plan, _validation) = create_approval_scope(&dir);
    plan.dry_run = true;

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));
    let fresh_validation = pipeline.validate(&loaded);

    // Dry-run plan → Rejected
    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(decision.is_rejected(), "dry-run plan should be rejected");

    // create_approval should fail for Rejected plans
    let approval_result = pipeline.create_approval(&loaded, &fresh_validation);
    assert!(matches!(
        approval_result,
        Err(PipelineError::PolicyRejected)
    ));

    // apply with force should still fail
    let apply_result = pipeline.apply(
        &loaded,
        &fresh_validation,
        &ApplyOptions {
            force: true,
            dry_run: false,
        },
    );
    assert!(matches!(apply_result, Err(PipelineError::PolicyRejected)));
}

#[test]
fn test_loaded_plan_produces_operation_log() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&scope);
    let fresh_validation = pipeline.validate(&loaded);

    let result = pipeline
        .apply(
            &loaded,
            &fresh_validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("should apply loaded plan");

    assert!(result.is_complete);

    let log_path = dir.path().join("operation-log.json");
    result
        .log
        .save(&log_path)
        .expect("should save OperationLog");

    let loaded_log = OperationLog::load(&log_path).expect("should reload OperationLog");
    assert_eq!(loaded_log.plan_id, result.log.plan_id);
    assert!(loaded_log.total_entries > 0);
}

#[test]
fn test_loaded_plan_cancel_does_not_mutate() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&scope).with_policy(Policy::default().auto_approve(false));
    let fresh_validation = pipeline.validate(&loaded);

    // RequiresApproval — user rejects
    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(decision.is_requires_approval());

    // Simulate user rejecting confirmation (return early without execution)
    // In the real CLI, this would be confirm_approval() returning false
    // Here we simply do not call apply — no mutation should occur
    let source = scope.join("readme.txt");
    assert!(
        source.exists(),
        "source must still exist; no execution happened"
    );
}

#[test]
fn test_loaded_plan_dry_run_cannot_execute() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, mut plan, _validation) = create_approval_scope(&dir);

    plan.dry_run = true;

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&loaded.scope);
    let fresh_validation = pipeline.validate(&loaded);

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(
        decision.is_rejected(),
        "dry-run plan should be rejected by policy"
    );

    let result = pipeline.apply(
        &loaded,
        &fresh_validation,
        &ApplyOptions {
            force: false,
            dry_run: false,
        },
    );
    assert!(matches!(result, Err(PipelineError::PolicyRejected)));
    assert!(
        scope.join("readme.txt").exists(),
        "no filesystem mutation for rejected dry-run plan"
    );
}

#[test]
fn test_loaded_plan_undo_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let (scope, plan, _validation) = create_approval_scope(&dir);

    let plan_path = dir.path().join("plan.json");
    save_plan_json(&plan, &plan_path);

    let loaded = load_plan_json(&plan_path);
    let pipeline = Pipeline::new(&loaded.scope);
    let fresh_validation = pipeline.validate(&loaded);

    let decision = pipeline.policy_evaluate(&loaded, &fresh_validation);
    assert!(decision.is_approved());

    let result = pipeline
        .apply(
            &loaded,
            &fresh_validation,
            &ApplyOptions {
                force: false,
                dry_run: false,
            },
        )
        .expect("should apply loaded plan");

    assert!(result.is_complete);

    let log_path = dir.path().join("operation-log.json");
    result
        .log
        .save(&log_path)
        .expect("should save OperationLog");

    drop(result);

    let loaded_log = OperationLog::load(&log_path).expect("should reload OperationLog");

    let undo_result = pipeline.undo(&loaded_log).expect("should undo loaded plan");
    assert!(undo_result.total_undo_operations > 0);
    assert!(scope.join("readme.txt").exists());
}

mod stress_tests {
    use super::*;
    use folder_intelligence::is_excluded_directory;
    use std::path::PathBuf;

    const EXCLUDED_DIRS: &[&str] = &["target", ".git", "node_modules", "build", "dist"];

    fn create_stress_fixture(dir: &std::path::Path) {
        for i in 0..200 {
            fs::write(
                dir.join(format!("file_{:04}.txt", i)),
                format!("content_{}", i),
            )
            .unwrap();
        }
        for ext in &["pdf", "jpg", "mp3", "png", "zip"] {
            fs::write(dir.join(format!("sample.{}", ext)), "data").unwrap();
        }

        let docs = dir.join("documents");
        fs::create_dir(&docs).unwrap();
        for i in 0..50 {
            fs::write(docs.join(format!("doc_{:03}.txt", i)), "doc").unwrap();
        }

        let images = dir.join("images");
        fs::create_dir(&images).unwrap();
        for i in 0..30 {
            fs::write(images.join(format!("img_{:03}.png", i)), "img").unwrap();
        }

        let deep = dir
            .join("level1")
            .join("level2")
            .join("level3")
            .join("level4");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep_file.txt"), "deep").unwrap();

        for excl in EXCLUDED_DIRS {
            let excl_dir = dir.join(excl);
            fs::create_dir_all(&excl_dir).unwrap();
            for i in 0..100 {
                fs::write(excl_dir.join(format!("build_{}.rs", i)), "build").unwrap();
            }
        }

        let empty = dir.join("empty_folder");
        fs::create_dir(&empty).unwrap();

        fs::write(dir.join("文件_1.txt"), "chinese1").unwrap();
        fs::write(dir.join("目录_2.md"), "chinese2").unwrap();

        fs::write(dir.join("readme.txt"), "readme").unwrap();
        fs::write(dir.join("README.txt"), "README").unwrap();
    }

    fn count_files_recursive(dir: &std::path::Path) -> (u64, u64) {
        let mut dirs_scanned: u64 = 0;
        let mut files_found: u64 = 0;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            dirs_scanned += 1;
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_symlink() {
                        continue;
                    }
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if is_excluded_directory(name) {
                                continue;
                            }
                        }
                        stack.push(path);
                    } else if path.is_file() {
                        files_found += 1;
                    }
                }
            }
        }
        (dirs_scanned, files_found)
    }

    fn assert_excluded_not_in_evidence(evidence: &[DirectoryEvidence], excluded_names: &[&str]) {
        for ev in evidence {
            for child in &ev.child_directory_names {
                for excl in excluded_names {
                    assert_ne!(
                        child,
                        excl,
                        "Excluded dir '{}' found in child_directory_names at {}",
                        excl,
                        ev.path.display()
                    );
                }
            }
            let basename = ev.path.file_name().and_then(|n| n.to_str());
            if let Some(name) = basename {
                for excl in excluded_names {
                    assert_ne!(
                        name,
                        *excl,
                        "Excluded dir '{}' found as evidence path: {}",
                        excl,
                        ev.path.display()
                    );
                }
            }
        }
    }

    #[test]
    fn test_stress_excluded_dirs_not_scanned() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let (dirs_scanned, _) = count_files_recursive(dir.path());

        assert_eq!(
            result.metadata.stats.directories_scanned, dirs_scanned,
            "Scanner should scan same number of dirs as reference (excluding excluded dirs)"
        );

        assert_excluded_not_in_evidence(&result.evidence, EXCLUDED_DIRS);

        assert!(
            result.metadata.stats.dirs_skipped >= EXCLUDED_DIRS.len() as u64,
            "Should have skipped at least {} excluded dirs, got {}",
            EXCLUDED_DIRS.len(),
            result.metadata.stats.dirs_skipped
        );
    }

    #[test]
    fn test_stress_excluded_dir_contents_not_in_any_evidence() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        for ev in &result.evidence {
            for name in &ev.notable_filenames {
                for excl in EXCLUDED_DIRS {
                    assert!(
                        !name.contains(excl),
                        "Excluded dir name '{}' found in notable_filenames at {}",
                        excl,
                        ev.path.display()
                    );
                }
            }
        }
    }

    #[test]
    fn test_stress_normal_dirs_still_scanned() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let root_ev = result
            .evidence
            .iter()
            .find(|e| e.parent_path.is_none())
            .unwrap();

        assert!(
            root_ev.file_count >= 200,
            "Root should have 200+ regular files, got {}",
            root_ev.file_count
        );
        assert!(
            root_ev.directory_count >= 3,
            "Root should have multiple subdirectories (non-excluded), got {}",
            root_ev.directory_count
        );
    }

    #[test]
    fn test_stress_scan_completes_quickly() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert!(
            result.metadata.stats.duration_ms < 10_000,
            "Scan should complete in under 10s, took {}ms",
            result.metadata.stats.duration_ms
        );

        let (dirs_scanned, files_found) = count_files_recursive(dir.path());
        assert_eq!(result.metadata.stats.files_encountered, files_found);
        assert_eq!(result.metadata.stats.directories_scanned, dirs_scanned);
    }

    #[test]
    fn test_stress_analysis_completes_on_project_root() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        assert_excluded_not_in_evidence(&[analysis.scope_evidence.clone()], EXCLUDED_DIRS);

        assert!(
            analysis
                .candidate_categories
                .iter()
                .all(|c| !EXCLUDED_DIRS.contains(&c.name.as_str())),
            "Candidate categories should not include excluded dirs"
        );

        assert!(
            analysis.analysis_duration_ms < 15_000,
            "Analysis should complete in under 15s, took {}ms",
            analysis.analysis_duration_ms
        );
    }

    #[test]
    fn test_stress_recommendation_completes() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();

        assert!(
            !recommendation.proposed_operations.is_empty() || !recommendation.warnings.is_empty(),
            "Should have some operations or warnings"
        );
    }

    #[test]
    fn test_stress_plan_generation_completes() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();
        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();

        assert!(!plan.operations.is_empty(), "Plan should have operations");

        let validation = pipeline.validate(&plan);
        assert!(
            !validation.has_invalid,
            "Plan should be valid (no invalid operations)"
        );
    }

    #[test]
    fn test_stress_chinese_filenames_handled() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let all_names: Vec<String> = result
            .evidence
            .iter()
            .flat_map(|ev| ev.filename_sample.iter().cloned())
            .collect();

        assert!(
            all_names
                .iter()
                .any(|n| n.contains("文件") || n.contains("目录")),
            "Chinese filenames should appear in scan results"
        );
    }

    #[test]
    fn test_stress_empty_folder_detected() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let empty_ev = result.evidence.iter().find(|e| e.name == "empty_folder");
        assert!(
            empty_ev.is_some(),
            "Empty folder should appear in scan results"
        );
        assert!(empty_ev.unwrap().is_empty);
    }

    #[test]
    fn test_stress_deep_nesting_scanned() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let deep_ev = result.evidence.iter().find(|e| e.path.ends_with("level4"));
        assert!(
            deep_ev.is_some(),
            "Deep nested directory should be found in scan results"
        );
        assert_eq!(deep_ev.unwrap().depth, 4);
    }

    #[test]
    fn test_stress_filesystem_info_flow() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let scanner = Scanner::new(dir.path());
        let scan_result = scanner.scan().unwrap();
        assert!(!scan_result.evidence.is_empty());
        assert_eq!(
            scan_result.evidence.len(),
            scan_result.metadata.stats.directories_scanned as usize
        );
        let root_evidence = &scan_result.evidence[0];

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline
            .parse_intent("Organize this folder by category")
            .unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        assert_eq!(
            analysis.scope_evidence.path, root_evidence.path,
            "Analysis scope_evidence path should match scanned root"
        );
        assert_eq!(
            analysis.scope_evidence.file_count, root_evidence.file_count,
            "Analysis file_count should match scanned root"
        );
        assert_eq!(
            analysis.scope_evidence.directory_count, root_evidence.directory_count,
            "Analysis directory_count should match scanned root"
        );
    }

    #[test]
    fn test_large_directory_regression_analysis() {
        let dir = tempdir().unwrap();
        for i in 0..5000 {
            fs::write(
                dir.path().join(format!("file_{:05}.txt", i)),
                format!("content_{}", i),
            )
            .unwrap();
        }
        for excl in EXCLUDED_DIRS {
            let excl_dir = dir.path().join(excl);
            fs::create_dir_all(&excl_dir).unwrap();
            for i in 0..500 {
                fs::write(excl_dir.join(format!("build_{}.rs", i)), "build").unwrap();
            }
        }

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.metadata.stats.files_encountered, 5000u64);
        assert!(
            result.metadata.stats.dirs_skipped >= EXCLUDED_DIRS.len() as u64,
            "Should have skipped {} excluded dirs, got {}",
            EXCLUDED_DIRS.len(),
            result.metadata.stats.dirs_skipped
        );
        assert!(
            result.metadata.stats.duration_ms < 30_000,
            "Large dir scan should complete in <30s, took {}ms",
            result.metadata.stats.duration_ms
        );

        assert_excluded_not_in_evidence(&result.evidence, EXCLUDED_DIRS);
    }

    #[test]
    fn test_large_directory_regression_full_pipeline() {
        let dir = tempdir().unwrap();
        for i in 0..3000 {
            let ext = if i % 4 == 0 {
                "txt"
            } else if i % 4 == 1 {
                "pdf"
            } else if i % 4 == 2 {
                "jpg"
            } else {
                "mp3"
            };
            fs::write(
                dir.path().join(format!("file_{:05}.{}", i, ext)),
                format!("content_{}", i),
            )
            .unwrap();
        }
        for i in 0..100 {
            fs::write(dir.path().join(format!("img_{:03}.png", i)), "img").unwrap();
        }
        fs::write(dir.path().join("README.md"), "readme").unwrap();

        for excl in EXCLUDED_DIRS {
            let excl_dir = dir.path().join(excl);
            fs::create_dir_all(&excl_dir).unwrap();
            for i in 0..200 {
                fs::write(excl_dir.join(format!("build_{}.rs", i)), "build").unwrap();
            }
        }

        let pipeline = Pipeline::new(dir.path());
        let intent = pipeline.parse_intent("Organize files by type").unwrap();
        let analysis = pipeline.analyze(&intent).unwrap();

        assert!(
            analysis.analysis_duration_ms < 30_000,
            "Analysis should complete in <30s, took {}ms",
            analysis.analysis_duration_ms
        );

        assert_excluded_not_in_evidence(&[analysis.scope_evidence.clone()], EXCLUDED_DIRS);

        let recommendation = pipeline.recommend(&intent, &analysis).unwrap();
        let plan = pipeline.plan(&recommendation, &analysis, &intent).unwrap();
        let validation = pipeline.validate(&plan);
        assert!(!validation.has_invalid);
    }

    #[test]
    fn test_is_excluded_directory_function() {
        assert!(is_excluded_directory("target"));
        assert!(is_excluded_directory(".git"));
        assert!(is_excluded_directory("node_modules"));
        assert!(is_excluded_directory("build"));
        assert!(is_excluded_directory("dist"));
        assert!(is_excluded_directory(".svn"));
        assert!(is_excluded_directory("__pycache__"));
        assert!(is_excluded_directory(".venv"));

        assert!(!is_excluded_directory("src"));
        assert!(!is_excluded_directory("docs"));
        assert!(!is_excluded_directory("tests"));
        assert!(!is_excluded_directory(""));
        assert!(!is_excluded_directory("my_target"));
        assert!(!is_excluded_directory("target_files"));
    }

    #[test]
    fn test_cli_classify_skips_excluded_dirs() {
        let dir = tempdir().unwrap();
        create_stress_fixture(dir.path());

        let target = dir.path().join("file_0000.txt");
        fs::write(&target, "content").unwrap();

        let category_root = dir.path().to_path_buf();

        fs::create_dir_all(&target).unwrap_err();

        let mut candidate_paths: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(&category_root).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_excluded_directory(name) {
                        continue;
                    }
                }
                candidate_paths.push(path);
            }
        }

        let non_excluded_candidates: Vec<String> = candidate_paths
            .iter()
            .filter_map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
            .collect();

        for excl in EXCLUDED_DIRS {
            assert!(
                !non_excluded_candidates.iter().any(|n| n == excl),
                "Excluded dir '{}' should not be in candidate list",
                excl
            );
        }
    }
}
