mod cli;
mod evidence;
mod scanner;
mod classification;
mod agent;

use cli::Cli;
use anyhow::Result;

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}