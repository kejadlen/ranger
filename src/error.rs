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
    #[error("can't mark parent task done: {count} subtask(s) still incomplete")]
    IncompleteSubtasks { count: usize },
    #[error("can't move parent task before its subtask")]
    ParentBeforeChild,
    #[error("can't move subtask after its parent")]
    ChildAfterParent,
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
