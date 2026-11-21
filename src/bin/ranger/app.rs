use ranger::db::SqlitePool;
use std::str::FromStr;

use ranger::filter::Filter;

/// What every command runs against: the database and the settings `main`
/// resolves once from flags and the environment. Commands read these from
/// here instead of taking them as separate arguments or reading env vars.
pub struct Ranger {
    pub pool: SqlitePool,
    /// The default filter narrowed by `--filter`. Commands work within it,
    /// and task keys resolve among the tasks it matches first.
    pub filter: Filter,
    /// Print JSON instead of human-readable output.
    pub json: bool,
    /// `$EDITOR`, for `task edit` with no field flags.
    pub editor: Option<String>,
}

/// The default filter, falling back to the deprecated `RANGER_DEFAULT_BACKLOG`
/// as that backlog's `project:` term. Also returns a warning to print
/// whenever the old variable is set.
pub fn default_filter(
    filter: Option<Filter>,
    legacy_backlog: Option<&str>,
) -> (Filter, Option<String>) {
    let Some(name) = legacy_backlog else {
        return (filter.unwrap_or_default(), None);
    };
    if let Some(filter) = filter {
        let warning = "RANGER_DEFAULT_BACKLOG is deprecated and ignored, since \
                       RANGER_DEFAULT_FILTER is set; unset it";
        return (filter, Some(warning.to_string()));
    }
    match ranger::filter::Term::from_str(&format!("project:{name}")) {
        Ok(term) => {
            let filter = Filter(vec![term]);
            let warning = format!(
                "RANGER_DEFAULT_BACKLOG is deprecated; set RANGER_DEFAULT_FILTER={filter} instead"
            );
            (filter, Some(warning))
        }
        Err(_) => {
            let warning = format!(
                "RANGER_DEFAULT_BACKLOG is deprecated, and {name:?} isn't a valid backlog \
                 name, so it's ignored; set RANGER_DEFAULT_FILTER instead"
            );
            (Filter::default(), Some(warning))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(s: &str) -> Filter {
        s.parse().unwrap()
    }

    #[test]
    fn default_filter_without_the_legacy_variable() {
        assert_eq!(default_filter(None, None), (Filter::default(), None));
        let f = filter("project:ranger -bug");
        assert_eq!(default_filter(Some(f.clone()), None), (f, None));
    }

    #[test]
    fn legacy_backlog_becomes_a_project_term() {
        let (f, warning) = default_filter(None, Some("ranger"));
        assert_eq!(f, filter("project:ranger"));
        assert_eq!(
            warning.unwrap(),
            "RANGER_DEFAULT_BACKLOG is deprecated; set RANGER_DEFAULT_FILTER=project:ranger instead"
        );
    }

    #[test]
    fn default_filter_wins_over_legacy_backlog() {
        let (f, warning) = default_filter(Some(filter("bug")), Some("ranger"));
        assert_eq!(f, filter("bug"));
        assert!(warning.unwrap().contains("ignored"));
    }

    #[test]
    fn invalid_legacy_backlog_is_ignored() {
        let (f, warning) = default_filter(None, Some("two words"));
        assert_eq!(f, Filter::default());
        assert!(
            warning
                .unwrap()
                .contains("\"two words\" isn't a valid backlog name")
        );
    }
}
