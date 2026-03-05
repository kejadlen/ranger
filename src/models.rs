use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Icebox,
    Queued,
    InProgress,
    Done,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Icebox => "icebox",
            State::Queued => "queued",
            State::InProgress => "in_progress",
            State::Done => "done",
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
            "queued" => Ok(State::Queued),
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
        s.parse::<State>()
            .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)).into())
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
    pub parent_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub state: State,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub body: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Blocker {
    pub id: i64,
    pub task_id: i64,
    pub blocked_by_task_id: i64,
}
