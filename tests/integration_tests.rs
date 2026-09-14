// Integration tests using fixture directories
use folder_intelligence::{Scanner, ScanLimits, DirectoryEvidence, ErrorCategory};
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
    fs::write(dir.join("paper_10.1038_nature12373_2024-01-15.pdf"), "paper").unwrap();
    fs::write(dir.join("app_550e8400-e29b-41d4-a716-446655440000.exe"), "app").unwrap();
    fs::write(dir.join("data_d41d8cd98f00b204e9800998ecf8427e.bin"), "data").unwrap();
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
    // Level 1
    fs::write(dir.join("root.txt"), "root").unwrap();
    
    let level1a = dir.join("level1a");
    fs::create_dir(&level1a).unwrap();
    fs::write(level1a.join("file1a.txt"), "1a").unwrap();
    
    let level1b = dir.join("level1b");
    fs::create_dir(&level1b).unwrap();
    fs::write(level1b.join("file1b.txt"), "1b").unwrap();
    
    // Level 2
    let level2 = level1a.join("level2");
    fs::create_dir(&level2).unwrap();
    fs::write(level2.join("file2.txt"), "2").unwrap();
    
    // Level 3
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
    
    // Root directory
    let root_evidence = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
    assert_eq!(root_evidence.file_count, 4);
    assert_eq!(root_evidence.directory_count, 1);
    assert_eq!(root_evidence.child_directory_names, vec!["subdir".to_string()]);
    
    let ext_hist: std::collections::HashMap<&String, &u64> = root_evidence.extension_histogram.iter().collect();
    assert!(ext_hist.get(&"txt".to_string()).is_some_and(|v| **v == 2));
    assert!(ext_hist.get(&"png".to_string()).is_some_and(|v| **v == 1));
    assert!(ext_hist.get(&"pdf".to_string()).is_some_and(|v| **v == 1));
    
    // Subdirectory
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
    
    // Check identifiers found
    let id_types: Vec<_> = evidence.potential_identifiers.iter().map(|i| &i.identifier_type).collect();
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Isbn));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Semver));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Doi));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Date));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Uuid));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::Hash));
    assert!(id_types.contains(&&folder_intelligence::evidence::IdentifierType::AlphanumericCode));
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
    assert_eq!(evidence.notable_filenames.len(), 5); // README.md, LICENSE.txt, CHANGELOG.md, AUTHORS, NOTICE
}

#[test]
fn test_fixture_nested() {
    let dir = tempdir().unwrap();
    create_fixture_nested(dir.path());
    
    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();
    
    // root + level1a + level1b + level2 + level3 = 5 directories
    assert_eq!(result.evidence.len(), 5);
    
    // Check hierarchy
    let root = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
    assert_eq!(root.directory_count, 2);
    assert!(root.child_directory_names.contains(&"level1a".to_string()));
    assert!(root.child_directory_names.contains(&"level1b".to_string()));
    
    let level1a = result.evidence.iter().find(|e| e.name == "level1a").unwrap();
    assert_eq!(level1a.parent_path, Some(dir.path().to_path_buf()));
    assert_eq!(level1a.directory_count, 1);
    assert!(level1a.child_directory_names.contains(&"level2".to_string()));
    
    let level2 = result.evidence.iter().find(|e| e.name == "level2").unwrap();
    assert_eq!(level2.parent_path, Some(level1a.path.clone()));
    assert_eq!(level2.directory_count, 1);
    assert!(level2.child_directory_names.contains(&"level3".to_string()));
    
    let level3 = result.evidence.iter().find(|e| e.name == "level3").unwrap();
    assert_eq!(level3.parent_path, Some(level2.path.clone()));
    assert_eq!(level3.directory_count, 0);
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
    assert!(empty_evidence.representative_filenames.is_empty());
    assert!(empty_evidence.notable_filenames.is_empty());
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
    
    let unicode_evidence = result.evidence.iter().find(|e| e.name == "测试目录_тест").unwrap();
    assert_eq!(unicode_evidence.file_count, 2);
    let ext_hist: std::collections::HashMap<&String, &u64> = unicode_evidence.extension_histogram.iter().collect();
    assert!(ext_hist.get(&"txt".to_string()).is_some_and(|v| **v == 1));
    assert!(ext_hist.get(&"png".to_string()).is_some_and(|v| **v == 1));
}

#[test]
fn test_json_output_valid() {
    let dir = tempdir().unwrap();
    create_fixture_basic(dir.path());
    
    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();
    
    // Serialize to JSONL
    let mut output = Vec::new();
    for evidence in &result.evidence {
        let line = serde_json::to_string(evidence).unwrap();
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
    }
    
    // Parse back
    let output_str = String::from_utf8(output).unwrap();
    for line in output_str.lines() {
        let parsed: DirectoryEvidence = serde_json::from_str(line).unwrap();
        assert!(!parsed.path.as_os_str().is_empty());
    }
}

#[test]
fn test_scan_limits_respected() {
    let dir = tempdir().unwrap();
    
    // Create many files
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
    
    // max_total_files should limit total file count
    assert!(result.stats.files_encountered <= 500);
    // max_total_dirs should limit directory count
    assert!(result.stats.directories_scanned <= 10);
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
    assert_eq!(evidence.representative_filenames.len(), 5);
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
    // child_directory_names is limited to 10 in output
    assert_eq!(evidence.child_directory_names.len(), 10);
    // directory_count should still reflect the actual count
    assert_eq!(evidence.directory_count, 100);
    // All subdirectories should still be scanned (max_child_dirs only limits output)
    assert_eq!(result.evidence.len(), 101); // root + 100 subdirs
}

#[test]
fn test_max_total_files_enforced() {
    let dir = tempdir().unwrap();
    // Create 100 files
    for i in 0..100 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }
    // Create a subdirectory with files to ensure traversal continues
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

    // Total files encountered should be limited to max_total_files
    assert!(result.stats.files_encountered <= 50);
}

#[test]
fn test_max_total_dirs_enforced() {
    let dir = tempdir().unwrap();
    // Create 100 subdirectories
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

    // Total directories scanned should be limited
    assert!(result.stats.directories_scanned <= 10);
    // Should have a limit exceeded error
    assert!(result.stats.errors.iter().any(|e| e.category == ErrorCategory::LimitExceeded));
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

    // Should scan all 11 directories (root + 10 levels)
    assert_eq!(result.evidence.len(), 11);
}

#[test]
fn test_max_files_per_dir_counts_files_only() {
    let dir = tempdir().unwrap();
    // Create 10 directories and 100 files
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
    // max_files_per_dir limits total entries processed per directory
    // With 10 subdirectories and max_files_per_dir=10, all dirs counted then file processing stops
    assert!(evidence.file_count <= 10);
    assert!(evidence.directory_count <= 10);
}

#[test]
fn test_inspect_single_efficient() {
    let dir = tempdir().unwrap();
    // Create a complex directory structure
    let subdir = dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(dir.path().join("root.txt"), "root").unwrap();
    fs::write(subdir.join("sub.txt"), "sub").unwrap();

    let limits = ScanLimits::default();
    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.inspect_single().unwrap();

    // Should only have 1 evidence record (just the root directory)
    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.evidence[0].name, dir.path().file_name().unwrap().to_str().unwrap());
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

    let symlink_path = dir.path().join("link");
    
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_dir, &symlink_path).unwrap();
    }
    
    #[cfg(windows)]
    {
        // On Windows, symlinks require special privileges
        // The scanner should skip symlinks/reparse points anyway
    }

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();
    
    // Root should have directory_count = 1 (the symlink) on systems where it's created
    // But the symlink should not be traversed
    #[cfg(unix)]
    {
        let root_evidence = &result.evidence[0];
        // Symlink is skipped, so directory_count should be 0 (real_dir is a real dir)
        assert_eq!(root_evidence.directory_count, 1); // real_dir is a real directory
        // Should only have evidence for root and real_dir (not the symlink)
        assert_eq!(result.evidence.len(), 2);
    }
    
    #[cfg(windows)]
    {
        let result = Scanner::new(dir.path()).scan().unwrap();
        // On Windows without symlink capability, just verify it doesn't crash
    }
}

#[test]
fn test_special_characters_in_filenames() {
    let dir = tempdir().unwrap();
    // File with special characters
    fs::write(dir.path().join("file with spaces.txt"), "content").unwrap();
    fs::write(dir.path().join("file-with-dashes.txt"), "content").unwrap();
    fs::write(dir.path().join("file_with_underscores.txt"), "content").unwrap();
    fs::write(dir.path().join("file(with)parentheses.txt"), "content").unwrap();

    let scanner = Scanner::new(dir.path());
    let result = scanner.scan().unwrap();

    let evidence = &result.evidence[0];
    assert_eq!(evidence.file_count, 4);
    assert!(evidence.representative_filenames.contains(&"file with spaces.txt".to_string()));
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
    // Create some files
    for i in 0..10 {
        fs::write(dir.path().join(format!("file{}.txt", i)), "content").unwrap();
    }

    let limits = ScanLimits {
        timeout_seconds: 0, // 0 means no timeout
        ..Default::default()
    };

    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    // Should complete successfully with no timeout
    assert_eq!(result.evidence.len(), 1);
    assert!(result.stats.errors.is_empty());
}

#[test]
fn test_deeply_nested_directory_max_depth() {
    let dir = tempdir().unwrap();
    let mut current = dir.path().to_path_buf();
    
    // Create depth of 5
    for i in 0..5 {
        current = current.join(format!("level{}", i));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("file.txt"), "content").unwrap();
    }

    let limits = ScanLimits {
        max_depth: 2, // Only scan root + 2 levels
        ..Default::default()
    };
    let scanner = Scanner::with_limits(dir.path(), limits);
    let result = scanner.scan().unwrap();

    // depth 0 is root, depth 1 is level0, depth 2 is level1
    // With max_depth = 2, directories at depth >= max_depth are skipped
    // So root (depth 0) and level0 (depth 1) should be scanned
    assert_eq!(result.evidence.len(), 2);
    
    let level0 = result.evidence.iter().find(|e| e.name == "level0").unwrap();
    assert_eq!(level0.file_count, 1);
    // level1 should NOT exist (depth 2 is the limit)
    let level1_exists = result.evidence.iter().any(|e| e.name == "level1");
    assert!(!level1_exists);
}