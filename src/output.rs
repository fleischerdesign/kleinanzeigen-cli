//! Output helpers for the stable machine contract.

use std::io::Write;

use serde::Serialize;

use crate::error::{Error, Result};

/// Version of the machine-readable JSON contract.
pub const SCHEMA_VERSION: u32 = 1;

/// Output format selected by the global `--json` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text.
    Human,
    /// Machine-readable JSON.
    Json,
}

#[derive(Serialize)]
struct Envelope<'a, T> {
    schema_version: u32,
    data: &'a T,
}

/// Writes a value as newline-terminated JSON to stdout.
///
/// Every command result is wrapped in an envelope carrying the schema version,
/// so agents can detect contract changes without parsing the binary version.
pub fn write_json<T: Serialize>(value: &T) -> Result<()> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        data: value,
    };

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, &envelope)
        .map_err(|error| Error::Internal(format!("could not write JSON: {error}")))?;
    writeln!(lock).map_err(|error| Error::Internal(format!("could not write newline: {error}")))
}

/// Writes one already-shaped JSON value as a flushed newline-terminated line.
///
/// This is for streaming commands such as `watch`; regular command results use
/// [`write_json`], which adds the success envelope.
pub fn write_json_line<T: Serialize>(value: &T) -> Result<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, value)
        .map_err(|error| Error::Internal(format!("could not write JSON line: {error}")))?;
    writeln!(lock).map_err(|error| Error::Internal(format!("could not write newline: {error}")))?;
    lock.flush()
        .map_err(|error| Error::Internal(format!("could not flush JSON line: {error}")))
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    schema_version: u32,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u64>,
}

/// Writes a structured error as newline-terminated JSON to stderr.
pub fn write_error_json(error: &Error) -> Result<()> {
    let envelope = ErrorEnvelope {
        schema_version: SCHEMA_VERSION,
        error: ErrorBody {
            code: error.code(),
            message: error.to_string(),
            retry_after_seconds: error.retry_after_seconds(),
        },
    };

    let stderr = std::io::stderr();
    let mut lock = stderr.lock();
    serde_json::to_writer(&mut lock, &envelope)
        .map_err(|error| Error::Internal(format!("could not write JSON error: {error}")))?;
    writeln!(lock).map_err(|error| Error::Internal(format!("could not write newline: {error}")))
}
