use axum::extract::State;
use axum::response::Html;
use axum::{Router, routing::get};
use ranger::key;
use ranger::models::Task;
use ranger::ops;
use ranger::ops::task::ListFilter;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;

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

    let app = Router::new().route("/", get(index)).with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("Listening on http://{addr}");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index(State(state): State<AppState>) -> Html<String> {
    match render_board(&state).await {
        Ok(html) => Html(html),
        Err(e) => Html(format!(
            "<html><body><h1>Error</h1><pre>{e}</pre></body></html>"
        )),
    }
}

struct TaskView {
    short_key: String,
    title: String,
    description: Option<String>,
    has_subtasks: bool,
    subtask_count: usize,
    done_subtask_count: usize,
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

    let backlog_panel = render_backlog_panel(&in_progress, &queued);
    let icebox_panel = render_column_panel("icebox", &icebox);
    let done_panel = render_column_panel("done", &done);

    Ok(format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ranger › {backlog_name}</title>
<style>
  :root {{
    --bg: #0e0e10;
    --surface: #1a1a1e;
    --border: #2a2a2e;
    --text: #e0e0e0;
    --text-dim: #888;
    --accent: #7c6fe0;
    --green: #4caf50;
    --yellow: #e0b44c;
    --blue: #5b9fe0;
  }}
  *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
    background: var(--bg);
    color: var(--text);
    line-height: 1.5;
  }}
  header {{
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 24px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }}
  header h1 {{
    font-size: 16px;
    font-weight: 600;
  }}
  header h1 span.sep {{
    color: var(--text-dim);
    margin: 0 6px;
  }}
  header .counts {{
    font-size: 13px;
    color: var(--text-dim);
  }}
  .board {{
    display: grid;
    grid-template-columns: 1fr 280px 300px;
    height: calc(100vh - 49px);
  }}
  .panel {{
    border-right: 1px solid var(--border);
    overflow-y: auto;
    padding: 16px;
  }}
  .panel:last-child {{
    border-right: none;
  }}
  .panel-header {{
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }}
  .panel-header h2 {{
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
  }}
  .panel-header .count {{
    font-size: 11px;
    color: var(--text-dim);
    background: var(--border);
    padding: 1px 6px;
    border-radius: 8px;
  }}
  .section-label {{
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--accent);
    margin: 16px 0 8px 0;
  }}
  .section-label:first-child {{
    margin-top: 0;
  }}
  .task {{
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    margin-bottom: 6px;
  }}
  .task .task-header {{
    display: flex;
    align-items: baseline;
    gap: 8px;
  }}
  .task .key {{
    font-family: "SF Mono", "Fira Code", monospace;
    font-size: 11px;
    color: var(--accent);
    flex-shrink: 0;
  }}
  .task .title {{
    font-size: 14px;
    font-weight: 500;
  }}
  .task .desc {{
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  .task .subtask-indicator {{
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 4px;
  }}
  .state-in_progress .task {{ border-left: 3px solid var(--yellow); }}
  .state-queued .task {{ border-left: 3px solid var(--blue); }}
  .state-done .task {{
    border-left: 3px solid var(--green);
    opacity: 0.7;
  }}
  .empty {{
    font-size: 13px;
    color: var(--text-dim);
    font-style: italic;
    padding: 20px 0;
    text-align: center;
  }}
</style>
</head>
<body>
<header>
  <h1>ranger<span class="sep">›</span>{backlog_name}</h1>
  <div class="counts">{active} active · {total} total</div>
</header>
<div class="board">
  {backlog_panel}
  {icebox_panel}
  {done_panel}
</div>
</body>
</html>"##,
        backlog_name = html_escape(&state.backlog_name),
    ))
}

fn render_backlog_panel(in_progress: &[TaskView], queued: &[TaskView]) -> String {
    let count = in_progress.len() + queued.len();
    let mut html = String::new();
    html.push_str(&format!(
        r#"<div class="panel">
  <div class="panel-header">
    <h2>Backlog</h2>
    <span class="count">{count}</span>
  </div>"#
    ));

    if in_progress.is_empty() && queued.is_empty() {
        html.push_str(r#"<div class="empty">No active tasks</div>"#);
    } else {
        if !in_progress.is_empty() {
            html.push_str(r#"<div class="section-label">In Progress</div>"#);
            html.push_str(r#"<div class="state-in_progress">"#);
            for task in in_progress {
                html.push_str(&render_task(task));
            }
            html.push_str("</div>");
        }

        if !queued.is_empty() {
            html.push_str(r#"<div class="section-label">Queued</div>"#);
            html.push_str(r#"<div class="state-queued">"#);
            for task in queued {
                html.push_str(&render_task(task));
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

fn render_column_panel(name: &str, tasks: &[TaskView]) -> String {
    let label = match name {
        "icebox" => "Icebox",
        "done" => "Done",
        _ => name,
    };
    let state_class = match name {
        "done" => "state-done",
        _ => "",
    };
    let count = tasks.len();
    let mut html = String::new();
    html.push_str(&format!(
        r#"<div class="panel">
  <div class="panel-header">
    <h2>{label}</h2>
    <span class="count">{count}</span>
  </div>"#
    ));

    if tasks.is_empty() {
        html.push_str(&format!(
            r#"<div class="empty">No {lower} tasks</div>"#,
            lower = label.to_lowercase()
        ));
    } else {
        if !state_class.is_empty() {
            html.push_str(&format!(r#"<div class="{state_class}">"#));
        }
        for task in tasks {
            html.push_str(&render_task(task));
        }
        if !state_class.is_empty() {
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

fn render_task(task: &TaskView) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="task">"#);
    html.push_str(r#"<div class="task-header">"#);
    html.push_str(&format!(
        r#"<span class="key">{}</span>"#,
        html_escape(&task.short_key)
    ));
    html.push_str(&format!(
        r#"<span class="title">{}</span>"#,
        html_escape(&task.title)
    ));
    html.push_str("</div>");
    if let Some(desc) = &task.description {
        html.push_str(&format!(
            r#"<div class="desc">{}</div>"#,
            html_escape(desc)
        ));
    }
    if task.has_subtasks {
        html.push_str(&format!(
            r#"<div class="subtask-indicator">◆ {}/{} subtasks</div>"#,
            task.done_subtask_count, task.subtask_count
        ));
    }
    html.push_str("</div>");
    html
}

async fn to_task_views(
    tasks: &[Task],
    prefixes: &std::collections::HashMap<String, usize>,
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> color_eyre::Result<Vec<TaskView>> {
    let mut views = Vec::with_capacity(tasks.len());
    for task in tasks {
        let prefix_len = prefixes.get(&task.key).copied().unwrap_or(8);
        let short_key = task.key[..8.min(task.key.len())].to_string();
        let short_key = format!(
            "{}{}",
            &short_key[..prefix_len.min(short_key.len())],
            if prefix_len < short_key.len() {
                &short_key[prefix_len..]
            } else {
                ""
            }
        );

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
            short_key,
            title: task.title.clone(),
            description: task.description.clone(),
            has_subtasks,
            subtask_count,
            done_subtask_count,
        });
    }
    Ok(views)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
