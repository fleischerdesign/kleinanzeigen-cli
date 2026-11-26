//! Typed errors and stable process exit codes.

/// The error type used across the crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid user input.
    #[error("invalid input: {0}")]
    Input(String),

    /// The requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// Authentication or authorization failed.
    #[error("authentication or access denied: {0}")]
    Auth(String),

    /// A temporary failure; retrying may help.
    #[error("temporary failure: {0}")]
    Temporary(String),

    /// The response did not match the expected format.
    #[error("unexpected response format: {0}")]
    Format(String),

    /// A mutation may or may not have been applied.
    #[error("mutation outcome unknown: {0}")]
    UnknownMutation(String),

    /// An internal invariant failed.
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Input(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::Auth(_) => "auth",
            Self::Temporary(_) => "temporary",
            Self::Format(_) => "format",
            Self::UnknownMutation(_) => "unknown_mutation",
            Self::Internal(_) => "internal",
        }
    }

    /// Stable process exit code.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Input(_) => 2,
            Self::NotFound(_) => 3,
            Self::Auth(_) => 4,
            Self::Temporary(_) => 5,
            Self::Format(_) => 6,
            Self::UnknownMutation(_) => 7,
            Self::Internal(_) => 1,
        }
    }
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, Error>;
