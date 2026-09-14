use folder_intelligence::evidence::*;
use folder_intelligence::scanner::*;

#[cfg(test)]
mod identifier_tests {
    use super::*;

    #[test]
    fn test_extract_isbn10() {
        let result = extract_isbn10("book_0-306-40615-2.pdf");
        assert_eq!(result, Some("0-306-40615-2".to_string()));

        let result = extract_isbn10("0306406152.txt");
        assert_eq!(result, Some("0306406152".to_string()));

        let result = extract_isbn10("book_0-306-40615-X.epub");
        assert_eq!(result, Some("0-306-40615-X".to_string()));

        let result = extract_isbn10("no_isbn_here.txt");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_isbn13() {
        let result = extract_isbn13("book_978-0-306-40615-7.pdf");
        assert_eq!(result, Some("978-0-306-40615-7".to_string()));

        let result = extract_isbn13("9780306406157.txt");
        assert_eq!(result, Some("9780306406157".to_string()));

        let result = extract_isbn13("not_an_isbn.txt");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_doi() {
        let result = extract_doi("paper_10.1038_nature12373.pdf");
        assert_eq!(result, Some("10.1038/nature12373".to_string()));

        let result = extract_doi("10.1109/5.771073.txt");
        assert_eq!(result, Some("10.1109/5.771073".to_string()));
    }

    #[test]
    fn test_extract_uuid() {
        let result = extract_uuid("file_550e8400-e29b-41d4-a716-446655440000.log");
        assert_eq!(result, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));

        let result = extract_uuid("550E8400-E29B-41D4-A716-446655440000.txt");
        assert_eq!(result, Some("550E8400-E29B-41D4-A716-446655440000".to_string()));
    }

    #[test]
    fn test_extract_semver() {
        let result = extract_semver("app_v1.2.3.tar.gz");
        assert_eq!(result, Some("v1.2.3".to_string()));

        let result = extract_semver("lib-2.0.0-beta.1.zip");
        assert_eq!(result, Some("2.0.0-beta.1".to_string()));

        let result = extract_semver("release_1.0.0+build.123.exe");
        assert_eq!(result, Some("1.0.0+build.123".to_string()));
    }

    #[test]
    fn test_extract_hash() {
        // MD5
        let result = extract_hash("file_d41d8cd98f00b204e9800998ecf8427e.txt");
        assert_eq!(result, Some("d41d8cd98f00b204e9800998ecf8427e".to_string()));

        // SHA1
        let result = extract_hash("file_da39a3ee5e6b4b0d3255bfef95601890afd80709.txt");
        assert_eq!(result, Some("da39a3ee5e6b4b0d3255bfef95601890afd80709".to_string()));

        // SHA256
        let result = extract_hash("file_e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.txt");
        assert_eq!(result, Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()));
    }

    #[test]
    fn test_extract_date() {
        let result = extract_date("photo_2024-01-15.jpg");
        assert_eq!(result, Some("2024-01-15".to_string()));

        let result = extract_date("backup_20240115.tar.gz");
        assert_eq!(result, Some("20240115".to_string()));

        let result = extract_date("report_15-01-2024.pdf");
        assert_eq!(result, Some("15-01-2024".to_string()));
    }

    #[test]
    fn test_extract_email() {
        let result = extract_email("contact_john.doe@example.com.txt");
        assert_eq!(result, Some("john.doe@example.com".to_string()));
    }

    #[test]
    fn test_extract_url() {
        let result = extract_url("ref_https://example.com/path.txt");
        assert_eq!(result, Some("https://example.com/path".to_string()));
    }

    #[test]
    fn test_extract_alphanumeric_codes() {
        let result = extract_alphanumeric_codes("SKU-001_product.pdf");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "SKU-001");
        assert_eq!(result[0].identifier_type, IdentifierType::AlphanumericCode);

        let result = extract_alphanumeric_codes("ABC123_file.txt");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "ABC123");

        let result = extract_alphanumeric_codes("PROD-2024-001_data.csv");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "PROD-2024-001");
    }

    #[test]
    fn test_extract_multiple_identifiers() {
        let filename = "book_ISBN-978-0-306-40615-7_v1.2.3_2024-01-15.pdf";
        let identifiers = extract_identifiers(filename);
        
        // Should find ISBN, semver, and date
        assert!(identifiers.iter().any(|i| i.identifier_type == IdentifierType::Isbn));
        assert!(identifiers.iter().any(|i| i.identifier_type == IdentifierType::Semver));
        assert!(identifiers.iter().any(|i| i.identifier_type == IdentifierType::Date));
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn test_directory_evidence_serialization() {
        let mut ext_hist = HashMap::new();
        ext_hist.insert("txt".to_string(), 5);
        ext_hist.insert("pdf".to_string(), 3);

        let evidence = DirectoryEvidence {
            path: PathBuf::from("/test/dir"),
            name: "dir".to_string(),
            parent_path: Some(PathBuf::from("/test")),
            file_count: 8,
            directory_count: 2,
            total_size: 1024,
            extension_histogram: ext_hist,
            child_directory_names: vec!["sub1".to_string(), "sub2".to_string()],
            representative_filenames: vec!["file1.txt".to_string(), "file2.pdf".to_string()],
            notable_filenames: vec!["README.md".to_string()],
            potential_identifiers: vec![],
            text_file_presence: TextFilePresence {
                has_readme: true,
                has_nfo: false,
                has_txt: true,
                has_md: true,
                has_license: false,
                has_changelog: false,
                text_files_found: vec!["README.md".to_string()],
            },
            scanned_at: 1234567890,
            scan_duration_ms: 100,
        };

        let json = serde_json::to_string(&evidence).unwrap();
        let parsed: DirectoryEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(evidence, parsed);
    }

    #[test]
    fn test_text_file_presence_default() {
        let presence = TextFilePresence::default();
        assert!(!presence.has_readme);
        assert!(!presence.has_nfo);
        assert!(!presence.has_txt);
        assert!(!presence.has_md);
        assert!(!presence.has_license);
        assert!(!presence.has_changelog);
        assert!(presence.text_files_found.is_empty());
    }

    #[test]
    fn test_scan_limits_default() {
        let limits = ScanLimits::default();
        assert_eq!(limits.max_depth, 50);
        assert_eq!(limits.max_files_per_dir, 10_000);
        assert_eq!(limits.max_total_files, 1_000_000);
        assert_eq!(limits.max_total_dirs, 100_000);
        assert_eq!(limits.max_representative_files, 20);
        assert_eq!(limits.max_child_dirs, 500);
        assert_eq!(limits.timeout_seconds, 3600);
    }
}

#[cfg(test)]
mod scanner_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_empty_directory() {
        let dir = tempdir().unwrap();
        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence.len(), 1);
        assert_eq!(result.evidence[0].file_count, 0);
        assert_eq!(result.evidence[0].directory_count, 0);
        assert_eq!(result.evidence[0].total_size, 0);
        assert!(result.evidence[0].extension_histogram.is_empty());
    }

    #[test]
    fn test_scan_with_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(dir.path().join("file2.txt"), "content2").unwrap();
        fs::write(dir.path().join("file3.pdf"), "content3").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence.len(), 1);
        let evidence = &result.evidence[0];
        assert_eq!(evidence.file_count, 3);
        assert_eq!(evidence.directory_count, 0);
        assert_eq!(evidence.total_size, 27); // 9 + 9 + 9
        assert_eq!(evidence.extension_histogram.get("txt"), Some(&2));
        assert_eq!(evidence.extension_histogram.get("pdf"), Some(&1));
    }

    #[test]
    fn test_scan_with_subdirectories() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("root_file.txt"), "root").unwrap();
        
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("sub_file.txt"), "sub").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence.len(), 2);
        
        // Root directory
        let root_evidence = result.evidence.iter().find(|e| e.name == dir.path().file_name().unwrap().to_str().unwrap()).unwrap();
        assert_eq!(root_evidence.file_count, 1);
        assert_eq!(root_evidence.directory_count, 1);
        assert_eq!(root_evidence.child_directory_names, vec!["subdir"]);

        // Subdirectory
        let sub_evidence = result.evidence.iter().find(|e| e.name == "subdir").unwrap();
        assert_eq!(sub_evidence.file_count, 1);
        assert_eq!(sub_evidence.directory_count, 0);
        assert_eq!(sub_evidence.parent_path, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_scan_unicode_paths() {
        let dir = tempdir().unwrap();
        let unicode_dir = dir.path().join("测试目录");
        fs::create_dir(&unicode_dir).unwrap();
        fs::write(unicode_dir.join("文件.txt"), "内容").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence.len(), 2);
        let unicode_evidence = result.evidence.iter().find(|e| e.name == "测试目录").unwrap();
        assert_eq!(unicode_evidence.file_count, 1);
        assert_eq!(unicode_evidence.extension_histogram.get("txt"), Some(&1));
    }

    #[test]
    fn test_scan_windows_paths() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("My Documents");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file (1).txt"), "content").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence.len(), 2);
        let sub_evidence = result.evidence.iter().find(|e| e.name == "My Documents").unwrap();
        assert_eq!(sub_evidence.file_count, 1);
    }

    #[test]
    fn test_scan_notable_filenames() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("README.md"), "# Readme").unwrap();
        fs::write(dir.path().join("LICENSE"), "MIT").unwrap();
        fs::write(dir.path().join("CHANGELOG.txt"), "v1.0").unwrap();
        fs::write(dir.path().join("regular.txt"), "content").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.text_file_presence.has_readme);
        assert!(evidence.text_file_presence.has_license);
        assert!(evidence.text_file_presence.has_changelog);
        assert!(evidence.text_file_presence.has_md);
        assert!(evidence.text_file_presence.has_txt);
        assert_eq!(evidence.notable_filenames.len(), 3);
    }

    #[test]
    fn test_scan_nfo_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("release.nfo"), "NFO content").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.text_file_presence.has_nfo);
        assert!(evidence.notable_filenames.contains(&"release.nfo".to_string()));
    }

    #[test]
    fn test_max_depth_limit() {
        let dir = tempdir().unwrap();
        let mut current = dir.path().to_path_buf();
        for i in 0..10 {
            current = current.join(format!("level{}", i));
            fs::create_dir(&current).unwrap();
            fs::write(current.join("file.txt"), "content").unwrap();
        }

        let limits = ScanLimits {
            max_depth: 3,
            ..Default::default()
        };
        let scanner = Scanner::with_limits(dir.path(), limits);
        let result = scanner.scan().unwrap();

        // Should only scan up to depth 3 (root + 3 levels = 4 directories)
        assert!(result.evidence.len() <= 4);
    }

    #[test]
    fn test_max_files_per_dir_limit() {
        let dir = tempdir().unwrap();
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
        // Should only count up to max_files_per_dir
        assert!(evidence.file_count <= 10);
    }
}