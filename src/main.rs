//! Binary entry point.

#![forbid(unsafe_code)]

use clap::Parser;
use clap::error::ErrorKind;
use kleinanzeigen_cli::cli::{Cli, Command};
use kleinanzeigen_cli::commands;
use kleinanzeigen_cli::doctor;
use kleinanzeigen_cli::error::{Error, Result};
use kleinanzeigen_cli::output::{Format, write_error_json};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let cli = parse_cli();
    let format = cli.format();
    init_tracing(&cli.log_level, format);

    if let Err(error) = run(cli).await {
        match format {
            Format::Json => {
                let _ = write_error_json(&error);
            }
            Format::Human => eprintln!("error: {error}"),
        }
        std::process::exit(i32::from(error.exit_code()));
    }
}

fn parse_cli() -> Cli {
    let json_requested = std::env::args_os().any(|argument| argument == "--json");

    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                let _ = error.print();
                std::process::exit(0);
            }
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand if !json_requested => {
                let _ = error.print();
                std::process::exit(2);
            }
            _ => {
                let message = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or("invalid command line")
                    .trim_start_matches("error: ")
                    .to_owned();
                let error = Error::Input(message);
                if json_requested {
                    let _ = write_error_json(&error);
                } else {
                    eprintln!("error: {error}");
                }
                std::process::exit(i32::from(error.exit_code()));
            }
        },
    }
}

async fn run(cli: Cli) -> Result<()> {
    let format = cli.format();
    match cli.command {
        Command::Doctor => doctor::run(format),
        Command::Search(args) => commands::search(args, format).await,
        Command::Get(args) => commands::get(args, format).await,
        Command::Categories(args) => commands::categories(args, format).await,
        Command::Locations(args) => commands::locations(args, format).await,
        Command::Similar(args) => commands::similar(args, format).await,
        Command::SellerOther(args) => commands::seller_other(args, format).await,
        Command::Gallery(args) => commands::gallery(args, format).await,
        Command::Images(args) => commands::images(args, format).await,
        Command::Profile(args) => commands::profile(args, format).await,
        Command::Stats(args) => commands::stats(args, format).await,
        Command::StatsBatch(args) => commands::stats_batch(args, format).await,
        Command::Location(args) => commands::location(args, format).await,
        Command::FlagReasons => commands::flag_reasons(format).await,
        Command::SearchMetadata(args) => commands::search_metadata(args, format).await,
        Command::Watch(args) => commands::watch(args, format).await,
        Command::History(args) => commands::history(args, format),
        Command::Saved(args) => commands::saved(args.command, format),
        Command::Export(args) => commands::export(args, format),
    }
}

fn init_tracing(filter: &str, format: Format) {
    // In JSON mode stderr is reserved for exactly one error object.
    if format == Format::Json {
        return;
    }

    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
