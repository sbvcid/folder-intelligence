use crate::agent::Pipeline;
use crate::classification::AiClassifier;
use crate::classification::ClassificationProcessor;
use crate::evidence::{ScanLimits, ScanResult};
use crate::scanner::Scanner;
use anyhow::{anyhow, Result};
use std::io::BufRead;
use std::path::{Path, PathBuf};

pub struct Cli {
    pub command: Commands,
}

pub enum Commands {
    Scan {
        path: PathBuf,
        limits: ScanLimits,
        output: Option<PathBuf>,
        quiet: bool,
    },
    Inspect {
        path: PathBuf,
        output: Option<PathBuf>,
    },
    Classify {
        target: PathBuf,
        category_root: PathBuf,
        output: Option<PathBuf>,
    },
    ClassifyAi {
        target: PathBuf,
        category_root: PathBuf,
        model: String,
        api_key: Option<String>,
        base_url: Option<String>,
        output: Option<PathBuf>,
    },
    Schema {
        output: Option<PathBuf>,
    },
    Intent {
        request: String,
        scope: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Analyze {
        request: String,
        scope: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Recommend {
        request: String,
        scope: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Clarify {
        request: String,
        scope: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Plan {
        request: String,
        scope: Option<PathBuf>,
        save_plan: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Validate {
        request: String,
        scope: Option<PathBuf>,
        dry_run: bool,
        output: Option<PathBuf>,
    },
    Apply {
        plan_file: PathBuf,
        dry_run: bool,
        force: bool,
        yes: bool,
        auto_approve: bool,
        save_log: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Undo {
        log_file: PathBuf,
        dry_run: bool,
        output: Option<PathBuf>,
    },
    Chat {
        config: Option<PathBuf>,
        auto_approve: bool,
    },
}

impl Cli {
    pub fn parse() -> Result<Self> {
        let mut args = pico_args::Arguments::from_env();

        let subcommand = args
            .subcommand()?
            .ok_or_else(|| anyhow!("No subcommand provided"))?;

        let command = match subcommand.as_str() {
            "scan" => {
                let path: PathBuf =
                    args.free_from_os_str::<PathBuf, anyhow::Error>(|s| Ok(PathBuf::from(s)))?;
                let max_depth = args
                    .opt_value_from_str(["-d", "--max-depth"])?
                    .unwrap_or(50);
                let max_files_per_dir = args
                    .opt_value_from_str(["-f", "--max-files-per-dir"])?
                    .unwrap_or(10000);
                let max_total_files = args
                    .opt_value_from_str(["-t", "--max-total-files"])?
                    .unwrap_or(1000000);
                let max_total_dirs = args
                    .opt_value_from_str(["-D", "--max-total-dirs"])?
                    .unwrap_or(100000);
                let max_representative_files = args
                    .opt_value_from_str("--max-representative-files")?
                    .unwrap_or(20);
                let max_child_dirs = args.opt_value_from_str("--max-child-dirs")?.unwrap_or(500);
                let timeout = args.opt_value_from_str("--timeout")?.unwrap_or(3600);
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let quiet = args.contains("--quiet");

                Commands::Scan {
                    path,
                    limits: ScanLimits {
                        max_depth,
                        max_files_per_dir,
                        max_total_files,
                        max_total_dirs,
                        max_representative_files,
                        max_child_dirs,
                        timeout_seconds: timeout,
                    },
                    output,
                    quiet,
                }
            }
            "inspect" => {
                let path: PathBuf =
                    args.free_from_os_str::<PathBuf, anyhow::Error>(|s| Ok(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Inspect { path, output }
            }
            "classify" => {
                let target = args
                    .value_from_os_str("--target", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let category_root = args.value_from_os_str("--category-root", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Classify {
                    target,
                    category_root,
                    output,
                }
            }
            "classify-ai" => {
                let target = args
                    .value_from_os_str("--target", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let category_root = args.value_from_os_str("--category-root", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let model = args
                    .opt_value_from_str("--model")?
                    .unwrap_or_else(|| "mock".to_string());
                let api_key = args.opt_value_from_str("--api-key")?;
                let base_url = args.opt_value_from_str("--base-url")?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::ClassifyAi {
                    target,
                    category_root,
                    model,
                    api_key,
                    base_url,
                    output,
                }
            }
            "schema" => {
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Schema { output }
            }
            "intent" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Intent {
                    request,
                    scope,
                    output,
                }
            }
            "analyze" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Analyze {
                    request,
                    scope,
                    output,
                }
            }
            "recommend" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Recommend {
                    request,
                    scope,
                    output,
                }
            }
            "clarify" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Clarify {
                    request,
                    scope,
                    output,
                }
            }
            "plan" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let save_plan = args.opt_value_from_os_str("--save-plan", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Plan {
                    request,
                    scope,
                    save_plan,
                    output,
                }
            }
            "validate" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let dry_run = args.contains("--dry-run");
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Validate {
                    request,
                    scope,
                    dry_run,
                    output,
                }
            }
            "apply" => {
                let plan_file =
                    args.value_from_os_str("--plan", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let dry_run = args.contains("--dry-run");
                let force = args.contains("--force");
                let yes = args.contains("--yes");
                let auto_approve = args.contains("--auto-approve");
                let save_log = args.opt_value_from_os_str("--save-log", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Apply {
                    plan_file,
                    dry_run,
                    force,
                    yes,
                    auto_approve,
                    save_log,
                    output,
                }
            }
            "undo" => {
                let log_file =
                    args.value_from_os_str("--log", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let dry_run = args.contains("--dry-run");
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                Commands::Undo {
                    log_file,
                    dry_run,
                    output,
                }
            }
            "chat" => {
                let config = args.opt_value_from_os_str("--config", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let auto_approve = args.contains("--auto-approve");
                Commands::Chat {
                    config,
                    auto_approve,
                }
            }
            _ => return Err(anyhow!("Unknown subcommand: {}", subcommand)),
        };

        Ok(Cli { command })
    }

    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::Scan {
                path,
                limits,
                output,
                quiet,
            } => {
                if !quiet {
                    eprintln!("Scanning: {}", path.display());
                }

                let scanner = Scanner::with_limits(path, limits);
                let result = scanner.scan()?;

                if !quiet {
                    eprintln!("Scan complete.");
                }

                write_jsonl(&result, output)?;
            }
            Commands::Inspect { path, output } => {
                let limits = ScanLimits::default();
                let scanner = Scanner::with_limits(&path, limits);
                let result = scanner.inspect_single()?;

                if result.evidence.is_empty() {
                    return Err(anyhow!("No evidence found for path: {}", path.display()));
                }

                write_jsonl(&result, output)?;
            }
            Commands::Classify {
                target,
                category_root,
                output,
            } => {
                let limits = ScanLimits::default();

                // Scan target directory
                let target_scanner = Scanner::with_limits(&target, limits.clone());
                let target_scan = target_scanner.inspect_single()?;
                let target_evidence =
                    target_scan.evidence.into_iter().next().ok_or_else(|| {
                        anyhow!("Failed to scan target path: {}", target.display())
                    })?;

                // Gather candidate directories from category root
                let mut candidate_paths: Vec<PathBuf> = Vec::new();
                if category_root.is_dir() {
                    for entry in std::fs::read_dir(&category_root)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_dir() && !path.is_symlink() {
                            candidate_paths.push(path);
                        }
                    }
                }

                let input = crate::classification::ClassificationInput::from_directory_evidence(
                    target_evidence,
                    &candidate_paths,
                    target_scan.metadata,
                )?;

                let processor = crate::classification::RuleBasedProcessor;
                let classification_result = processor.classify(&input)?;

                let json_output = serde_json::to_string_pretty(&classification_result)?;
                write_output(&json_output, output)?;
            }
            #[cfg_attr(not(feature = "network"), allow(unused_variables))]
            Commands::ClassifyAi {
                target,
                category_root,
                model,
                api_key,
                base_url,
                output,
            } => {
                let limits = ScanLimits::default();

                let target_scanner = Scanner::with_limits(&target, limits.clone());
                let target_scan = target_scanner.inspect_single()?;
                let target_evidence =
                    target_scan.evidence.into_iter().next().ok_or_else(|| {
                        anyhow!("Failed to scan target path: {}", target.display())
                    })?;

                let mut candidate_paths: Vec<PathBuf> = Vec::new();
                if category_root.is_dir() {
                    for entry in std::fs::read_dir(&category_root)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_dir() && !path.is_symlink() {
                            candidate_paths.push(path);
                        }
                    }
                }

                let input = crate::classification::ClassificationInput::from_directory_evidence(
                    target_evidence,
                    &candidate_paths,
                    target_scan.metadata,
                )?;

                let processor = crate::classification::RuleBasedProcessor;
                let baseline = processor.classify(&input)?;

                let ai_classifier = if model == "mock" {
                    crate::classification::RealAiClassifier::Mock(
                        crate::classification::MockAiClassifier::default(),
                    )
                } else {
                    #[cfg(feature = "network")]
                    {
                        let key = api_key.ok_or_else(|| {
                            anyhow!("--api-key is required for model '{}'", model)
                        })?;
                        crate::classification::RealAiClassifier::OpenAi(
                            crate::classification::OpenAiProvider::new(
                                crate::classification::OpenAiProviderConfig {
                                    base_url: base_url.unwrap_or_else(|| {
                                        "https://api.openai.com/v1/chat/completions".to_string()
                                    }),
                                    api_key: key,
                                    model: model.clone(),
                                    timeout: std::time::Duration::from_secs(60),
                                },
                            ),
                        )
                    }
                    #[cfg(not(feature = "network"))]
                    {
                        return Err(anyhow!(
                            "Network feature not enabled for provider '{}'. Use --model mock",
                            model
                        ));
                    }
                };

                let ai_result = match ai_classifier {
                    crate::classification::RealAiClassifier::Mock(m) => {
                        m.classify(&input, &baseline)?
                    }
                    #[cfg(feature = "network")]
                    crate::classification::RealAiClassifier::OpenAi(p) => {
                        p.classify(&input, &baseline)?
                    }
                };

                let json_output = serde_json::to_string_pretty(&ai_result)?;
                write_output(&json_output, output)?;
            }
            Commands::Schema { output } => {
                let schema = include_str!("../schemas/directory-evidence.json");
                write_output(schema, output)?;
            }
            Commands::Intent {
                request,
                scope,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let json_output = serde_json::to_string_pretty(&intent)?;
                write_output(&json_output, output)?;
            }
            Commands::Analyze {
                request,
                scope,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let analysis = pipeline.analyze(&intent)?;
                let json_output = serde_json::to_string_pretty(&analysis)?;
                write_output(&json_output, output)?;
            }
            Commands::Recommend {
                request,
                scope,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let analysis = pipeline.analyze(&intent)?;
                let recommendation = pipeline.recommend(&intent, &analysis)?;
                let json_output = serde_json::to_string_pretty(&recommendation)?;
                write_output(&json_output, output)?;
            }
            Commands::Clarify {
                request,
                scope,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let analysis = pipeline.analyze(&intent)?;
                let recommendation = pipeline.recommend(&intent, &analysis)?;
                let summary = pipeline.clarify(&recommendation);
                write_output(&summary, output)?;
            }
            Commands::Plan {
                request,
                scope,
                save_plan,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let analysis = pipeline.analyze(&intent)?;
                let recommendation = pipeline.recommend(&intent, &analysis)?;
                let plan = pipeline.plan(&recommendation, &analysis, &intent)?;
                let json_output = serde_json::to_string_pretty(&plan)?;
                write_output(&json_output, output)?;

                if let Some(save_path) = &save_plan {
                    save_plan_to_file(&plan, save_path)?;
                    eprintln!("Plan saved to: {}", save_path.display());
                }
            }
            Commands::Validate {
                request,
                scope,
                dry_run,
                output,
            } => {
                let pipeline = if let Some(s) = &scope {
                    Pipeline::new(s)
                } else {
                    Pipeline::default()
                };
                let intent = pipeline.parse_intent(&request)?;
                let analysis = pipeline.analyze(&intent)?;
                let recommendation = pipeline.recommend(&intent, &analysis)?;
                let mut plan = pipeline.plan(&recommendation, &analysis, &intent)?;
                if dry_run {
                    plan.dry_run = true;
                }
                let validation = pipeline.validate(&plan);
                let preview = pipeline.preview(&plan, &validation);
                write_output(&preview, output)?;
            }
            Commands::Apply {
                plan_file,
                dry_run,
                force,
                yes,
                auto_approve,
                save_log,
                output,
            } => {
                let plan = load_plan_from_file(&plan_file)?;
                if plan.validation_context.is_none() {
                    eprintln!(
                        "Warning: plan has no validation context (legacy format). Fresh validation will reject all operations."
                    );
                }
                if plan.dry_run {
                    return Err(anyhow!("Plan has dry_run=true; cannot apply"));
                }

                let pipeline = Pipeline::new(&plan.scope)
                    .with_policy(crate::agent::Policy::default().auto_approve(auto_approve));
                let validation = pipeline.validate(&plan);

                let apply_options = crate::agent::ApplyOptions { force, dry_run };

                let decision = pipeline.policy_evaluate(&plan, &validation);

                let result = match decision {
                    crate::agent::PolicyDecision::Rejected => {
                        let reason = if plan.dry_run {
                            "plan is a dry-run"
                        } else if validation.has_invalid {
                            "plan has invalid operations"
                        } else if validation.has_conflicts {
                            "plan has conflicting operations"
                        } else {
                            "plan rejected by policy"
                        };
                        eprintln!("Plan rejected by policy: {}", reason);
                        return Ok(());
                    }
                    crate::agent::PolicyDecision::Approved => {
                        pipeline.apply(&plan, &validation, &apply_options)?
                    }
                    crate::agent::PolicyDecision::RequiresApproval => {
                        eprintln!("Plan requires explicit approval.");
                        eprintln!("Plan ID: {}", plan.id);
                        eprintln!("Operations: {}", plan.operations.len());
                        eprintln!(
                            "Files moved: {}, Dirs created: {}",
                            plan.estimated_impact.files_moved, plan.estimated_impact.dirs_created
                        );
                        eprintln!("Scope: {}", plan.scope.display());

                        if !yes {
                            eprintln!("Execute this plan? [y/N]: ");
                            let confirmed = confirm_approval();
                            if !confirmed {
                                eprintln!("Execution cancelled.");
                                return Ok(());
                            }
                        }

                        let approval = pipeline
                            .create_approval(&plan, &validation)
                            .map_err(|e| anyhow!("Failed to create approval: {}", e))?;

                        pipeline.apply_with_approval(&plan, &approval, &apply_options)?
                    }
                };

                if let Some(log_path) = save_log {
                    result.log.save(&log_path)?;
                }

                let json_output = serde_json::to_string_pretty(&result)?;
                write_output(&json_output, output)?;
            }
            Commands::Undo {
                log_file,
                dry_run,
                output,
            } => {
                let log = crate::agent::OperationLog::load(&log_file)?;

                let pipeline = Pipeline::default();

                let result = if dry_run {
                    pipeline.preview_undo(&log)
                } else {
                    pipeline.undo(&log)?
                };
                let json_output = serde_json::to_string_pretty(&result)?;
                write_output(&json_output, output)?;
            }
            Commands::Chat {
                config,
                auto_approve,
            } => {
                let chat = crate::llm::ChatCommand::new(config.as_deref(), auto_approve)?;
                chat.run()?;
            }
        }
        Ok(())
    }
}

fn confirm_approval() -> bool {
    let stdin = std::io::stdin();
    let mut input = String::new();
    match stdin.lock().read_line(&mut input) {
        Ok(_) => parse_confirmation(&input),
        Err(_) => false,
    }
}

fn parse_confirmation(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "y" || trimmed == "yes"
}

fn save_plan_to_file(plan: &crate::agent::OperationPlan, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(plan)?;
    std::fs::write(path, json)?;
    Ok(())
}

fn load_plan_from_file(path: &Path) -> Result<crate::agent::OperationPlan> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read plan file '{}': {}", path.display(), e))?;
    let plan: crate::agent::OperationPlan = serde_json::from_str(&json)
        .map_err(|e| anyhow!("Failed to parse plan JSON from '{}': {}", path.display(), e))?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{EstimatedImpact, FileSystemOperation, PlanValidationContext};

    fn make_test_plan(scope: &Path) -> crate::agent::OperationPlan {
        crate::agent::OperationPlan {
            id: "test-plan".to_string(),
            recommendation_id: "rec-1".to_string(),
            scope: scope.to_path_buf(),
            operations: vec![FileSystemOperation::Move {
                source: scope.join("a.txt"),
                dest: scope.join("subdir").join("a.txt"),
            }],
            estimated_impact: EstimatedImpact {
                files_moved: 1,
                dirs_created: 1,
                files_deleted: 0,
                dirs_affected: 1,
                total_bytes: 10,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        }
    }

    #[test]
    fn test_parse_confirmation_accepts_yes() {
        assert!(parse_confirmation("y"));
        assert!(parse_confirmation("Y"));
        assert!(parse_confirmation("yes"));
        assert!(parse_confirmation("YES"));
        assert!(parse_confirmation("y\n"));
        assert!(parse_confirmation("yes\r\n"));
    }

    #[test]
    fn test_parse_confirmation_rejects_others() {
        assert!(!parse_confirmation("n"));
        assert!(!parse_confirmation("no"));
        assert!(!parse_confirmation(""));
        assert!(!parse_confirmation("\n"));
        assert!(!parse_confirmation("yep"));
        assert!(!parse_confirmation("maybe"));
    }

    #[test]
    fn test_save_plan_and_load_plan_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let plan = make_test_plan(dir.path());

        let plan_path = dir.path().join("plan.json");
        save_plan_to_file(&plan, &plan_path).expect("save should succeed");

        let loaded = load_plan_from_file(&plan_path).expect("load should succeed");

        assert_eq!(loaded.id, plan.id);
        assert_eq!(loaded.scope, plan.scope);
        assert_eq!(loaded.operations.len(), plan.operations.len());
        assert_eq!(loaded.dry_run, plan.dry_run);
        assert_eq!(loaded.validation_context, plan.validation_context);
    }

    #[test]
    fn test_load_plan_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("nonexistent.json");

        let result = load_plan_from_file(&plan_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read plan file"),
            "error should mention file read failure"
        );
    }

    #[test]
    fn test_load_plan_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("bad.json");
        std::fs::write(&plan_path, "{ not valid json").unwrap();

        let result = load_plan_from_file(&plan_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse plan JSON"),
            "error should mention JSON parse failure"
        );
    }

    #[test]
    fn test_load_plan_legacy_no_validation_context() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("legacy.json");

        let json = r#"{
            "id": "legacy-plan",
            "recommendation_id": "rec-1",
            "scope": "/tmp/test",
            "operations": [],
            "estimated_impact": {
                "files_moved": 0,
                "dirs_created": 0,
                "files_deleted": 0,
                "dirs_affected": 0,
                "total_bytes": 0
            },
            "validation_warnings": [],
            "has_conflicts": false,
            "dry_run": false,
            "created_at": 0
        }"#;
        std::fs::write(&plan_path, json).unwrap();

        let loaded = load_plan_from_file(&plan_path).expect("legacy plan should deserialize");

        assert!(
            loaded.validation_context.is_none(),
            "legacy plan without validation_context should be None"
        );
    }
}

fn write_jsonl(result: &ScanResult, output: Option<PathBuf>) -> Result<()> {
    let mut writer: Box<dyn std::io::Write> = match output {
        Some(path) => Box::new(std::fs::File::create(path)?),
        None => Box::new(std::io::stdout()),
    };

    let meta_line = serde_json::to_string(&result.metadata)?;
    writeln!(writer, "{}", meta_line)?;

    for e in &result.evidence {
        let line = serde_json::to_string(e)?;
        writeln!(writer, "{}", line)?;
    }
    writer.flush()?;
    Ok(())
}

fn write_output(content: &str, output: Option<PathBuf>) -> Result<()> {
    match output {
        Some(path) => std::fs::write(path, content)?,
        None => print!("{}", content),
    }
    Ok(())
}
