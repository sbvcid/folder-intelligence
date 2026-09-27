mod agent;
mod classification;
mod cli;
mod evidence;
mod llm;
mod scanner;

use anyhow::Result;
use cli::Cli;

fn main() {
    // A double-clicked console window closes the moment the process exits, so
    // the user would never see the output. Hold the window open in that case.
    let interactive_launch = std::env::args_os().len() <= 1;

    match run_app() {
        Ok(()) => {
            if interactive_launch {
                hold_window_open();
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            hold_window_open();
            std::process::exit(1);
        }
    }
}

fn hold_window_open() {
    eprintln!();
    eprintln!("Press Enter to close this window...");
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
}

fn run_app() -> Result<()> {
    let cli = Cli::parse()?;
    cli.run()
}
