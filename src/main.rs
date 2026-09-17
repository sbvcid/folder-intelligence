mod agent;
mod classification;
mod cli;
mod evidence;
mod llm;
mod scanner;

use anyhow::Result;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}
