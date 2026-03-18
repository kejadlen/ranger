use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use axum::{Json, Router, routing::get, routing::post};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use ranger::key;
use ranger::models::Task;
use ranger::ops;
use ranger::ops::task::ListFilter;
use serde::Deserialize;
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
) -> Result<(), ranger::error::RangerError> {
    let state = AppState {
        pool: pool.clone(),
        default_backlog,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/b/{name}", get(board))
        .route("/static/style.css", get(serve_css))
        .route("/api/tasks/{key}/move", post(api_move_task))
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

#[derive(Deserialize)]
struct MoveRequest {
    state: Option<String>,
    before: Option<String>,
    after: Option<String>,
}

async fn api_move_task(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<MoveRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let task = ops::task::get_by_key_prefix(&mut conn, &key, None)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Apply state change if requested
    if let Some(ref state_str) = body.state {
        let new_state: ranger::models::State =
            state_str
                .parse()
                .map_err(|e: ranger::models::InvalidStateError| {
                    (StatusCode::BAD_REQUEST, e.to_string())
                })?;
        let updated = ops::task::edit(&mut conn, task.id, None, None, Some(new_state))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Position within the new state group
        match (&body.before, &body.after) {
            (Some(b), Some(a)) => {
                let before = ops::task::get_by_key_prefix(&mut conn, b, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                let after = ops::task::get_by_key_prefix(&mut conn, a, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(
                    &mut conn,
                    &updated,
                    ops::task::Placement::Between {
                        after: &after,
                        before: &before,
                    },
                )
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (Some(b), None) => {
                let before = ops::task::get_by_key_prefix(&mut conn, b, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(&mut conn, &updated, ops::task::Placement::Before(&before))
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (None, Some(a)) => {
                let after = ops::task::get_by_key_prefix(&mut conn, a, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(&mut conn, &updated, ops::task::Placement::After(&after))
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (None, None) => {
                // No position specified — state change auto-positions
            }
        }
    } else {
        // No state change — just reorder within same state
        match (&body.before, &body.after) {
            (Some(b), Some(a)) => {
                let before = ops::task::get_by_key_prefix(&mut conn, b, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                let after = ops::task::get_by_key_prefix(&mut conn, a, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(
                    &mut conn,
                    &task,
                    ops::task::Placement::Between {
                        after: &after,
                        before: &before,
                    },
                )
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (Some(b), None) => {
                let before = ops::task::get_by_key_prefix(&mut conn, b, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(&mut conn, &task, ops::task::Placement::Before(&before))
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (None, Some(a)) => {
                let after = ops::task::get_by_key_prefix(&mut conn, a, None)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
                ops::task::move_task(&mut conn, &task, ops::task::Placement::After(&after))
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
            (None, None) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "before or after is required when not changing state".into(),
                ));
            }
        }
    }

    Ok(StatusCode::NO_CONTENT)
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
    key: String,
    key_prefix: String,
    key_rest: String,
    title: String,
    description: Option<String>,
    tags: Vec<String>,
}

async fn render_board(
    state: &AppState,
    backlog_name: &str,
) -> Result<Markup, ranger::error::RangerError> {
    let mut conn = state.pool.acquire().await?;

    // Fetch all backlogs for the selector
    let backlogs = ops::backlog::list(&mut conn).await?;
    let backlog_names: Vec<String> = backlogs.iter().map(|b| b.name.clone()).collect();

    let backlog = ops::backlog::get_by_name(&mut conn, backlog_name).await?;
    let all_keys = ops::task::keys_for_backlog(&mut conn, backlog.id).await?;
    let prefixes = key::unique_prefix_lengths(&all_keys);

    let mut in_progress = Vec::new();
    let mut ready = Vec::new();
    let mut icebox = Vec::new();
    let mut done = Vec::new();

    for s in [
        ranger::models::State::InProgress,
        ranger::models::State::Ready,
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
            ranger::models::State::Ready => ready = views,
            ranger::models::State::Icebox => icebox = views,
            ranger::models::State::Done => done = views,
        }
    }

    let total = in_progress.len() + ready.len() + icebox.len() + done.len();
    let active = in_progress.len() + ready.len();

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
                    (render_backlog_panel(&in_progress, &ready))
                    (render_column_panel("Icebox", "state-icebox", Some("icebox"), &icebox))
                    (render_column_panel("Done", "state-done", None, &done))
                }
                (keyboard_nav_script())
            }
        }
    })
}

fn render_backlog_panel(in_progress: &[TaskView], ready: &[TaskView]) -> Markup {
    let count = in_progress.len() + ready.len();
    html! {
        div.panel {
            div.panel-header {
                h2 { "Backlog" }
                span.count { (count) }
            }
            @if !in_progress.is_empty() {
                div.state-in-progress {
                    @for task in in_progress {
                        (render_task(task))
                    }
                }
            }
            div.state-ready.drop-zone data-state="ready" {
                @for task in ready {
                    (render_task(task))
                }
            }
        }
    }
}

fn render_column_panel(
    label: &str,
    state_class: &str,
    drop_state: Option<&str>,
    tasks: &[TaskView],
) -> Markup {
    let count = tasks.len();
    let classes = match drop_state {
        Some(_) => format!("{state_class} drop-zone"),
        None => state_class.to_string(),
    };
    html! {
        div.panel {
            div.panel-header {
                h2 { (label) }
                span.count { (count) }
            }
            div class=(classes) data-state=[drop_state] {
                @if tasks.is_empty() {
                    div.empty { "No " (label.to_lowercase()) " tasks" }
                } @else {
                    @for task in tasks {
                        (render_task(task))
                    }
                }
            }
        }
    }
}

fn truncate_desc(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}…", &first_line[..max])
    }
}

fn render_task(task: &TaskView) -> Markup {
    let has_details = task.description.is_some();
    html! {
        @if has_details {
            details.task draggable="true" data-key=(task.key) {
                summary tabindex="0" {
                    div.task-row {
                        span.key {
                            span.key-prefix { (task.key_prefix) }
                            span.key-rest { (task.key_rest) }
                        }
                        div.task-content {
                            div.task-title-row {
                                span.title { (task.title) }
                                @if !task.tags.is_empty() {
                                    span.tags {
                                        @for tag in &task.tags {
                                            span.tag { (tag) }
                                        }
                                    }
                                }
                            }
                            @if let Some(desc) = &task.description {
                                div.subtitle { (truncate_desc(desc, 80)) }
                            }
                        }
                    }
                }
                @if let Some(desc) = &task.description {
                    div.task-row {
                        span.key-spacer {}
                        div.task-content {
                            div.desc { (desc) }
                        }
                    }
                }
            }
        } @else {
            div.task data-key=(task.key) tabindex="0" {
                div.task-row {
                    span.key {
                        span.key-prefix { (task.key_prefix) }
                        span.key-rest { (task.key_rest) }
                    }
                    div.task-content {
                        div.task-title-row {
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
    }
}

fn keyboard_nav_script() -> Markup {
    html! {
        script {
            (PreEscaped(r#"
            (function() {
                // === Backlog popover ===
                document.addEventListener('click', function(e) {
                    var dialog = document.getElementById('backlog-dialog');
                    if (dialog && dialog.open && !dialog.contains(e.target) && !e.target.closest('.backlog-trigger')) {
                        dialog.close();
                    }
                });

                // === Keyboard navigation ===
                function getFocusables() {
                    return Array.from(document.querySelectorAll(
                        'details.task > summary, div.task[data-key]'
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

                // === Drag and drop (ready + icebox only) ===
                var DRAGGABLE_STATES = ['ready', 'icebox'];
                document.querySelectorAll('.drop-zone').forEach(function(z) {
                    if (DRAGGABLE_STATES.indexOf(z.dataset.state) !== -1) {
                        z.querySelectorAll('[data-key]').forEach(function(t) { t.draggable = true; });
                    }
                });
                var draggedKey = null;
                var draggedEl = null;

                function getTaskEl(el) { return el.closest('[data-key]'); }
                function getDropZone(el) { return el.closest('.drop-zone'); }
                function isDraggableZone(zone) {
                    return zone && DRAGGABLE_STATES.indexOf(zone.dataset.state) !== -1;
                }

                document.addEventListener('dragstart', function(e) {
                    var task = getTaskEl(e.target);
                    if (!task) return;
                    var zone = getDropZone(task);
                    if (!isDraggableZone(zone)) { e.preventDefault(); return; }
                    draggedKey = task.dataset.key;
                    draggedEl = task;
                    task.classList.add('dragging');
                    document.querySelectorAll('.drop-zone').forEach(function(z) {
                        if (isDraggableZone(z)) z.classList.add('drag-active');
                    });
                    e.dataTransfer.effectAllowed = 'move';
                    e.dataTransfer.setData('text/plain', draggedKey);
                });

                document.addEventListener('dragend', function() {
                    if (draggedEl) draggedEl.classList.remove('dragging');
                    document.querySelectorAll('.drop-indicator').forEach(function(el) { el.remove(); });
                    document.querySelectorAll('.drop-zone-active').forEach(function(el) { el.classList.remove('drop-zone-active'); });
                    document.querySelectorAll('.drag-active').forEach(function(el) { el.classList.remove('drag-active'); });
                    draggedKey = null;
                    draggedEl = null;
                });

                document.addEventListener('dragover', function(e) {
                    var zone = getDropZone(e.target);
                    if (!isDraggableZone(zone) || !draggedKey) return;
                    e.preventDefault();
                    e.dataTransfer.dropEffect = 'move';

                    document.querySelectorAll('.drop-indicator').forEach(function(el) { el.remove(); });
                    document.querySelectorAll('.drop-zone-active').forEach(function(el) { el.classList.remove('drop-zone-active'); });

                    var tasks = Array.from(zone.querySelectorAll('[data-key]'));
                    if (tasks.length === 0) { zone.classList.add('drop-zone-active'); return; }

                    var closestTask = null, insertBefore = true, minDist = Infinity;
                    for (var i = 0; i < tasks.length; i++) {
                        var rect = tasks[i].getBoundingClientRect();
                        var midY = rect.top + rect.height / 2;
                        var dist = Math.abs(e.clientY - midY);
                        if (dist < minDist) { minDist = dist; closestTask = tasks[i]; insertBefore = e.clientY < midY; }
                    }
                    if (closestTask) {
                        var indicator = document.createElement('div');
                        indicator.className = 'drop-indicator';
                        closestTask.parentNode.insertBefore(indicator, insertBefore ? closestTask : closestTask.nextSibling);
                    }
                });

                document.addEventListener('drop', function(e) {
                    e.preventDefault();
                    var zone = getDropZone(e.target);
                    if (!isDraggableZone(zone) || !draggedKey) return;

                    var targetState = zone.dataset.state;
                    var tasks = Array.from(zone.querySelectorAll('[data-key]'))
                        .filter(function(t) { return t.dataset.key !== draggedKey; });

                    var beforeKey = null, afterKey = null;
                    if (tasks.length > 0) {
                        var closestTask = null, insertBefore = true, minDist = Infinity;
                        for (var i = 0; i < tasks.length; i++) {
                            var rect = tasks[i].getBoundingClientRect();
                            var midY = rect.top + rect.height / 2;
                            var dist = Math.abs(e.clientY - midY);
                            if (dist < minDist) { minDist = dist; closestTask = tasks[i]; insertBefore = e.clientY < midY; }
                        }
                        if (closestTask) {
                            var idx = tasks.indexOf(closestTask);
                            if (insertBefore) {
                                beforeKey = closestTask.dataset.key;
                                if (idx > 0) afterKey = tasks[idx - 1].dataset.key;
                            } else {
                                afterKey = closestTask.dataset.key;
                                if (idx < tasks.length - 1) beforeKey = tasks[idx + 1].dataset.key;
                            }
                        }
                    }

                    var body = {};
                    var draggedZone = draggedEl ? getDropZone(draggedEl) : null;
                    var currentState = draggedZone ? draggedZone.dataset.state : null;
                    if (targetState !== currentState) body.state = targetState;
                    if (beforeKey) body.before = beforeKey;
                    if (afterKey) body.after = afterKey;

                    fetch('/api/tasks/' + encodeURIComponent(draggedKey) + '/move', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(body)
                    }).then(function(res) {
                        if (res.ok) window.location.reload();
                        else res.text().then(function(t) { console.error('Move failed:', t); });
                    });
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
) -> Result<Vec<TaskView>, ranger::error::RangerError> {
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
            key: task.key.clone(),
            key_prefix,
            key_rest,
            title: task.title.clone(),
            description: task.description.clone(),
            tags,
        });
    }
    Ok(views)
}
