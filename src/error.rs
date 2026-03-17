#[derive(Debug, thiserror::Error)]
pub enum RangerError {
    #[error("no key matching prefix '{0}'")]
    KeyNotFound(String),
    #[error("ambiguous prefix '{0}' matches multiple keys")]
    AmbiguousPrefix(String),
    #[error("can't move {task_state} task relative to {anchor_state} task")]
    StateMismatch {
        task_state: String,
        anchor_state: String,
    },
    #[error("backlog not found: '{0}'")]
    BacklogNotFound(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
