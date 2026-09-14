use crate::evidence::{DirectoryEvidence, ScanLimits};
use crate::scanner::Scanner;
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
    Schema {
        output: Option<PathBuf>,
    },
}

impl Cli {
    pub fn parse() -> Result<Self> {
        let mut args = pico_args::Arguments::from_env();
        
        // Handle subcommand
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
                        ..Default::default()
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

                write_jsonl(&result.evidence, output)?;
            }
            Commands::Inspect { path, output } => {
                let limits = ScanLimits::default();
                let scanner = Scanner::with_limits(&path, limits);
                let result = scanner.scan()?;

                let evidence = result.evidence.iter().find(|e| e.path == path);
                match evidence {
                    Some(e) => write_jsonl(&[e.clone()], output)?,
                    None => return Err(anyhow!("No evidence found for path: {}", path.display())),
                }
            }
            Commands::Schema { output } => {
                let schema = include_str!("../schemas/directory-evidence.json");
                write_output(schema, output)?;
            }
        }
        Ok(())
    }
}

fn write_jsonl(evidence: &[DirectoryEvidence], output: Option<PathBuf>) -> Result<()> {
    let mut writer: Box<dyn std::io::Write> = match output {
        Some(path) => Box::new(std::fs::File::create(path)?),
        None => Box::new(std::io::stdout()),
    };

    for e in evidence {
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