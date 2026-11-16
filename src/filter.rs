//! The filter term language: a space-separated conjunction of terms such as
//! `project:ranger -someday state:ready`.
//!
//! - A bare term (`bug`) or a `key:value` term matches tasks with that tag.
//! - `state:<state>` matches the task's state rather than a tag.
//! - A leading `-` negates a term.

use std::fmt;
use std::str::FromStr;

use crate::models::{EmptyTagValueError, InvalidStateError, State, TagParts};

/// What a single term matches, before negation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    /// Tasks carrying the tag with this key and value. The key is empty for
    /// a bare tag.
    Tag { key: String, value: String },
    /// Tasks in this state.
    State(State),
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
    /// The positive tag terms. A task created under this filter gets these
    /// tags, so it shows up in the listing it was created from.
    pub fn tags(&self) -> impl Iterator<Item = TagParts<'_>> {
        self.0.iter().filter_map(|term| match &term.matcher {
            Matcher::Tag { key, value } if !term.negated => Some(TagParts {
                key: key.as_str(),
                value: value.as_str(),
            }),
            _ => None,
        })
    }

    /// The first positive state term, which a task created under this filter
    /// starts in unless a state is given explicitly.
    pub fn state(&self) -> Option<&State> {
        self.0.iter().find_map(|term| match &term.matcher {
            Matcher::State(state) if !term.negated => Some(state),
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
    EmptyTagValue(#[from] EmptyTagValueError),
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
            Some(state) => Matcher::State(state.parse()?),
            None => {
                let parts = TagParts::try_from(body)?;
                Matcher::Tag {
                    key: parts.key.to_string(),
                    value: parts.value.to_string(),
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
            Matcher::Tag { key, value } => {
                let parts = TagParts {
                    key: key.as_str(),
                    value: value.as_str(),
                };
                write!(f, "{parts}")
            }
            Matcher::State(state) => write!(f, "state:{}", state.as_str()),
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

    fn tag(key: &str, value: &str) -> Matcher {
        Matcher::Tag {
            key: key.to_string(),
            value: value.to_string(),
        }
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
                term(false, Matcher::State(State::Ready)),
                term(true, Matcher::State(State::Done)),
            ]
        );
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
        assert!(matches!(err, FilterError::EmptyTagValue(_)));
    }

    #[test]
    fn tags_and_state_for_new_tasks() {
        let filter: Filter = "project:ranger -someday bug -state:done state:ready state:icebox"
            .parse()
            .unwrap();
        let tags: Vec<String> = filter.tags().map(|t| t.to_string()).collect();
        assert_eq!(tags, ["project:ranger", "bug"]);
        assert_eq!(filter.state(), Some(&State::Ready));

        let negated_only: Filter = "-bug -state:done".parse().unwrap();
        assert_eq!(negated_only.tags().count(), 0);
        assert_eq!(negated_only.state(), None);
    }

    #[test]
    fn display_round_trips() {
        let s = "project:ranger bug -someday state:in_progress -state:done";
        let filter: Filter = s.parse().unwrap();
        assert_eq!(filter.to_string(), s);
        assert_eq!(":bug".parse::<Term>().unwrap().to_string(), "bug");
    }
}
