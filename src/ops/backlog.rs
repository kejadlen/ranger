use crate::error::RangerError;
use crate::filter::{Matcher, Term};
use crate::models::{Backlog, TagParts};
use sqlx::sqlite::SqliteConnection;

/// The `project:` tag that mirrors a backlog. Every task in the backlog
/// carries it.
pub fn project_tag(name: &str) -> String {
    format!("project:{name}")
}

/// The filter term matching a backlog's tasks, through its `project:` tag.
pub fn project_term(name: &str) -> Result<Term, RangerError> {
    let tag = project_tag(name);
    let parts = TagParts::try_from(tag.as_str())?;
    Ok(Term {
        negated: false,
        matcher: Matcher::Tag {
            key: parts.key.to_string(),
            value: parts.value.to_string(),
        },
    })
}

/// Create a backlog. Its name must work as the value of its `project:` tag.
pub async fn create(conn: &mut SqliteConnection, name: &str) -> Result<Backlog, RangerError> {
    TagParts::try_from(project_tag(name).as_str())?;
    let backlog = sqlx::query_as::<_, Backlog>(
        "INSERT INTO backlogs (name) VALUES (?) RETURNING id, name, created_at, updated_at",
    )
    .bind(name)
    .fetch_one(&mut *conn)
    .await?;
    crate::ops::tag::get_or_create(conn, &project_tag(name)).await?;
    Ok(backlog)
}

pub async fn list(conn: &mut SqliteConnection) -> Result<Vec<Backlog>, RangerError> {
    let backlogs = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs ORDER BY name",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(backlogs)
}

pub async fn get_by_name(conn: &mut SqliteConnection, name: &str) -> Result<Backlog, RangerError> {
    let backlog = sqlx::query_as::<_, Backlog>(
        "SELECT id, name, created_at, updated_at FROM backlogs WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| RangerError::BacklogNotFound(name.to_string()))?;
    Ok(backlog)
}

/// Delete a backlog and its tasks. Its `project:` tag stays, since tasks in
/// other backlogs may carry it; `tag prune` removes it once it's unused.
pub async fn delete(conn: &mut SqliteConnection, name: &str) -> Result<Backlog, RangerError> {
    let backlog = get_by_name(&mut *conn, name).await?;
    // Its tasks are the ones carrying its `project:` tag.
    sqlx::query(
        "DELETE FROM tasks WHERE id IN (SELECT tt.task_id FROM task_tags tt \
         JOIN tags t ON t.id = tt.tag_id WHERE t.key = 'project' AND t.value = ?)",
    )
    .bind(&backlog.name)
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM backlogs WHERE id = ?")
        .bind(backlog.id)
        .execute(&mut *conn)
        .await?;
    Ok(backlog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    async fn test_pool() -> sqlx::SqlitePool {
        let dir = tempdir().unwrap();
        let dir = Box::leak(Box::new(dir));
        db::connect(&dir.path().join("test.db")).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let backlog = create(&mut conn, "my-backlog").await.unwrap();
        assert_eq!(backlog.name, "my-backlog");

        let fetched = get_by_name(&mut conn, "my-backlog").await.unwrap();
        assert_eq!(fetched.id, backlog.id);
    }

    #[tokio::test]
    async fn create_rejects_names_that_cant_be_a_project_tag() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        for name in ["My Backlog", ""] {
            let err = create(&mut conn, name).await.unwrap_err();
            assert!(matches!(err, RangerError::InvalidTag(_)), "{name:?}");
        }
        assert_eq!(
            create(&mut conn, "My Backlog")
                .await
                .unwrap_err()
                .to_string(),
            "tag 'project:My Backlog' can't contain whitespace, which separates filter terms"
        );
        assert!(list(&mut conn).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_backlogs() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        create(&mut conn, "First").await.unwrap();
        create(&mut conn, "Second").await.unwrap();

        let backlogs = list(&mut conn).await.unwrap();
        assert_eq!(backlogs.len(), 2);
    }

    #[tokio::test]
    async fn get_by_name_not_found() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = get_by_name(&mut conn, "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_backlog() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        create(&mut conn, "ToDelete").await.unwrap();
        let deleted = delete(&mut conn, "ToDelete").await.unwrap();
        assert_eq!(deleted.name, "ToDelete");

        let result = get_by_name(&mut conn, "ToDelete").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_backlog_deletes_only_its_tasks() {
        use crate::ops::{tag, task};
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        create(&mut conn, "gone").await.unwrap();
        for (title, project) in [("doomed", "gone"), ("spared", "kept")] {
            let t = task::create(
                &mut conn,
                task::CreateTask {
                    title,
                    state: None,
                    description: None,
                },
            )
            .await
            .unwrap();
            tag::add(&mut conn, t.id, &project_tag(project))
                .await
                .unwrap();
        }

        delete(&mut conn, "gone").await.unwrap();
        let left: Vec<String> = task::list(&mut conn, &Default::default())
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(left, ["spared"]);
    }

    #[tokio::test]
    async fn delete_backlog_not_found() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = delete(&mut conn, "nonexistent").await;
        assert!(result.is_err());
    }
}
