use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use axum::{Router, routing::get};
use maud::{DOCTYPE, Markup, PreEscaped, html};
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
    default_backlog: Option<String>,
}

pub async fn run(
    pool: &SqlitePool,
    port: u16,
    default_backlog: Option<String>,
) -> color_eyre::Result<()> {
    let state = AppState {
        pool: pool.clone(),
        default_backlog,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/b/{name}", get(board))
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

async fn index(State(state): State<AppState>) -> Result<Redirect, Markup> {
    // If a default backlog is set, redirect to it
    if let Some(ref name) = state.default_backlog {
        return Ok(Redirect::to(&format!("/b/{name}")));
    }

    // Otherwise, redirect to the first backlog
    let mut conn = state.pool.acquire().await.map_err(error_page)?;
    let backlogs = ops::backlog::list(&mut conn).await.map_err(error_page)?;

    match backlogs.first() {
        Some(b) => Ok(Redirect::to(&format!("/b/{}", b.name))),
        None => Err(error_page("No backlogs found")),
    }
}

async fn board(State(state): State<AppState>, Path(name): Path<String>) -> Markup {
    match render_board(&state, &name).await {
        Ok(markup) => markup,
        Err(e) => error_page(e),
    }
}

fn error_page(e: impl std::fmt::Display) -> Markup {
    html! {
        (DOCTYPE)
        html {
            body {
                h1 { "Error" }
                pre { (e.to_string()) }
            }
        }
    }
}

struct TaskView {
    key_prefix: String,
    key_rest: String,
    title: String,
    description: Option<String>,
    tags: Vec<String>,
}

async fn render_board(state: &AppState, backlog_name: &str) -> color_eyre::Result<Markup> {
    let mut conn = state.pool.acquire().await?;

    // Fetch all backlogs for the selector
    let backlogs = ops::backlog::list(&mut conn).await?;
    let backlog_names: Vec<String> = backlogs.iter().map(|b| b.name.clone()).collect();

    let backlog = ops::backlog::get_by_name(&mut conn, backlog_name).await?;
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
                    h1 {
                        "ranger" span.sep { "›" }
                        @if backlog_names.len() > 1 {
                            span.backlog-picker {
                                button.backlog-trigger onclick="document.getElementById('backlog-dialog').show()" {
                                    (backlog_name)
                                    span.backlog-caret { "▾" }
                                }
                                dialog #backlog-dialog {
                                    ul.backlog-list {
                                        @for name in &backlog_names {
                                            li {
                                                a.backlog-option class=@if name == backlog_name { "active" }
                                                  href=(format!("/b/{name}")) {
                                                    (name)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } @else {
                            (backlog_name)
                        }
                    }
                    div.counts { (active) " active · " (total) " total" }
                }
                div.board {
                    (render_backlog_panel(&in_progress, &queued))
                    (render_column_panel("Icebox", "state-icebox", &icebox))
                    (render_column_panel("Done", "state-done", &done))
                }
                (keyboard_nav_script())
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
                    div.state-in-progress {
                        @for task in in_progress {
                            (render_task(task))
                        }
                    }
                }
                @if !queued.is_empty() {
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
    let has_details = task.description.is_some();
    html! {
        @if has_details {
            details.task {
                summary.task-header tabindex="0" {
                    span.key {
                        span.key-prefix { (task.key_prefix) }
                        span.key-rest { (task.key_rest) }
                    }
                    span.title { (task.title) }
                    @if !task.tags.is_empty() {
                        span.tags {
                            @for tag in &task.tags {
                                span.tag { (tag) }
                            }
                        }
                    }
                    span.expand-icon { "›" }
                }
                div.task-body {
                    @if let Some(desc) = &task.description {
                        div.desc { (desc) }
                    }
                }
            }
        } @else {
            div.task tabindex="0" {
                div.task-header {
                    span.key {
                        span.key-prefix { (task.key_prefix) }
                        span.key-rest { (task.key_rest) }
                    }
                    span.title { (task.title) }
                    @if !task.tags.is_empty() {
                        span.tags {
                            @for tag in &task.tags {
                                span.tag { (tag) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn keyboard_nav_script() -> Markup {
    html! {
        script {
            (PreEscaped(r#"
            (function() {
                // Close backlog popover on outside click
                document.addEventListener('click', function(e) {
                    var dialog = document.getElementById('backlog-dialog');
                    if (dialog && dialog.open && !dialog.contains(e.target) && !e.target.closest('.backlog-trigger')) {
                        dialog.close();
                    }
                });
                function getFocusables() {
                    return Array.from(document.querySelectorAll(
                        'details.task > summary, div.task'
                    ));
                }
                function focusEl(els, idx) {
                    if (idx >= 0 && idx < els.length) {
                        els[idx].focus();
                        els[idx].scrollIntoView({ block: 'nearest' });
                    }
                }
                document.addEventListener('keydown', function(e) {
                    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
                    var els = getFocusables();
                    var current = els.indexOf(document.activeElement);
                    if (e.key === 'j' || e.key === 'ArrowDown') {
                        e.preventDefault();
                        focusEl(els, current < 0 ? 0 : current + 1);
                    } else if (e.key === 'k' || e.key === 'ArrowUp') {
                        e.preventDefault();
                        focusEl(els, current < 0 ? 0 : current - 1);
                    } else if ((e.key === 'Enter' || e.key === ' ') && document.activeElement.tagName === 'SUMMARY') {
                        e.preventDefault();
                        document.activeElement.click();
                    }
                });
            })();
            "#))
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

        // Fetch tags
        let tags = ops::tag::list_for_task(&mut *conn, task.id)
            .await?
            .into_iter()
            .map(|t| t.name)
            .collect();

        views.push(TaskView {
            key_prefix,
            key_rest,
            title: task.title.clone(),
            description: task.description.clone(),
            tags,
        });
    }
    Ok(views)
}
