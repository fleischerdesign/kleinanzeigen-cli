//! Binary entry point.

#![forbid(unsafe_code)]

use clap::Parser;
use kleinanzeigen_cli::cli::{Cli, Command};
use kleinanzeigen_cli::doctor;
use kleinanzeigen_cli::error::Result;
use kleinanzeigen_cli::output::write_error_json;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    init_tracing(&cli.log_level);

    if let Err(error) = run(cli).await {
        if json {
            let _ = write_error_json(&error);
        } else {
            eprintln!("error: {error}");
        }
        std::process::exit(i32::from(error.exit_code()));
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Doctor => doctor::run(cli.format()),
    }
}

fn init_tracing(filter: &str) {
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
