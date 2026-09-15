use crate::evidence::{ScanLimits, ScanResult};
use crate::scanner::Scanner;
use crate::classification::ClassificationProcessor;
use crate::classification::AiClassifier;
use crate::agent::TaskIntentParser;
use anyhow::{anyhow, Result};
use std::path::PathBuf;

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
        output: Option<PathBuf>,
    },
}

impl Cli {
    pub fn parse() -> Result<Self> {
        let mut args = pico_args::Arguments::from_env();

        let subcommand = args.subcommand()?.ok_or_else(|| anyhow!("No subcommand provided"))?;

        let command = match subcommand.as_str() {
            "scan" => {
                let path: PathBuf = args.free_from_os_str::<PathBuf, anyhow::Error>(|s| Ok(PathBuf::from(s)))?;
                let max_depth = args.opt_value_from_str(["-d", "--max-depth"])?.unwrap_or(50);
                let max_files_per_dir = args.opt_value_from_str(["-f", "--max-files-per-dir"])?.unwrap_or(10000);
                let max_total_files = args.opt_value_from_str(["-t", "--max-total-files"])?.unwrap_or(1000000);
                let max_total_dirs = args.opt_value_from_str(["-D", "--max-total-dirs"])?.unwrap_or(100000);
                let max_representative_files = args.opt_value_from_str("--max-representative-files")?.unwrap_or(20);
                let max_child_dirs = args.opt_value_from_str("--max-child-dirs")?.unwrap_or(500);
                let timeout = args.opt_value_from_str("--timeout")?.unwrap_or(3600);
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
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
                let path: PathBuf = args.free_from_os_str::<PathBuf, anyhow::Error>(|s| Ok(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Inspect { path, output }
            }
            "classify" => {
                let target = args.value_from_os_str("--target", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let category_root = args.value_from_os_str("--category-root", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Classify { target, category_root, output }
            }
            "classify-ai" => {
                let target = args.value_from_os_str("--target", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let category_root = args.value_from_os_str("--category-root", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let model = args.opt_value_from_str("--model")?.unwrap_or_else(|| "mock".to_string());
                let api_key = args.opt_value_from_str("--api-key")?;
                let base_url = args.opt_value_from_str("--base-url")?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::ClassifyAi { target, category_root, model, api_key, base_url, output }
            }
            "schema" => {
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Schema { output }
            }
            "intent" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Intent { request, scope, output }
            }
            "analyze" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Analyze { request, scope, output }
            }
            "recommend" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Recommend { request, scope, output }
            }
            "clarify" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Clarify { request, scope, output }
            }
            "plan" => {
                let request = args
                    .free_from_str::<String>()
                    .map_err(|_| anyhow!("No intent request provided"))?;
                let scope = args.opt_value_from_os_str("--scope", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Plan { request, scope, output }
            }
            _ => return Err(anyhow!("Unknown subcommand: {}", subcommand)),
        };

        Ok(Cli { command })
    }

    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::Scan { path, limits, output, quiet } => {
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
            Commands::Classify { target, category_root, output } => {
                let limits = ScanLimits::default();

                // Scan target directory
                let target_scanner = Scanner::with_limits(&target, limits.clone());
                let target_scan = target_scanner.inspect_single()?;
                let target_evidence = target_scan.evidence.into_iter().next()
                    .ok_or_else(|| anyhow!("Failed to scan target path: {}", target.display()))?;

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
            Commands::ClassifyAi { target, category_root, model, api_key, base_url, output } => {
                let limits = ScanLimits::default();

                let target_scanner = Scanner::with_limits(&target, limits.clone());
                let target_scan = target_scanner.inspect_single()?;
                let target_evidence = target_scan.evidence.into_iter().next()
                    .ok_or_else(|| anyhow!("Failed to scan target path: {}", target.display()))?;

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
                    crate::classification::RealAiClassifier::Mock(crate::classification::MockAiClassifier::default())
                } else {
                    #[cfg(feature = "network")]
                    {
                        let key = api_key.ok_or_else(|| anyhow!("--api-key is required for model '{}'", model))?;
                        crate::classification::RealAiClassifier::OpenAi(crate::classification::OpenAiProvider::new(
                            crate::classification::OpenAiProviderConfig {
                                base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string()),
                                api_key: key,
                                model: model.clone(),
                                timeout: std::time::Duration::from_secs(60),
                            },
                        ))
                    }
                    #[cfg(not(feature = "network"))]
                    {
                        return Err(anyhow!("Network feature not enabled for provider '{}'. Use --model mock", model));
                    }
                };

                let ai_result = match ai_classifier {
                    crate::classification::RealAiClassifier::Mock(m) => m.classify(&input, &baseline)?,
                    #[cfg(feature = "network")]
                    crate::classification::RealAiClassifier::OpenAi(p) => p.classify(&input, &baseline)?,
                };

                let json_output = serde_json::to_string_pretty(&ai_result)?;
                write_output(&json_output, output)?;
            }
            Commands::Schema { output } => {
                let schema = include_str!("../schemas/directory-evidence.json");
                write_output(schema, output)?;
            }
            Commands::Intent { request, scope, output } => {
                let parser = if let Some(s) = scope {
                    TaskIntentParser::new(s)
                } else {
                    TaskIntentParser::default()
                };
                let intent = parser.parse(&request)?;
                let json_output = serde_json::to_string_pretty(&intent)?;
                write_output(&json_output, output)?;
            }
            Commands::Analyze { request, scope, output } => {
                let parser = if let Some(s) = scope {
                    TaskIntentParser::new(s)
                } else {
                    TaskIntentParser::default()
                };
                let intent = parser.parse(&request)?;
                let analyzer = crate::agent::EvidenceAnalyzer;
                let analysis = analyzer.analyze(&intent)?;
                let json_output = serde_json::to_string_pretty(&analysis)?;
                write_output(&json_output, output)?;
            }
            Commands::Recommend { request, scope, output } => {
                let parser = if let Some(s) = scope {
                    TaskIntentParser::new(s)
                } else {
                    TaskIntentParser::default()
                };
                let intent = parser.parse(&request)?;
                let analyzer = crate::agent::EvidenceAnalyzer;
                let analysis = analyzer.analyze(&intent)?;
                let engine = crate::agent::RecommendationEngine;
                let recommendation = engine.recommend(&intent, &analysis)?;
                let json_output = serde_json::to_string_pretty(&recommendation)?;
                write_output(&json_output, output)?;
            }
            Commands::Clarify { request, scope, output } => {
                let parser = if let Some(s) = scope {
                    TaskIntentParser::new(s)
                } else {
                    TaskIntentParser::default()
                };
                let intent = parser.parse(&request)?;
                let analyzer = crate::agent::EvidenceAnalyzer;
                let analysis = analyzer.analyze(&intent)?;
                let engine = crate::agent::RecommendationEngine;
                let recommendation = engine.recommend(&intent, &analysis)?;

                let clarifier = crate::agent::ClarificationEngine;
                let summary = clarifier.summarize(&recommendation);
                write_output(&summary, output)?;
            }
            Commands::Plan { request, scope, output } => {
                let parser = if let Some(s) = scope {
                    TaskIntentParser::new(s)
                } else {
                    TaskIntentParser::default()
                };
                let intent = parser.parse(&request)?;
                let analyzer = crate::agent::EvidenceAnalyzer;
                let analysis = analyzer.analyze(&intent)?;
                let engine = crate::agent::RecommendationEngine;
                let recommendation = engine.recommend(&intent, &analysis)?;

                let generator = crate::agent::PlanGenerator;
                let plan = generator.generate(&recommendation, &analysis, &[])?;
                let preview = generator.preview(&plan);
                write_output(&preview, output)?;
            }
        }
        Ok(())
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
