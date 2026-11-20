use crate::error::RangerError;
use crate::filter::Filter;
use crate::models::{Tag, TagParts};
use sqlx::SqliteConnection;

/// Tag columns for `query_as::<_, Tag>`, deriving `name` from the key and
/// value the same way `TagParts` displays it: a bare tag drops its empty
/// key unless the value has a colon. Selects from a table aliased `t`.
const TAG_COLUMNS: &str = "t.id, \
     CASE WHEN t.key = '' AND instr(t.value, ':') = 0 THEN t.value \
     ELSE t.key || ':' || t.value END AS name, \
     t.key, t.value";

/// Get or create a tag by name. Returns the existing tag if it already exists.
/// `:bug` and `bug` name the same tag.
pub async fn get_or_create(conn: &mut SqliteConnection, name: &str) -> Result<Tag, RangerError> {
    let parts = TagParts::try_from(name)?;

    sqlx::query("INSERT OR IGNORE INTO tags (key, value) VALUES (?, ?)")
        .bind(parts.key)
        .bind(parts.value)
        .execute(&mut *conn)
        .await?;

    Ok(sqlx::query_as::<_, Tag>(&format!(
        "SELECT {TAG_COLUMNS} FROM tags t WHERE t.key = ? AND t.value = ?"
    ))
    .bind(parts.key)
    .bind(parts.value)
    .fetch_one(&mut *conn)
    .await?)
}

/// Add a tag to a task. Creates the tag if it doesn't exist. No-op if already tagged.
pub async fn add(
    conn: &mut SqliteConnection,
    task_id: i64,
    name: &str,
) -> Result<Tag, RangerError> {
    let tag = get_or_create(conn, name).await?;

    sqlx::query("INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?, ?)")
        .bind(task_id)
        .bind(tag.id)
        .execute(&mut *conn)
        .await?;

    Ok(tag)
}

/// Remove a tag from a task.
pub async fn remove(
    conn: &mut SqliteConnection,
    task_id: i64,
    name: &str,
) -> Result<(), RangerError> {
    let parts = TagParts::try_from(name)?;
    sqlx::query(
        "DELETE FROM task_tags WHERE task_id = ? \
         AND tag_id = (SELECT id FROM tags WHERE key = ? AND value = ?)",
    )
    .bind(task_id)
    .bind(parts.key)
    .bind(parts.value)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// List all tags for a task.
pub async fn list_for_task(
    conn: &mut SqliteConnection,
    task_id: i64,
) -> Result<Vec<Tag>, RangerError> {
    Ok(sqlx::query_as::<_, Tag>(&format!(
        "SELECT {TAG_COLUMNS} FROM tags t \
             JOIN task_tags tt ON t.id = tt.tag_id \
             WHERE tt.task_id = ? ORDER BY name"
    ))
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?)
}

/// List all tags (globally).
pub async fn list_all(conn: &mut SqliteConnection) -> Result<Vec<Tag>, RangerError> {
    Ok(
        sqlx::query_as::<_, Tag>(&format!("SELECT {TAG_COLUMNS} FROM tags t ORDER BY name"))
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// Delete tags with no associated tasks. Returns the removed tags.
/// If dry_run is true, returns what would be removed without deleting.
pub async fn prune(conn: &mut SqliteConnection, dry_run: bool) -> Result<Vec<Tag>, RangerError> {
    let orphaned = sqlx::query_as::<_, Tag>(&format!(
        "SELECT {TAG_COLUMNS} FROM tags t \
             WHERE t.id NOT IN (SELECT tag_id FROM task_tags) ORDER BY name"
    ))
    .fetch_all(&mut *conn)
    .await?;

    if !dry_run {
        sqlx::query("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM task_tags)")
            .execute(&mut *conn)
            .await?;
    }

    Ok(orphaned)
}

/// List the tags used by tasks matching `scope`.
pub async fn list_used_by(
    conn: &mut SqliteConnection,
    scope: &Filter,
) -> Result<Vec<Tag>, RangerError> {
    let (clause, binds) = crate::ops::task::terms_sql(&scope.0);
    let query = format!(
        "SELECT DISTINCT {TAG_COLUMNS} FROM tags t \
             JOIN task_tags tt ON t.id = tt.tag_id \
             JOIN tasks ON tt.task_id = tasks.id \
             WHERE 1{clause} ORDER BY name"
    );
    let mut q = sqlx::query_as::<_, Tag>(&query);
    for bind in binds {
        q = q.bind(bind);
    }
    Ok(q.fetch_all(&mut *conn).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::ops::task::CreateTask;

    /// Drop the `project:` tag every task gets from its backlog, so tests
    /// can count the tags they added themselves.
    fn user_tags(tags: Vec<Tag>) -> Vec<Tag> {
        tags.into_iter().filter(|t| t.key != "project").collect()
    }

    async fn setup() -> (sqlx::SqlitePool, i64, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let task = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Test task",
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        (pool, task.id, dir)
    }

    async fn setup_two_backlogs() -> (sqlx::SqlitePool, i64, i64, i64, i64, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let bl1 = ops::backlog::create(&mut conn, "Alpha").await.unwrap();
        let bl2 = ops::backlog::create(&mut conn, "Beta").await.unwrap();
        let t1 = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Alpha task",
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        let t2 = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Beta task",
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();
        add(&mut conn, t1.id, "project:Alpha").await.unwrap();
        add(&mut conn, t2.id, "project:Beta").await.unwrap();
        (pool, bl1.id, bl2.id, t1.id, t2.id, dir)
    }

    /// The name the database derives for a tag is the canonical name
    /// `TagParts` displays, so it names the same tag when typed back in.
    #[hegel::test]
    fn prop_stored_name_is_canonical(tc: hegel::TestCase) {
        use hegel::generators as gs;

        let names =
            tc.draw(gs::vecs(gs::text().alphabet("ab:-é").min_size(1).max_size(6)).max_size(4));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (pool, task_id, _dir) = setup().await;
            let mut conn = pool.acquire().await.unwrap();
            for name in &names {
                let Ok(parts) = TagParts::try_from(name.as_str()) else {
                    assert!(add(&mut conn, task_id, name).await.is_err());
                    continue;
                };
                let tag = add(&mut conn, task_id, name).await.unwrap();
                assert_eq!(tag.name, parts.to_string(), "{name:?}");
                assert_eq!(
                    (tag.key.as_str(), tag.value.as_str()),
                    (parts.key, parts.value)
                );
                let again = add(&mut conn, task_id, &tag.name).await.unwrap();
                assert_eq!(again.id, tag.id, "{name:?} vs {:?}", tag.name);
            }
        });
    }

    #[tokio::test]
    async fn add_rejects_empty_value() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        assert!(add(&mut conn, task_id, "x:").await.is_err());
        assert!(user_tags(list_all(&mut conn).await.unwrap()).is_empty());
    }

    #[tokio::test]
    async fn add_stores_key_and_value() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let tag = add(&mut conn, task_id, "area:cli").await.unwrap();
        assert_eq!(tag.key, "area");
        assert_eq!(tag.value, "cli");

        let bare = add(&mut conn, task_id, "bug").await.unwrap();
        assert_eq!(bare.key, "");
        assert_eq!(bare.value, "bug");

        let tags = user_tags(list_for_task(&mut conn, task_id).await.unwrap());
        assert_eq!(tags[0].key, "area");
        assert_eq!(tags[0].value, "cli");
    }

    #[tokio::test]
    async fn empty_key_names_the_bare_tag() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let bare = add(&mut conn, task_id, "bug").await.unwrap();
        let colon = add(&mut conn, task_id, ":bug").await.unwrap();
        assert_eq!(colon.id, bare.id);
        assert_eq!(colon.name, "bug");
        assert_eq!(user_tags(list_all(&mut conn).await.unwrap()).len(), 1);

        remove(&mut conn, task_id, ":bug").await.unwrap();
        assert!(user_tags(list_for_task(&mut conn, task_id).await.unwrap()).is_empty());
    }

    #[tokio::test]
    async fn remove_rejects_invalid_tag() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        assert!(remove(&mut conn, task_id, "state:x").await.is_err());
    }

    #[tokio::test]
    async fn add_and_list_tags() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "frontend").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let tags = user_tags(list_for_task(&mut conn, task_id).await.unwrap());
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "bug");
        assert_eq!(tags[1].name, "frontend");
    }

    #[tokio::test]
    async fn add_duplicate_tag_is_noop() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "bug").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let tags = user_tags(list_for_task(&mut conn, task_id).await.unwrap());
        assert_eq!(tags.len(), 1);
    }

    #[tokio::test]
    async fn remove_tag() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "bug").await.unwrap();
        remove(&mut conn, task_id, "bug").await.unwrap();

        let tags = user_tags(list_for_task(&mut conn, task_id).await.unwrap());
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_tag_is_noop() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        remove(&mut conn, task_id, "nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn list_all_tags() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "frontend").await.unwrap();
        add(&mut conn, task_id, "bug").await.unwrap();

        let all = user_tags(list_all(&mut conn).await.unwrap());
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "bug");
        assert_eq!(all[1].name, "frontend");
    }

    #[tokio::test]
    async fn tags_shared_across_tasks() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        let task2 = ops::task::create(
            &mut conn,
            CreateTask {
                title: "Second task",
                state: None,
                description: None,
            },
        )
        .await
        .unwrap();

        add(&mut conn, task_id, "shared").await.unwrap();
        add(&mut conn, task2.id, "shared").await.unwrap();

        let all = user_tags(list_all(&mut conn).await.unwrap());
        assert_eq!(all.len(), 1); // One tag, used by two tasks

        let tags1 = user_tags(list_for_task(&mut conn, task_id).await.unwrap());
        let tags2 = user_tags(list_for_task(&mut conn, task2.id).await.unwrap());
        assert_eq!(tags1.len(), 1);
        assert_eq!(tags2.len(), 1);
        assert_eq!(tags1[0].name, "shared");
        assert_eq!(tags2[0].name, "shared");
    }

    #[tokio::test]
    async fn list_used_by_scopes_to_matching_tasks() {
        let (pool, _bl1_id, _bl2_id, t1_id, t2_id, _dir) = setup_two_backlogs().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, t1_id, "frontend").await.unwrap();
        add(&mut conn, t2_id, "backend").await.unwrap();
        // Shared tag used in both backlogs
        add(&mut conn, t1_id, "urgent").await.unwrap();
        add(&mut conn, t2_id, "urgent").await.unwrap();

        let alpha_tags = user_tags(
            list_used_by(&mut conn, &"project:Alpha".parse().unwrap())
                .await
                .unwrap(),
        );
        assert_eq!(alpha_tags.len(), 2);
        let alpha_names: Vec<&str> = alpha_tags.iter().map(|t| t.name.as_str()).collect();
        assert!(alpha_names.contains(&"frontend"));
        assert!(alpha_names.contains(&"urgent"));
        assert!(!alpha_names.contains(&"backend"));

        let beta_tags = user_tags(
            list_used_by(&mut conn, &"project:Beta".parse().unwrap())
                .await
                .unwrap(),
        );
        assert_eq!(beta_tags.len(), 2);
        let beta_names: Vec<&str> = beta_tags.iter().map(|t| t.name.as_str()).collect();
        assert!(beta_names.contains(&"backend"));
        assert!(beta_names.contains(&"urgent"));
        assert!(!beta_names.contains(&"frontend"));
    }

    #[tokio::test]
    async fn list_used_by_empty_scope_match() {
        let (pool, _bl1_id, _bl2_id, _t1_id, _t2_id, _dir) = setup_two_backlogs().await;
        let mut conn = pool.acquire().await.unwrap();

        let tags = user_tags(
            list_used_by(&mut conn, &"project:Alpha".parse().unwrap())
                .await
                .unwrap(),
        );
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn prune_removes_orphaned_tags() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "used").await.unwrap();
        add(&mut conn, task_id, "also-used").await.unwrap();
        // Create an orphan by adding then removing
        add(&mut conn, task_id, "orphan").await.unwrap();
        remove(&mut conn, task_id, "orphan").await.unwrap();

        let pruned = prune(&mut conn, false).await.unwrap();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].name, "orphan");

        let all = user_tags(list_all(&mut conn).await.unwrap());
        assert_eq!(all.len(), 2);
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"used"));
        assert!(names.contains(&"also-used"));
    }

    #[tokio::test]
    async fn prune_dry_run_does_not_delete() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "orphan").await.unwrap();
        remove(&mut conn, task_id, "orphan").await.unwrap();

        let pruned = prune(&mut conn, true).await.unwrap();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].name, "orphan");

        // Tag still exists after dry run
        let all = user_tags(list_all(&mut conn).await.unwrap());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "orphan");
    }

    #[tokio::test]
    async fn prune_no_orphans_is_empty() {
        let (pool, task_id, _dir) = setup().await;
        let mut conn = pool.acquire().await.unwrap();

        add(&mut conn, task_id, "used").await.unwrap();

        let pruned = prune(&mut conn, false).await.unwrap();
        assert!(pruned.is_empty());
    }
}
