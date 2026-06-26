//! Fluent SELECT query builder.

use std::marker::PhantomData;

use crate::{
    connection::DuckLakeConnection,
    error::DuckLakeError,
    pool::PooledConnection,
    query::filter::{
        ColumnExpr, FilterExpr, OrderExpr, SqlValue, build_where_clause, params_to_refs,
    },
    schema::DuckLakeTable,
};

/// A DuckLake-specific time travel reference — selects a historical state of the table.
///
/// Pass a `SnapshotRef` to [`SelectBuilder::at_snapshot`] or
/// [`SelectBuilder::at_timestamp`] to read the table as it was at a particular
/// snapshot version or point in time.
///
/// ## DuckLake `AT` syntax
///
/// DuckLake translates these variants into the `AT (…)` clause:
///
/// | Variant | Generated SQL |
/// |---------|--------------|
/// | `Version(42)` | `… AT (VERSION => 42)` |
/// | `Timestamp("2025-01-01T00:00:00Z")` | `… AT (TIMESTAMP => '2025-01-01T00:00:00Z')` |
///
/// You do not construct `SnapshotRef` directly; use the builder methods instead.
#[derive(Debug, Clone)]
pub enum SnapshotRef {
    /// Read the table at a specific, integer snapshot version.
    ///
    /// DuckLake assigns an incrementing version number to every transaction
    /// that modifies the table. Use `Version(n)` to read the state of the
    /// table **after** transaction `n` was committed.
    Version(u64),
    /// Read the table as it existed at a specific point in time.
    ///
    /// The inner string must be a timestamp literal understood by DuckDB,
    /// such as an ISO 8601 string `"2025-06-01T12:00:00Z"`.
    Timestamp(String),
}

/// Fluent builder for `SELECT` queries.
///
/// Create one with [`DuckLakeConnection::select`](crate::connection::DuckLakeConnection::select)
/// or [`PooledConnection::select`](crate::pool::PooledConnection::select), then
/// chain zero or more of the modifier methods, and finish with one of the
/// terminal execution methods.
///
/// ## Method chaining
///
/// All modifier methods consume `self` and return a new `SelectBuilder`, so the
/// builder can be stored in a variable and reused up to the point where a
/// terminal method is called:
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// let base = db.select::<Sale>().filter(Sale::region().eq("EU"));
///
/// // Reuse the partially-built query:
/// let count = base.count().unwrap();
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
///
/// ## Generated SQL shape
///
/// ```text
/// SELECT col1, col2, …
/// FROM [catalog.]schema.table [AT (VERSION => n | TIMESTAMP => 'ts')]
/// [WHERE …]
/// [GROUP BY …]
/// [HAVING …]
/// [ORDER BY …]
/// [LIMIT n]
/// [OFFSET n]
/// ```
///
/// ## Full example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// let rows: Vec<Sale> = db
///     .select::<Sale>()
///     .filter(Sale::amount().gt(100.0))
///     .filter(Sale::region().eq("EU"))     // additional filters are AND-ed
///     .order_by(Sale::amount().desc())
///     .limit(10)
///     .offset(20)
///     .fetch_all()
///     .unwrap();
/// ```
pub struct SelectBuilder<'conn, T: DuckLakeTable> {
    raw: &'conn duckdb::Connection,
    catalog: Option<String>,
    filters: Vec<FilterExpr>,
    group_by: Vec<&'static str>,
    having: Vec<FilterExpr>,
    order_by: Vec<OrderExpr>,
    limit: Option<usize>,
    offset: Option<usize>,
    snapshot: Option<SnapshotRef>,
    _marker: PhantomData<T>,
}

impl<'conn, T: DuckLakeTable> SelectBuilder<'conn, T> {
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
            group_by: Vec::new(),
            having: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            snapshot: None,
            _marker: PhantomData,
        }
    }

    // ── WHERE ────────────────────────────────────────────────────────────────

    /// Add a `WHERE` predicate.
    ///
    /// Multiple calls to `.filter()` are **AND-ed** together. To combine
    /// predicates with `OR`, use [`FilterExpr::or`](crate::query::FilterExpr::or)
    /// before passing the expression:
    ///
    /// ```rust,ignore
    /// // AND: both conditions must be true
    /// .filter(Sale::amount().gt(100.0))
    /// .filter(Sale::region().eq("EU"))
    ///
    /// // OR: either condition is sufficient
    /// .filter(Sale::region().eq("EU").or(Sale::region().eq("UK")))
    /// ```
    pub fn filter(mut self, expr: FilterExpr) -> Self {
        self.filters.push(expr);
        self
    }

    // ── GROUP BY / HAVING ────────────────────────────────────────────────────

    /// Add a `GROUP BY` column by name.
    ///
    /// Use the bare column name string (same as the struct field name), for
    /// example `"region"`. Multiple calls append columns in order:
    ///
    /// ```rust,ignore
    /// .group_by("region")
    /// .group_by("status")
    /// // SQL: GROUP BY region, status
    /// ```
    ///
    /// ## Note on the return struct
    ///
    /// When you use `GROUP BY`, `SELECT` typically returns aggregated rows that
    /// do not correspond 1:1 to the base table struct. Consider defining a
    /// separate "aggregation result" struct with `#[derive(Table)]` mapped to
    /// the same table, but containing only the grouped / aggregated columns.
    pub fn group_by(mut self, col: &'static str) -> Self {
        self.group_by.push(col);
        self
    }

    /// Add a `HAVING` predicate (applied **after** `GROUP BY` aggregation).
    ///
    /// Works exactly like [`filter`](Self::filter) but generates a `HAVING`
    /// clause instead of a `WHERE` clause. Multiple calls are AND-ed.
    ///
    /// ```rust,ignore
    /// .group_by("region")
    /// .having(Sale::amount().gt(500.0))
    /// // SQL: GROUP BY region HAVING amount > $1
    /// ```
    ///
    /// In practice, the column reference in a `HAVING` clause often refers to
    /// an aggregate expression (e.g., `SUM(amount)`). Because the ORM does
    /// not yet support aggregate expressions in `HAVING`, complex cases should
    /// be handled with a raw `execute` call.
    pub fn having(mut self, expr: FilterExpr) -> Self {
        self.having.push(expr);
        self
    }

    // ── ORDER BY ─────────────────────────────────────────────────────────────

    /// Add an `ORDER BY` clause.
    ///
    /// Pass an [`OrderExpr`](crate::query::OrderExpr) produced by
    /// [`ColumnExpr::asc`](crate::query::ColumnExpr::asc) or
    /// [`ColumnExpr::desc`](crate::query::ColumnExpr::desc). Multiple calls
    /// append columns in order:
    ///
    /// ```rust,ignore
    /// .order_by(Sale::region().asc())
    /// .order_by(Sale::amount().desc())
    /// // SQL: ORDER BY region ASC, amount DESC
    /// ```
    pub fn order_by(mut self, expr: OrderExpr) -> Self {
        self.order_by.push(expr);
        self
    }

    // ── LIMIT / OFFSET ───────────────────────────────────────────────────────

    /// Limit the number of rows returned.
    ///
    /// ```rust,ignore
    /// .limit(10)   // SQL: LIMIT 10
    /// ```
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Skip the first `n` rows (pagination offset).
    ///
    /// ```rust,ignore
    /// .limit(10).offset(20)   // SQL: LIMIT 10 OFFSET 20  (page 3 of 10)
    /// ```
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    // ── DuckLake time travel ─────────────────────────────────────────────────

    /// Read the table at a specific DuckLake **snapshot version**.
    ///
    /// Generates `AT (VERSION => n)` in the SQL `FROM` clause. This requires
    /// that a DuckLake catalog has been attached (via
    /// [`DuckLakeConnection::attach_ducklake`](crate::connection::DuckLakeConnection::attach_ducklake)
    /// or `auto_attach = true` in the pool config).
    ///
    /// ```rust,ignore
    /// db.select::<Sale>().at_snapshot(42).fetch_all()
    /// // SQL: SELECT … FROM lake.main.sales AT (VERSION => 42)
    /// ```
    pub fn at_snapshot(mut self, version: u64) -> Self {
        self.snapshot = Some(SnapshotRef::Version(version));
        self
    }

    /// Read the table as it was at a specific **point in time**.
    ///
    /// Generates `AT (TIMESTAMP => 'ts')` in the SQL `FROM` clause.
    /// The `ts` argument must be a timestamp literal understood by DuckDB,
    /// such as an ISO 8601 string: `"2025-06-01T00:00:00Z"`.
    ///
    /// ```rust,ignore
    /// db.select::<Sale>().at_timestamp("2025-06-01T00:00:00Z").fetch_all()
    /// // SQL: SELECT … FROM lake.main.sales AT (TIMESTAMP => '2025-06-01T00:00:00Z')
    /// ```
    pub fn at_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.snapshot = Some(SnapshotRef::Timestamp(ts.into()));
        self
    }

    // ── internal SQL builder ─────────────────────────────────────────────────

    fn table_ref(&self) -> String {
        let schema = T::schema_name();
        let table = T::table_name();
        match self.catalog.as_deref() {
            Some(cat) => format!("{cat}.{schema}.{table}"),
            None => format!("{schema}.{table}"),
        }
    }

    fn build(&self) -> (String, Vec<SqlValue>) {
        let cols = T::column_names().join(", ");
        let table = self.table_ref();

        let at = match &self.snapshot {
            Some(SnapshotRef::Version(v)) => format!(" AT (VERSION => {v})"),
            Some(SnapshotRef::Timestamp(ts)) => {
                // DuckDB does not accept a bind parameter inside an `AT`
                // clause, so the timestamp must be interpolated as a string
                // literal. Escape `'` (and `\` for defence-in-depth) to
                // neutralise any attempt to break out of the literal and
                // inject SQL.
                let escaped = crate::ident::escape_sql_string(ts);
                format!(" AT (TIMESTAMP => '{escaped}')")
            }
            None => String::new(),
        };

        let mut sql = format!("SELECT {cols} FROM {table}{at}");
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.filters.is_empty() {
            let (clause, p) = build_where_clause(&self.filters, 1);
            sql.push_str(" WHERE ");
            sql.push_str(&clause);
            params.extend(p);
        }

        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        if !self.having.is_empty() {
            let (clause, p) = build_where_clause(&self.having, params.len() + 1);
            sql.push_str(" HAVING ");
            sql.push_str(&clause);
            params.extend(p);
        }

        if !self.order_by.is_empty() {
            let parts: Vec<_> = self.order_by.iter().map(|o| o.to_sql_fragment()).collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&parts.join(", "));
        }

        if let Some(n) = self.limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }
        if let Some(n) = self.offset {
            sql.push_str(&format!(" OFFSET {n}"));
        }

        (sql, params)
    }

    // ── Terminal execution methods ────────────────────────────────────────────

    /// Execute the query and return **all** matching rows.
    ///
    /// Executes `SELECT col1, col2, … FROM … WHERE … ORDER BY … LIMIT …`
    /// and deserialises each row with [`DuckLakeTable::from_row`].
    ///
    /// Returns an empty `Vec` (not an error) when the query matches zero rows.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if the SQL is invalid or if row
    /// deserialisation fails (e.g., a column value does not fit the Rust type).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let rows: Vec<Sale> = db.select::<Sale>().fetch_all()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn fetch_all(self) -> Result<Vec<T>, DuckLakeError> {
        let (sql, params) = self.build();
        let mut stmt = self.raw.prepare(&sql)?;
        let refs = params_to_refs(&params);
        let rows = stmt.query_map(refs.as_slice(), |row| T::from_row(row))?;
        rows.map(|r| r.map_err(DuckLakeError::Duckdb)).collect()
    }

    /// Execute the query and return **exactly one** row.
    ///
    /// Internally applies `LIMIT 1`. If no row matches, returns
    /// [`DuckLakeError::NotFound`].
    ///
    /// Use [`fetch_optional`](Self::fetch_optional) when the absence of a row
    /// is expected and should not be an error.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::NotFound`] — zero rows matched.
    /// - [`DuckLakeError::Duckdb`] — SQL error or deserialisation failure.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, DuckLakeError, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// match db.select::<Sale>().filter(Sale::id().eq(1i64)).fetch_one() {
    ///     Ok(sale) => println!("{:?}", sale),
    ///     Err(DuckLakeError::NotFound) => println!("not found"),
    ///     Err(e) => return Err(e),
    /// }
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn fetch_one(self) -> Result<T, DuckLakeError> {
        self.limit(1)
            .fetch_all()?
            .into_iter()
            .next()
            .ok_or(DuckLakeError::NotFound)
    }

    /// Execute the query and return the **first row if it exists**, or `None`.
    ///
    /// Internally applies `LIMIT 1`. Unlike [`fetch_one`](Self::fetch_one),
    /// this never returns `DuckLakeError::NotFound` — a missing row is
    /// represented as `Ok(None)`.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] on SQL errors or deserialisation
    /// failure only.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// if let Some(sale) = db.select::<Sale>().filter(Sale::id().eq(99i64)).fetch_optional()? {
    ///     println!("found: {:?}", sale);
    /// } else {
    ///     println!("not found");
    /// }
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn fetch_optional(self) -> Result<Option<T>, DuckLakeError> {
        Ok(self.limit(1).fetch_all()?.into_iter().next())
    }

    // ── Aggregates ────────────────────────────────────────────────────────────

    fn aggregate_scalar<R: duckdb::types::FromSql>(
        &self,
        agg_expr: &str,
    ) -> Result<Option<R>, DuckLakeError> {
        let table = self.table_ref();
        let mut sql = format!("SELECT {agg_expr} FROM {table}");
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.filters.is_empty() {
            let (clause, p) = build_where_clause(&self.filters, 1);
            sql.push_str(" WHERE ");
            sql.push_str(&clause);
            params.extend(p);
        }

        let mut stmt = self.raw.prepare(&sql)?;
        let refs = params_to_refs(&params);
        let val: Option<R> = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
        Ok(val)
    }

    /// Compute `SUM(col)` over the matching rows.
    ///
    /// Returns `None` when no rows match (SQL `SUM` of an empty set is `NULL`).
    /// Only `WHERE` filters are applied; `ORDER BY`, `LIMIT`, and `OFFSET` are ignored.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let total: Option<f64> = db.select::<Sale>().sum(Sale::amount())?;
    /// let eu_total: Option<f64> = db.select::<Sale>()
    ///     .filter(Sale::region().eq("EU"))
    ///     .sum(Sale::amount())?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn sum<R: duckdb::types::FromSql>(
        self,
        col: ColumnExpr,
    ) -> Result<Option<R>, DuckLakeError> {
        self.aggregate_scalar(&format!("SUM({})", col.name))
    }

    /// Compute `AVG(col)` over the matching rows.
    ///
    /// Returns `None` when no rows match.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let avg: Option<f64> = db.select::<Sale>().avg(Sale::amount())?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn avg<R: duckdb::types::FromSql>(
        self,
        col: ColumnExpr,
    ) -> Result<Option<R>, DuckLakeError> {
        self.aggregate_scalar(&format!("AVG({})", col.name))
    }

    /// Compute `MIN(col)` over the matching rows.
    ///
    /// Returns `None` when no rows match.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let min_amount: Option<f64> = db.select::<Sale>().min(Sale::amount())?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn min<R: duckdb::types::FromSql>(
        self,
        col: ColumnExpr,
    ) -> Result<Option<R>, DuckLakeError> {
        self.aggregate_scalar(&format!("MIN({})", col.name))
    }

    /// Compute `MAX(col)` over the matching rows.
    ///
    /// Returns `None` when no rows match.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let max_amount: Option<f64> = db.select::<Sale>().max(Sale::amount())?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn max<R: duckdb::types::FromSql>(
        self,
        col: ColumnExpr,
    ) -> Result<Option<R>, DuckLakeError> {
        self.aggregate_scalar(&format!("MAX({})", col.name))
    }

    /// Execute a `SELECT COUNT(*)` and return the number of matching rows.
    ///
    /// Only the `WHERE` filters applied to this builder are used; `ORDER BY`,
    /// `GROUP BY`, `HAVING`, `LIMIT`, and `OFFSET` are ignored.
    ///
    /// This is more efficient than `fetch_all().len()` because no rows are
    /// transferred or deserialised.
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
    /// let total: i64 = db.select::<Sale>().count()?;
    /// let eu_count: i64 = db.select::<Sale>().filter(Sale::region().eq("EU")).count()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn count(self) -> Result<i64, DuckLakeError> {
        let table = self.table_ref();
        let mut sql = format!("SELECT COUNT(*) FROM {table}");
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.filters.is_empty() {
            let (clause, p) = build_where_clause(&self.filters, 1);
            sql.push_str(" WHERE ");
            sql.push_str(&clause);
            params.extend(p);
        }

        let mut stmt = self.raw.prepare(&sql)?;
        let refs = params_to_refs(&params);
        let count: i64 = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }
}
