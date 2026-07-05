use thiserror::Error;

/// Unified error type for the RemoteDesk project
#[derive(Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Protobuf error: {0}")]
    Protobuf(#[from] protobuf::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Capture error: {0}")]
    Capture(String),

    #[error("Codec error: {0}")]
    Codec(String),

    #[error("Input error: {0}")]
    Input(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("{0}")]
    Other(String),
}

/// Convenience Result type
pub type Result<T> = std::result::Result<T, Error>;
