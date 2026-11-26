//! Output helpers for JSON and human-readable text.

use std::io::Write;

use serde::Serialize;

use crate::error::{Error, Result};

/// Output format selected by the global `--json` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text.
    Human,
    /// Machine-readable JSON.
    Json,
}

/// Writes a value as newline-terminated JSON to stdout.
pub fn write_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, value)
        .map_err(|error| Error::Internal(format!("could not write JSON: {error}")))?;
    writeln!(lock).map_err(|error| Error::Internal(format!("could not write newline: {error}")))
}

/// Writes a structured error as newline-terminated JSON to stderr.
pub fn write_error_json(error: &Error) -> Result<()> {
    #[derive(Serialize)]
    struct Envelope<'a> {
        error: Body<'a>,
    }

    #[derive(Serialize)]
    struct Body<'a> {
        code: &'a str,
        message: String,
    }

    let envelope = Envelope {
        error: Body {
            code: error.code(),
            message: error.to_string(),
        },
    };

    let stderr = std::io::stderr();
    let mut lock = stderr.lock();
    serde_json::to_writer(&mut lock, &envelope)
        .map_err(|error| Error::Internal(format!("could not write JSON error: {error}")))?;
    writeln!(lock).map_err(|error| Error::Internal(format!("could not write newline: {error}")))
}
