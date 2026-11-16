use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum State {
    Icebox,
    Ready,
    InProgress,
    Done,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Icebox => "icebox",
            State::Ready => "ready",
            State::InProgress => "in_progress",
            State::Done => "done",
        }
    }

    /// Numeric rank following the natural flow: icebox(0) → ready(1) → in_progress(2) → done(3).
    pub fn rank(&self) -> u8 {
        match self {
            State::Icebox => 0,
            State::Ready => 1,
            State::InProgress => 2,
            State::Done => 3,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid state: '{0}'")]
pub struct InvalidStateError(String);

impl std::str::FromStr for State {
    type Err = InvalidStateError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "icebox" => Ok(State::Icebox),
            "ready" => Ok(State::Ready),
            "in_progress" => Ok(State::InProgress),
            "done" => Ok(State::Done),
            _ => Err(InvalidStateError(s.to_string())),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl sqlx::Type<sqlx::Sqlite> for State {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for State {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(s.parse::<State>()?)
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for State {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&self.as_str(), buf)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Backlog {
    pub id: i64,
    pub name: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub id: i64,
    pub key: String,
    pub backlog_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub state: State,
    pub position: String,
    pub archived: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub done_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    /// The part of `name` before its first colon, or empty for a bare tag.
    pub key: String,
    /// The part of `name` after its first colon, or all of it for a bare tag.
    pub value: String,
}

/// A tag name split into its key and value on the first colon. A bare tag
/// such as `bug` has an empty key, so `bug` and `:bug` are the same tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagParts<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// Why a name can't be a tag. Each rule keeps every tag writable as a filter
/// term, which splits on whitespace and gives `-` and `state:` meaning.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum InvalidTagError {
    #[error(
        "tag '{0}' has no value; add one after the colon, or drop the colon to make a bare tag"
    )]
    EmptyValue(String),
    #[error("tag '{0}' can't contain whitespace, which separates filter terms")]
    Whitespace(String),
    #[error("tag '{0}' can't have a key starting with '-', which reads as a negation in filters")]
    DashKey(String),
    #[error("tag '{0}' can't use the key 'state', which filters reserve for task state")]
    StateKey(String),
}

impl<'a> TagParts<'a> {
    /// Split a name on its first colon without validating it. Use this only
    /// to look up a tag that may predate the rules in `try_from`.
    pub fn split(name: &'a str) -> Self {
        let (key, value) = name.split_once(':').unwrap_or(("", name));
        TagParts { key, value }
    }
}

impl<'a> TryFrom<&'a str> for TagParts<'a> {
    type Error = InvalidTagError;

    /// Rejects a name with nothing after its colon (`x:`), with whitespace,
    /// with a key starting with `-` (`-a:b`), or with the key `state`.
    fn try_from(name: &'a str) -> Result<Self, Self::Error> {
        let parts = TagParts::split(name);
        let err = if parts.value.is_empty() {
            InvalidTagError::EmptyValue
        } else if name.chars().any(char::is_whitespace) {
            InvalidTagError::Whitespace
        } else if parts.key.starts_with('-') {
            InvalidTagError::DashKey
        } else if parts.key == "state" {
            InvalidTagError::StateKey
        } else {
            return Ok(parts);
        };
        Err(err(name.to_string()))
    }
}

/// The canonical name: `value` for a bare tag, `key:value` otherwise. A bare
/// tag whose value has a colon keeps its leading colon (`:a:b`), since `a:b`
/// would parse back as key `a`.
impl std::fmt::Display for TagParts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.key.is_empty() && !self.value.contains(':') {
            f.write_str(self.value)
        } else {
            write!(f, "{}:{}", self.key, self.value)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub body: String,
    pub created_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hegel::TestCase;
    use hegel::generators as gs;

    const STATES: [State; 4] = [State::Icebox, State::Ready, State::InProgress, State::Done];

    #[hegel::test]
    fn prop_tag_parts_split_and_rejoin(tc: TestCase) {
        // Mix arbitrary text with the fragments the rules care about.
        let fragments = ["a", "é", "-", ":", " ", "\t", "state"];
        let name = if tc.draw(gs::booleans()) {
            tc.draw(gs::text().max_size(20))
        } else {
            tc.draw(gs::vecs(gs::sampled_from(fragments.to_vec())).max_size(6))
                .concat()
        };
        match TagParts::try_from(name.as_str()) {
            Ok(parts) => {
                assert!(!parts.value.is_empty());
                assert!(!parts.key.contains(':'));
                // Display is canonical: it reparses to the same parts, and
                // differs from the input only by a dropped leading colon.
                let canonical = parts.to_string();
                assert_eq!(TagParts::try_from(canonical.as_str()).unwrap(), parts);
                assert!(canonical == name || format!(":{canonical}") == name);
            }
            Err(_) => {
                // Rejected only for a reason the rules name.
                let parts = TagParts::split(&name);
                let reasons = [
                    parts.value.is_empty(),
                    name.chars().any(char::is_whitespace),
                    parts.key.starts_with('-'),
                    parts.key == "state",
                ];
                assert!(reasons.contains(&true), "{name:?} rejected for no reason");
            }
        }
    }

    /// Every valid tag can be written as a positive and a negated filter
    /// term that both match it.
    #[hegel::test]
    fn prop_valid_tags_are_writable_as_terms(tc: TestCase) {
        use crate::filter::{Matcher, Term};

        let name = tc.draw(gs::text().alphabet("ab-:é_ s").max_size(8));
        let Ok(parts) = TagParts::try_from(name.as_str()) else {
            return;
        };
        let matcher = Matcher::Tag {
            key: parts.key.to_string(),
            value: parts.value.to_string(),
        };
        for negated in [false, true] {
            let term = Term {
                negated,
                matcher: matcher.clone(),
            };
            let written = term.to_string();
            assert_eq!(
                written.parse::<Term>().unwrap(),
                term,
                "{name:?} as {written:?}"
            );
        }
    }

    #[hegel::test]
    fn prop_tag_parts_roundtrip_from_parts(tc: TestCase) {
        let key = tc.draw(gs::text().max_size(10).exclude_characters(":"));
        let value = tc.draw(gs::text().min_size(1).max_size(10));
        tc.assume(!format!("{key}{value}").chars().any(char::is_whitespace));
        tc.assume(!key.starts_with('-') && key != "state");
        let parts = TagParts {
            key: &key,
            value: &value,
        };
        let name = parts.to_string();
        assert_eq!(TagParts::try_from(name.as_str()).unwrap(), parts);
    }

    #[hegel::test]
    fn prop_state_roundtrips(tc: TestCase) {
        let state = tc.draw(gs::sampled_from(STATES.to_vec()));
        assert_eq!(state.to_string().parse::<State>().unwrap(), state);
    }

    #[hegel::test]
    fn prop_state_parse_accepts_only_known_names(tc: TestCase) {
        let s = tc.draw(gs::text().max_size(12));
        let known = STATES.iter().any(|state| state.as_str() == s);
        assert_eq!(s.parse::<State>().is_ok(), known, "{s:?}");
    }

    #[hegel::test]
    fn prop_state_rank_is_injective(tc: TestCase) {
        let a = tc.draw(gs::sampled_from(STATES.to_vec()));
        let b = tc.draw(gs::sampled_from(STATES.to_vec()));
        assert_eq!(a.rank() == b.rank(), a == b);
    }

    fn parts<'a>(key: &'a str, value: &'a str) -> TagParts<'a> {
        TagParts { key, value }
    }

    #[test]
    fn tag_parts_reject_unwritable_tags() {
        for (name, err) in [
            ("a b", InvalidTagError::Whitespace("a b".into())),
            ("x:\ty", InvalidTagError::Whitespace("x:\ty".into())),
            ("-a:b", InvalidTagError::DashKey("-a:b".into())),
            ("state:x", InvalidTagError::StateKey("state:x".into())),
        ] {
            assert_eq!(TagParts::try_from(name).unwrap_err(), err, "{name:?}");
        }
        assert_eq!(
            TagParts::try_from("-state:x").unwrap_err().to_string(),
            "tag '-state:x' can't have a key starting with '-', which reads as a negation in filters"
        );
        // A bare tag may start with `-`; it's written as `:-x` in filters.
        assert_eq!(TagParts::try_from("-x").unwrap(), parts("", "-x"));
        // `state` is fine as a bare tag or a value.
        assert!(TagParts::try_from("state").is_ok());
        assert!(TagParts::try_from("kind:state").is_ok());
    }

    #[test]
    fn tag_parts_bare_tag_has_empty_key() {
        assert_eq!(TagParts::try_from("bug").unwrap(), parts("", "bug"));
        assert_eq!(TagParts::try_from(":bug").unwrap(), parts("", "bug"));
    }

    #[test]
    fn tag_parts_split_on_first_colon() {
        assert_eq!(
            TagParts::try_from("project:ranger").unwrap(),
            parts("project", "ranger")
        );
        assert_eq!(
            TagParts::try_from("url:http://x").unwrap(),
            parts("url", "http://x")
        );
    }

    #[test]
    fn tag_parts_reject_empty_value() {
        for name in ["x:", ":", ""] {
            assert!(TagParts::try_from(name).is_err(), "{name:?} should fail");
        }
        assert_eq!(
            TagParts::try_from("x:").unwrap_err().to_string(),
            "tag 'x:' has no value; add one after the colon, or drop the colon to make a bare tag"
        );
    }

    #[test]
    fn tag_parts_display_canonical_name() {
        assert_eq!(parts("", "bug").to_string(), "bug");
        assert_eq!(parts("project", "ranger").to_string(), "project:ranger");
        assert_eq!(parts("", "a:b").to_string(), ":a:b");
    }

    #[test]
    fn state_roundtrips_through_display_and_parse() {
        for (state, expected) in [
            (State::Icebox, "icebox"),
            (State::Ready, "ready"),
            (State::InProgress, "in_progress"),
            (State::Done, "done"),
        ] {
            assert_eq!(state.as_str(), expected);
            assert_eq!(state.to_string(), expected);
            let parsed: State = expected.parse().unwrap();
            assert_eq!(parsed.as_str(), expected);
        }
    }

    #[test]
    fn state_parse_invalid_returns_error() {
        let err = "bogus".parse::<State>().unwrap_err();
        assert_eq!(err.to_string(), "invalid state: 'bogus'");
    }

    #[tokio::test]
    async fn state_sqlx_encode_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();

        let state = State::Done;
        let row: (String,) = sqlx::query_as("SELECT ?")
            .bind(&state)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.0, "done");
    }
}
