//! Top-level error type and the exit-code map (design §6).
//!
//! Exit codes: `0` ok · `1` API/runtime error · `2` usage · `3` not authenticated ·
//! `4` billing required (402) · `75` rate-limited/timeout after honoring
//! `Retry-After` + backoff.

use std::process::ExitCode;

/// Shorthand used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

// The API/auth slices construct `NotAuthenticated`, `BillingRequired` and `RateLimited`;
// the exit-code map below is part of the Slice 0 contract, so they are dead for now.
#[allow(dead_code)]
/// Every failure path in the CLI maps to exactly one exit code.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Slice 0 staging: the command is registered but a later slice implements it.
    #[error("not implemented yet: {path}")]
    NotImplemented {
        /// Full command path as a user would type it, e.g. `postgres users list`.
        path: String,
    },

    /// Bad invocation, unresolvable command path (exit 2).
    #[error("usage error: {0}")]
    Usage(String),

    /// No usable credentials for the selected profile (exit 3).
    #[error("not authenticated: {0}")]
    NotAuthenticated(String),

    /// The API answered 402 `BILLING_REQUIRED` / `INSUFFICIENT_CREDIT` (exit 4).
    #[error("billing required: {0}")]
    BillingRequired(String),

    /// 429 that survived `Retry-After` + backoff (exit 75).
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// Anything else — I/O, transport, decoding (exit 1).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    /// The staging error for a command path that has no implementation yet.
    pub fn not_implemented(path: impl Into<String>) -> Self {
        Self::NotImplemented { path: path.into() }
    }

    /// Process exit code for this error (design §6).
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NotImplemented { .. } | Self::Other(_) => 1,
            Self::Usage(_) => 2,
            Self::NotAuthenticated(_) => 3,
            Self::BillingRequired(_) => 4,
            Self::RateLimited(_) => 75,
        }
    }

    /// [`ExitCode`] form of [`Self::exit_code`].
    pub fn code(&self) -> ExitCode {
        ExitCode::from(self.exit_code())
    }
}
