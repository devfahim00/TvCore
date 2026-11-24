//! Error type shared by every TvCore module.

use std::fmt;

/// All errors produced by TvCore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TvError {
    /// A network request failed (transport level, DNS, TLS, timeout...).
    Http(String),
    /// The server answered, but with an unexpected status code.
    HttpStatus(u16),
    /// A playlist could not be parsed.
    Parse(String),
    /// No channel matched the given id / query.
    NotFound(String),
    /// Caller passed something invalid (empty query, bad path...).
    InvalidInput(String),
    /// An I/O error (reading a local playlist file, ...).
    Io(String),
}

impl fmt::Display for TvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TvError::Http(m) => write!(f, "http error: {}", m),
            TvError::HttpStatus(s) => write!(f, "http status error: {}", s),
            TvError::Parse(m) => write!(f, "parse error: {}", m),
            TvError::NotFound(m) => write!(f, "not found: {}", m),
            TvError::InvalidInput(m) => write!(f, "invalid input: {}", m),
            TvError::Io(m) => write!(f, "io error: {}", m),
        }
    }
}

impl std::error::Error for TvError {}

impl From<ureq::Error> for TvError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(code, _) => TvError::HttpStatus(code),
            ureq::Error::Transport(t) => TvError::Http(t.to_string()),
        }
    }
}

impl From<std::io::Error> for TvError {
    fn from(e: std::io::Error) -> Self {
        TvError::Io(e.to_string())
    }
}

/// Convenience alias used across the crate.
pub type TvResult<T> = Result<T, TvError>;
