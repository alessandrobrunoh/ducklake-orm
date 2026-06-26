//! Connection pool for DuckLake / DuckDB.
//!
//! [`DuckLakePool`] maintains a fixed-size pool of [`duckdb::Connection`]s
//! using the battle-tested [`r2d2`] library. Calling [`DuckLakePool::get`]
//! checks out one connection; it is returned to the pool automatically when
//! the returned [`PooledConnection`] is dropped.
//!
//! ## When to use a pool
//!
//! DuckDB supports multiple concurrent **readers** on the same file.
//! A pool lets your application saturate all available parallelism without
//! paying the cost of opening and closing connections on every request.
//!
//! For single-threaded scripts and tests, [`DuckLakeConnection`] is simpler.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConfig, DuckLakePool, Table};
//!
//! # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
//! let config = DuckLakeConfig::from_file("ducklake.toml")?;
//! let pool   = DuckLakePool::from_config(&config)?;
//!
//! // Use the pool from multiple threads:
//! let rows = {
//!     let conn = pool.get()?;  // blocks until a connection is available
//!     conn.select::<Sale>().fetch_all()?
//! };  // ← conn is returned to the pool here
//! # Ok::<(), ducklake_orm::DuckLakeError>(())
//! ```
//!
//! [`DuckLakeConnection`]: crate::connection::DuckLakeConnection

use std::time::Duration;

use duckdb::DuckdbConnectionManager;
use r2d2::Pool;

use crate::{
    config::{DuckLakeAttachConfig, DuckLakeConfig, PoolConfig},
    error::DuckLakeError,
    query::{
        BulkInsertBuilder, DeleteBuilder, InsertBuilder, SelectBuilder, SqlValue, UpdateBuilder,
        params_to_refs,
    },
    schema::DuckLakeTable,
};

/// An r2d2-backed connection pool for DuckLake / DuckDB.
///
/// Create a pool with [`DuckLakePool::open`] (supplying path and pool config
/// directly) or with [`DuckLakePool::from_config`] (reading from a
/// [`DuckLakeConfig`]).
///
/// Each call to [`get`](Self::get) returns a [`PooledConnection`] that exposes
/// the same ORM API as [`DuckLakeConnection`](crate::connection::DuckLakeConnection).
/// The connection is returned to the pool when the [`PooledConnection`] is
/// dropped (end of scope, `?` propagation, etc.).
///
/// [`DuckLakePool`] is `Send + Sync` and can be shared across threads via an
/// `Arc<DuckLakePool>`.
///
/// ## Example — sharing across threads
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use ducklake_orm::{DuckLakePool, Table};
/// use ducklake_orm::config::PoolConfig;
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// let pool = Arc::new(DuckLakePool::open(":memory:", &PoolConfig::default())?);
///
/// let handles: Vec<_> = (0..4).map(|_| {
///     let pool = Arc::clone(&pool);
///     std::thread::spawn(move || {
///         let conn = pool.get().expect("pool exhausted");
///         conn.select::<Sale>().count().expect("query failed")
///     })
/// }).collect();
///
/// for h in handles { h.join().unwrap(); }
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct DuckLakePool {
    inner: Pool<DuckdbConnectionManager>,
    ducklake: Option<DuckLakeAttachConfig>,
}

impl DuckLakePool {
    /// Open a connection pool to a DuckDB database file (or `":memory:"`).
    ///
    /// This is the low-level constructor. You supply the database path and pool
    /// settings directly without a `ducklake.toml` file. No DuckLake catalog
    /// is attached; use [`from_config`](Self::from_config) or call
    /// [`PooledConnection::execute`] with raw `ATTACH` SQL if you need
    /// DuckLake support.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Duckdb`] — if DuckDB cannot open the file.
    /// - [`DuckLakeError::Pool`] — if r2d2 cannot build the pool (e.g., initial
    ///   connection test fails).
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakePool;
    /// use ducklake_orm::config::PoolConfig;
    ///
    /// let pool = DuckLakePool::open(":memory:", &PoolConfig::default())?;
    /// let conn = pool.get()?;
    /// conn.execute("SELECT 1")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn open(path: &str, pool_cfg: &PoolConfig) -> Result<Self, DuckLakeError> {
        let manager = DuckdbConnectionManager::file(path)?;
        let pool = Pool::builder()
            .max_size(pool_cfg.size)
            .connection_timeout(Duration::from_secs(pool_cfg.connection_timeout_secs))
            .build(manager)?;
        Ok(Self {
            inner: pool,
            ducklake: None,
        })
    }

    /// Build a connection pool from a fully parsed [`DuckLakeConfig`].
    ///
    /// This is the recommended constructor for production. It reads the database
    /// path, pool size, timeout, and optional DuckLake catalog settings all from
    /// the config struct (which is typically loaded from `ducklake.toml`).
    ///
    /// If `config.ducklake` is set and `auto_attach` is `true`, every connection
    /// checked out of the pool will automatically run:
    /// ```sql
    /// INSTALL ducklake; LOAD ducklake;
    /// ATTACH '<catalog_path>' AS <catalog_name> (TYPE DUCKLAKE);
    /// ```
    ///
    /// # Errors
    ///
    /// Same as [`open`](Self::open).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConfig, DuckLakePool};
    ///
    /// let config = DuckLakeConfig::from_file("ducklake.toml")?;
    /// let pool   = DuckLakePool::from_config(&config)?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn from_config(cfg: &DuckLakeConfig) -> Result<Self, DuckLakeError> {
        let manager = DuckdbConnectionManager::file(&cfg.database.path)?;
        let pool = Pool::builder()
            .max_size(cfg.pool.size)
            .connection_timeout(Duration::from_secs(cfg.pool.connection_timeout_secs))
            .build(manager)?;
        Ok(Self {
            inner: pool,
            ducklake: cfg.ducklake.clone(),
        })
    }

    /// Check out a connection from the pool.
    ///
    /// If a connection is available it is returned immediately. If all
    /// connections are in use, this call blocks until either one becomes
    /// available or the `connection_timeout_secs` deadline expires (configured
    /// in [`PoolConfig`](crate::config::PoolConfig)).
    ///
    /// The returned [`PooledConnection`] holds a RAII guard: when it is
    /// dropped the underlying `duckdb::Connection` is returned to the pool
    /// and can be reused by the next caller.
    ///
    /// If `auto_attach` is enabled (see [`DuckLakeAttachConfig`](crate::config::DuckLakeAttachConfig)),
    /// the DuckLake catalog is attached to the connection before it is handed
    /// to the caller.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Pool`] — if the timeout expired before a connection
    ///   became available.
    /// - [`DuckLakeError::Duckdb`] — if the `auto_attach` SQL fails (extension
    ///   not found, catalog file unreachable, etc.).
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakePool;
    /// use ducklake_orm::config::PoolConfig;
    ///
    /// let pool = DuckLakePool::open(":memory:", &PoolConfig::default())?;
    /// {
    ///     let conn = pool.get()?;
    ///     conn.execute("SELECT 42")?;
    /// } // ← connection returned to pool here
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn get(&self) -> Result<PooledConnection<'_>, DuckLakeError> {
        let conn = self.inner.get()?;

        if let Some(dl) = &self.ducklake {
            if dl.auto_attach {
                conn.execute_batch("INSTALL ducklake; LOAD ducklake;")?;
                conn.execute(
                    &format!(
                        "ATTACH '{}' AS {} (TYPE DUCKLAKE)",
                        dl.catalog_path, dl.catalog_name
                    ),
                    [],
                )?;
            }
        }

        Ok(PooledConnection {
            inner: conn,
            catalog: self.ducklake.as_ref().map(|dl| dl.catalog_name.clone()),
            _marker: std::marker::PhantomData,
        })
    }
}

/// A single connection checked out from a [`DuckLakePool`].
///
/// `PooledConnection` exposes the same query API as
/// [`DuckLakeConnection`](crate::connection::DuckLakeConnection):
/// `select`, `insert`, `insert_many`, `update`, `delete`, and `execute`.
///
/// ## Lifetime and RAII
///
/// The `'pool` lifetime ties this value to the pool it came from. When
/// `PooledConnection` is dropped — end of scope, moved out of scope, or
/// propagated via `?` — the underlying [`duckdb::Connection`] is returned
/// to the pool transparently.
///
/// You should keep a `PooledConnection` for the shortest time needed to
/// complete a unit of work (e.g., a single HTTP request handler or a single
/// background job iteration).
///
/// ## Example
///
/// ```rust
/// use ducklake_orm::{DuckLakePool, Table};
/// use ducklake_orm::config::PoolConfig;
///
/// # #[derive(Table, Debug)] #[ducklake(table = "t")] struct T { id: i64 }
/// let pool = DuckLakePool::open(":memory:", &PoolConfig::default())?;
///
/// let count = {
///     let conn = pool.get()?;
///     conn.execute("CREATE TABLE main.t (id BIGINT)")?;
///     conn.select::<T>().count()?
/// };
/// assert_eq!(count, 0);
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct PooledConnection<'pool> {
    pub(crate) inner: r2d2::PooledConnection<DuckdbConnectionManager>,
    pub(crate) catalog: Option<String>,
    pub(crate) _marker: std::marker::PhantomData<&'pool ()>,
}

impl<'pool> PooledConnection<'pool> {
    /// Execute one or more raw SQL statements.
    ///
    /// Identical in behaviour to
    /// [`DuckLakeConnection::execute`](crate::connection::DuckLakeConnection::execute).
    /// Useful for DDL or any statement not covered by the query builders.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] on any SQL error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakePool;
    /// use ducklake_orm::config::PoolConfig;
    ///
    /// let pool = DuckLakePool::open(":memory:", &PoolConfig::default())?;
    /// let conn = pool.get()?;
    /// conn.execute("CREATE TABLE main.log (msg VARCHAR)")?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn execute(&self, sql: &str) -> Result<(), DuckLakeError> {
        self.inner.execute_batch(sql)?;
        Ok(())
    }

    /// Begin a fluent SELECT query for table `T`.
    ///
    /// See [`DuckLakeConnection::select`](crate::connection::DuckLakeConnection::select)
    /// for full documentation and examples.
    pub fn select<T: DuckLakeTable>(&self) -> SelectBuilder<'_, T> {
        SelectBuilder::new_pooled(self)
    }

    /// Begin a fluent INSERT for a single record of type `T`.
    ///
    /// See [`DuckLakeConnection::insert`](crate::connection::DuckLakeConnection::insert)
    /// for full documentation and examples.
    pub fn insert<T: DuckLakeTable>(&self, record: T) -> InsertBuilder<'_, T> {
        InsertBuilder::new_pooled(self, record)
    }

    /// Begin a fluent bulk INSERT for multiple records of type `T`.
    ///
    /// All rows are wrapped in a single transaction. See
    /// [`DuckLakeConnection::insert_many`](crate::connection::DuckLakeConnection::insert_many)
    /// for full documentation and examples.
    pub fn insert_many<T: DuckLakeTable>(&self, records: Vec<T>) -> BulkInsertBuilder<'_, T> {
        BulkInsertBuilder::new_pooled(self, records)
    }

    /// Begin a fluent UPDATE query for table `T`.
    ///
    /// A filter is required before calling `.execute()`. See
    /// [`DuckLakeConnection::update`](crate::connection::DuckLakeConnection::update)
    /// for full documentation and examples.
    pub fn update<T: DuckLakeTable>(&self) -> UpdateBuilder<'_, T> {
        UpdateBuilder::new_pooled(self)
    }

    /// Begin a fluent DELETE query for table `T`.
    ///
    /// A filter is required before calling `.execute()`. See
    /// [`DuckLakeConnection::delete`](crate::connection::DuckLakeConnection::delete)
    /// for full documentation and examples.
    pub fn delete<T: DuckLakeTable>(&self) -> DeleteBuilder<'_, T> {
        DeleteBuilder::new_pooled(self)
    }

    /// Execute a raw SQL `SELECT` and deserialise the result rows into `T`.
    ///
    /// See [`DuckLakeConnection::select_raw`](crate::connection::DuckLakeConnection::select_raw)
    /// for full documentation and examples.
    pub fn select_raw<T: DuckLakeTable>(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<T>, DuckLakeError> {
        use std::ops::Deref;
        let mut stmt = self.inner.deref().prepare(sql)?;
        let refs = params_to_refs(params);
        let rows = stmt.query_map(refs.as_slice(), |row| T::from_row(row))?;
        rows.map(|r| r.map_err(DuckLakeError::Duckdb)).collect()
    }
}
