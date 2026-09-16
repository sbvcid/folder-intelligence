use crate::agent::intent::IntentParseError;
use crate::agent::intent::TaskIntentParser;
use crate::agent::{CleanRule, ConstraintSet, Goal};
use std::path::PathBuf;

#[test]
fn test_organize_general() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Help me organize Downloads")
        .expect("should parse");

    match &intent.goal {
        Goal::Organize { scope, purpose } => {
            assert_eq!(purpose, "general_organization");
            assert!(scope.exists() || scope == &PathBuf::from("."));
        }
        _ => panic!("expected Organize, got {:?}", intent.goal),
    }

    assert!(intent
        .unknown_factors
        .contains(&"desired_category_structure".to_string()));
}

#[test]
fn test_organize_with_taxonomy() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Organize by work, personal, entertainment")
        .expect("should parse");

    match &intent.goal {
        Goal::Organize { purpose, .. } => {
            assert_eq!(purpose, "work/personal/entertainment");
        }
        _ => panic!("expected Organize, got {:?}", intent.goal),
    }
}

#[test]
fn test_organize_by_category() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Please organize by category")
        .expect("should parse");

    match &intent.goal {
        Goal::Organize { purpose, .. } => {
            assert_eq!(purpose, "by_category");
        }
        _ => panic!("expected Organize, got {:?}", intent.goal),
    }
}

#[test]
fn test_reorganize_detect() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Rearrange the desktop folder")
        .expect("should parse");

    match &intent.goal {
        Goal::Reorganize { strategy, .. } => {
            assert!(strategy.is_some(), "strategy should be set");
        }
        _ => panic!("expected Reorganize, got {:?}", intent.goal),
    }
}

#[test]
fn test_reorganize_chinese() {
    let parser = TaskIntentParser::default();
    let intent = parser.parse("重新整理這個資料夾").expect("should parse");

    assert!(matches!(intent.goal, Goal::Reorganize { .. }));
}

#[test]
fn test_clean_detect() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Clean up the downloads folder and delete temp files")
        .expect("should parse");

    match &intent.goal {
        Goal::Clean { rules, .. } => {
            assert!(rules.iter().any(|r| *r == CleanRule::DeleteTemps));
        }
        _ => panic!("expected Clean, got {:?}", intent.goal),
    }
}

#[test]
fn test_clean_with_archive() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Archive files older than 90 days")
        .expect("should parse");

    match &intent.goal {
        Goal::Clean { rules, .. } => {
            assert!(rules.iter().any(|r| matches!(r, CleanRule::ArchiveOld(_))));
        }
        _ => panic!("expected Clean, got {:?}", intent.goal),
    }
}

#[test]
fn test_unrecognized_intent() {
    let parser = TaskIntentParser::default();
    let result = parser.parse("Tell me a joke");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), IntentParseError::UnrecognizedIntent);
}

#[test]
fn test_constraint_preserve_existing() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Organize Downloads and preserve existing folders")
        .expect("should parse");

    assert!(intent.constraints.preserve_existing_folders);
}

#[test]
fn test_constraint_merge_duplicates() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Organize Downloads with merge duplicates")
        .expect("should parse");

    assert!(intent.constraints.merge_duplicates);
}

#[test]
fn test_constraint_archive_old() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Organize Downloads, archive old files 365 days")
        .expect("should parse");

    assert!(intent.constraints.archive_old.is_some());
}

#[test]
fn test_constraint_auto_delete_temps() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Clean up downloads, delete temp files")
        .expect("should parse");

    assert!(intent.constraints.auto_delete_temps);
}

#[test]
fn test_hints_dry_run() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Organize Downloads with dry run preview verbose")
        .expect("should parse");

    assert_eq!(intent.user_hints.get("dry_run"), Some(&"true".to_string()));
    assert_eq!(intent.user_hints.get("verbose"), Some(&"true".to_string()));
}

#[test]
fn test_max_questions_default() {
    let parser = TaskIntentParser::default();
    let intent = parser.parse("Organize Downloads").expect("should parse");

    assert_eq!(intent.constraints.max_interactive_questions, 10);
}

#[test]
fn test_unknown_factors_absent_for_clean() {
    let parser = TaskIntentParser::default();
    let intent = parser
        .parse("Delete temp files from downloads")
        .expect("should parse");

    assert!(intent.unknown_factors.is_empty());
}

#[test]
fn test_custom_default_scope() {
    let parser = TaskIntentParser::new(PathBuf::from("/tmp/test_scope"));
    let intent = parser.parse("Help me organize").expect("should parse");

    match &intent.goal {
        Goal::Organize { scope, .. } => {
            assert_eq!(scope, &PathBuf::from("/tmp/test_scope"));
        }
        _ => panic!("expected Organize, got {:?}", intent.goal),
    }
}

#[test]
fn test_intent_serialization() {
    let parser = TaskIntentParser::default();
    let intent = parser.parse("Organize by category").expect("should parse");

    let json = serde_json::to_string(&intent).expect("should serialize");
    let deserialized: crate::agent::TaskIntent =
        serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(intent, deserialized);
}

#[test]
fn test_clean_rule_serialization() {
    let rule = CleanRule::ArchiveOld(std::time::Duration::from_secs(30 * 86400));
    let json = serde_json::to_string(&rule).expect("should serialize");
    let deserialized: CleanRule = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(rule, deserialized);
}

#[test]
fn test_constraint_set_defaults() {
    let cs = ConstraintSet::default();
    assert!(!cs.preserve_existing_folders);
    assert!(!cs.merge_duplicates);
    assert!(!cs.auto_delete_temps);
    assert!(cs.archive_old.is_none());
    assert_eq!(cs.max_interactive_questions, 0);
}
