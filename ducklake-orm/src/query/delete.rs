//! Fluent DELETE query builder.

use std::marker::PhantomData;

use crate::{
    connection::DuckLakeConnection,
    error::DuckLakeError,
    pool::PooledConnection,
    query::{
        filter::{FilterExpr, SqlValue, build_where_clause, params_to_refs},
        insert::table_ref,
    },
    schema::DuckLakeTable,
};

/// Fluent builder for `DELETE` queries.
///
/// Create one with [`DuckLakeConnection::delete`](crate::connection::DuckLakeConnection::delete)
/// or [`PooledConnection::delete`](crate::pool::PooledConnection::delete), then:
///
/// 1. Chain one or more [`.filter(expr)`](Self::filter) calls to narrow the
///    rows to be deleted.
/// 2. Call [`.execute()`](Self::execute) to run the statement.
///
/// ## Safety guardrail
///
/// [`.execute()`](Self::execute) returns `Err(DuckLakeError::Query(…))` if no
/// filter has been added, preventing accidental deletion of every row in the
/// table. Use [`.delete_all()`](Self::delete_all) when a full-table delete is
/// intentional.
///
/// ## Generated SQL
///
/// ```sql
/// DELETE FROM [catalog.]schema.table WHERE col = $1 [AND …]
/// ```
///
/// ## Example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// // Delete one row by primary key:
/// db.delete::<Sale>()
///     .filter(Sale::id().eq(1i64))
///     .execute()?;
///
/// // Delete all EU rows:
/// db.delete::<Sale>()
///     .filter(Sale::region().eq("EU"))
///     .execute()?;
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct DeleteBuilder<'conn, T: DuckLakeTable> {
    raw: &'conn duckdb::Connection,
    catalog: Option<String>,
    filters: Vec<FilterExpr>,
    _marker: PhantomData<T>,
}

impl<'conn, T: DuckLakeTable> DeleteBuilder<'conn, T> {
    pub(crate) fn new(src: &'conn DuckLakeConnection) -> Self {
        Self::from_raw(src.inner(), src.catalog().map(str::to_owned))
    }

    pub(crate) fn new_pooled(src: &'conn PooledConnection<'_>) -> Self {
        use std::ops::Deref;
        Self::from_raw(src.inner.deref(), src.catalog.clone())
    }

    fn from_raw(raw: &'conn duckdb::Connection, catalog: Option<String>) -> Self {
        Self {
            raw,
            catalog,
            filters: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Add a `WHERE` predicate. Multiple calls are AND-ed together.
    ///
    /// At least one call is required before [`.execute()`](Self::execute).
    /// If you omit a filter you will receive `Err(DuckLakeError::Query(…))`.
    ///
    /// ```rust,ignore
    /// .filter(Sale::region().eq("EU"))
    /// .filter(Sale::amount().lt(0.0))  // AND-ed: both must be true
    /// ```
    pub fn filter(mut self, expr: FilterExpr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Execute the `DELETE FROM … WHERE …` statement.
    ///
    /// # Returns
    ///
    /// The number of rows deleted.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Query`] — if no `.filter()` was called (safety guardrail).
    /// - [`DuckLakeError::Duckdb`] — if DuckDB rejects the statement (e.g.,
    ///   a foreign-key constraint prevents deletion).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, DuckLakeError, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // ✅ correct usage
    /// let deleted = db.delete::<Sale>().filter(Sale::id().eq(1i64)).execute()?;
    ///
    /// // ❌ missing filter → Err(DuckLakeError::Query)
    /// let err = db.delete::<Sale>().execute();
    /// assert!(matches!(err, Err(DuckLakeError::Query(_))));
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        if self.filters.is_empty() {
            return Err(DuckLakeError::Query(
                "DELETE requires at least one .filter() to prevent full-table deletion; \
                 call .delete_all() if you intentionally want to delete every row"
                    .into(),
            ));
        }

        let tref = table_ref::<T>(self.catalog.as_deref());
        let mut params: Vec<SqlValue> = Vec::new();
        let (where_clause, where_params) = build_where_clause(&self.filters, 1);
        params.extend(where_params);

        let sql = format!("DELETE FROM {tref} WHERE {where_clause}");
        let refs = params_to_refs(&params);
        Ok(self.raw.execute(&sql, refs.as_slice())?)
    }

    /// Execute a `DELETE FROM …` statement **without a `WHERE` clause**,
    /// removing every row from the table.
    ///
    /// This is the explicit escape hatch for intentional full-table deletes.
    /// It is named differently from [`.execute()`](Self::execute) so that it
    /// cannot be called by accident.
    ///
    /// # Returns
    ///
    /// The number of rows deleted (equal to the total row count of the table
    /// before the operation).
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] on SQL errors.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // Wipe the entire table:
    /// let deleted = db.delete::<Sale>().delete_all()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn delete_all(self) -> Result<usize, DuckLakeError> {
        let tref = table_ref::<T>(self.catalog.as_deref());
        let sql = format!("DELETE FROM {tref}");
        Ok(self.raw.execute(&sql, [])?)
    }
}
