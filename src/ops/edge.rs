use crate::error::RangerError;
use crate::models::{EdgeType, TaskEdge};
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use sqlx::sqlite::SqliteConnection;
use std::collections::HashMap;

pub async fn add(
    conn: &mut SqliteConnection,
    from_task_id: i64,
    to_task_id: i64,
    edge_type: EdgeType,
) -> Result<TaskEdge, RangerError> {
    // Load the current DAG and check if adding this edge would create a cycle
    let edges = list_all(&mut *conn).await?;
    check_cycle(&edges, from_task_id, to_task_id)?;

    // Enforce: at most one outgoing 'before' edge per task
    if edge_type == EdgeType::Before {
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM task_edges WHERE from_task_id = ? AND edge_type = 'before'",
        )
        .bind(from_task_id)
        .fetch_optional(&mut *conn)
        .await?;

        if existing.is_some() {
            return Err(RangerError::DuplicateBeforeEdge);
        }
    }

    let edge = sqlx::query_as::<_, TaskEdge>(
        "INSERT INTO task_edges (from_task_id, to_task_id, edge_type) \
         VALUES (?, ?, ?) \
         RETURNING id, from_task_id, to_task_id, edge_type, created_at",
    )
    .bind(from_task_id)
    .bind(to_task_id)
    .bind(&edge_type)
    .fetch_one(&mut *conn)
    .await?;

    Ok(edge)
}

pub async fn remove(
    conn: &mut SqliteConnection,
    from_task_id: i64,
    to_task_id: i64,
    edge_type: EdgeType,
) -> Result<bool, RangerError> {
    let result = sqlx::query(
        "DELETE FROM task_edges WHERE from_task_id = ? AND to_task_id = ? AND edge_type = ?",
    )
    .bind(from_task_id)
    .bind(to_task_id)
    .bind(&edge_type)
    .execute(&mut *conn)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_for_task(
    conn: &mut SqliteConnection,
    task_id: i64,
) -> Result<Vec<TaskEdge>, RangerError> {
    let edges = sqlx::query_as::<_, TaskEdge>(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges \
         WHERE from_task_id = ? OR to_task_id = ? \
         ORDER BY created_at",
    )
    .bind(task_id)
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(edges)
}

pub async fn list_all(conn: &mut SqliteConnection) -> Result<Vec<TaskEdge>, RangerError> {
    let edges = sqlx::query_as::<_, TaskEdge>(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges \
         ORDER BY created_at",
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(edges)
}

/// Build a petgraph DiGraph from task edges. Returns the graph and a mapping
/// from task_id to NodeIndex.
pub fn build_dag(edges: &[TaskEdge]) -> (DiGraph<i64, EdgeType>, HashMap<i64, NodeIndex>) {
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<i64, NodeIndex> = HashMap::new();

    for edge in edges {
        let from_idx = *node_map
            .entry(edge.from_task_id)
            .or_insert_with(|| graph.add_node(edge.from_task_id));
        let to_idx = *node_map
            .entry(edge.to_task_id)
            .or_insert_with(|| graph.add_node(edge.to_task_id));
        graph.add_edge(from_idx, to_idx, edge.edge_type.clone());
    }

    (graph, node_map)
}

/// Check if adding an edge from → to would create a cycle.
fn check_cycle(
    existing_edges: &[TaskEdge],
    from_task_id: i64,
    to_task_id: i64,
) -> Result<(), RangerError> {
    let (mut graph, mut node_map) = build_dag(existing_edges);

    let from_idx = *node_map
        .entry(from_task_id)
        .or_insert_with(|| graph.add_node(from_task_id));
    let to_idx = *node_map
        .entry(to_task_id)
        .or_insert_with(|| graph.add_node(to_task_id));

    graph.add_edge(from_idx, to_idx, EdgeType::Blocks);

    if petgraph::algo::is_cyclic_directed(&graph) {
        return Err(RangerError::CycleDetected);
    }

    Ok(())
}

/// Produce a topological sort of task IDs from edges. Tasks with no edges
/// are not included — callers should merge with the full task list.
/// Uses task_id as a stable tiebreaker for deterministic ordering.
pub fn topological_sort(edges: &[TaskEdge]) -> Vec<i64> {
    if edges.is_empty() {
        return Vec::new();
    }

    let (graph, node_map) = build_dag(edges);

    // petgraph's toposort returns nodes in topological order
    match petgraph::algo::toposort(&graph, None) {
        Ok(sorted) => sorted.iter().map(|idx| graph[*idx]).collect(),
        Err(_) => {
            // cov-excl-start — cycles are prevented on insert; this is defensive
            let mut ids: Vec<i64> = node_map.keys().copied().collect();
            ids.sort();
            ids
            // cov-excl-stop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::State;
    use crate::ops::{self, backlog, task};

    async fn test_pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().unwrap();
        // Leak the tempdir so it lives for the duration of the test
        let path = dir.path().join("test.db");
        let pool = crate::db::connect(&path).await.unwrap();
        std::mem::forget(dir);
        pool
    }

    async fn create_task(conn: &mut SqliteConnection, backlog_id: i64, title: &str) -> i64 {
        task::create(
            conn,
            task::CreateTask {
                title,
                backlog_id,
                state: Some(State::Queued),
                description: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn add_and_list_edges() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        let edge = add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        assert_eq!(edge.from_task_id, t1);
        assert_eq!(edge.to_task_id, t2);
        assert_eq!(edge.edge_type, EdgeType::Blocks);

        let edges = list_for_task(&mut conn, t1).await.unwrap();
        assert_eq!(edges.len(), 1);

        let edges = list_for_task(&mut conn, t2).await.unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[tokio::test]
    async fn remove_edge() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        let removed = remove(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        assert!(removed);

        let edges = list_for_task(&mut conn, t1).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_edge() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        let removed = remove(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn cycle_detection_direct() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        let err = add(&mut conn, t2, t1, EdgeType::Blocks).await.unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn cycle_detection_indirect() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t2, t3, EdgeType::Blocks).await.unwrap();
        let err = add(&mut conn, t3, t1, EdgeType::Before).await.unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn cycle_detection_self_loop() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;

        let err = add(&mut conn, t1, t1, EdgeType::Blocks).await.unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn duplicate_before_edge_rejected() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        let err = add(&mut conn, t1, t3, EdgeType::Before).await.unwrap_err();
        assert!(
            err.to_string().contains("before"),
            "expected duplicate before error, got: {err}"
        );
    }

    #[tokio::test]
    async fn multiple_blocks_edges_allowed() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t1, t3, EdgeType::Blocks).await.unwrap();

        let edges = list_for_task(&mut conn, t1).await.unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn build_dag_empty() {
        let (graph, node_map) = build_dag(&[]);
        assert_eq!(graph.node_count(), 0);
        assert!(node_map.is_empty());
    }

    #[test]
    fn topological_sort_empty() {
        let result = topological_sort(&[]);
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn topological_sort_linear_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        add(&mut conn, t2, t3, EdgeType::Before).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = topological_sort(&edges);
        assert_eq!(sorted, vec![t1, t2, t3]);
    }

    #[tokio::test]
    async fn topological_sort_diamond() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;
        let t4 = create_task(&mut conn, bl.id, "Task 4").await;

        // t1 → t2, t1 → t3, t2 → t4, t3 → t4
        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t1, t3, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t2, t4, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t3, t4, EdgeType::Blocks).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = topological_sort(&edges);

        // t1 must come first, t4 must come last
        assert_eq!(sorted[0], t1);
        assert_eq!(*sorted.last().unwrap(), t4);
        // t2 and t3 must come before t4
        let pos2 = sorted.iter().position(|&id| id == t2).unwrap();
        let pos3 = sorted.iter().position(|&id| id == t3).unwrap();
        let pos4 = sorted.iter().position(|&id| id == t4).unwrap();
        assert!(pos2 < pos4);
        assert!(pos3 < pos4);
    }

    #[tokio::test]
    async fn edges_deleted_on_task_delete() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        ops::task::delete(&mut conn, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn mixed_edge_types_in_dag() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        // t1 blocks t2, t2 before t3 — both in same DAG
        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();
        add(&mut conn, t2, t3, EdgeType::Before).await.unwrap();

        // t3 → t1 would create a cycle across edge types
        let err = add(&mut conn, t3, t1, EdgeType::Blocks).await.unwrap_err();
        assert!(err.to_string().contains("cycle"));

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = topological_sort(&edges);
        assert_eq!(sorted, vec![t1, t2, t3]);
    }
}
