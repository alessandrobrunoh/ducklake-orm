//! Fluent UPDATE query builder.

use std::marker::PhantomData;

use crate::{
    connection::DuckLakeConnection,
    error::DuckLakeError,
    pool::PooledConnection,
    query::{
        filter::{ColumnExpr, FilterExpr, SqlValue, build_where_clause, params_to_refs},
        insert::table_ref,
    },
    schema::DuckLakeTable,
};

/// Fluent builder for `UPDATE` queries.
///
/// Create one with [`DuckLakeConnection::update`](crate::connection::DuckLakeConnection::update)
/// or [`PooledConnection::update`](crate::pool::PooledConnection::update), then:
///
/// 1. Chain one or more [`.set(col, value)`](Self::set) calls.
/// 2. Chain one or more [`.filter(expr)`](Self::filter) calls.
/// 3. Call [`.execute()`](Self::execute) to run the update.
///
/// ## Safety guardrail
///
/// [`.execute()`](Self::execute) enforces two preconditions:
///
/// - **At least one `.set()` call** — updating zero columns is a programmer
///   error and probably a mistake.
/// - **At least one `.filter()` call** — updating every row in a table
///   accidentally is a dangerous and common mistake. The ORM returns
///   `Err(DuckLakeError::Query(…))` unless you explicitly use
///   [`.update_all()`](Self::update_all) to signal intent.
///
/// ## Generated SQL
///
/// ```sql
/// UPDATE [catalog.]schema.table
/// SET col1 = $1, col2 = $2
/// WHERE filter_col = $3
/// ```
///
/// ## Example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// // Update a single field on one row:
/// db.update::<Sale>()
///     .set(Sale::region(), "EMEA")
///     .filter(Sale::id().eq(1i64))
///     .execute()?;
///
/// // Update multiple fields:
/// db.update::<Sale>()
///     .set(Sale::region(), "APAC")
///     .set(Sale::amount(), 0.0)
///     .filter(Sale::amount().lt(0.0))
///     .execute()?;
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct UpdateBuilder<'conn, T: DuckLakeTable> {
    raw: &'conn duckdb::Connection,
    catalog: Option<String>,
    sets: Vec<(&'static str, SqlValue)>,
    filters: Vec<FilterExpr>,
    _marker: PhantomData<T>,
}

impl<'conn, T: DuckLakeTable> UpdateBuilder<'conn, T> {
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
            sets: Vec::new(),
            filters: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Assign a new value to a column.
    ///
    /// `col` must be a column accessor returned by one of the methods generated
    /// by `#[derive(Table)]` — for example `Sale::amount()`. `val` can be any
    /// type that implements `Into<SqlValue>` (`i64`, `f64`, `String`, `&str`,
    /// `bool`, etc.).
    ///
    /// Multiple calls to `.set()` accumulate assignments and are all applied in
    /// a single `SET col1 = $1, col2 = $2, …` clause.
    ///
    /// ```rust,ignore
    /// .set(Sale::region(), "EMEA")          // region = 'EMEA'
    /// .set(Sale::amount(), 0.0)             // amount = 0.0
    /// ```
    pub fn set<V: Into<SqlValue>>(mut self, col: ColumnExpr, val: V) -> Self {
        self.sets.push((col.name, val.into()));
        self
    }

    /// Add a `WHERE` predicate. Multiple calls are AND-ed together.
    ///
    /// At least one call is required before [`.execute()`](Self::execute); if
    /// you omit it you will get `Err(DuckLakeError::Query(…))`. This prevents
    /// accidentally updating every row in the table.
    ///
    /// ```rust,ignore
    /// .filter(Sale::id().eq(1i64))
    /// .filter(Sale::region().eq("EU"))    // both conditions must be true
    /// ```
    pub fn filter(mut self, expr: FilterExpr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Execute the `UPDATE … SET … WHERE …` statement.
    ///
    /// # Returns
    ///
    /// The number of rows that were modified.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Query`] — if no `.set()` or no `.filter()` was called.
    /// - [`DuckLakeError::Duckdb`] — if DuckDB rejects the statement (constraint
    ///   violation, type mismatch, etc.).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, DuckLakeError, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // ✅ correct usage
    /// db.update::<Sale>()
    ///     .set(Sale::amount(), 999.0)
    ///     .filter(Sale::id().eq(1i64))
    ///     .execute()?;
    ///
    /// // ❌ missing filter → Err(DuckLakeError::Query)
    /// let err = db.update::<Sale>().set(Sale::amount(), 0.0).execute();
    /// assert!(matches!(err, Err(DuckLakeError::Query(_))));
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        if self.sets.is_empty() {
            return Err(DuckLakeError::Query(
                "UPDATE requires at least one .set() call".into(),
            ));
        }
        if self.filters.is_empty() {
            return Err(DuckLakeError::Query(
                "UPDATE requires at least one .filter() to prevent full-table overwrite; \
                 call .update_all() if you intentionally want to update every row"
                    .into(),
            ));
        }

        let tref = table_ref::<T>(self.catalog.as_deref());
        let mut params: Vec<SqlValue> = Vec::new();

        let set_clause = self
            .sets
            .iter()
            .enumerate()
            .map(|(i, (col, val))| {
                params.push(val.clone());
                format!("{col} = ${}", i + 1)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let (where_clause, where_params) = build_where_clause(&self.filters, params.len() + 1);
        params.extend(where_params);

        let sql = format!("UPDATE {tref} SET {set_clause} WHERE {where_clause}");
        let refs = params_to_refs(&params);
        Ok(self.raw.execute(&sql, refs.as_slice())?)
    }

    /// Execute an `UPDATE … SET …` statement **without a `WHERE` clause**,
    /// modifying every row in the table.
    ///
    /// This is the explicit escape hatch for intentional full-table updates. It
    /// is named differently from [`.execute()`](Self::execute) so that you cannot
    /// call it by accident.
    ///
    /// # Returns
    ///
    /// The number of rows modified (equal to the total row count of the table).
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Query`] — if no `.set()` was called.
    /// - [`DuckLakeError::Duckdb`] — on SQL errors.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // Set region = 'GLOBAL' on every row:
    /// db.update::<Sale>()
    ///     .set(Sale::region(), "GLOBAL")
    ///     .update_all()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn update_all(self) -> Result<usize, DuckLakeError> {
        if self.sets.is_empty() {
            return Err(DuckLakeError::Query(
                "UPDATE requires at least one .set() call".into(),
            ));
        }

        let tref = table_ref::<T>(self.catalog.as_deref());
        let mut params: Vec<SqlValue> = Vec::new();

        let set_clause = self
            .sets
            .iter()
            .enumerate()
            .map(|(i, (col, val))| {
                params.push(val.clone());
                format!("{col} = ${}", i + 1)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!("UPDATE {tref} SET {set_clause}");
        let refs = params_to_refs(&params);
        Ok(self.raw.execute(&sql, refs.as_slice())?)
    }
}
