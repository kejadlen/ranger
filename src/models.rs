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
    /// The part of `name` before its first colon, or `None` for a bare tag.
    pub key: Option<String>,
    /// The part of `name` after its first colon, or `None` for a bare tag.
    pub value: Option<String>,
}

/// A tag name split into its key and value on the first colon. Bare tags
/// such as `bug` have neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagParts<'a> {
    pub key: Option<&'a str>,
    pub value: Option<&'a str>,
}

#[derive(Debug, thiserror::Error)]
#[error("tag '{0}' has no key before its colon; drop the colon or add a key, as in 'project{0}'")]
pub struct EmptyTagKeyError(String);

impl<'a> TryFrom<&'a str> for TagParts<'a> {
    type Error = EmptyTagKeyError;

    /// Rejects a name with nothing before its colon, such as `:x`.
    fn try_from(name: &'a str) -> Result<Self, Self::Error> {
        match name.split_once(':') {
            Some(("", _)) => Err(EmptyTagKeyError(name.to_string())),
            Some((key, value)) => Ok(TagParts {
                key: Some(key),
                value: Some(value),
            }),
            None => Ok(TagParts {
                key: None,
                value: None,
            }),
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

    fn parts<'a>(key: Option<&'a str>, value: Option<&'a str>) -> TagParts<'a> {
        TagParts { key, value }
    }

    #[test]
    fn tag_parts_bare_tag() {
        assert_eq!(TagParts::try_from("bug").unwrap(), parts(None, None));
    }

    #[test]
    fn tag_parts_split_on_first_colon() {
        assert_eq!(
            TagParts::try_from("project:ranger").unwrap(),
            parts(Some("project"), Some("ranger"))
        );
        assert_eq!(
            TagParts::try_from("url:http://x").unwrap(),
            parts(Some("url"), Some("http://x"))
        );
        assert_eq!(
            TagParts::try_from("x:").unwrap(),
            parts(Some("x"), Some(""))
        );
    }

    #[test]
    fn tag_parts_reject_empty_key() {
        let err = TagParts::try_from(":x").unwrap_err();
        assert_eq!(
            err.to_string(),
            "tag ':x' has no key before its colon; drop the colon or add a key, as in 'project:x'"
        );
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
