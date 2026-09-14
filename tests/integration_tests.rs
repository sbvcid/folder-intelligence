// Integration tests using fixture directories
use folder_intelligence::{Scanner, ScanLimits, DirectoryEvidence};
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
    assert_eq!(root_evidence.child_directory_names, vec!["subdir"]);
    
    let ext_hist: std::collections::HashMap<_, _> = root_evidence.extension_histogram.iter().collect();
    assert_eq!(ext_hist.get(&"txt".to_string()), Some(&2));
    assert_eq!(ext_hist.get(&"png".to_string()), Some(&1));
    assert_eq!(ext_hist.get(&"pdf".to_string()), Some(&1));
    
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
    
    assert_eq!(result.evidence.len(), 4); // root + level1a + level1b + level2 + level3 = 5? Wait, level3 is under level2
    // Actually: root, level1a, level1b, level2, level3 = 5
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
    let ext_hist: std::collections::HashMap<_, _> = unicode_evidence.extension_histogram.iter().collect();
    assert_eq!(ext_hist.get(&"txt".to_string()), Some(&1));
    assert_eq!(ext_hist.get(&"png".to_string()), Some(&1));
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
    
    let evidence = &result.evidence[0];
    assert!(evidence.file_count <= 100);
    assert!(result.stats.files_encountered <= 500);
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
    assert_eq!(evidence.child_directory_names.len(), 10);
    assert_eq!(evidence.directory_count, 100); // Actual count should still be accurate
}