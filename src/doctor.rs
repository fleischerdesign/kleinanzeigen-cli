//! Local diagnostics for the CLI.

use serde::Serialize;

use crate::error::Result;
use crate::output::{Format, write_json};
use crate::provider::mobile_api::AppApiSettings;

/// Result of the local diagnostic check.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    /// Overall CLI status.
    pub status: &'static str,
    /// CLI version.
    pub version: &'static str,
    /// Operating system.
    pub platform: &'static str,
    /// CPU architecture.
    pub arch: &'static str,
    /// Configured provider.
    pub provider: &'static str,
    /// App version used for provider requests, when configuration is readable.
    pub app_version: Option<String>,
    /// Human-readable notes.
    pub notes: Vec<String>,
}

/// Runs the local diagnostic check and writes the report.
pub fn run(format: Format) -> Result<()> {
    let report = report();

    match format {
        Format::Json => write_json(&report),
        Format::Human => {
            println!("kleinanzeigen-cli {}", report.version);
            println!("status: {}", report.status);
            println!("platform: {} {}", report.platform, report.arch);
            println!("provider: {}", report.provider);
            if let Some(app_version) = &report.app_version {
                println!("app version: {app_version}");
            }
            for note in &report.notes {
                println!("note: {note}");
            }
            Ok(())
        }
    }
}

fn report() -> DoctorReport {
    let base = DoctorReport {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        provider: "mobile-api (unconfigured)",
        app_version: None,
        notes: vec!["No browser or Chromium is required.".to_owned()],
    };

    match AppApiSettings::from_env() {
        Ok(settings) if settings.is_configured() => DoctorReport {
            provider: "mobile-api (configured)",
            app_version: Some(settings.app_version().to_owned()),
            notes: with_read_note(base.notes.clone()),
            ..base
        },
        Ok(settings) => DoctorReport {
            provider: "mobile-api (missing credentials)",
            app_version: Some(settings.app_version().to_owned()),
            notes: with_credentials_note(base.notes.clone()),
            ..base
        },
        Err(error) => DoctorReport {
            notes: {
                let mut notes = base.notes.clone();
                notes.push(error.to_string());
                notes
            },
            ..base
        },
    }
}

fn with_read_note(mut notes: Vec<String>) -> Vec<String> {
    notes.push("Read-only: detail and search use the mobile API.".to_owned());
    notes
}

fn with_credentials_note(mut notes: Vec<String>) -> Vec<String> {
    notes.push(
        "Set KLEINANZEIGEN_BASIC_USER and KLEINANZEIGEN_BASIC_PW to enable reads.".to_owned(),
    );
    notes
}
