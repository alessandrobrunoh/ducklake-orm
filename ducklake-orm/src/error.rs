//! Error type returned by every fallible API in `ducklake-orm`.

use thiserror::Error;

/// The unified error type for the `ducklake-orm` crate.
///
/// Every public method that can fail returns `Result<T, DuckLakeError>`.
/// This enum covers the full set of failure modes: database engine errors,
/// pool exhaustion, bad configuration, query-builder misuse, missing rows,
/// and I/O problems when reading configuration files.
///
/// ## Matching errors
///
/// Use a `match` expression to react to specific variants:
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, DuckLakeError, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "t")] struct T { id: i64 }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// match db.select::<T>().filter(T::id().eq(99i64)).fetch_one() {
///     Ok(row) => println!("found: {:?}", row),
///     Err(DuckLakeError::NotFound) => println!("no such row"),
///     Err(e) => eprintln!("unexpected error: {e}"),
/// }
/// ```
#[derive(Debug, Error)]
pub enum DuckLakeError {
    /// An error originating in the underlying DuckDB engine.
    ///
    /// This wraps [`duckdb::Error`] and is returned whenever DuckDB itself
    /// reports a problem — for example, a SQL syntax error, a constraint
    /// violation, a type mismatch in `FROM ROW`, or a failed extension load.
    #[error("DuckDB error: {0}")]
    Duckdb(#[from] duckdb::Error),

    /// The connection pool could not satisfy a [`DuckLakePool::get`] request.
    ///
    /// Common causes:
    /// - The pool has reached its maximum size and every connection is in use.
    /// - The `connection_timeout_secs` deadline expired before a connection
    ///   became available (see [`config::PoolConfig`](crate::config::PoolConfig)).
    ///
    /// To increase tolerance, raise `pool.size` or `pool.connection_timeout_secs`
    /// in `ducklake.toml`.
    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    /// The `ducklake.toml` file could not be parsed.
    ///
    /// The inner `String` contains the TOML parsing error message.
    /// This is returned by [`DuckLakeConfig::from_file`](crate::config::DuckLakeConfig::from_file)
    /// and [`DuckLakeConfig::from_toml`](crate::config::DuckLakeConfig::from_toml).
    #[error("Configuration error: {0}")]
    Config(String),

    /// A query builder was used incorrectly.
    ///
    /// This variant enforces the safety guardrails built into `UpdateBuilder`
    /// and `DeleteBuilder`:
    ///
    /// - Calling [`UpdateBuilder::execute`](crate::query::UpdateBuilder::execute)
    ///   without at least one `.set()` and one `.filter()`.
    /// - Calling [`DeleteBuilder::execute`](crate::query::DeleteBuilder::execute)
    ///   without at least one `.filter()`.
    ///
    /// The inner `String` describes which precondition was violated.
    /// Use the explicit escape-hatch methods (`.update_all()`, `.delete_all()`)
    /// when you genuinely want to modify or delete every row.
    #[error("Query builder error: {0}")]
    Query(String),

    /// A row was expected but the query returned zero results.
    ///
    /// This is returned by [`SelectBuilder::fetch_one`](crate::query::SelectBuilder::fetch_one)
    /// when no row matches the applied filters. Use
    /// [`fetch_optional`](crate::query::SelectBuilder::fetch_optional) instead
    /// if the absence of a row is a normal case in your application.
    #[error("No rows found")]
    NotFound,

    /// A file I/O error occurred.
    ///
    /// Currently only returned when reading the `ducklake.toml` configuration
    /// file via [`DuckLakeConfig::from_file`](crate::config::DuckLakeConfig::from_file).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
