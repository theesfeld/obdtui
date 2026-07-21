//! Error types for `obd-io`.

use std::io;
use std::path::PathBuf;

/// Library error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("no serial OBD adapter found")]
    NoAdapter,

    #[error("adapter init failed: {0}")]
    AdapterInit(String),

    #[error("timeout while wait for adapter response")]
    Timeout,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("capture error: {0}")]
    Capture(String),

    #[error("profile error at {path}: {message}")]
    Profile { path: PathBuf, message: String },

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("TOML parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("operation is write-gated: {0}")]
    WriteGated(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
