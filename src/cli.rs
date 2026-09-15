use crate::evidence::{ScanLimits, ScanResult};
use crate::scanner::Scanner;
use crate::classification::ClassificationProcessor;
use crate::classification::AiClassifier;
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
        output: Option<PathBuf>,
    },
    Schema {
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
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::ClassifyAi { target, category_root, model, output }
            }
            "schema" => {
                let output = args.opt_value_from_os_str("--output", |s| Ok::<_, anyhow::Error>(PathBuf::from(s)))?;
                Commands::Schema { output }
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
            Commands::ClassifyAi { target, category_root, model, output } => {
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
                    crate::classification::MockAiClassifier::default()
                } else {
                    return Err(anyhow!("Provider '{}' not available in Phase 4A. Use --model mock", model));
                };

                let ai_result = ai_classifier.classify(&input, &baseline)?;

                let json_output = serde_json::to_string_pretty(&ai_result)?;
                write_output(&json_output, output)?;
            }
            Commands::Schema { output } => {
                let schema = include_str!("../schemas/directory-evidence.json");
                write_output(schema, output)?;
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
