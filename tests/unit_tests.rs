use folder_intelligence::evidence::*;
use folder_intelligence::scanner::Scanner;
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(test)]
mod evidence_tests {
    use super::*;

    fn make_test_evidence() -> DirectoryEvidence {
        let mut ext_hist = HashMap::new();
        ext_hist.insert("txt".to_string(), 5);
        ext_hist.insert("pdf".to_string(), 3);

        let mut id_by_type = HashMap::new();
        id_by_type.insert(IdentifierType::Isbn, 1);

        DirectoryEvidence {
            path: PathBuf::from("/test/dir"),
            name: "dir".to_string(),
            parent_path: Some(PathBuf::from("/test")),
            depth: 1,
            file_count: 8,
            directory_count: 2,
            total_size: 1024,
            extension_histogram: ext_hist,
            dominant_extensions: vec![
                DominantExtension {
                    extension: "txt".to_string(),
                    count: 5,
                    percentage: 62.5,
                },
                DominantExtension {
                    extension: "pdf".to_string(),
                    count: 3,
                    percentage: 37.5,
                },
            ],
            identifier_summary: IdentifierSummary {
                total: 1,
                by_type: id_by_type,
            },
            child_directory_names: vec!["sub1".to_string(), "sub2".to_string()],
            filename_sample: vec!["file1.txt".to_string(), "file2.pdf".to_string()],
            notable_filenames: vec!["README.md".to_string()],
            syntactic_identifiers: vec![],
            text_file_presence: TextFilePresence {
                has_readme: true,
                has_nfo: false,
                has_txt: true,
                has_md: true,
                has_license: false,
                has_changelog: false,
                text_files_found: vec!["README.md".to_string()],
            },
            is_empty: false,
            partial_scan: false,
            scanned_at: 1234567890,
            scan_duration_ms: 100,
            schema_version: SCHEMA_VERSION.to_string(),
        }
    }

    #[test]
    fn test_directory_evidence_serialization() {
        let evidence = make_test_evidence();
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

    #[test]
    fn test_identifier_summary_empty() {
        let summary = IdentifierSummary::default();
        assert_eq!(summary.total, 0);
        assert!(summary.by_type.is_empty());
    }

    #[test]
    fn test_dominant_extension_struct() {
        let ext = DominantExtension {
            extension: "mp3".to_string(),
            count: 10,
            percentage: 100.0,
        };
        assert_eq!(ext.extension, "mp3");
        assert_eq!(ext.count, 10);
        assert_eq!(ext.percentage, 100.0);
    }

    #[test]
    fn test_scan_metadata_struct() {
        let metadata = ScanMetadata {
            _type: "scan_metadata".to_string(),
            schema_version: SCHEMA_VERSION.to_string(),
            scan_batch_id: "test-batch-id".to_string(),
            scan_started_at: 1700000000,
            root_path: PathBuf::from("/test"),
            limits: ScanLimits::default(),
            stats: ScanStats {
                directories_scanned: 0,
                files_encountered: 0,
                bytes_scanned: 0,
                dirs_skipped: 0,
                files_skipped: 0,
                errors: Vec::new(),
                duration_ms: 0,
            },
        };
        assert_eq!(metadata._type, "scan_metadata");
        assert_eq!(metadata.schema_version, SCHEMA_VERSION);
        assert_eq!(metadata.scan_batch_id, "test-batch-id");
        assert_eq!(metadata.root_path, PathBuf::from("/test"));
    }

    #[test]
    fn test_directory_evidence_is_empty_helper() {
        let ev = DirectoryEvidence {
            path: PathBuf::from("/test"),
            name: "test".to_string(),
            parent_path: None,
            depth: 0,
            file_count: 0,
            directory_count: 0,
            total_size: 0,
            extension_histogram: HashMap::new(),
            dominant_extensions: Vec::new(),
            identifier_summary: IdentifierSummary::default(),
            child_directory_names: Vec::new(),
            filename_sample: Vec::new(),
            notable_filenames: Vec::new(),
            syntactic_identifiers: Vec::new(),
            text_file_presence: TextFilePresence::default(),
            is_empty: true,
            partial_scan: false,
            scanned_at: 0,
            scan_duration_ms: 0,
            schema_version: SCHEMA_VERSION.to_string(),
        };
        assert!(ev.is_empty);
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
        assert!(result.evidence[0].is_empty);
        assert_eq!(result.evidence[0].depth, 0);
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
        assert_eq!(evidence.total_size, 24);
        assert_eq!(evidence.extension_histogram.get("txt").copied(), Some(2u64));
        assert_eq!(evidence.extension_histogram.get("pdf").copied(), Some(1u64));
        assert!(!evidence.is_empty);
        assert_eq!(evidence.depth, 0);
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

        let root_evidence = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
        assert_eq!(root_evidence.file_count, 1);
        assert_eq!(root_evidence.directory_count, 1);
        assert_eq!(root_evidence.child_directory_names, vec!["subdir".to_string()]);
        assert_eq!(root_evidence.depth, 0);

        let sub_evidence = result.evidence.iter().find(|e| e.name == "subdir").unwrap();
        assert_eq!(sub_evidence.file_count, 1);
        assert_eq!(sub_evidence.directory_count, 0);
        assert_eq!(sub_evidence.parent_path, Some(dir.path().to_path_buf()));
        assert_eq!(sub_evidence.depth, 1);
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
        assert_eq!(unicode_evidence.extension_histogram.get("txt").copied(), Some(1u64));
        assert_eq!(unicode_evidence.depth, 1);
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
        assert_eq!(sub_evidence.depth, 1);
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

        assert_eq!(result.evidence.len(), 3);

        assert!(result.evidence.iter().any(|e| e.name == "level0"));
        assert!(result.evidence.iter().any(|e| e.name == "level1"));
        assert!(!result.evidence.iter().any(|e| e.name == "level2"));

        let root = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
        assert_eq!(root.depth, 0);
        let level0 = result.evidence.iter().find(|e| e.name == "level0").unwrap();
        assert_eq!(level0.depth, 1);
        let level1 = result.evidence.iter().find(|e| e.name == "level1").unwrap();
        assert_eq!(level1.depth, 2);
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
        assert_eq!(evidence.file_count, 10);
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
    fn test_max_total_files_sets_partial_scan() {
        let dir = tempdir().unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("file{:03}.txt", i)), "x").unwrap();
        }

        let limits = ScanLimits {
            max_total_files: 10,
            max_representative_files: 5,
            ..Default::default()
        };

        let scanner = Scanner::with_limits(dir.path(), limits);
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.partial_scan, "Should be partial when max_total_files is reached during directory iteration");
        assert_eq!(evidence.file_count, 10, "File count should be capped at max_total_files");
        assert_eq!(result.metadata.stats.files_skipped, 10, "10 files should be skipped");
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
        assert_eq!(result.evidence[0].name, dir.path().file_name().unwrap().to_str().unwrap());
    }

    #[test]
    fn test_depth_field_populated() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(dir.path().join("root.txt"), "root").unwrap();
        fs::write(subdir.join("sub.txt"), "sub").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let root_ev = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
        assert_eq!(root_ev.depth, 0);

        let sub_ev = result.evidence.iter().find(|e| e.name == "subdir").unwrap();
        assert_eq!(sub_ev.depth, 1);
    }

    #[test]
    fn test_is_empty_flag() {
        let dir = tempdir().unwrap();
        let empty_subdir = dir.path().join("empty");
        fs::create_dir(&empty_subdir).unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let empty_ev = result.evidence.iter().find(|e| e.name == "empty").unwrap();
        assert!(empty_ev.is_empty);
        assert_eq!(empty_ev.file_count, 0);
        assert_eq!(empty_ev.directory_count, 0);

        let root_ev = result.evidence.iter().find(|e| e.parent_path.is_none()).unwrap();
        assert!(!root_ev.is_empty);
    }

    #[test]
    fn test_dominant_extensions_computed() {
        let dir = tempdir().unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("file{:02}.mp3", i)), "x").unwrap();
        }
        fs::write(dir.path().join("readme.md"), "x").unwrap();
        fs::write(dir.path().join("cover.jpg"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(!evidence.dominant_extensions.is_empty());
        let top = &evidence.dominant_extensions[0];
        assert_eq!(top.extension, "mp3");
        assert_eq!(top.count, 10);
        assert!((top.percentage - 83.33).abs() < 0.1);
    }

    #[test]
    fn test_identifier_summary_computed() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("book_978-0-306-40615-7.pdf"), "x").unwrap();
        fs::write(dir.path().join("song_550e8400-e29b-41d4-a716-446655440000.mp3"), "x").unwrap();
        fs::write(dir.path().join("data_v1.2.3.bin"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        let evidence = &result.evidence[0];
        assert!(evidence.identifier_summary.total >= 3);
        assert!(evidence.identifier_summary.by_type.contains_key(&IdentifierType::Isbn));
        assert!(evidence.identifier_summary.by_type.contains_key(&IdentifierType::Uuid));
        assert!(evidence.identifier_summary.by_type.contains_key(&IdentifierType::Semver));
    }

    #[test]
    fn test_schema_version_in_evidence() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert_eq!(result.evidence[0].schema_version, SCHEMA_VERSION);
        assert_eq!(result.metadata.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_scan_batch_id_in_metadata() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert!(!result.metadata.scan_batch_id.is_empty());
        assert_eq!(result.metadata.schema_version, SCHEMA_VERSION);
        assert_eq!(result.metadata.root_path, dir.path());
        assert_eq!(result.metadata._type, "scan_metadata");
    }

    #[test]
    fn test_scan_started_at_in_metadata() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();

        let scanner = Scanner::new(dir.path());
        let result = scanner.scan().unwrap();

        assert!(result.metadata.scan_started_at > 0);
    }
}
