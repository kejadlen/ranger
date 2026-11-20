use ranger::db::SqlitePool;
use ranger::filter::Filter;

/// What every command runs against: the database and the settings `main`
/// resolves once from flags and the environment. Commands read these from
/// here instead of taking them as separate arguments or reading env vars.
pub struct Ranger {
    pub pool: SqlitePool,
    /// The default filter narrowed by `--filter`. Commands work within it,
    /// and task keys resolve among the tasks it matches first.
    pub filter: Filter,
    /// Print JSON instead of human-readable output.
    pub json: bool,
    /// `$EDITOR`, for `task edit` with no field flags.
    pub editor: Option<String>,
}
