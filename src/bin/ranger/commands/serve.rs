use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::{Router, routing::get};
use ranger::key;
use ranger::models::Task;
use ranger::ops;
use ranger::ops::task::ListFilter;
use serde::Serialize;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::sync::Arc;
use tera::{Context, Tera};
use tokio::net::TcpListener;

/// Static CSS embedded at compile time from `static/style.css`.
const STYLE_CSS: &str = include_str!("../../../../static/style.css");

/// Raw template strings embedded at compile time.
const TEMPLATES: &[(&str, &str)] = &[
    ("base.html", include_str!("../../../../templates/base.html")),
    (
        "board.html",
        include_str!("../../../../templates/board.html"),
    ),
    (
        "panels/backlog.html",
        include_str!("../../../../templates/panels/backlog.html"),
    ),
    (
        "panels/column.html",
        include_str!("../../../../templates/panels/column.html"),
    ),
    ("task.html", include_str!("../../../../templates/task.html")),
];

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    backlog_name: String,
    tera: Arc<Tera>,
}

pub async fn run(pool: &SqlitePool, port: u16, backlog_name: String) -> color_eyre::Result<()> {
    let mut tera = Tera::default();
    for &(name, content) in TEMPLATES {
        tera.add_raw_template(name, content)?;
    }

    let state = AppState {
        pool: pool.clone(),
        backlog_name,
        tera: Arc::new(tera),
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

async fn index(State(state): State<AppState>) -> Html<String> {
    match render_board(&state).await {
        Ok(html) => Html(html),
        Err(e) => Html(format!(
            "<html><body><h1>Error</h1><pre>{e}</pre></body></html>"
        )),
    }
}

#[derive(Serialize)]
struct TaskView {
    key_prefix: String,
    key_rest: String,
    title: String,
    description: Option<String>,
    has_subtasks: bool,
    subtask_count: usize,
    done_subtask_count: usize,
}

#[derive(Serialize)]
struct ColumnView {
    label: String,
    state_class: String,
    tasks: Vec<TaskView>,
}

async fn render_board(state: &AppState) -> color_eyre::Result<String> {
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

    let columns = vec![
        ColumnView {
            label: "Icebox".to_string(),
            state_class: "state-icebox".to_string(),
            tasks: icebox,
        },
        ColumnView {
            label: "Done".to_string(),
            state_class: "state-done".to_string(),
            tasks: done,
        },
    ];

    let mut context = Context::new();
    context.insert("backlog_name", &state.backlog_name);
    context.insert("active", &active);
    context.insert("total", &total);
    context.insert("in_progress", &in_progress);
    context.insert("queued", &queued);
    context.insert("columns", &columns);

    Ok(state.tera.render("board.html", &context)?)
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
