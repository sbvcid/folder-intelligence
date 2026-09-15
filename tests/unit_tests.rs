use folder_intelligence::evidence::*;
use folder_intelligence::scanner::Scanner;

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
            partial_scan: false,
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
        assert_eq!(evidence.total_size, 24); // 8 + 8 + 8
        assert_eq!(evidence.extension_histogram.get("txt").copied(), Some(2u64));
        assert_eq!(evidence.extension_histogram.get("pdf").copied(), Some(1u64));
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
        assert_eq!(root_evidence.child_directory_names, vec!["subdir".to_string()]);

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
        assert_eq!(unicode_evidence.extension_histogram.get("txt").copied(), Some(1u64));
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

        // With max_depth=3, directories at depth >= max_depth are skipped
        // So depth 0 (root), 1 (level0), 2 (level1) are scanned = 3 directories
        // level2 at depth 3 is skipped
        assert_eq!(result.evidence.len(), 3);
        
        assert!(result.evidence.iter().any(|e| e.name == "level0"));
        assert!(result.evidence.iter().any(|e| e.name == "level1"));
        assert!(!result.evidence.iter().any(|e| e.name == "level2"));
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
        // file_count should be limited to max_files_per_dir
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
        assert_eq!(evidence.representative_filenames.len(), 5);
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
        assert_eq!(result.stats.files_skipped, 10, "10 files should be skipped");
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
}
