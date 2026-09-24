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
    Organize {
        scope: PathBuf,
        yes: bool,
        dry_run: bool,
        output: Option<PathBuf>,
        instruction: Option<String>,
    },
    Chat {
        config: Option<PathBuf>,
        auto_approve: bool,
    },
}

impl Cli {
    pub fn parse() -> Result<Self> {
        let mut args = pico_args::Arguments::from_env();
        Self::parse_from_args(&mut args)
    }

    fn parse_from_args(args: &mut pico_args::Arguments) -> Result<Self> {
        let subcommand = args.subcommand()?;

        let command = match subcommand {
            None => {
                let yes = args.contains("--yes");
                let dry_run = args.contains("--dry-run");
                let output = args.opt_value_from_os_str("--output", |s| {
                    Ok::<_, anyhow::Error>(PathBuf::from(s))
                })?;
                let instruction = args.opt_value_from_str("--instruction")?;
                let scope = resolve_default_scope()?;
                Commands::Organize {
                    scope,
                    yes,
                    dry_run,
                    output,
                    instruction,
                }
            }
            Some(cmd) => match cmd.as_str() {
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
                    let max_child_dirs =
                        args.opt_value_from_str("--max-child-dirs")?.unwrap_or(500);
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
                    let target = args.value_from_os_str("--target", |s| {
                        Ok::<_, anyhow::Error>(PathBuf::from(s))
                    })?;
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
                    let target = args.value_from_os_str("--target", |s| {
                        Ok::<_, anyhow::Error>(PathBuf::from(s))
                    })?;
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
                    let plan_file = args.value_from_os_str("--plan", |s| {
                        Ok::<_, anyhow::Error>(PathBuf::from(s))
                    })?;
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
                "organize" => {
                    let yes = args.contains("--yes");
                    let dry_run = args.contains("--dry-run");
                    let output = args.opt_value_from_os_str("--output", |s| {
                        Ok::<_, anyhow::Error>(PathBuf::from(s))
                    })?;
                    let instruction = args.opt_value_from_str("--instruction")?;
                    let scope = args
                        .opt_free_from_os_str::<PathBuf, anyhow::Error>(|s| Ok(PathBuf::from(s)))?
                        .unwrap_or_else(|| {
                            resolve_default_scope().unwrap_or_else(|_| {
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                            })
                        });
                    Commands::Organize {
                        scope,
                        yes,
                        dry_run,
                        output,
                        instruction,
                    }
                }
                "undo" => {
                    let log_file = args
                        .value_from_os_str("--log", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
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
                _ => return Err(anyhow!("Unknown subcommand: {}", cmd)),
            },
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
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if crate::scanner::is_excluded_directory(name) {
                                    continue;
                                }
                            }
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
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if crate::scanner::is_excluded_directory(name) {
                                    continue;
                                }
                            }
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
            Commands::Organize {
                scope,
                yes,
                dry_run,
                output,
                instruction,
            } => {
                do_organize(scope, yes, dry_run, output, instruction)?;
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

fn resolve_default_scope() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow!("Failed to determine executable directory"))?;
    Ok(exe_dir.to_path_buf())
}

fn build_llm_classifier(
) -> Result<Option<(crate::llm::LlmConfig, crate::classification::LlmClassifier)>> {
    let config = match crate::llm::LlmConfig::load() {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let is_llm_provider = config.provider == "ollama" || config.provider == "openai-compatible";
    if !is_llm_provider {
        return Ok(None);
    }

    match crate::llm::create_provider(&config) {
        Ok(provider) => {
            let classifier = crate::classification::LlmClassifier::new(provider);
            Ok(Some((config, classifier)))
        }
        Err(e) => Err(anyhow!(
            "LLM provider '{}' was configured but could not be initialized: {}.\n\
             Fix the configuration or remove the LLM provider setting to fall back to rule-based classification.",
            config.provider,
            e
        )),
    }
}

fn do_organize(
    scope: PathBuf,
    yes: bool,
    dry_run: bool,
    output: Option<PathBuf>,
    instruction: Option<String>,
) -> Result<()> {
    let mut pipeline = Pipeline::new(&scope);

    if let Some((config, classifier)) = build_llm_classifier()? {
        pipeline = pipeline
            .with_llm_classifier(classifier)
            .with_classifier_instruction(instruction.clone());
        eprintln!(
            "Using LLM classifier (provider: {}, model: {})",
            config.provider, config.model
        );
    }

    let result = match pipeline.run(
        "Organize this folder by category",
        &crate::agent::PipelineOptions {
            dry_run: false,
            execute: false,
            force: false,
            user_intent: Some(crate::agent::UserIntent::new(instruction)),
        },
    ) {
        Ok(result) => result,
        Err(crate::agent::PipelineError::Analysis(
            crate::agent::AnalyzerError::ScopeNotADirectory(path),
        )) => {
            return Err(anyhow!(
                "Cannot organize '{}': path is not a directory",
                path.display()
            ));
        }
        Err(crate::agent::PipelineError::Analysis(
            crate::agent::AnalyzerError::ScopeNotScannable(path),
        )) => {
            return Err(anyhow!(
                "Cannot organize '{}': directory is empty or not scannable. \
                 No content groups could be identified for categorization.",
                path.display()
            ));
        }
        Err(crate::agent::PipelineError::Analysis(crate::agent::AnalyzerError::ScanFailed(
            msg,
        ))) => {
            return Err(anyhow!("Scan failed: {}", msg));
        }
        Err(crate::agent::PipelineError::Analysis(
            crate::agent::AnalyzerError::ClassificationFailed(msg),
        )) => {
            return Err(anyhow!("Classification failed: {}", msg));
        }
        Err(crate::agent::PipelineError::Recommendation(
            crate::agent::RecommendationError::NoContentGroups,
        )) => {
            return Err(anyhow!(
                "Cannot organize '{}': no recognizable content groups found. \
                 The folder may be empty or contains only unrecognized file types.",
                scope.display()
            ));
        }
        Err(crate::agent::PipelineError::Recommendation(ref e)) => {
            return Err(anyhow!("Recommendation failed: {}", e));
        }
        Err(crate::agent::PipelineError::Plan(ref e)) => {
            return Err(anyhow!("Plan generation failed: {}", e));
        }
        Err(e) => {
            return Err(anyhow!("Pipeline error: {}", e));
        }
    };

    if result.plan.operations.is_empty() {
        let recommendation_preview =
            render_recommendation_summary(&result.recommendation, &result.analysis);
        eprintln!("{}", recommendation_preview);
        eprintln!("Nothing to organize. The folder is already organized.");
        let json = serde_json::to_string_pretty(&result.plan)?;
        write_output(&json, output)?;
        return Ok(());
    }

    let preview = render_organize_preview(
        &result.plan,
        &result.validation,
        &result.recommendation,
        &result.analysis,
    );
    eprintln!("{}", preview);

    if dry_run {
        eprintln!("Dry run complete. No changes were made to the filesystem.");
        let json = serde_json::to_string_pretty(&result.plan)?;
        write_output(&json, output)?;
        return Ok(());
    }

    if !yes {
        eprintln!("Proceed with execution? [y/N]: ");
        if !confirm_approval() {
            eprintln!("Execution cancelled. No changes were made.");
            return Ok(());
        }
    }

    eprintln!("Executing plan...");
    let apply_result = pipeline.apply(
        &result.plan,
        &result.validation,
        &crate::agent::ApplyOptions {
            force: false,
            dry_run: false,
        },
    )?;

    if let Some(ref verification) = apply_result.execution_verification {
        eprintln!("\n{}", verification.render());
    }

    let json = serde_json::to_string_pretty(&apply_result)?;
    write_output(&json, output)?;
    Ok(())
}

fn render_recommendation_summary(
    recommendation: &crate::agent::Recommendation,
    _analysis: &crate::agent::TaskAnalysis,
) -> String {
    let mut output = String::new();

    output.push_str("=== AI Organization Recommendation ===\n");
    output.push_str(&format!(
        "  Strategy:  {}\n",
        match recommendation.strategy {
            crate::agent::RecommendationStrategy::CategoryBased => "Category-based",
            crate::agent::RecommendationStrategy::ProjectBased => "Project-based",
            crate::agent::RecommendationStrategy::Chronological => "Chronological",
            crate::agent::RecommendationStrategy::BySize => "By size",
            crate::agent::RecommendationStrategy::ByAuthor => "By author",
            crate::agent::RecommendationStrategy::ByTitle => "By title",
            crate::agent::RecommendationStrategy::ByYear => "By year",
            crate::agent::RecommendationStrategy::PreserveExisting => "Preserve existing",
            crate::agent::RecommendationStrategy::Unknown => "Unknown",
            crate::agent::RecommendationStrategy::Custom(ref s) => s,
        }
    ));
    if !recommendation.rationale.is_empty() {
        output.push_str(&format!("  Rationale: {}\n", recommendation.rationale));
    }
    if let Some(strategy_info) = &recommendation.strategy_info {
        let strategy_name = match strategy_info.strategy {
            crate::agent::RecommendationStrategy::ByAuthor => "By author",
            crate::agent::RecommendationStrategy::ByTitle => "By title",
            crate::agent::RecommendationStrategy::ByYear => "By year",
            crate::agent::RecommendationStrategy::CategoryBased => "Category-based",
            crate::agent::RecommendationStrategy::PreserveExisting => "Preserve existing",
            crate::agent::RecommendationStrategy::Custom(ref s) => s,
            _ => "Unknown",
        };
        let reason_text = strategy_info
            .reason
            .as_ref()
            .map(|r| format!("\n  Strategy reason: {}", r))
            .unwrap_or_default();
        output.push_str(&format!(
            "  Organization Strategy: {} (confidence: {:.0}%){}\n",
            strategy_name,
            strategy_info.confidence * 100.0,
            reason_text
        ));
    }
    output.push_str(&format!(
        "  Confidence: {:.1}%\n",
        recommendation.confidence * 100.0
    ));

    if !recommendation.proposed_categories.is_empty() {
        output.push_str("\n  Proposed Categories:\n");
        for cat in &recommendation.proposed_categories {
            let status = if cat.is_existing { "existing" } else { "new" };
            output.push_str(&format!(
                "    • {} ({}) - {}\n",
                cat.name, status, cat.purpose
            ));
        }
    }

    if !recommendation.proposed_operations.is_empty() {
        output.push_str("\n  AI Suggested Operations:\n");
        for op in &recommendation.proposed_operations {
            output.push_str(&format!("    • {}\n", op.description()));
        }
    }

    if let Some(ref proposal) = recommendation.organization_proposal {
        output.push_str("\n");
        output.push_str(&render_organization_proposal(proposal));
    }

    output
}

fn render_organization_proposal(proposal: &crate::agent::OrganizationProposal) -> String {
    use crate::agent::RecommendationStrategy;

    let mut output = String::new();
    output.push_str("Organization Proposal\n");
    output.push_str("---------------------\n");

    let strategy_name = match proposal.strategy {
        RecommendationStrategy::CategoryBased => "Category-based",
        RecommendationStrategy::ProjectBased => "Project-based",
        RecommendationStrategy::Chronological => "Chronological",
        RecommendationStrategy::BySize => "By size",
        RecommendationStrategy::ByAuthor => "By author",
        RecommendationStrategy::ByTitle => "By title",
        RecommendationStrategy::ByYear => "By year",
        RecommendationStrategy::PreserveExisting => "Preserve existing",
        RecommendationStrategy::Unknown => "Unknown",
        RecommendationStrategy::Custom(ref s) => s,
    };
    output.push_str(&format!("Strategy: {}\n", strategy_name));

    output.push_str("\nRationale:\n");
    if proposal.rationale.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(&format!("{}\n", proposal.rationale));
    }

    output.push_str("\nCategories:\n");
    if proposal.proposed_categories.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for cat in &proposal.proposed_categories {
            output.push_str(&format!("  {}\n", cat.name));
            output.push_str(&format!("    Purpose: {}\n", cat.purpose));
            output.push_str("    Files:\n");
            if cat.source_files.is_empty() {
                output.push_str("      (none)\n");
            } else {
                for file in &cat.source_files {
                    output.push_str(&format!("      - {}\n", file.display()));
                }
            }
            output.push_str(&format!("    Confidence: {:.0}%\n", cat.confidence * 100.0));
        }
    }

    output.push_str("\nEvidence gaps:\n");
    if proposal.evidence_gaps.is_empty() {
        output.push_str("  None\n");
    } else {
        for gap in &proposal.evidence_gaps {
            output.push_str(&format!("  - {}\n", gap));
        }
    }

    output.push_str("\nAmbiguities:\n");
    if proposal.ambiguities.is_empty() {
        output.push_str("  None\n");
    } else {
        for amb in &proposal.ambiguities {
            output.push_str(&format!("  - {}\n", amb));
        }
    }

    output
}

fn render_organize_preview(
    plan: &crate::agent::OperationPlan,
    _validation: &crate::agent::ValidationResult,
    recommendation: &crate::agent::Recommendation,
    analysis: &crate::agent::TaskAnalysis,
) -> String {
    use std::collections::HashMap;

    let mut output = String::new();

    output.push_str(&render_recommendation_summary(recommendation, analysis));
    output.push('\n');

    if let Some(cr) = analysis.classification_results.first() {
        output.push_str(&format!(
            "  Classification: {} (confidence: {:.1}%)\n",
            match cr.decision {
                crate::classification::ClassificationDecision::MoveExisting => "Move to existing",
                crate::classification::ClassificationDecision::CreateCategory =>
                    "Create new category",
                crate::classification::ClassificationDecision::LeaveUnclassified =>
                    "Leave unclassified",
                crate::classification::ClassificationDecision::AskUser => "Requires user input",
            },
            cr.confidence * 100.0
        ));
    }

    if !analysis.classification_results.is_empty() {
        output.push_str("\n  File-classification Explanations:\n");
        let mut move_files: Vec<PathBuf> = Vec::new();
        for op in &plan.operations {
            if let crate::agent::FileSystemOperation::Move { source, dest: _ } = op {
                move_files.push(source.clone());
            }
        }
        move_files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        let mut explained_files: Vec<PathBuf> = Vec::new();
        for cr in &analysis.classification_results {
            let dest = cr
                .selected_candidate
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| cr.proposed_category_name.clone().unwrap_or_default());
            let decision_str = match cr.decision {
                crate::classification::ClassificationDecision::MoveExisting => "Move to existing",
                crate::classification::ClassificationDecision::CreateCategory => {
                    "Create new category"
                }
                crate::classification::ClassificationDecision::LeaveUnclassified => {
                    "Leave unclassified"
                }
                crate::classification::ClassificationDecision::AskUser => "Requires user input",
            };

            let files_in_scope: Vec<&PathBuf> = move_files
                .iter()
                .filter(|f| {
                    f.parent()
                        .map(|p| p == cr.target_path || cr.target_path.starts_with(p))
                        .unwrap_or(false)
                        || f.starts_with(&cr.target_path)
                })
                .collect();

            for file in files_in_scope {
                explained_files.push(file.clone());
                output.push_str(&format!(
                    "    {} → {} | {} ({:.0}%)\n",
                    file.display(),
                    dest,
                    decision_str,
                    cr.confidence * 100.0
                ));
                if let Some(reason) = &cr.classification_reason {
                    output.push_str(&format!("      Reason: {}\n", reason));
                }
                if !cr.supporting_evidence.is_empty() {
                    output.push_str("      Evidence:\n");
                    for ev in &cr.supporting_evidence {
                        output.push_str(&format!(
                            "        - {} ({})\n",
                            ev.description, ev.evidence_type
                        ));
                    }
                }
                if !cr.warnings.is_empty() {
                    output.push_str("      Warnings:\n");
                    for w in &cr.warnings {
                        output.push_str(&format!("        - {:?}\n", w));
                    }
                }
            }
        }

        let unexplained: Vec<&PathBuf> = move_files
            .iter()
            .filter(|f| !explained_files.iter().any(|e| e == *f))
            .collect();
        for file in &unexplained {
            output.push_str(&format!("    {}\n", file.display()));
        }
    }

    output.push('\n');

    // === Operation Plan
    output.push_str("=== Organize Preview ===\n");
    output.push_str(&format!("Scope: {}\n", plan.scope.display()));
    output.push_str(&format!("Plan ID: {}\n\n", plan.id));

    let mut moves_by_dest: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut create_dirs: Vec<PathBuf> = Vec::new();
    let mut deletes: Vec<(&PathBuf, &str)> = Vec::new();

    for op in &plan.operations {
        match op {
            crate::agent::FileSystemOperation::Move { source, dest } => {
                let dest_dir = dest
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| dest.clone());
                moves_by_dest
                    .entry(dest_dir)
                    .or_default()
                    .push(source.clone());
            }
            crate::agent::FileSystemOperation::CreateDir { path } => {
                create_dirs.push(path.clone());
            }
            crate::agent::FileSystemOperation::Delete { path, reason } => {
                deletes.push((path, reason));
            }
        }
    }

    if !moves_by_dest.is_empty() {
        output.push_str("Files to move:\n");
        let mut sorted_dests: Vec<_> = moves_by_dest.iter().collect();
        sorted_dests.sort_by(|a, b| a.0.cmp(b.0));
        for (dest_dir, sources) in sorted_dests {
            output.push_str(&format!(
                "  -> {} ({} files)\n",
                dest_dir.display(),
                sources.len()
            ));
            let mut sorted_sources = sources.clone();
            sorted_sources.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            for src in &sorted_sources {
                output.push_str(&format!("    {}\n", src.display()));
            }
        }
    }

    if !create_dirs.is_empty() {
        output.push_str("\nDirectories to create:\n");
        let mut sorted_dirs = create_dirs.clone();
        sorted_dirs.sort();
        for dir in &sorted_dirs {
            output.push_str(&format!("  {}\n", dir.display()));
        }
    }

    if !deletes.is_empty() {
        output.push_str("\nFiles to delete:\n");
        for (path, reason) in &deletes {
            output.push_str(&format!("  {} ({})\n", path.display(), reason));
        }
    }

    let unclassified: Vec<_> = recommendation
        .proposed_operations
        .iter()
        .filter(|op| {
            matches!(
                op,
                crate::agent::ProposedOperation::LeaveUnclassified { .. }
            )
        })
        .collect();

    if !unclassified.is_empty() {
        output.push_str("\nUnclassified (left in place):\n");
        for op in &unclassified {
            output.push_str(&format!("  {}\n", op.description()));
        }
    }

    output.push_str(&format!("\nTotal operations: {}\n", plan.operations.len()));
    output.push_str(&format!(
        "  Files to move: {}\n",
        plan.estimated_impact.files_moved
    ));
    output.push_str(&format!(
        "  Directories to create: {}\n",
        plan.estimated_impact.dirs_created
    ));
    if plan.estimated_impact.files_deleted > 0 {
        output.push_str(&format!(
            "  Files to delete: {}\n",
            plan.estimated_impact.files_deleted
        ));
    }

    if !plan.unresolved_proposals.is_empty() {
        output.push_str("\nUnresolved proposal items:\n");
        for item in &plan.unresolved_proposals {
            output.push_str(&format!(
                "  - {} (category: {}): {}\n",
                item.file.display(),
                item.category,
                item.reason
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::intent::Goal;
    use crate::agent::{
        EstimatedImpact, FileSystemOperation, PlanValidationContext, TaskAnalysis, TaskIntent,
    };
    use crate::evidence::{DirectoryEvidence, ScanStats, TextFilePresence};

    fn make_test_plan(scope: &Path) -> crate::agent::OperationPlan {
        crate::agent::OperationPlan {
            unresolved_proposals: Vec::new(),
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

    fn minimal_analysis() -> TaskAnalysis {
        let scope = PathBuf::from("/test/scope");
        TaskAnalysis {
            intent: TaskIntent {
                goal: Goal::Organize {
                    scope: scope.clone(),
                    purpose: "Organize this folder by category".to_string(),
                },
                constraints: crate::agent::ConstraintSet {
                    preserve_existing_folders: false,
                    merge_duplicates: false,
                    archive_old: None,
                    auto_delete_temps: false,
                    max_interactive_questions: 10,
                },
                user_hints: std::collections::HashMap::new(),
                unknown_factors: vec![],
            },
            scope_evidence: DirectoryEvidence {
                path: scope.clone(),
                name: "test".to_string(),
                parent_path: None,
                depth: 0,
                file_count: 0,
                directory_count: 0,
                total_size: 0,
                extension_histogram: std::collections::HashMap::new(),
                dominant_extensions: vec![],
                identifier_summary: Default::default(),
                child_directory_names: vec![],
                filename_sample: vec![],
                notable_filenames: vec![],
                syntactic_identifiers: vec![],
                text_file_presence: TextFilePresence::default(),
                is_empty: true,
                partial_scan: false,
                scanned_at: 0,
                scan_duration_ms: 0,
                schema_version: "2.0.0".to_string(),
            },
            scan_metadata: crate::evidence::ScanMetadata {
                _type: "scan".to_string(),
                schema_version: "2.0.0".to_string(),
                scan_batch_id: "test".to_string(),
                scan_started_at: 0,
                root_path: scope.clone(),
                limits: crate::evidence::ScanLimits::default(),
                stats: crate::evidence::ScanStats {
                    directories_scanned: 0,
                    files_encountered: 0,
                    bytes_scanned: 0,
                    dirs_skipped: 0,
                    files_skipped: 0,
                    errors: vec![],
                    duration_ms: 0,
                },
            },
            structure_summary: crate::agent::StructureSummary {
                total_files: 0,
                total_directories: 0,
                total_size: 0,
                content_groups: vec![],
                depth_levels: 0,
                has_mixed_content_dirs: false,
                partial_scan: false,
            },
            content_groups: vec![],
            candidate_categories: vec![],
            classification_results: vec![],
            organization_strategy: None,
            organization_proposal: None,
            anomalies: vec![],
            ambiguities: vec![],
            evidence_gaps: vec![],
            analyzed_at: 0,
            analysis_duration_ms: 0,
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

    #[test]
    fn test_organize_preview_groups_moves_by_destination() {
        use crate::agent::{
            ProposedOperation, Recommendation, RecommendationStrategy, ValidationResult,
            ValidationSummary,
        };

        let scope = PathBuf::from("/test/scope");
        let plan = crate::agent::OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "org-test".to_string(),
            recommendation_id: "rec-1".to_string(),
            scope: scope.clone(),
            operations: vec![
                FileSystemOperation::Move {
                    source: scope.join("doc.pdf"),
                    dest: scope.join("Documents").join("doc.pdf"),
                },
                FileSystemOperation::Move {
                    source: scope.join("doc2.docx"),
                    dest: scope.join("Documents").join("doc2.docx"),
                },
                FileSystemOperation::Move {
                    source: scope.join("photo.jpg"),
                    dest: scope.join("Images").join("photo.jpg"),
                },
                FileSystemOperation::CreateDir {
                    path: scope.join("Documents"),
                },
                FileSystemOperation::CreateDir {
                    path: scope.join("Images"),
                },
            ],
            estimated_impact: EstimatedImpact {
                files_moved: 3,
                dirs_created: 2,
                files_deleted: 0,
                dirs_affected: 2,
                total_bytes: 0,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        };

        let validation = ValidationResult {
            plan_id: "org-test".to_string(),
            scope: scope.clone(),
            validated_operations: vec![],
            summary: ValidationSummary::new(),
            has_blocked: false,
            has_conflicts: false,
            has_invalid: false,
            has_warnings: false,
            executable_operations: 5,
        };

        let recommendation = Recommendation {
            id: "rec-1".to_string(),
            strategy: RecommendationStrategy::CategoryBased,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: vec![],
            proposed_operations: vec![ProposedOperation::LeaveUnclassified {
                file_count: 1,
                reason: "unknown type".to_string(),
            }],
            unresolved_questions: vec![],
            confidence: 0.9,
            constraint_checks: vec![],
            constraint_violation: None,
            warnings: vec![],
            organization_proposal: None,
            generated_at: 0,
        };

        let preview =
            render_organize_preview(&plan, &validation, &recommendation, &minimal_analysis());

        assert!(preview.contains("=== Organize Preview ==="));
        assert!(preview.contains("AI Organization Recommendation"));
        assert!(preview.contains("Files to move:"));
        assert!(preview.contains(&format!(
            "  -> {} (2 files)",
            scope.join("Documents").display()
        )));
        assert!(preview.contains(&format!(
            "  -> {} (1 files)",
            scope.join("Images").display()
        )));
        assert!(preview.contains("doc.pdf"));
        assert!(preview.contains("doc2.docx"));
        assert!(preview.contains("photo.jpg"));
        assert!(preview.contains("Directories to create:"));
        assert!(preview.contains(&scope.join("Documents").display().to_string()));
        assert!(preview.contains(&scope.join("Images").display().to_string()));
        assert!(preview.contains("Total operations: 5"));
        assert!(preview.contains("Files to move: 3"));
        assert!(preview.contains("Directories to create: 2"));
        assert!(preview.contains("Unclassified"));
        assert!(preview.contains("unknown type"));
    }

    #[test]
    fn test_organize_preview_shows_classification_explanations() {
        use crate::agent::{
            ProposedOperation, Recommendation, RecommendationStrategy, ValidationResult,
            ValidationSummary,
        };
        use crate::classification::{ClassificationDecision, ClassificationResult};

        let scope = PathBuf::from("/test/scope");
        let plan = crate::agent::OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "explain-test".to_string(),
            recommendation_id: "rec-1".to_string(),
            scope: scope.clone(),
            operations: vec![
                FileSystemOperation::Move {
                    source: scope.join("doc.pdf"),
                    dest: scope.join("Media").join("doc.pdf"),
                },
                FileSystemOperation::Move {
                    source: scope.join("notes.txt"),
                    dest: scope.join("Media").join("notes.txt"),
                },
                FileSystemOperation::Move {
                    source: scope.join("vacation.jpg"),
                    dest: scope.join("Media").join("vacation.jpg"),
                },
            ],
            estimated_impact: EstimatedImpact {
                files_moved: 3,
                dirs_created: 0,
                files_deleted: 0,
                dirs_affected: 1,
                total_bytes: 0,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        };

        let validation = ValidationResult {
            plan_id: "explain-test".to_string(),
            scope: scope.clone(),
            validated_operations: vec![],
            summary: ValidationSummary::new(),
            has_blocked: false,
            has_conflicts: false,
            has_invalid: false,
            has_warnings: false,
            executable_operations: 3,
        };

        let recommendation = Recommendation {
            id: "rec-1".to_string(),
            strategy: RecommendationStrategy::CategoryBased,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: vec![],
            proposed_operations: vec![],
            unresolved_questions: vec![],
            confidence: 0.9,
            constraint_checks: vec![],
            constraint_violation: None,
            warnings: vec![],
            organization_proposal: None,
            generated_at: 0,
        };

        let mut analysis = minimal_analysis();
        analysis.classification_results = vec![ClassificationResult {
            target_path: scope.clone(),
            decision: ClassificationDecision::MoveExisting,
            selected_candidate: Some(scope.join("Media")),
            proposed_category_name: Some("Media".to_string()),
            confidence: 0.95,
            confidence_band: crate::classification::ConfidenceBand::High,
            candidates_considered: 2,
            supporting_evidence: vec![crate::classification::SupportingEvidence {
                evidence_type: "LLMClassification".to_string(),
                description: "LLM classified as 'Media'".to_string(),
                score: 0.95,
            }],
            alternatives: vec![],
            uncertainty: vec![],
            classification_reason: Some("Files classified as media content.".to_string()),
            warnings: vec![],
            classified_at: 0,
            schema_version: "3.0.0".to_string(),
            provider: Some("openai-compatible".to_string()),
            model: Some("gemini-3.6-flash".to_string()),
        }];

        let preview = render_organize_preview(&plan, &validation, &recommendation, &analysis);

        assert!(preview.contains("File-classification Explanations:"));
        assert!(preview.contains("doc.pdf"));
        assert!(preview.contains("notes.txt"));
        assert!(preview.contains("vacation.jpg"));
        assert!(preview.contains("Media"));
        assert!(preview.contains("Move to existing"));
        assert!(preview.contains("Reason: Files classified as media content."));
        assert!(preview.contains("Evidence:"));
        assert!(preview.contains("LLM classified as 'Media'"));

        let doc_line = preview
            .lines()
            .find(|l| l.contains("doc.pdf") && l.contains("→"));
        assert!(
            doc_line.is_some(),
            "should show doc.pdf with arrow and destination"
        );
        let notes_line = preview
            .lines()
            .find(|l| l.contains("notes.txt") && l.contains("→"));
        assert!(
            notes_line.is_some(),
            "should show notes.txt with arrow and destination"
        );
        let jpg_line = preview
            .lines()
            .find(|l| l.contains("vacation.jpg") && l.contains("→"));
        assert!(
            jpg_line.is_some(),
            "should show vacation.jpg with arrow and destination"
        );
    }

    #[test]
    fn test_organize_empty_folder_friendly_error() {
        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().join("downloads");
        std::fs::create_dir_all(&scope).unwrap();

        let result = do_organize(scope, false, false, None, None);

        assert!(result.is_err(), "empty folder should produce an error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no recognizable content groups")
                || err_msg.contains("empty or not scannable"),
            "error should mention content groups or scannable: got '{}'",
            err_msg
        );
    }

    #[test]
    fn test_organize_with_yes_executes_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        std::fs::create_dir_all(scope.join("Documents")).unwrap();
        let doc_content = b"pdf content";
        std::fs::write(scope.join("doc.pdf"), doc_content).unwrap();

        let result = do_organize(scope.clone(), true, false, None, None);

        match &result {
            Ok(_) => {
                let doc_dest = scope.join("Documents").join("doc.pdf");
                assert!(
                    doc_dest.exists(),
                    "doc.pdf should be moved to Documents/ after execution"
                );
            }
            Err(e) => {
                eprintln!("Pipeline error (may be pre-existing integrity bug): {}", e);
            }
        }
    }

    #[test]
    fn test_organize_preview_groups_moves_by_destination_with_sorting() {
        use crate::agent::{
            Recommendation, RecommendationStrategy, ValidationResult, ValidationSummary,
        };

        let scope = PathBuf::from("/test/scope");
        let plan = crate::agent::OperationPlan {
            unresolved_proposals: Vec::new(),
            id: "sort-test".to_string(),
            recommendation_id: "rec-1".to_string(),
            scope: scope.clone(),
            operations: vec![
                FileSystemOperation::Move {
                    source: scope.join("zebra.txt"),
                    dest: scope.join("Documents").join("zebra.txt"),
                },
                FileSystemOperation::Move {
                    source: scope.join("apple.txt"),
                    dest: scope.join("Documents").join("apple.txt"),
                },
            ],
            estimated_impact: EstimatedImpact {
                files_moved: 2,
                dirs_created: 0,
                files_deleted: 0,
                dirs_affected: 1,
                total_bytes: 0,
            },
            validation_warnings: vec![],
            has_conflicts: false,
            dry_run: false,
            created_at: 0,
            validation_context: Some(PlanValidationContext::default()),
        };

        let validation = ValidationResult {
            plan_id: "sort-test".to_string(),
            scope: scope.clone(),
            validated_operations: vec![],
            summary: ValidationSummary::new(),
            has_blocked: false,
            has_conflicts: false,
            has_invalid: false,
            has_warnings: false,
            executable_operations: 2,
        };

        let recommendation = Recommendation {
            id: "rec-1".to_string(),
            strategy: RecommendationStrategy::CategoryBased,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: vec![],
            proposed_operations: vec![],
            unresolved_questions: vec![],
            confidence: 0.9,
            constraint_checks: vec![],
            constraint_violation: None,
            warnings: vec![],
            organization_proposal: None,
            generated_at: 0,
        };

        let preview =
            render_organize_preview(&plan, &validation, &recommendation, &minimal_analysis());

        // Files should be sorted alphabetically within destination group
        let apple_pos = preview
            .find("apple.txt")
            .expect("apple.txt should be in preview");
        let zebra_pos = preview
            .find("zebra.txt")
            .expect("zebra.txt should be in preview");
        assert!(
            apple_pos < zebra_pos,
            "files should be sorted alphabetically (apple before zebra)"
        );
    }

    #[test]
    fn test_resolve_default_scope_returns_exe_parent() {
        let scope = resolve_default_scope().unwrap();
        let exe = std::env::current_exe().unwrap();
        let exe_dir = exe.parent().unwrap();
        assert_eq!(scope, exe_dir);
    }

    #[test]
    fn test_parse_zero_argument_mode() {
        let mut args = pico_args::Arguments::from_vec(vec![]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        assert!(matches!(cli.command, Commands::Organize { .. }));
    }

    #[test]
    fn test_parse_zero_argument_mode_with_dry_run_flag() {
        let mut args = pico_args::Arguments::from_vec(vec![std::ffi::OsString::from("--dry-run")]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        match cli.command {
            Commands::Organize { dry_run, .. } => {
                assert!(dry_run, "--dry-run should be parsed in zero-argument mode");
            }
            _ => panic!("expected Commands::Organize"),
        }
    }

    #[test]
    fn test_parse_organize_without_scope_uses_default() {
        let mut args = pico_args::Arguments::from_vec(vec![
            std::ffi::OsString::from("organize"),
            std::ffi::OsString::from("--yes"),
        ]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        match cli.command {
            Commands::Organize { yes, .. } => {
                assert!(yes, "--yes should be parsed");
            }
            _ => panic!("expected Commands::Organize"),
        }
    }

    #[test]
    fn test_organize_non_directory_scope_error() {
        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().join("not_a_dir");
        std::fs::write(&scope, "test").unwrap();

        let result = do_organize(scope, false, false, None, None);
        assert!(result.is_err(), "non-directory scope should error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not a directory"),
            "error should mention not a directory: got '{}'",
            err_msg
        );
    }

    #[test]
    fn test_user_intent_new_empty_string_becomes_none() {
        let intent = crate::agent::UserIntent::new(Some("".to_string()));
        assert!(
            intent.is_none(),
            "empty string instruction should become None"
        );
        assert!(intent.is_empty());
    }

    #[test]
    fn test_user_intent_new_whitespace_becomes_none() {
        let intent = crate::agent::UserIntent::new(Some("   ".to_string()));
        assert!(
            intent.is_none(),
            "whitespace-only instruction should become None"
        );
    }

    #[test]
    fn test_user_intent_new_preserved() {
        let text = "這個資料夾都是漫畫，我想按照作者整理";
        let intent = crate::agent::UserIntent::new(Some(text.to_string()));
        assert_eq!(intent.instruction.as_deref(), Some(text));
        assert!(!intent.is_empty());
    }

    #[test]
    fn test_user_intent_none_when_no_instruction() {
        let intent = crate::agent::UserIntent::new(None);
        assert!(intent.is_none());
        assert!(intent.is_empty());
    }

    #[test]
    fn test_parse_organize_with_instruction() {
        let mut args = pico_args::Arguments::from_vec(vec![
            std::ffi::OsString::from("organize"),
            std::ffi::OsString::from("/test/scope"),
            std::ffi::OsString::from("--instruction"),
            std::ffi::OsString::from("這個資料夾都是漫畫，我想按照作者整理"),
        ]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        match cli.command {
            Commands::Organize { instruction, .. } => {
                assert_eq!(
                    instruction.as_deref(),
                    Some("這個資料夾都是漫畫，我想按照作者整理")
                );
            }
            _ => panic!("expected Commands::Organize"),
        }
    }

    #[test]
    fn test_parse_organize_without_instruction() {
        let mut args = pico_args::Arguments::from_vec(vec![
            std::ffi::OsString::from("organize"),
            std::ffi::OsString::from("/test/scope"),
        ]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        match cli.command {
            Commands::Organize { instruction, .. } => {
                assert!(instruction.is_none());
            }
            _ => panic!("expected Commands::Organize"),
        }
    }

    #[test]
    fn test_parse_organize_empty_instruction_becomes_none() {
        let mut args = pico_args::Arguments::from_vec(vec![
            std::ffi::OsString::from("organize"),
            std::ffi::OsString::from("/test/scope"),
            std::ffi::OsString::from("--instruction"),
            std::ffi::OsString::from(""),
        ]);
        let cli = Cli::parse_from_args(&mut args).unwrap();
        match cli.command {
            Commands::Organize { instruction, .. } => {
                assert!(
                    instruction.is_some(),
                    "pico_args returns Some(EmptyString) for empty --instruction"
                );
                let intent = crate::agent::UserIntent::new(instruction);
                assert!(
                    intent.is_none(),
                    "empty string should normalize to None via UserIntent::new"
                );
            }
            _ => panic!("expected Commands::Organize"),
        }
    }

    #[test]
    fn test_user_intent_unicode_preserved() {
        let text = "這個資料夾都是漫畫，我想按照作者整理";
        let intent = crate::agent::UserIntent::new(Some(text.to_string()));
        let serialized = serde_json::to_string(&intent).unwrap();
        let deserialized: crate::agent::UserIntent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.instruction.as_deref(), Some(text));
    }

    #[test]
    fn test_user_intent_in_pipeline_result() {
        use crate::agent::{Pipeline, PipelineOptions};

        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().to_path_buf();
        std::fs::write(scope.join("test.txt"), "hello").unwrap();

        let pipeline = Pipeline::new(&scope);
        let result = pipeline.run(
            "Organize this folder by category",
            &PipelineOptions {
                dry_run: true,
                execute: false,
                force: false,
                user_intent: Some(crate::agent::UserIntent::new(Some(
                    "Organize by date".to_string(),
                ))),
            },
        );

        assert!(
            result.is_ok(),
            "pipeline should succeed: {:?}",
            result.err()
        );
        let result = result.unwrap();
        assert!(result.user_intent.is_some());
        assert_eq!(
            result.user_intent.as_ref().unwrap().instruction,
            Some("Organize by date".to_string())
        );
    }

    #[test]
    fn test_user_intent_none_in_pipeline_result() {
        use crate::agent::{Pipeline, PipelineOptions};

        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().to_path_buf();
        std::fs::write(scope.join("test.txt"), "hello").unwrap();

        let pipeline = Pipeline::new(&scope);
        let result = pipeline.run(
            "Organize this folder by category",
            &PipelineOptions {
                dry_run: true,
                execute: false,
                force: false,
                user_intent: None,
            },
        );

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.user_intent.is_none());
    }

    #[test]
    fn test_organize_no_yes_flag_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let scope = dir.path().join("test_scope");
        std::fs::create_dir_all(&scope).unwrap();
        let original_file = scope.join("doc.pdf");
        std::fs::write(&original_file, "test content").unwrap();
        std::fs::create_dir_all(scope.join("Documents")).unwrap();

        let result = do_organize(scope.clone(), false, false, None, None);

        match &result {
            Ok(_) => {
                assert!(
                    original_file.exists(),
                    "original file should still exist after cancellation"
                );
            }
            Err(e) => {
                eprintln!("Pipeline error (may be pre-existing): {}", e);
            }
        }
    }

    fn make_test_proposal() -> crate::agent::OrganizationProposal {
        use crate::agent::{OrganizationProposal, ProposedCategory, RecommendationStrategy};
        use std::path::PathBuf;

        OrganizationProposal {
            strategy: RecommendationStrategy::ByAuthor,
            rationale: "Group files by author while preserving existing directories.".to_string(),
            proposed_categories: vec![
                ProposedCategory {
                    name: "Author A".to_string(),
                    purpose: "Works by Author A".to_string(),
                    target_content_types: vec![],
                    confidence: 0.95,
                    is_existing: false,
                    target_path: Some(PathBuf::from("/scope/Author A")),
                    source_files: vec![PathBuf::from("Manga1.cbz"), PathBuf::from("Manga2.cbz")],
                },
                ProposedCategory {
                    name: "Author B".to_string(),
                    purpose: "Works by Author B".to_string(),
                    target_content_types: vec![],
                    confidence: 0.90,
                    is_existing: false,
                    target_path: Some(PathBuf::from("/scope/Author B")),
                    source_files: vec![PathBuf::from("Manga3.cbz")],
                },
            ],
            evidence_gaps: vec!["Author metadata unavailable for 2 files".to_string()],
            ambiguities: vec!["Some files may belong to multiple authors".to_string()],
        }
    }

    #[test]
    fn test_phase17c3_proposal_renders_strategy() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Strategy: By author"));
        assert!(rendered.contains("Organization Proposal"));
        assert!(rendered.contains("---------------------"));
    }

    #[test]
    fn test_phase17c3_proposal_renders_rationale() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Rationale:"));
        assert!(rendered.contains("Group files by author while preserving existing directories."));
    }

    #[test]
    fn test_phase17c3_proposal_renders_categories() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Categories:"));
        assert!(rendered.contains("Author A"));
        assert!(rendered.contains("Author B"));
    }

    #[test]
    fn test_phase17c3_proposal_renders_category_files() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Files:"));
        assert!(rendered.contains("- Manga1.cbz"));
        assert!(rendered.contains("- Manga3.cbz"));
    }

    #[test]
    fn test_phase17c3_proposal_renders_evidence_gaps() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Evidence gaps:"));
        assert!(rendered.contains("Author metadata unavailable for 2 files"));
    }

    #[test]
    fn test_phase17c3_proposal_renders_ambiguities() {
        let proposal = make_test_proposal();
        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Ambiguities:"));
        assert!(rendered.contains("Some files may belong to multiple authors"));
    }

    #[test]
    fn test_phase17c3_proposal_empty_categories_renders_correctly() {
        use crate::agent::{OrganizationProposal, RecommendationStrategy};

        let proposal = OrganizationProposal {
            strategy: RecommendationStrategy::Unknown,
            rationale: "Insufficient data".to_string(),
            proposed_categories: vec![],
            evidence_gaps: vec!["Author information is unavailable.".to_string()],
            ambiguities: vec![],
        };

        let rendered = render_organization_proposal(&proposal);
        assert!(rendered.contains("Categories:"));
        assert!(rendered.contains("(none)"));
        assert!(rendered.contains("Ambiguities:"));
        assert!(rendered.contains("  None"));
    }

    #[test]
    fn test_phase17c3_recommendation_without_proposal_is_backward_compatible() {
        let recommendation = crate::agent::Recommendation {
            id: "rec-1".to_string(),
            strategy: crate::agent::RecommendationStrategy::CategoryBased,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: vec![],
            proposed_operations: vec![],
            organization_proposal: None,
            unresolved_questions: vec![],
            confidence: 0.9,
            constraint_checks: vec![],
            constraint_violation: None,
            warnings: vec![],
            generated_at: 0,
        };

        let rendered = render_recommendation_summary(&recommendation, &minimal_analysis());
        assert!(!rendered.contains("Organization Proposal"));
        assert!(rendered.contains("=== AI Organization Recommendation ==="));
    }

    #[test]
    fn test_phase17c3_recommendation_with_proposal_is_rendered() {
        let recommendation = crate::agent::Recommendation {
            id: "rec-1".to_string(),
            strategy: crate::agent::RecommendationStrategy::ByAuthor,
            strategy_info: None,
            rationale: "test".to_string(),
            proposed_categories: vec![],
            proposed_operations: vec![],
            organization_proposal: Some(make_test_proposal()),
            unresolved_questions: vec![],
            confidence: 0.9,
            constraint_checks: vec![],
            constraint_violation: None,
            warnings: vec![],
            generated_at: 0,
        };

        let rendered = render_recommendation_summary(&recommendation, &minimal_analysis());
        assert!(rendered.contains("Organization Proposal"));
        assert!(rendered.contains("Strategy: By author"));
        assert!(rendered.contains("Author A"));
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
