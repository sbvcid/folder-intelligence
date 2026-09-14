mod cli;
mod evidence;
mod scanner;

use cli::Cli;
use anyhow::Result;

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}