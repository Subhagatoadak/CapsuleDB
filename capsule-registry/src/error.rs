use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("capsule error: {0}")]
    Capsule(#[from] capsule_core::CapsuleError),
    #[error("capsule failed validation:\n{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    InvalidTransition(#[from] capsule_core::InvalidTransition),
    #[error("not found: {0}")]
    NotFound(String),
}
