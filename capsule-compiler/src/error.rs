use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error("source is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("unsupported source format: {0}")]
    UnsupportedFormat(String),
}
