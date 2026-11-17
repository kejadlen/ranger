pub mod backlog;
pub mod comment;
pub mod serve;
pub mod tag;
pub mod task;

use ranger::filter::{Filter, Matcher, Term};
use ranger::models::TagParts;

/// The tasks a key resolves against first: those in the default backlog,
/// narrowed by `filter`. Keys outside it still resolve when nothing inside
/// matches; see `ops::task::get_by_key_prefix`.
pub fn key_scope(backlog: Option<&str>, filter: &Filter) -> Filter {
    let backlog_term = backlog.and_then(|name| {
        let tag = ranger::ops::backlog::project_tag(name);
        let parts = TagParts::try_from(tag.as_str()).ok()?;
        Some(Term {
            negated: false,
            matcher: Matcher::Tag {
                key: parts.key.to_string(),
                value: parts.value.to_string(),
            },
        })
    });
    Filter(
        backlog_term
            .into_iter()
            .chain(filter.0.iter().cloned())
            .collect(),
    )
}
