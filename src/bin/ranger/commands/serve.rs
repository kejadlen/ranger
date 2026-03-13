use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use maud::{DOCTYPE, Markup, html};
use ranger::key;
use ranger::models::Task;
use ranger::ops;
use ranger::ops::task::ListFilter;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Static CSS embedded at compile time from `static/style.css`.
const STYLE_CSS: &str = include_str!("../../../../static/style.css");

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    backlog_name: String,
}

pub async fn run(pool: &SqlitePool, port: u16, backlog_name: String) -> color_eyre::Result<()> {
    let state = AppState {
        pool: pool.clone(),
        backlog_name,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/static/style.css", get(serve_css))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("Listening on http://{addr}");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE_CSS)
}

async fn index(State(state): State<AppState>) -> Markup {
    match render_board(&state).await {
        Ok(markup) => markup,
        Err(e) => html! {
            html {
                body {
                    h1 { "Error" }
                    pre { (e) }
                }
            }
        },
    }
}

struct TaskView {
    key_prefix: String,
    key_rest: String,
    title: String,
    description: Option<String>,
    has_subtasks: bool,
    subtask_count: usize,
    done_subtask_count: usize,
}

async fn render_board(state: &AppState) -> color_eyre::Result<Markup> {
    let mut conn = state.pool.acquire().await?;
    let backlog = ops::backlog::get_by_name(&mut conn, &state.backlog_name).await?;
    let all_keys = ops::task::keys_for_backlog(&mut conn, backlog.id).await?;
    let prefixes = key::unique_prefix_lengths(&all_keys);

    let mut in_progress = Vec::new();
    let mut queued = Vec::new();
    let mut icebox = Vec::new();
    let mut done = Vec::new();

    for s in [
        ranger::models::State::InProgress,
        ranger::models::State::Queued,
        ranger::models::State::Icebox,
        ranger::models::State::Done,
    ] {
        let filter = ListFilter {
            state: Some(s.clone()),
            ..Default::default()
        };
        let tasks = ops::task::list(&mut conn, backlog.id, &filter).await?;
        let views = to_task_views(&tasks, &prefixes, &mut conn).await?;
        match s {
            ranger::models::State::InProgress => in_progress = views,
            ranger::models::State::Queued => queued = views,
            ranger::models::State::Icebox => icebox = views,
            ranger::models::State::Done => done = views,
        }
    }

    let total = in_progress.len() + queued.len() + icebox.len() + done.len();
    let active = in_progress.len() + queued.len();
    let backlog_name = &state.backlog_name;

    Ok(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "ranger › " (backlog_name) }
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                header {
                    h1 { "ranger" span.sep { "›" } (backlog_name) }
                    div.counts { (active) " active · " (total) " total" }
                }
                div.board {
                    (render_backlog_panel(&in_progress, &queued))
                    (render_column_panel("Icebox", "state-icebox", &icebox))
                    (render_column_panel("Done", "state-done", &done))
                }
            }
        }
    })
}

fn render_backlog_panel(in_progress: &[TaskView], queued: &[TaskView]) -> Markup {
    let count = in_progress.len() + queued.len();
    html! {
        div.panel {
            div.panel-header {
                h2 { "Backlog" }
                span.count { (count) }
            }
            @if in_progress.is_empty() && queued.is_empty() {
                div.empty { "No active tasks" }
            } @else {
                @if !in_progress.is_empty() {
                    div.section-label.section-label-in-progress {
                        span.dot { "●" } " In Progress"
                    }
                    div.state-in-progress {
                        @for task in in_progress {
                            (render_task(task))
                        }
                    }
                }
                @if !queued.is_empty() {
                    div.section-label.section-label-queued {
                        span.dot { "●" } " Queued"
                    }
                    div.state-queued {
                        @for task in queued {
                            (render_task(task))
                        }
                    }
                }
            }
        }
    }
}

fn render_column_panel(label: &str, state_class: &str, tasks: &[TaskView]) -> Markup {
    let count = tasks.len();
    html! {
        div.panel {
            div.panel-header {
                h2 { (label) }
                span.count { (count) }
            }
            @if tasks.is_empty() {
                div.empty { "No " (label.to_lowercase()) " tasks" }
            } @else {
                div class=(state_class) {
                    @for task in tasks {
                        (render_task(task))
                    }
                }
            }
        }
    }
}

fn render_task(task: &TaskView) -> Markup {
    html! {
        div.task {
            div.task-header {
                span.key {
                    span.key-prefix { (task.key_prefix) }
                    span.key-rest { (task.key_rest) }
                }
                span.title { (task.title) }
            }
            @if let Some(desc) = &task.description {
                div.desc { (desc) }
            }
            @if task.has_subtasks {
                div.subtask-indicator {
                    "◆ " (task.done_subtask_count) "/" (task.subtask_count) " subtasks"
                }
            }
        }
    }
}

async fn to_task_views(
    tasks: &[Task],
    prefixes: &std::collections::HashMap<String, usize>,
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> color_eyre::Result<Vec<TaskView>> {
    let mut views = Vec::with_capacity(tasks.len());
    for task in tasks {
        let prefix_len = prefixes.get(&task.key).copied().unwrap_or(8);
        let display_len = 8.min(task.key.len());
        let key_prefix = task.key[..prefix_len.min(display_len)].to_string();
        let key_rest = task.key[prefix_len.min(display_len)..display_len].to_string();

        // Check for subtasks
        let subtasks: Vec<Task> = sqlx::query_as(
            "SELECT id, key, backlog_id, parent_id, title, description, state, position, archived, created_at, updated_at \
             FROM tasks WHERE parent_id = ? AND archived = 0 ORDER BY position",
        )
        .bind(task.id)
        .fetch_all(&mut **conn)
        .await?;

        let has_subtasks = !subtasks.is_empty();
        let subtask_count = subtasks.len();
        let done_subtask_count = subtasks
            .iter()
            .filter(|t| t.state == ranger::models::State::Done)
            .count();

        views.push(TaskView {
            key_prefix,
            key_rest,
            title: task.title.clone(),
            description: task.description.clone(),
            has_subtasks,
            subtask_count,
            done_subtask_count,
        });
    }
    Ok(views)
}
