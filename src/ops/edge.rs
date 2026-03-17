use crate::error::RangerError;
use crate::models::{EdgeType, TaskEdge};
use petgraph::graph::DiGraph;
use petgraph::graph::NodeIndex;
use sqlx::sqlite::SqliteConnection;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub async fn add(
    conn: &mut SqliteConnection,
    from_task_id: i64,
    to_task_id: i64,
    edge_type: EdgeType,
) -> Result<TaskEdge, RangerError> {
    // Load the current DAG and check if adding this edge would create a cycle
    let edges = list_all(&mut *conn).await?;
    check_cycle(&edges, from_task_id, to_task_id)?;

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

/// Deterministic topological sort using Kahn's algorithm with min-ID tiebreaker.
///
/// Only considers `before` edges between task IDs in `task_ids`.
/// Every ID in `task_ids` appears exactly once in the output.
pub fn ordered_ids(task_ids: &[i64], edges: &[TaskEdge]) -> Vec<i64> {
    if task_ids.is_empty() {
        return Vec::new();
    }

    let id_set: HashSet<i64> = task_ids.iter().copied().collect();

    // Build adjacency list and in-degree map — only 'before' edges within the set
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut in_degree: HashMap<i64, usize> = HashMap::new();

    for &id in task_ids {
        adj.entry(id).or_default();
        in_degree.entry(id).or_insert(0);
    }

    for edge in edges {
        if edge.edge_type == EdgeType::Before
            && id_set.contains(&edge.from_task_id)
            && id_set.contains(&edge.to_task_id)
        {
            adj.entry(edge.from_task_id)
                .or_default()
                .push(edge.to_task_id);
            *in_degree.entry(edge.to_task_id).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm with a min-heap for deterministic ordering
    let mut heap: BinaryHeap<Reverse<i64>> = BinaryHeap::new();
    for (&id, &deg) in &in_degree {
        if deg == 0 {
            heap.push(Reverse(id));
        }
    }

    let mut result = Vec::with_capacity(task_ids.len());
    while let Some(Reverse(id)) = heap.pop() {
        result.push(id);
        for &next in &adj[&id] {
            let deg = in_degree.get_mut(&next).unwrap();
            *deg -= 1;
            if *deg == 0 {
                heap.push(Reverse(next));
            }
        }
    }

    result
}

/// Remove a task from any `before` chains it participates in.
///
/// If the task has predecessors (P → task) and a successor (task → S),
/// reconnects each predecessor directly to the successor (P → S).
pub async fn splice_out_before(
    conn: &mut SqliteConnection,
    task_id: i64,
) -> Result<(), RangerError> {
    // Get the outgoing 'before' edge: task → successor
    let outgoing: Option<TaskEdge> = sqlx::query_as(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges WHERE from_task_id = ? AND edge_type = 'before'",
    )
    .bind(task_id)
    .fetch_optional(&mut *conn)
    .await?;

    // Get all incoming 'before' edges: predecessor → task
    let incoming: Vec<TaskEdge> = sqlx::query_as(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges WHERE to_task_id = ? AND edge_type = 'before'",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?;

    // Delete all 'before' edges involving this task
    sqlx::query("DELETE FROM task_edges WHERE (from_task_id = ? OR to_task_id = ?) AND edge_type = 'before'")
        .bind(task_id)
        .bind(task_id)
        .execute(&mut *conn)
        .await?;

    // Reconnect: each predecessor → successor
    if let Some(succ) = &outgoing {
        for pred in &incoming {
            sqlx::query(
                "INSERT OR IGNORE INTO task_edges (from_task_id, to_task_id, edge_type) \
                 VALUES (?, ?, 'before')",
            )
            .bind(pred.from_task_id)
            .bind(succ.to_task_id)
            .execute(&mut *conn)
            .await?;
        }
    }

    Ok(())
}

/// Insert a task immediately before a target in the `before` chain.
///
/// Any predecessors of the target are rewired to point to the inserted task.
pub async fn insert_before_task(
    conn: &mut SqliteConnection,
    task_id: i64,
    target_id: i64,
) -> Result<(), RangerError> {
    // Rewire incoming 'before' edges to target → point to task instead
    let incoming: Vec<TaskEdge> = sqlx::query_as(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges WHERE to_task_id = ? AND edge_type = 'before'",
    )
    .bind(target_id)
    .fetch_all(&mut *conn)
    .await?;

    for pred in &incoming {
        sqlx::query("DELETE FROM task_edges WHERE id = ?")
            .bind(pred.id)
            .execute(&mut *conn)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO task_edges (from_task_id, to_task_id, edge_type) \
             VALUES (?, ?, 'before')",
        )
        .bind(pred.from_task_id)
        .bind(task_id)
        .execute(&mut *conn)
        .await?;
    }

    // Add task → target
    add(&mut *conn, task_id, target_id, EdgeType::Before).await?;
    Ok(())
}

/// Insert a task immediately after an anchor in the `before` chain.
///
/// All outgoing `before` edges from the anchor are rewired through the task.
pub async fn insert_after_task(
    conn: &mut SqliteConnection,
    task_id: i64,
    anchor_id: i64,
) -> Result<(), RangerError> {
    // Collect all outgoing 'before' edges from anchor
    let outgoing: Vec<TaskEdge> = sqlx::query_as(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges WHERE from_task_id = ? AND edge_type = 'before'",
    )
    .bind(anchor_id)
    .fetch_all(&mut *conn)
    .await?;

    for succ in &outgoing {
        let successor_id = succ.to_task_id;
        // Remove anchor → successor
        remove(&mut *conn, anchor_id, successor_id, EdgeType::Before).await?;
        // Add task → successor
        add(&mut *conn, task_id, successor_id, EdgeType::Before).await?;
    }

    // Add anchor → task
    add(&mut *conn, anchor_id, task_id, EdgeType::Before).await?;
    Ok(())
}

/// Fetch all edges involving any of the given task IDs.
pub async fn list_for_task_ids(
    conn: &mut SqliteConnection,
    task_ids: &[i64],
) -> Result<Vec<TaskEdge>, RangerError> {
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build a comma-separated placeholder list
    let placeholders: String = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, from_task_id, to_task_id, edge_type, created_at \
         FROM task_edges \
         WHERE from_task_id IN ({placeholders}) OR to_task_id IN ({placeholders}) \
         ORDER BY created_at"
    );

    let mut q = sqlx::query_as::<_, TaskEdge>(&query);
    // Bind task_ids twice (once for each IN clause)
    for &id in task_ids {
        q = q.bind(id);
    }
    for &id in task_ids {
        q = q.bind(id);
    }

    let edges = q.fetch_all(&mut *conn).await?;
    Ok(edges)
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
    async fn multiple_before_edges_allowed() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        add(&mut conn, t1, t3, EdgeType::Before).await.unwrap();

        let edges = list_for_task(&mut conn, t1).await.unwrap();
        let before_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Before)
            .collect();
        assert_eq!(before_edges.len(), 2);
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

    // -- ordered_ids tests --

    #[test]
    fn ordered_ids_empty() {
        assert!(ordered_ids(&[], &[]).is_empty());
    }

    #[test]
    fn ordered_ids_no_edges_sorts_by_id() {
        let result = ordered_ids(&[30, 10, 20], &[]);
        assert_eq!(result, vec![10, 20, 30]);
    }

    #[tokio::test]
    async fn ordered_ids_respects_before_edges() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        // t3 before t1 (override natural id order)
        add(&mut conn, t3, t1, EdgeType::Before).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let result = ordered_ids(&[t1, t2, t3], &edges);
        // t2 has lowest id among unconstrained, t3 must come before t1
        assert_eq!(result, vec![t2, t3, t1]);
    }

    #[tokio::test]
    async fn ordered_ids_ignores_blocks_edges() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        // t2 blocks t1 — should NOT affect ordering
        add(&mut conn, t2, t1, EdgeType::Blocks).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let result = ordered_ids(&[t1, t2], &edges);
        assert_eq!(result, vec![t1, t2], "blocks edges should not affect order");
    }

    #[tokio::test]
    async fn ordered_ids_filters_to_given_ids() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        // Only ask about t2 and t3 — edge t1→t2 should be ignored since t1 not in set
        let result = ordered_ids(&[t2, t3], &edges);
        assert_eq!(result, vec![t2, t3]);
    }

    // -- splice_out_before tests --

    #[tokio::test]
    async fn splice_out_middle_of_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        add(&mut conn, t2, t3, EdgeType::Before).await.unwrap();

        splice_out_before(&mut conn, t2).await.unwrap();

        // t1 → t3 should now exist; no edges involving t2
        let edges = list_all(&mut conn).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_task_id, t1);
        assert_eq!(edges[0].to_task_id, t3);
        assert_eq!(edges[0].edge_type, EdgeType::Before);
    }

    #[tokio::test]
    async fn splice_out_head_of_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();

        splice_out_before(&mut conn, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn splice_out_tail_of_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();

        splice_out_before(&mut conn, t2).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn splice_out_preserves_blocks_edges() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        add(&mut conn, t1, t2, EdgeType::Blocks).await.unwrap();

        splice_out_before(&mut conn, t2).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_type, EdgeType::Blocks);
    }

    #[tokio::test]
    async fn splice_out_no_edges_is_noop() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;

        // Should not error
        splice_out_before(&mut conn, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        assert!(edges.is_empty());
    }

    // -- insert_before_task tests --

    #[tokio::test]
    async fn insert_before_into_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        // Chain: t1 → t2
        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();

        // Insert t3 before t2 → chain becomes t1 → t3 → t2
        insert_before_task(&mut conn, t3, t2).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let before_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Before)
            .collect();
        assert_eq!(before_edges.len(), 2);

        let sorted = ordered_ids(&[t1, t2, t3], &edges);
        assert_eq!(sorted, vec![t1, t3, t2]);
    }

    #[tokio::test]
    async fn insert_before_at_head() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        // Insert t2 before t1 (t1 has no predecessor)
        insert_before_task(&mut conn, t2, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = ordered_ids(&[t1, t2], &edges);
        assert_eq!(sorted, vec![t2, t1]);
    }

    // -- insert_after_task tests --

    #[tokio::test]
    async fn insert_after_into_chain() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        // Chain: t1 → t2
        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();

        // Insert t3 after t1 → chain becomes t1 → t3 → t2
        insert_after_task(&mut conn, t3, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = ordered_ids(&[t1, t2, t3], &edges);
        assert_eq!(sorted, vec![t1, t3, t2]);
    }

    #[tokio::test]
    async fn insert_after_at_tail() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;

        // Insert t2 after t1 (t1 has no successor)
        insert_after_task(&mut conn, t2, t1).await.unwrap();

        let edges = list_all(&mut conn).await.unwrap();
        let sorted = ordered_ids(&[t1, t2], &edges);
        assert_eq!(sorted, vec![t1, t2]);
    }

    // -- list_for_task_ids tests --

    #[tokio::test]
    async fn list_for_task_ids_empty() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let edges = list_for_task_ids(&mut conn, &[]).await.unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn list_for_task_ids_returns_relevant_edges() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let bl = backlog::create(&mut conn, "Test").await.unwrap();
        let t1 = create_task(&mut conn, bl.id, "Task 1").await;
        let t2 = create_task(&mut conn, bl.id, "Task 2").await;
        let t3 = create_task(&mut conn, bl.id, "Task 3").await;

        add(&mut conn, t1, t2, EdgeType::Before).await.unwrap();
        add(&mut conn, t2, t3, EdgeType::Before).await.unwrap();

        // Only ask about t1 and t2
        let edges = list_for_task_ids(&mut conn, &[t1, t2]).await.unwrap();
        // Should get t1→t2 and t2→t3 (t2 is involved in both)
        assert_eq!(edges.len(), 2);
    }
}
