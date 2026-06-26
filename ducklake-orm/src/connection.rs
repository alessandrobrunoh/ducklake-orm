//! Single direct connection to a DuckLake / DuckDB database.
//!
//! [`DuckLakeConnection`] is the entry point for single-threaded or
//! single-connection use cases. For production applications that need concurrent
//! access from multiple threads, use [`DuckLakePool`](crate::pool::DuckLakePool)
//! instead.

use crate::{
    config::DuckLakeConfig,
    error::DuckLakeError,
    query::{
        BulkInsertBuilder, DeleteBuilder, InsertBuilder, SelectBuilder, SqlValue, UpdateBuilder,
        params_to_refs,
    },
    schema::DuckLakeTable,
};

/// A single, direct DuckLake / DuckDB connection (not pooled).
///
/// `DuckLakeConnection` wraps a single [`duckdb::Connection`] and exposes all
/// ORM operations: `select`, `insert`, `insert_many`, `update`, `delete`, and
/// raw `execute`.
///
/// ## When to use this vs [`DuckLakePool`](crate::pool::DuckLakePool)
///
/// | Use case | Recommended type |
/// |----------|-----------------|
/// | Single-threaded application or script | `DuckLakeConnection` |
/// | Unit / integration tests | `DuckLakeConnection::open_in_memory()` |
/// | Multi-threaded web server or background workers | [`DuckLakePool`](crate::pool::DuckLakePool) |
///
/// ## Opening a connection
///
/// ```rust,no_run
/// use ducklake_orm::DuckLakeConnection;
///
/// // Persistent file — created if it does not exist.
/// let db = DuckLakeConnection::open("warehouse.duckdb")?;
///
/// // In-memory — discarded when the connection closes.
/// let db = DuckLakeConnection::open_in_memory()?;
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
///
/// ## Using DuckLake time travel
///
/// After calling [`attach_ducklake`](Self::attach_ducklake), every `select`
/// query gains `.at_snapshot(n)` and `.at_timestamp("…")`:
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// let mut db = DuckLakeConnection::open_in_memory()?;
/// db.attach_ducklake("catalog.duckdb", "lake")?;
///
/// let rows: Vec<Sale> = db
///     .select::<Sale>()
///     .at_snapshot(10)
///     .fetch_all()?;
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct DuckLakeConnection {
    pub(crate) inner: duckdb::Connection,
    pub(crate) catalog: Option<String>,
}

impl DuckLakeConnection {
    /// Open or create a persistent DuckDB database file at `path`.
    ///
    /// The file is created automatically if it does not exist. If it already
    /// exists, the existing data is preserved.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if DuckDB cannot open the file
    /// (for example, if the path is invalid or the process lacks write
    /// permission).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::DuckLakeConnection;
    ///
    /// let db = DuckLakeConnection::open("data/warehouse.duckdb")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn open(path: &str) -> Result<Self, DuckLakeError> {
        let inner = duckdb::Connection::open(path)?;
        Ok(Self {
            inner,
            catalog: None,
        })
    }

    /// Open an in-memory DuckDB database.
    ///
    /// All data exists only for the lifetime of this `DuckLakeConnection`. When
    /// the connection is dropped, all tables and data are lost. This is ideal
    /// for tests and ephemeral processing pipelines.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] in the unlikely event that DuckDB
    /// cannot allocate the in-memory database.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakeConnection;
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// db.execute("CREATE TABLE main.t (id BIGINT)")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn open_in_memory() -> Result<Self, DuckLakeError> {
        let inner = duckdb::Connection::open_in_memory()?;
        Ok(Self {
            inner,
            catalog: None,
        })
    }

    /// Create a connection from a parsed [`DuckLakeConfig`].
    ///
    /// This is a convenience method that calls [`open`](Self::open) with
    /// `config.database.path` and optionally calls
    /// [`attach_ducklake`](Self::attach_ducklake) if
    /// `config.ducklake.auto_attach` is `true`.
    ///
    /// For multi-connection workloads, prefer
    /// [`DuckLakePool::from_config`](crate::pool::DuckLakePool::from_config).
    ///
    /// # Errors
    ///
    /// Propagates any error from [`open`](Self::open) or
    /// [`attach_ducklake`](Self::attach_ducklake).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConfig, DuckLakeConnection};
    ///
    /// let config = DuckLakeConfig::from_file("ducklake.toml")?;
    /// let db = DuckLakeConnection::from_config(&config)?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn from_config(cfg: &DuckLakeConfig) -> Result<Self, DuckLakeError> {
        let mut conn = Self::open(&cfg.database.path)?;
        if let Some(dl) = &cfg.ducklake {
            if dl.auto_attach {
                conn.attach_ducklake(&dl.catalog_path, &dl.catalog_name)?;
            }
        }
        Ok(conn)
    }

    /// Install and load the `ducklake` DuckDB extension, then attach a DuckLake
    /// catalog to this connection.
    ///
    /// After this call, all table references in queries are qualified as
    /// `<catalog_name>.<schema>.<table>` — for example `lake.main.sales`.
    ///
    /// The `INSTALL ducklake` step downloads the extension binary from the
    /// DuckDB extension registry on the first call. Subsequent calls use the
    /// locally cached binary.
    ///
    /// ## Requirements
    ///
    /// - DuckDB ≥ 1.5.2 (the minimum version that ships the stable
    ///   `ducklake` extension).
    /// - Internet access for the initial `INSTALL` step.
    ///
    /// ## Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if:
    /// - The extension cannot be downloaded or loaded.
    /// - The `ATTACH` statement fails (e.g., bad path, permission denied).
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::DuckLakeConnection;
    ///
    /// let mut db = DuckLakeConnection::open_in_memory()?;
    /// db.attach_ducklake("data/catalog.duckdb", "lake")?;
    ///
    /// // Now tables are referenced as lake.<schema>.<table>
    /// db.execute("CREATE TABLE lake.main.events (id BIGINT, ts TIMESTAMP)")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn attach_ducklake(
        &mut self,
        catalog_path: &str,
        catalog_name: &str,
    ) -> Result<(), DuckLakeError> {
        // `catalog_name` is interpolated as a bare SQL identifier (no quotes),
        // so it MUST be validated to prevent SQL injection. `catalog_path`
        // appears inside a single-quoted string literal, so we escape it.
        crate::ident::validate_identifier(catalog_name, "catalog_name")?;
        let escaped_path = crate::ident::escape_sql_string(catalog_path);

        self.inner
            .execute_batch("INSTALL ducklake; LOAD ducklake;")?;
        self.inner.execute(
            &format!("ATTACH '{escaped_path}' AS {catalog_name} (TYPE DUCKLAKE)"),
            [],
        )?;
        self.catalog = Some(catalog_name.to_string());
        Ok(())
    }

    /// Execute one or more raw SQL statements.
    ///
    /// Use this for DDL (`CREATE TABLE`, `DROP TABLE`, …), pragmas, or any SQL
    /// that does not fit the query builders. Multiple statements can be
    /// separated by `;`.
    ///
    /// This method does **not** return rows. For queries that produce results,
    /// use [`select`](Self::select) or the raw DuckDB connection obtained via
    /// `connection.inner()` (note: `inner` is `pub(crate)` and not part of the
    /// public API).
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] on any SQL error (syntax error,
    /// object not found, permission denied, etc.).
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakeConnection;
    ///
    /// let db = DuckLakeConnection::open_in_memory()?;
    /// db.execute("CREATE TABLE main.users (id BIGINT PRIMARY KEY, name VARCHAR)")?;
    /// db.execute("INSERT INTO main.users VALUES (1, 'Alice'); INSERT INTO main.users VALUES (2, 'Bob')")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn execute(&self, sql: &str) -> Result<(), DuckLakeError> {
        self.inner.execute_batch(sql)?;
        Ok(())
    }

    /// Begin a fluent SELECT query for table `T`.
    ///
    /// Returns a [`SelectBuilder`] on which you can chain `.filter()`,
    /// `.order_by()`, `.limit()`, `.group_by()`, `.at_snapshot()`, etc.,
    /// before calling one of the terminal methods:
    /// [`fetch_all`](SelectBuilder::fetch_all),
    /// [`fetch_one`](SelectBuilder::fetch_one),
    /// [`fetch_optional`](SelectBuilder::fetch_optional), or
    /// [`count`](SelectBuilder::count).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// let rows: Vec<Sale> = db
    ///     .select::<Sale>()
    ///     .filter(Sale::amount().gt(100.0))
    ///     .order_by(Sale::id().asc())
    ///     .limit(20)
    ///     .fetch_all()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn select<T: DuckLakeTable>(&self) -> SelectBuilder<'_, T> {
        SelectBuilder::new(self)
    }

    /// Begin a fluent INSERT for a single record of type `T`.
    ///
    /// Returns an [`InsertBuilder`]. Call `.execute()` to send the statement
    /// to DuckDB.
    ///
    /// For inserting multiple records efficiently, prefer
    /// [`insert_many`](Self::insert_many), which wraps all rows in a single
    /// transaction.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// db.insert(Sale { id: 1, amount: 99.9, region: "EU".into() }).execute()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn insert<T: DuckLakeTable>(&self, record: T) -> InsertBuilder<'_, T> {
        InsertBuilder::new(self, record)
    }

    /// Begin a fluent bulk INSERT for multiple records of type `T`.
    ///
    /// Returns a [`BulkInsertBuilder`]. Call `.execute()` to wrap all rows
    /// in a single `BEGIN` / `COMMIT` transaction, which is significantly
    /// faster than calling [`insert`](Self::insert) in a loop.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// let rows = vec![
    ///     Sale { id: 1, amount: 10.0, region: "EU".into() },
    ///     Sale { id: 2, amount: 20.0, region: "US".into() },
    /// ];
    /// let inserted = db.insert_many(rows).execute()?;
    /// assert_eq!(inserted, 2);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn insert_many<T: DuckLakeTable>(&self, records: Vec<T>) -> BulkInsertBuilder<'_, T> {
        BulkInsertBuilder::new(self, records)
    }

    /// Begin a fluent UPDATE query for table `T`.
    ///
    /// Returns an [`UpdateBuilder`]. You must call at least one `.set()` and
    /// at least one `.filter()` before calling `.execute()` — otherwise an
    /// `Err(DuckLakeError::Query(…))` is returned to prevent accidental
    /// full-table overwrites.
    ///
    /// To intentionally update every row, use `.update_all()` instead of
    /// `.execute()`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// // Update a single row:
    /// db.update::<Sale>()
    ///     .set(Sale::region(), "EMEA")
    ///     .filter(Sale::id().eq(1i64))
    ///     .execute()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn update<T: DuckLakeTable>(&self) -> UpdateBuilder<'_, T> {
        UpdateBuilder::new(self)
    }

    /// Begin a fluent DELETE query for table `T`.
    ///
    /// Returns a [`DeleteBuilder`]. You must call at least one `.filter()`
    /// before calling `.execute()` — otherwise an
    /// `Err(DuckLakeError::Query(…))` is returned to prevent accidental
    /// full-table deletes.
    ///
    /// To intentionally delete every row, use `.delete_all()` instead of
    /// `.execute()`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// db.delete::<Sale>()
    ///     .filter(Sale::region().eq("EU"))
    ///     .execute()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn delete<T: DuckLakeTable>(&self) -> DeleteBuilder<'_, T> {
        DeleteBuilder::new(self)
    }

    /// Execute a raw SQL `SELECT` and deserialise the result rows into `T`.
    ///
    /// This is the escape hatch for queries that the fluent builders cannot
    /// express — window functions, CTEs, subqueries, or complex joins.
    /// The SQL must produce columns in exactly the same order as
    /// [`DuckLakeTable::column_names`] (i.e., the same order as the struct
    /// fields), because rows are deserialised by positional index.
    ///
    /// Parameters are positional `$1`, `$2`, … placeholders bound from
    /// `params`. Pass an empty slice `&[]` for queries with no parameters.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if the SQL is invalid or if a value
    /// cannot be converted to the expected Rust type.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    /// use ducklake_orm::query::SqlValue;
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// let db = DuckLakeConnection::open_in_memory()?;
    ///
    /// // Complex query the builder can't express:
    /// let rows: Vec<Sale> = db.select_raw::<Sale>(
    ///     "SELECT id, amount, region FROM main.sales
    ///      WHERE region = $1
    ///      ORDER BY amount DESC
    ///      LIMIT 5",
    ///     &[SqlValue::Text("EU".into())],
    /// )?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn select_raw<T: DuckLakeTable>(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<T>, DuckLakeError> {
        let mut stmt = self.inner.prepare(sql)?;
        let refs = params_to_refs(params);
        let rows = stmt.query_map(refs.as_slice(), |row| T::from_row(row))?;
        rows.map(|r| r.map_err(DuckLakeError::Duckdb)).collect()
    }

    pub(crate) fn catalog(&self) -> Option<&str> {
        self.catalog.as_deref()
    }

    pub(crate) fn inner(&self) -> &duckdb::Connection {
        &self.inner
    }
}
