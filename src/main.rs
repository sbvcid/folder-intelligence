mod cli;
mod evidence;
mod scanner;
mod classification;

use cli::Cli;
use anyhow::Result;

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}