use thiserror::Error;

/// Errors produced when loading, validating, or canonicalizing a milestone specification.
#[derive(Debug, Error)]
pub enum SpecError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid specification format: {0}")]
    InvalidFormat(String),
}
