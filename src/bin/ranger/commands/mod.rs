pub mod backlog;
pub mod comment;
pub mod serve;
pub mod tag;
pub mod task;

use ranger::filter::Filter;
use ranger::ops::backlog::project_term;

/// The tasks a key resolves against first: those in the default backlog,
/// narrowed by `filter`. Keys outside it still resolve when nothing inside
/// matches; see `ops::task::get_by_key_prefix`.
pub fn key_scope(backlog: Option<&str>, filter: &Filter) -> Filter {
    let backlog_term = backlog.and_then(|name| project_term(name).ok());
    Filter(
        backlog_term
            .into_iter()
            .chain(filter.0.iter().cloned())
            .collect(),
    )
}
