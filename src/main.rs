mod agent;
mod classification;
mod cli;
mod evidence;
mod llm;
mod scanner;

use anyhow::Result;
use cli::Cli;

fn main() {
    if let Err(e) = run_app() {
        eprintln!("Error: {}", e);
        eprintln!("Press Enter to continue...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        std::process::exit(1);
    }
}

fn run_app() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}
