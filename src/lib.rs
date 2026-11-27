//! Unofficial, browser-free CLI for Kleinanzeigen.
//!
//! The crate is split into a thin command-line surface, a domain model, and a
//! provider boundary. A concrete provider can change without touching the CLI
//! or the domain.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Command-line parsing and the stable shell contract.
pub mod cli;

/// Command implementations.
pub mod commands;

/// Local diagnostics for the CLI.
pub mod doctor;

/// Core domain types shared by the CLI and providers.
pub mod domain;

/// Typed errors and stable process exit codes.
pub mod error;

/// Read-only Model Context Protocol server over stdio.
pub mod mcp;

/// Output helpers for JSON and human-readable text.
pub mod output;

/// The boundary between the CLI and a concrete Kleinanzeigen access path.
pub mod provider;

/// Local no-account state for watch progress, price history and saved searches.
mod state;
