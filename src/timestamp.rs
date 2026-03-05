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
}
