//! Command-line surface.

use clap::{Parser, Subcommand};

use crate::output::Format;

/// Unofficial, browser-free CLI for Kleinanzeigen.
#[derive(Debug, Parser)]
#[command(
    name = "kleinanzeigen",
    version,
    about = "Unofficial, browser-free CLI for Kleinanzeigen",
    long_about = None,
    arg_required_else_help = true,
    color = clap::ColorChoice::Never
)]
pub struct Cli {
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Log filter (error, warn, info, debug, trace).
    #[arg(long, global = true, env = "KLEINANZEIGEN_LOG", default_value = "warn")]
    pub log_level: String,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Returns the selected output format.
    pub const fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            Format::Human
        }
    }
}

/// Available commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the local installation and provider configuration.
    Doctor,
}
