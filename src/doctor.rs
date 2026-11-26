//! Local diagnostics for the CLI.

use serde::Serialize;

use crate::error::Result;
use crate::output::{Format, write_json};

/// Result of the local diagnostic check.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    /// Overall status.
    pub status: &'static str,
    /// CLI version.
    pub version: &'static str,
    /// Operating system.
    pub platform: &'static str,
    /// CPU architecture.
    pub arch: &'static str,
    /// Configured provider, if any.
    pub provider: &'static str,
    /// Human-readable notes.
    pub notes: Vec<&'static str>,
}

/// Runs the local diagnostic check and writes the report.
pub fn run(format: Format) -> Result<()> {
    let report = DoctorReport {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        provider: "not configured",
        notes: vec![
            "No browser or Chromium is required.",
            "The mobile-API provider is not implemented yet.",
        ],
    };

    match format {
        Format::Json => write_json(&report),
        Format::Human => {
            println!("kleinanzeigen-cli {}", report.version);
            println!("status: {}", report.status);
            println!("platform: {} {}", report.platform, report.arch);
            println!("provider: {}", report.provider);
            for note in &report.notes {
                println!("note: {note}");
            }
            Ok(())
        }
    }
}
