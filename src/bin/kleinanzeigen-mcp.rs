//! Stdio entry point for the read-only MCP server.
//!
//! Stdout is reserved for JSON-RPC messages; fatal errors go to stderr.

#![forbid(unsafe_code)]

#[tokio::main]
async fn main() {
    if let Err(error) = kleinanzeigen_cli::mcp::run().await {
        eprintln!("kleinanzeigen-mcp: {error}");
        std::process::exit(1);
    }
}
