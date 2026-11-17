use ranger::db::SqlitePool;
use ranger::filter::{Filter, Matcher, Term};
use ranger::models::TagParts;

/// What every command runs against: the database and the settings `main`
/// resolves once from flags and the environment. Commands read these from
/// here instead of taking them as separate arguments or reading env vars.
pub struct Ranger {
    pub pool: SqlitePool,
    /// The default backlog. Task keys resolve within it first.
    pub default_backlog: Option<String>,
    /// Print JSON instead of human-readable output.
    pub json: bool,
    /// `$EDITOR`, for `task edit` with no field flags.
    pub editor: Option<String>,
}

impl Ranger {
    /// The tasks a key resolves against first: those in the default backlog,
    /// narrowed by `filter`. Keys outside it still resolve when nothing inside
    /// matches; see `ops::task::get_by_key_prefix`.
    pub fn key_scope(&self, filter: &Filter) -> Filter {
        key_scope(self.default_backlog.as_deref(), filter)
    }
}

fn key_scope(default_backlog: Option<&str>, filter: &Filter) -> Filter {
    let backlog_term = default_backlog.and_then(|name| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use hegel::TestCase;
    use hegel::generators as gs;

    fn text() -> gs::TextGenerator {
        gs::text().alphabet("ab-:é_ ").max_size(6)
    }

    /// The scope is the filter's own terms, led by the default backlog's
    /// `project:` tag when there is one that can be written as a tag.
    #[hegel::test]
    fn prop_key_scope_prepends_backlog_term(tc: TestCase) {
        let count = tc.draw(gs::integers::<usize>().max_value(4));
        let terms = (0..count)
            .map(|_| Term {
                negated: tc.draw(gs::booleans()),
                matcher: Matcher::Tag {
                    key: tc.draw(text()),
                    value: tc.draw(text()),
                },
            })
            .collect();
        let filter = Filter(terms);
        let backlog = tc.draw(gs::optional(text()));

        let scope = key_scope(backlog.as_deref(), &filter);

        let tag = backlog.as_deref().map(ranger::ops::backlog::project_tag);
        match tag.as_deref().map(TagParts::try_from) {
            Some(Ok(parts)) => {
                assert_eq!(scope.0[1..], filter.0[..]);
                let first = &scope.0[0];
                assert!(!first.negated);
                assert_eq!(
                    first.matcher,
                    Matcher::Tag {
                        key: parts.key.to_string(),
                        value: parts.value.to_string(),
                    }
                );
            }
            _ => assert_eq!(scope, filter),
        }
    }
}
