#[derive(Debug, thiserror::Error)]
pub enum RangerError {
    #[error("no key matching prefix '{0}'")]
    KeyNotFound(String),
    #[error("ambiguous prefix '{0}' matches multiple keys")]
    AmbiguousPrefix(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
