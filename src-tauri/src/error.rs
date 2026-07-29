use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Invalid request: {0}")]
    Invalid(String),
    #[error("Required executable is not available: {0}")]
    MissingExecutable(String),
    #[error("Command failed: {0}")]
    Command(String),
    #[error("Operation timed out: {0}")]
    Timeout(String),
    #[error("File access failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("URL is not allowed: {0}")]
    Url(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
