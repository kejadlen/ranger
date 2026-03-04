use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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

impl std::str::FromStr for State {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "icebox" => Ok(State::Icebox),
            "queued" => Ok(State::Queued),
            "in_progress" => Ok(State::InProgress),
            "done" => Ok(State::Done),
            _ => Err(format!("invalid state: {s}")),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Backlog {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub id: i64,
    pub key: String,
    pub parent_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub body: String,
    pub created_at: String,
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
