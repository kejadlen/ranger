use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::timestamp::Timestamp;

/// Controls how tasks are ordered within a backlog/state group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ordering {
    /// Lexicographic position strings (legacy).
    #[default]
    Position,
    /// DAG topological sort using `before` edges, with task ID as tiebreaker.
    Dag,
}

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

    /// Numeric rank following the natural flow: icebox(0) → queued(1) → in_progress(2) → done(3).
    pub fn rank(&self) -> u8 {
        match self {
            State::Icebox => 0,
            State::Queued => 1,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub body: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Blocks,
    Before,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Blocks => "blocks",
            EdgeType::Before => "before",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid edge type: '{0}'")]
pub struct InvalidEdgeTypeError(String);

impl std::str::FromStr for EdgeType {
    type Err = InvalidEdgeTypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "blocks" => Ok(EdgeType::Blocks),
            "before" => Ok(EdgeType::Before),
            _ => Err(InvalidEdgeTypeError(s.to_string())),
        }
    }
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl sqlx::Type<sqlx::Sqlite> for EdgeType {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for EdgeType {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(s.parse::<EdgeType>()?)
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for EdgeType {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&self.as_str(), buf)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskEdge {
    pub id: i64,
    pub from_task_id: i64,
    pub to_task_id: i64,
    pub edge_type: EdgeType,
    pub created_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrips_through_display_and_parse() {
        for (state, expected) in [
            (State::Icebox, "icebox"),
            (State::Queued, "queued"),
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
    fn edge_type_roundtrips_through_display_and_parse() {
        for (edge_type, expected) in [(EdgeType::Blocks, "blocks"), (EdgeType::Before, "before")] {
            assert_eq!(edge_type.as_str(), expected);
            assert_eq!(edge_type.to_string(), expected);
            let parsed: EdgeType = expected.parse().unwrap();
            assert_eq!(parsed.as_str(), expected);
        }
    }

    #[test]
    fn edge_type_parse_invalid_returns_error() {
        let err = "bogus".parse::<EdgeType>().unwrap_err();
        assert_eq!(err.to_string(), "invalid edge type: 'bogus'");
    }

    #[tokio::test]
    async fn edge_type_sqlx_encode_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();

        let edge_type = EdgeType::Blocks;
        let row: (String,) = sqlx::query_as("SELECT ?")
            .bind(&edge_type)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.0, "blocks");
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
