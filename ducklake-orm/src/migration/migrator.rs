//! The [`Migrator`] runner — applies, rolls back, and reports on migrations.

use duckdb::types::ToSql;

use crate::{connection::DuckLakeConnection, error::DuckLakeError, pool::PooledConnection};

use super::migration::{Migration, MigrationStatus, SqlMigration};

/// The fully-qualified name of the internal migrations tracking table.
const MIGRATIONS_TABLE: &str = "main._ducklake_migrations";

// ── Migrator ──────────────────────────────────────────────────────────────────

/// Collects migrations and runs them against a database connection.
///
/// `Migrator` maintains an ordered list of [`Migration`] implementations.
/// On [`run`](Self::run), it applies every migration whose version number has
/// not yet been recorded in the internal `_ducklake_migrations` tracking table.
/// On [`rollback`](Self::rollback), it reverses the most recently applied
/// migrations.
///
/// ## Tracking table
///
/// The migrator automatically creates `main._ducklake_migrations` on first use:
///
/// ```sql
/// CREATE TABLE IF NOT EXISTS main._ducklake_migrations (
///     version     BIGINT  PRIMARY KEY,
///     description VARCHAR NOT NULL,
///     applied_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
/// )
/// ```
///
/// You should **not** create or modify this table manually.
///
/// ## Enabling the feature
///
/// The migration system is opt-in. Add the feature flag to your `Cargo.toml`:
///
/// ```toml
/// [dependencies]
/// ducklake-orm = { version = "0.1", features = ["migrations"] }
/// ```
///
/// ## Example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, DuckLakeError};
/// use ducklake_orm::migration::{Migrator, SqlMigration};
///
/// let db = DuckLakeConnection::open("warehouse.duckdb")?;
///
/// let migrator = Migrator::new(&db)
///     .add(SqlMigration::new(
///         1,
///         "create sales table",
///         "CREATE TABLE IF NOT EXISTS main.sales (
///              id     BIGINT PRIMARY KEY,
///              amount DOUBLE NOT NULL,
///              region VARCHAR NOT NULL
///          )",
///         "DROP TABLE IF EXISTS main.sales",
///     ))
///     .add(SqlMigration::new(
///         2,
///         "add sold_at column",
///         "ALTER TABLE main.sales ADD COLUMN sold_at TIMESTAMP",
///         "ALTER TABLE main.sales DROP COLUMN sold_at",
///     ));
///
/// // Apply all pending migrations:
/// let applied = migrator.run()?;
/// println!("{applied} migration(s) applied");
///
/// // Check status:
/// for s in migrator.status()? {
///     println!("v{} [{}] {}", s.version, if s.applied { "✓" } else { " " }, s.description);
/// }
/// # Ok::<(), DuckLakeError>(())
/// ```
pub struct Migrator<'conn> {
    conn: &'conn duckdb::Connection,
    migrations: Vec<Box<dyn Migration>>,
}

impl<'conn> Migrator<'conn> {
    /// Create a `Migrator` from a [`DuckLakeConnection`].
    ///
    /// The migrator borrows the connection for its lifetime, so the connection
    /// must outlive the migrator.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::Migrator};
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// let migrator = Migrator::new(&db);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn new(conn: &'conn DuckLakeConnection) -> Self {
        Self {
            conn: conn.inner(),
            migrations: Vec::new(),
        }
    }

    /// Create a `Migrator` from a [`PooledConnection`].
    ///
    /// Useful when the pool is already open and you want to run migrations
    /// using a pooled connection at startup:
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakePool, DuckLakeConfig, migration::Migrator};
    ///
    /// let cfg  = DuckLakeConfig::from_file("ducklake.toml")?;
    /// let pool = DuckLakePool::from_config(&cfg)?;
    /// let conn = pool.get()?;
    /// let migrator = Migrator::from_pooled(&conn);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn from_pooled(conn: &'conn PooledConnection<'_>) -> Self {
        use std::ops::Deref;
        Self {
            conn: conn.inner.deref(),
            migrations: Vec::new(),
        }
    }

    /// Register a migration with this runner.
    ///
    /// Migrations can be added in any order — [`run`](Self::run) and
    /// [`rollback`](Self::rollback) always sort by [`Migration::version`]
    /// before executing. For clarity, add them in ascending version order.
    ///
    /// Returns `self` so calls can be chained:
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::{Migrator, SqlMigration}};
    ///
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let migrator = Migrator::new(&db)
    ///     .add(SqlMigration::new(1, "first",  "SELECT 1", "SELECT 1"))
    ///     .add(SqlMigration::new(2, "second", "SELECT 2", "SELECT 2"));
    /// ```
    pub fn add(mut self, migration: impl Migration + 'static) -> Self {
        self.migrations.push(Box::new(migration));
        self
    }

    /// Discover and register every migration file in `dir`.
    ///
    /// Files must follow the naming convention used by FluentMigrator / DbUp:
    ///
    /// ```text
    /// V<version>__<description>.up.sql        // required
    /// V<version>__<description>.down.sql      // optional — omit for irreversible migrations
    /// ```
    ///
    /// `<version>` is an unsigned integer parsed into `i64` (e.g. `V1`, `V20250101`).
    /// `<description>` is free-form but cannot be empty and must not contain `.`
    /// before the suffix. Migrations missing their `.up.sql` file are reported as
    /// an error; missing `.down.sql` is allowed and produces a non-reversible
    /// migration (see [`SqlMigration::new_irreversible`]).
    ///
    /// Files that do not match the pattern are silently ignored, so you can keep
    /// README / notes alongside the migrations.
    ///
    /// # Example
    ///
    /// Given a directory layout:
    ///
    /// ```text
    /// migrations/
    /// ├── V1__create_sales.up.sql
    /// ├── V1__create_sales.down.sql
    /// ├── V2__add_sold_at.up.sql
    /// └── V3__drop_legacy_column.up.sql      // no .down.sql → not reversible
    /// ```
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::Migrator};
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// let applied = Migrator::new(&db)
    ///     .add_directory("migrations")?
    ///     .run()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Io`] — the directory cannot be read.
    /// - [`DuckLakeError::Query`] — a migration version has an `.up.sql` missing,
    ///   the version number cannot be parsed, or the same version is registered
    ///   twice in the directory.
    pub fn add_directory<P: AsRef<std::path::Path>>(
        mut self,
        dir: P,
    ) -> Result<Self, DuckLakeError> {
        use std::collections::HashMap;
        use std::fs;
        use std::path::PathBuf;

        let dir = dir.as_ref();

        #[derive(Default)]
        struct Entry {
            description: Option<String>,
            up: Option<PathBuf>,
            down: Option<PathBuf>,
        }

        let mut found: HashMap<i64, Entry> = HashMap::new();

        for dir_entry in fs::read_dir(dir)? {
            let path = dir_entry?.path();
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            let Some((version, description, is_down)) = parse_migration_filename(file_name)
            else {
                continue;
            };

            let entry = found.entry(version).or_default();
            entry.description = Some(description.to_string());
            if is_down {
                entry.down = Some(path);
            } else {
                entry.up = Some(path);
            }
        }

        for (version, entry) in found {
            let description = entry.description.ok_or_else(|| {
                DuckLakeError::Query(format!(
                    "migration V{version}: invalid directory entry (missing description)"
                ))
            })?;
            let up_path = entry.up.ok_or_else(|| {
                DuckLakeError::Query(format!(
                    "migration V{version} ('{description}'): no .up.sql file found"
                ))
            })?;
            let up_sql = fs::read_to_string(&up_path)?;

            let migration = match entry.down {
                Some(down_path) => {
                    let down_sql = fs::read_to_string(down_path)?;
                    SqlMigration::new(version, description, up_sql, down_sql)
                }
                None => SqlMigration::new_irreversible(version, description, up_sql),
            };
            self = self.add(migration);
        }

        Ok(self)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn ensure_table(&self) -> Result<(), DuckLakeError> {
        self.conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (
                version     BIGINT  PRIMARY KEY,
                description VARCHAR NOT NULL,
                applied_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )"
        ))?;
        Ok(())
    }

    fn applied_versions(&self) -> Result<Vec<i64>, DuckLakeError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT version FROM {MIGRATIONS_TABLE} ORDER BY version ASC"
        ))?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        rows.map(|r| r.map_err(DuckLakeError::Duckdb)).collect()
    }

    fn validate_unique_versions(&self) -> Result<(), DuckLakeError> {
        let mut seen = std::collections::HashSet::new();
        for m in &self.migrations {
            if !seen.insert(m.version()) {
                return Err(DuckLakeError::Query(format!(
                    "duplicate migration version {} registered",
                    m.version()
                )));
            }
        }
        Ok(())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Apply all pending migrations in ascending version order.
    ///
    /// A migration is **pending** if its version number is not already recorded
    /// in `_ducklake_migrations`. Each migration is wrapped in its own
    /// `BEGIN`/`COMMIT` transaction: if [`Migration::up`] returns `Err`, the
    /// transaction is rolled back and the error is returned immediately without
    /// processing further migrations.
    ///
    /// # Returns
    ///
    /// The number of migrations that were applied. Returns `Ok(0)` if all
    /// migrations are already up to date.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Query`] — duplicate version numbers were registered.
    /// - [`DuckLakeError::Duckdb`] — a migration's SQL failed; the migration
    ///   is rolled back and no further migrations are applied.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::{Migrator, SqlMigration}};
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// let applied = Migrator::new(&db)
    ///     .add(SqlMigration::new(1, "init", "CREATE TABLE main.t (id BIGINT)", "DROP TABLE main.t"))
    ///     .run()?;
    /// assert_eq!(applied, 1);
    ///
    /// // Running again is a no-op:
    /// let applied = Migrator::new(&db)
    ///     .add(SqlMigration::new(1, "init", "CREATE TABLE main.t (id BIGINT)", "DROP TABLE main.t"))
    ///     .run()?;
    /// assert_eq!(applied, 0);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn run(&self) -> Result<usize, DuckLakeError> {
        self.validate_unique_versions()?;
        self.ensure_table()?;

        let applied = self.applied_versions()?;
        let mut pending: Vec<&dyn Migration> = self
            .migrations
            .iter()
            .map(|m| m.as_ref())
            .filter(|m| !applied.contains(&m.version()))
            .collect();
        pending.sort_by_key(|m| m.version());

        let count = pending.len();
        for m in pending {
            self.conn.execute_batch("BEGIN")?;
            match m.up(self.conn) {
                Ok(()) => {}
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
            let version = m.version();
            let description = m.description();
            let params: &[&dyn ToSql] = &[&version, &description];
            self.conn.execute(
                &format!("INSERT INTO {MIGRATIONS_TABLE} (version, description) VALUES ($1, $2)"),
                params,
            )?;
            self.conn.execute_batch("COMMIT")?;
        }
        Ok(count)
    }

    /// Roll back the last `steps` applied migrations in reverse version order.
    ///
    /// For each migration being rolled back, [`Migration::down`] is called
    /// inside a `BEGIN`/`COMMIT` transaction together with the deletion of its
    /// row from `_ducklake_migrations`. If `down` returns `Err`, the
    /// transaction is rolled back and the error is returned immediately.
    ///
    /// # Returns
    ///
    /// The number of migrations that were rolled back. If fewer than `steps`
    /// migrations have been applied, only the available ones are rolled back.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Query`] — a migration version found in the tracking
    ///   table is not registered with this `Migrator` (cannot call `down`).
    /// - [`DuckLakeError::Duckdb`] — a migration's rollback SQL failed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::{Migrator, SqlMigration}};
    ///
    /// let db  = DuckLakeConnection::open_in_memory()?;
    /// let m1  = || SqlMigration::new(1, "create t", "CREATE TABLE main.t (id BIGINT)", "DROP TABLE main.t");
    /// let m2  = || SqlMigration::new(2, "create u", "CREATE TABLE main.u (id BIGINT)", "DROP TABLE main.u");
    ///
    /// Migrator::new(&db).add(m1()).add(m2()).run()?;
    /// // Roll back only the last migration (v2):
    /// Migrator::new(&db).add(m1()).add(m2()).rollback(1)?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn rollback(&self, steps: usize) -> Result<usize, DuckLakeError> {
        self.validate_unique_versions()?;
        self.ensure_table()?;

        let applied = self.applied_versions()?;
        let to_rollback: Vec<i64> = applied.into_iter().rev().take(steps).collect();

        for version in &to_rollback {
            let m = self.migrations
                .iter()
                .find(|m| m.version() == *version)
                .ok_or_else(|| DuckLakeError::Query(format!(
                    "cannot roll back version {version}: no migration registered for that version"
                )))?;

            self.conn.execute_batch("BEGIN")?;
            match m.down(self.conn) {
                Ok(()) => {}
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
            let params: &[&dyn ToSql] = &[version];
            self.conn.execute(
                &format!("DELETE FROM {MIGRATIONS_TABLE} WHERE version = $1"),
                params,
            )?;
            self.conn.execute_batch("COMMIT")?;
        }
        Ok(to_rollback.len())
    }

    /// Return the status of every registered migration.
    ///
    /// Results are sorted by version in ascending order. Each entry indicates
    /// whether the migration has been applied and, if so, when.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, migration::{Migrator, SqlMigration}};
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// let migrator = Migrator::new(&db)
    ///     .add(SqlMigration::new(1, "create t", "CREATE TABLE main.t (id BIGINT)", "DROP TABLE main.t"));
    ///
    /// migrator.run()?;
    ///
    /// for s in migrator.status()? {
    ///     println!(
    ///         "v{:<4} [{}] {}",
    ///         s.version,
    ///         if s.applied { "applied" } else { "pending" },
    ///         s.description,
    ///     );
    /// }
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn status(&self) -> Result<Vec<MigrationStatus>, DuckLakeError> {
        self.ensure_table()?;

        let mut stmt = self.conn.prepare(&format!(
            "SELECT version, CAST(applied_at AS VARCHAR) FROM {MIGRATIONS_TABLE}"
        ))?;
        let applied_map: std::collections::HashMap<i64, Option<String>> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<_, _>>()?;

        let mut statuses: Vec<MigrationStatus> = self
            .migrations
            .iter()
            .map(|m| {
                let applied = applied_map.contains_key(&m.version());
                MigrationStatus {
                    version: m.version(),
                    description: m.description().to_string(),
                    applied,
                    applied_at: applied_map.get(&m.version()).and_then(|v| v.clone()),
                }
            })
            .collect();
        statuses.sort_by_key(|s| s.version);
        Ok(statuses)
    }
}

// ── Filename parsing ──────────────────────────────────────────────────────────

/// Parse a migration filename like `V1__create_users.up.sql` (or `.down.sql`).
///
/// Returns `(version, description, is_down)` on success, or `None` if the file
/// name does not follow the [`Migrator::add_directory`] convention.
///
/// Rules:
/// - Must start with `V`.
/// - `V` is followed by an ASCII digit run that parses as `i64`.
/// - Then `__` followed by a non-empty description.
/// - Then `.up.sql` or `.down.sql`.
fn parse_migration_filename(name: &str) -> Option<(i64, &str, bool)> {
    let rest = name.strip_prefix('V')?;
    let sep = rest.find("__")?;
    let version: i64 = rest[..sep].parse().ok()?;
    let after = &rest[sep + 2..];

    let (description, is_down) = if let Some(base) = after.strip_suffix(".down.sql") {
        (base, true)
    } else {
        let base = after.strip_suffix(".up.sql")?;
        (base, false)
    };

    if description.is_empty() {
        return None;
    }
    Some((version, description, is_down))
}
