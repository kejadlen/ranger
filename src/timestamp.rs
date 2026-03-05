use serde::{Deserialize, Serialize};

/// UTC timestamp backed by `jiff::Timestamp`.
///
/// Wraps `jiff::Timestamp` with `sqlx` and `serde` integration so it can be
/// used directly in model structs that derive `FromRow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub jiff::Timestamp);

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.strftime("%Y-%m-%dT%H:%M:%SZ"))
    }
}

impl sqlx::Type<sqlx::Sqlite> for Timestamp {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Timestamp {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        let ts: jiff::Timestamp = s.parse()?;
        Ok(Timestamp(ts))
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Timestamp {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = self.to_string();
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(s, buf)
    }
}

// Timestamp is used as a field in model structs that derive FromRow,
// which requires both Decode and Encode. Decode is exercised by every
// query that returns a model. Encode is exercised when a Timestamp is
// bound as a query parameter — currently only in tests, since production
// code lets SQLite generate timestamps via strftime.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_as_iso8601() {
        let ts = Timestamp(jiff::Timestamp::from_second(1_700_000_000).unwrap());
        assert_eq!(ts.to_string(), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn serde_roundtrips() {
        let ts = Timestamp(jiff::Timestamp::from_second(1_700_000_000).unwrap());
        let json = serde_json::to_string(&ts).unwrap();
        let deserialized: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, deserialized);
    }

    #[tokio::test]
    async fn sqlx_encode_roundtrips_through_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::connect(&dir.path().join("test.db"))
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();

        let ts = Timestamp(jiff::Timestamp::from_second(1_700_000_000).unwrap());
        let row: (String,) = sqlx::query_as("SELECT ?")
            .bind(&ts)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(row.0, "2023-11-14T22:13:20Z");
    }
}
