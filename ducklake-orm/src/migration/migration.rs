//! Core migration trait and built-in implementations.

use std::borrow::Cow;

use crate::error::DuckLakeError;

// ── Migration trait ───────────────────────────────────────────────────────────

/// A single, versioned schema change that can be applied (`up`) and reversed (`down`).
///
/// Implement this trait when you need migration logic that goes beyond plain SQL
/// — for example, a data backfill that uses the ORM builders, or a migration
/// that calls an external service. For SQL-only migrations, use [`SqlMigration`]
/// instead.
///
/// # Contract
///
/// - [`version`](Migration::version) must be **unique** across all migrations
///   registered with a [`Migrator`](super::Migrator). The migrator sorts
///   migrations by version before applying them, so gaps in the sequence are fine.
/// - [`up`](Migration::up) must be **idempotent** when possible (e.g., use
///   `CREATE TABLE IF NOT EXISTS`) because the migrator does not prevent
///   re-running a migration that is already tracked in an inconsistent state.
/// - [`down`](Migration::down) must exactly undo what `up` did. If a migration
///   is not reversible, return `Err(DuckLakeError::Query("…not reversible…".into()))`.
///
/// # Example
///
/// ```rust,ignore
/// use ducklake_orm::{DuckLakeError, migration::Migration};
///
/// struct SeedSales;
///
/// impl Migration for SeedSales {
///     fn version(&self) -> i64 { 3 }
///     fn description(&self) -> &str { "seed initial sales rows" }
///
///     fn up(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
///         conn.execute_batch(
///             "INSERT INTO main.sales VALUES (1, 100.0, 'EU');
///              INSERT INTO main.sales VALUES (2, 200.0, 'US');"
///         )?;
///         Ok(())
///     }
///
///     fn down(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
///         conn.execute_batch("DELETE FROM main.sales WHERE id IN (1, 2)")?;
///         Ok(())
///     }
/// }
/// ```
pub trait Migration: Send + Sync {
    /// A strictly increasing integer that identifies this migration.
    ///
    /// Migrations are applied in ascending `version` order. Gaps are allowed
    /// (e.g., 1, 5, 10), but duplicates will cause
    /// [`Migrator::run`](super::Migrator::run) to return an error.
    fn version(&self) -> i64;

    /// A short human-readable description stored in `_ducklake_migrations`.
    ///
    /// Shown by [`Migrator::status`](super::Migrator::status). Keep it short
    /// and in the imperative tense, e.g. `"create sales table"`.
    fn description(&self) -> &str;

    /// Apply this migration — create tables, add columns, seed data, etc.
    ///
    /// The [`Migrator`](super::Migrator) wraps this call in a `BEGIN`/`COMMIT`
    /// transaction together with the insert into `_ducklake_migrations`, so
    /// the migration is recorded atomically. If this method returns `Err`, the
    /// transaction is rolled back and the migration is **not** recorded as applied.
    fn up(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError>;

    /// Reverse this migration — drop tables, remove columns, delete seeded data, etc.
    ///
    /// Called by [`Migrator::rollback`](super::Migrator::rollback). The migrator
    /// wraps this call in a transaction, so the removal of the record from
    /// `_ducklake_migrations` is atomic with the schema change.
    ///
    /// If this migration is intentionally not reversible, return:
    /// ```rust,ignore
    /// Err(DuckLakeError::Query("migration 3 is not reversible".into()))
    /// ```
    fn down(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError>;
}

// ── SqlMigration ──────────────────────────────────────────────────────────────

/// A migration defined by a pair of raw SQL strings.
///
/// This is the most common migration type: you supply an `up` SQL script and a
/// `down` SQL script, and `SqlMigration` runs them with
/// [`execute_batch`](duckdb::Connection::execute_batch), which supports
/// multiple statements separated by `;`.
///
/// # Example
///
/// ```rust,no_run
/// use ducklake_orm::migration::{Migrator, SqlMigration};
/// use ducklake_orm::DuckLakeConnection;
///
/// let db = DuckLakeConnection::open_in_memory()?;
///
/// Migrator::new(&db)
///     .add(SqlMigration::new(
///         1,
///         "create sales table",
///         "CREATE TABLE IF NOT EXISTS main.sales (
///              id      BIGINT PRIMARY KEY,
///              amount  DOUBLE NOT NULL,
///              region  VARCHAR NOT NULL
///          )",
///         "DROP TABLE IF EXISTS main.sales",
///     ))
///     .add(SqlMigration::new(
///         2,
///         "add sold_at column",
///         "ALTER TABLE main.sales ADD COLUMN sold_at TIMESTAMP",
///         "ALTER TABLE main.sales DROP COLUMN sold_at",
///     ))
///     .run()?;
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct SqlMigration {
    version: i64,
    description: Cow<'static, str>,
    up_sql: Cow<'static, str>,
    // `None` marks a migration as non-reversible: calling `down` returns an error.
    down_sql: Option<Cow<'static, str>>,
}

impl SqlMigration {
    /// Create a new reversible SQL migration.
    ///
    /// - `version` — unique, monotonically increasing integer.
    /// - `description` — short label stored in the migrations tracking table.
    ///   Accepts either a `&'static str` literal or an owned `String`.
    /// - `up_sql` — SQL executed when applying the migration. Multiple statements
    ///   separated by `;` are supported via `execute_batch`. Accepts a `&'static str`
    ///   or an owned `String` (e.g. loaded from a file).
    /// - `down_sql` — SQL executed when rolling back the migration. Same ownership rules.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::migration::SqlMigration;
    ///
    /// let m = SqlMigration::new(
    ///     1,
    ///     "create users table",
    ///     "CREATE TABLE IF NOT EXISTS main.users (id BIGINT, name VARCHAR)",
    ///     "DROP TABLE IF EXISTS main.users",
    /// );
    /// ```
    pub fn new(
        version: i64,
        description: impl Into<Cow<'static, str>>,
        up_sql: impl Into<Cow<'static, str>>,
        down_sql: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            version,
            description: description.into(),
            up_sql: up_sql.into(),
            down_sql: Some(down_sql.into()),
        }
    }

    /// Create a SQL migration that cannot be rolled back.
    ///
    /// Calling [`Migration::down`] on a migration built this way returns an
    /// [`DuckLakeError::Query`] explaining that the migration is not reversible.
    /// This is useful for data-destructive `up` scripts (e.g. `DROP COLUMN`,
    /// `TRUNCATE`, schema refactor) where a meaningful `down` cannot be written,
    /// and for migrations loaded from a directory that has no `.down.sql` file.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::migration::SqlMigration;
    ///
    /// let m = SqlMigration::new_irreversible(
    ///     5,
    ///     "drop legacy column",
    ///     "ALTER TABLE main.users DROP COLUMN legacy_flag",
    /// );
    /// ```
    pub fn new_irreversible(
        version: i64,
        description: impl Into<Cow<'static, str>>,
        up_sql: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            version,
            description: description.into(),
            up_sql: up_sql.into(),
            down_sql: None,
        }
    }
}

impl Migration for SqlMigration {
    fn version(&self) -> i64 {
        self.version
    }
    fn description(&self) -> &str {
        &self.description
    }

    fn up(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
        conn.execute_batch(&self.up_sql)?;
        Ok(())
    }

    fn down(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
        match &self.down_sql {
            Some(sql) => {
                conn.execute_batch(sql)?;
                Ok(())
            }
            None => Err(DuckLakeError::Query(format!(
                "migration v{} ('{}') is not reversible: no down script was provided",
                self.version, self.description
            ))),
        }
    }
}

// ── MigrationStatus ───────────────────────────────────────────────────────────

/// The status of a single migration as returned by [`Migrator::status`](super::Migrator::status).
#[derive(Debug)]
pub struct MigrationStatus {
    /// The migration version number.
    pub version: i64,
    /// The migration description.
    pub description: String,
    /// `true` if this migration has already been applied to the database.
    pub applied: bool,
    /// The UTC timestamp at which the migration was applied, if `applied` is `true`.
    ///
    /// The value is a string formatted by DuckDB (`YYYY-MM-DD HH:MM:SS[.mmm]`).
    pub applied_at: Option<String>,
}
