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
    #[error("temporary failure: {message}")]
    Temporary {
        /// Human-readable failure description.
        message: String,
        /// Seconds the provider asked the client to wait, if it said so.
        retry_after_seconds: Option<u64>,
    },

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
    /// Creates a temporary failure without a retry hint.
    pub fn temporary(message: impl Into<String>) -> Self {
        Self::Temporary {
            message: message.into(),
            retry_after_seconds: None,
        }
    }

    /// Creates a temporary failure with the provider's retry hint.
    pub fn temporary_after(message: impl Into<String>, retry_after_seconds: Option<u64>) -> Self {
        Self::Temporary {
            message: message.into(),
            retry_after_seconds,
        }
    }

    /// Stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Input(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::Auth(_) => "auth",
            Self::Temporary { .. } => "temporary",
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
            Self::Temporary { .. } => 5,
            Self::Format(_) => 6,
            Self::UnknownMutation(_) => 7,
            Self::Internal(_) => 1,
        }
    }

    /// Returns the provider's retry hint, if any.
    pub const fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::Temporary {
                retry_after_seconds,
                ..
            } => *retry_after_seconds,
            _ => None,
        }
    }
}

/// Convenience result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_errors_keep_their_retry_hint() {
        let error = Error::temporary_after("rate limited", Some(30));
        assert_eq!(error.code(), "temporary");
        assert_eq!(error.exit_code(), 5);
        assert_eq!(error.retry_after_seconds(), Some(30));
    }

    #[test]
    fn other_errors_have_no_retry_hint() {
        assert_eq!(Error::Input("x".to_owned()).retry_after_seconds(), None);
    }
}
