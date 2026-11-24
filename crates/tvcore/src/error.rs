use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected http status {0}")]
    Status(u16),
    #[error("channel not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("built-in networking is not compiled in (feature `http`)")]
    NoNetwork,
}

impl Error {
    /// Stable machine readable code, used by the FFI layer.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Parse(_) => "parse",
            Error::Network(_) => "network",
            Error::Status(_) => "status",
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) => "invalid_input",
            Error::NoNetwork => "no_network",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
