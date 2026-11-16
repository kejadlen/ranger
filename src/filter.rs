//! The filter term language: a space-separated conjunction of terms such as
//! `project:ranger -someday state:ready`.
//!
//! - A bare term (`bug`) or a `key:value` term matches tasks with that tag.
//! - `state:<state>` matches the task's state rather than a tag.
//! - `|` separates alternative values for the same key, as in
//!   `project:ranger|dotfiles`, `bug|chore` or `state:ready|in_progress`.
//! - A leading `-` negates a term, alternatives included.

use std::fmt;
use std::str::FromStr;

use crate::models::{InvalidStateError, InvalidTagError, State, TagParts};

/// What a single term matches, before negation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    /// Tasks carrying a tag with this key and any of these values. The key
    /// is empty for a bare tag. There is always at least one value.
    Tag { key: String, values: Vec<String> },
    /// Tasks in any of these states. There is always at least one.
    State(Vec<State>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub negated: bool,
    pub matcher: Matcher,
}

/// A conjunction of terms. An empty filter matches every task.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter(pub Vec<Term>);

impl Filter {
    /// The first value of each positive tag term. A task created under this
    /// filter gets these tags, so it shows up in the listing it was created
    /// from.
    pub fn tags(&self) -> impl Iterator<Item = TagParts<'_>> {
        self.0.iter().filter_map(|term| match &term.matcher {
            Matcher::Tag { key, values } if !term.negated => Some(TagParts {
                key: key.as_str(),
                value: values[0].as_str(),
            }),
            _ => None,
        })
    }

    /// Whether any term, positive or negated, constrains the state.
    pub fn has_state_term(&self) -> bool {
        self.0
            .iter()
            .any(|term| matches!(term.matcher, Matcher::State(_)))
    }

    /// The first value of the first positive state term, which a task
    /// created under this filter starts in unless a state is given
    /// explicitly.
    pub fn state(&self) -> Option<&State> {
        self.0.iter().find_map(|term| match &term.matcher {
            Matcher::State(states) if !term.negated => states.first(),
            _ => None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    #[error("'-' needs a term after it, as in '-bug'")]
    BareNegation,
    #[error(transparent)]
    InvalidState(#[from] InvalidStateError),
    #[error(transparent)]
    InvalidTag(#[from] InvalidTagError),
}

impl FromStr for Term {
    type Err = FilterError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (negated, body) = match s.strip_prefix('-') {
            Some("") => return Err(FilterError::BareNegation),
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let matcher = match body.strip_prefix("state:") {
            Some(states) => Matcher::State(
                states
                    .split('|')
                    .map(str::parse)
                    .collect::<Result<_, _>>()?,
            ),
            None => {
                let (key, alternatives) = body.split_once(':').unwrap_or(("", body));
                let values = alternatives
                    .split('|')
                    .map(|value| {
                        // Validate each alternative as the tag it names.
                        let name = format!("{key}:{value}");
                        TagParts::try_from(name.as_str())?;
                        Ok(value.to_string())
                    })
                    .collect::<Result<_, InvalidTagError>>()?;
                Matcher::Tag {
                    key: key.to_string(),
                    values,
                }
            }
        };
        Ok(Term { negated, matcher })
    }
}

impl FromStr for Filter {
    type Err = FilterError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map(Filter)
    }
}

/// The canonical spelling, which parses back to the same term.
impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("-")?;
        }
        match &self.matcher {
            Matcher::Tag { key, values } => {
                // A bare tag can usually leave its empty key out, but not if
                // a value has a colon (it would split there), or if a
                // positive term would start with `-` and read as a negation.
                // A negated term already strips exactly one `-`.
                let spell_key = !key.is_empty()
                    || values.iter().any(|v| v.contains(':'))
                    || (!self.negated && values[0].starts_with('-'));
                if spell_key {
                    write!(f, "{key}:")?;
                }
                f.write_str(&values.join("|"))
            }
            Matcher::State(states) => {
                let names: Vec<&str> = states.iter().map(State::as_str).collect();
                write!(f, "state:{}", names.join("|"))
            }
        }
    }
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, term) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{term}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hegel::TestCase;
    use hegel::generators as gs;

    const STATES: [State; 4] = [State::Icebox, State::Ready, State::InProgress, State::Done];

    /// Text with no whitespace, which would split a term in two, biased
    /// toward the characters the term syntax gives meaning to. Keys can't
    /// contain a colon, since a tag splits on its first one.
    fn key() -> gs::TextGenerator {
        gs::text().alphabet("ab-é_").max_size(6)
    }

    fn value() -> gs::TextGenerator {
        gs::text().alphabet("ab-:é_").min_size(1).max_size(6)
    }

    /// Filter-ish input built from fragments of the term syntax.
    fn filter_input(tc: &TestCase) -> String {
        let fragments = [
            "-", ":", "a", "bé", " ", "  ", "|", "state:", "ready", "done", "x:", "-state:",
        ];
        let pieces = tc.draw(gs::vecs(gs::sampled_from(fragments.to_vec())).max_size(12));
        pieces.concat()
    }

    #[hegel::test]
    fn prop_parsed_filter_display_reparses(tc: TestCase) {
        let input = filter_input(&tc);
        if let Ok(filter) = input.parse::<Filter>() {
            let canonical = filter.to_string();
            assert_eq!(
                canonical.parse::<Filter>().unwrap(),
                filter,
                "{canonical:?}"
            );
            assert_eq!(filter.0.len(), input.split_whitespace().count());
        }
    }

    #[hegel::test]
    fn prop_terms_roundtrip_through_display(tc: TestCase) {
        let count = tc.draw(gs::integers::<usize>().max_value(4));
        let mut terms = Vec::new();
        for _ in 0..count {
            let negated = tc.draw(gs::booleans());
            let matcher = if tc.draw(gs::booleans()) {
                Matcher::State(
                    tc.draw(
                        gs::vecs(gs::sampled_from(STATES.to_vec()))
                            .min_size(1)
                            .max_size(3),
                    ),
                )
            } else {
                let key = tc.draw(key());
                // Tags can't have these keys; see `TagParts::try_from`.
                tc.assume(key != "state" && !key.starts_with('-'));
                Matcher::Tag {
                    key,
                    values: tc.draw(gs::vecs(value()).min_size(1).max_size(3)),
                }
            };
            terms.push(Term { negated, matcher });
        }
        let filter = Filter(terms);
        let shown = filter.to_string();
        assert_eq!(shown.parse::<Filter>().unwrap(), filter, "{shown:?}");
    }

    fn tag(key: &str, value: &str) -> Matcher {
        tags(key, &[value])
    }

    fn tags(key: &str, values: &[&str]) -> Matcher {
        Matcher::Tag {
            key: key.to_string(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn states(states: &[State]) -> Matcher {
        Matcher::State(states.to_vec())
    }

    fn term(negated: bool, matcher: Matcher) -> Term {
        Term { negated, matcher }
    }

    #[test]
    fn parses_each_kind_of_term() {
        let filter: Filter = "project:ranger bug -someday state:ready -state:done"
            .parse()
            .unwrap();
        assert_eq!(
            filter.0,
            vec![
                term(false, tag("project", "ranger")),
                term(false, tag("", "bug")),
                term(true, tag("", "someday")),
                term(false, states(&[State::Ready])),
                term(true, states(&[State::Done])),
            ]
        );
    }

    #[test]
    fn parses_alternatives() {
        let filter: Filter = "project:ranger|dotfiles -bug|chore state:ready|in_progress :a:b|c"
            .parse()
            .unwrap();
        assert_eq!(
            filter.0,
            vec![
                term(false, tags("project", &["ranger", "dotfiles"])),
                term(true, tags("", &["bug", "chore"])),
                term(false, states(&[State::Ready, State::InProgress])),
                term(false, tags("", &["a:b", "c"])),
            ]
        );
    }

    #[test]
    fn rejects_empty_alternatives() {
        for input in ["a||b", "x|", "project:|a", "state:ready|", "state:|done"] {
            assert!(input.parse::<Term>().is_err(), "{input:?} should fail");
        }
    }

    #[test]
    fn empty_filter_has_no_terms() {
        assert_eq!("".parse::<Filter>().unwrap(), Filter::default());
        assert_eq!("  ".parse::<Filter>().unwrap(), Filter::default());
    }

    #[test]
    fn colon_prefix_names_the_bare_tag() {
        assert_eq!(":bug".parse::<Term>().unwrap(), term(false, tag("", "bug")));
    }

    #[test]
    fn rejects_bare_negation() {
        let err = "-".parse::<Term>().unwrap_err();
        assert!(matches!(err, FilterError::BareNegation));
        assert_eq!(err.to_string(), "'-' needs a term after it, as in '-bug'");
    }

    #[test]
    fn rejects_unknown_state() {
        let err = "state:someday".parse::<Term>().unwrap_err();
        assert!(matches!(err, FilterError::InvalidState(_)));
    }

    #[test]
    fn rejects_empty_tag_value() {
        let err = "-x:".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterError::InvalidTag(_)));
    }

    #[test]
    fn tags_and_state_for_new_tasks() {
        let filter: Filter =
            "project:ranger|dotfiles -someday bug -state:done state:ready|done state:icebox"
                .parse()
                .unwrap();
        let tags: Vec<String> = filter.tags().map(|t| t.to_string()).collect();
        assert_eq!(tags, ["project:ranger", "bug"]);
        assert_eq!(filter.state(), Some(&State::Ready));

        let negated_only: Filter = "-bug -state:done".parse().unwrap();
        assert_eq!(negated_only.tags().count(), 0);
        assert_eq!(negated_only.state(), None);
        assert!(negated_only.has_state_term());
        assert!(!"bug".parse::<Filter>().unwrap().has_state_term());
    }

    #[test]
    fn display_round_trips() {
        let s = "project:ranger bug -someday state:in_progress -state:done";
        let filter: Filter = s.parse().unwrap();
        assert_eq!(filter.to_string(), s);
        let s = "project:ranger|dotfiles -bug|chore state:ready|done";
        assert_eq!(s.parse::<Filter>().unwrap().to_string(), s);
        // A colon in any alternative of a bare tag keeps the empty key.
        assert_eq!(
            ":bug|a:b".parse::<Term>().unwrap(),
            term(false, tags("", &["bug", "a:b"]))
        );
        assert_eq!(":bug|a:b".parse::<Term>().unwrap().to_string(), ":bug|a:b");
        // Without it, the first colon would make `bug|a` a key.
        assert!(matches!(
            "bug|a:b".parse::<Term>(),
            Err(FilterError::InvalidTag(InvalidTagError::Pipe(_)))
        ));
        assert_eq!(":-x|y".parse::<Term>().unwrap().to_string(), ":-x|y");
        assert_eq!("--x|y".parse::<Term>().unwrap().to_string(), "--x|y");
        assert_eq!(":bug".parse::<Term>().unwrap().to_string(), "bug");
        assert_eq!(":-x".parse::<Term>().unwrap().to_string(), ":-x");
        assert_eq!("-:-x".parse::<Term>().unwrap().to_string(), "--x");
        assert_eq!("::-:".parse::<Term>().unwrap().to_string(), "::-:");
        assert_eq!(":-:".parse::<Term>().unwrap().to_string(), ":-:");
        assert_eq!("--x".parse::<Term>().unwrap().to_string(), "--x");
        // Tags can't have a key starting with `-`, so neither can terms.
        assert!(matches!(
            "--a:b".parse::<Term>(),
            Err(FilterError::InvalidTag(_))
        ));
    }
}
