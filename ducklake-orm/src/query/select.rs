use std::marker::PhantomData;

use duckdb::types::ToSql;

use crate::{
    connection::DuckLakeConnection,
    error::DuckLakeError,
    query::filter::{build_where_clause, FilterExpr, OrderExpr, SqlValue},
    schema::DuckLakeTable,
};

/// DuckLake-specific time travel target.
#[derive(Debug, Clone)]
pub enum SnapshotRef {
    /// `AT (VERSION => n)` — specific snapshot version
    Version(u64),
    /// `AT (TIMESTAMP => 'ts')` — point-in-time read
    Timestamp(String),
}

/// Fluent SELECT builder — created via `DuckLakeConnection::select::<T>()`.
///
/// All methods consume `self` and return a new builder (type-state pattern),
/// so invalid combinations are caught at compile time.
pub struct SelectBuilder<'conn, T: DuckLakeTable> {
    conn: &'conn DuckLakeConnection,
    filters: Vec<FilterExpr>,
    order_by: Vec<OrderExpr>,
    limit: Option<usize>,
    offset: Option<usize>,
    snapshot: Option<SnapshotRef>,
    _marker: PhantomData<T>,
}

impl<'conn, T: DuckLakeTable> SelectBuilder<'conn, T> {
    pub(crate) fn new(conn: &'conn DuckLakeConnection) -> Self {
        Self {
            conn,
            filters: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            snapshot: None,
            _marker: PhantomData,
        }
    }

    /// Add a WHERE predicate. Multiple calls are AND-ed together.
    pub fn filter(mut self, expr: FilterExpr) -> Self {
        self.filters.push(expr);
        self
    }

    /// Add an ORDER BY clause. Multiple calls append columns in order.
    pub fn order_by(mut self, expr: OrderExpr) -> Self {
        self.order_by.push(expr);
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    /// Read from a specific DuckLake snapshot version (`AT (VERSION => n)`).
    pub fn at_snapshot(mut self, version: u64) -> Self {
        self.snapshot = Some(SnapshotRef::Version(version));
        self
    }

    /// Read as of a point in time (`AT (TIMESTAMP => 'ts')`).
    /// `ts` should be an ISO 8601 string, e.g. `"2025-01-01T00:00:00Z"`.
    pub fn at_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.snapshot = Some(SnapshotRef::Timestamp(ts.into()));
        self
    }

    fn table_ref(&self) -> String {
        let schema = T::schema_name();
        let table = T::table_name();
        match self.conn.catalog() {
            Some(catalog) => format!("{catalog}.{schema}.{table}"),
            None => format!("{schema}.{table}"),
        }
    }

    fn build_sql_and_params(&self) -> (String, Vec<SqlValue>) {
        let cols = T::column_names().join(", ");
        let table = self.table_ref();

        let at_clause = match &self.snapshot {
            Some(SnapshotRef::Version(v)) => format!(" AT (VERSION => {v})"),
            Some(SnapshotRef::Timestamp(ts)) => format!(" AT (TIMESTAMP => '{ts}')"),
            None => String::new(),
        };

        let mut sql = format!("SELECT {cols} FROM {table}{at_clause}");
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.filters.is_empty() {
            let (where_sql, where_params) = build_where_clause(&self.filters, 1);
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params.extend(where_params);
        }

        if !self.order_by.is_empty() {
            let order_sql = self
                .order_by
                .iter()
                .map(|o| o.to_sql_fragment())
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_sql);
        }

        if let Some(n) = self.limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }

        if let Some(n) = self.offset {
            sql.push_str(&format!(" OFFSET {n}"));
        }

        (sql, params)
    }

    /// Execute the query and return all matching rows.
    pub fn fetch_all(self) -> Result<Vec<T>, DuckLakeError> {
        let (sql, params) = self.build_sql_and_params();
        let conn = self.conn.inner();
        let mut stmt = conn.prepare(&sql)?;

        let boxed: Vec<Box<dyn ToSql>> = params.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>).collect();
        let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(refs.as_slice(), |row| T::from_row(row))?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Execute the query and return the first row, or `DuckLakeError::NotFound`.
    pub fn fetch_one(self) -> Result<T, DuckLakeError> {
        let mut results = self.limit(1).fetch_all()?;
        results.pop().ok_or(DuckLakeError::NotFound)
    }

    /// Execute the query and return an optional first row.
    pub fn fetch_optional(self) -> Result<Option<T>, DuckLakeError> {
        let mut results = self.limit(1).fetch_all()?;
        Ok(results.pop())
    }

    /// Return the count of matching rows without fetching them.
    pub fn count(self) -> Result<i64, DuckLakeError> {
        let table = self.table_ref();
        let mut sql = format!("SELECT COUNT(*) FROM {table}");
        let mut params: Vec<SqlValue> = Vec::new();

        if !self.filters.is_empty() {
            let (where_sql, where_params) = build_where_clause(&self.filters, 1);
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params.extend(where_params);
        }

        let conn = self.conn.inner();
        let mut stmt = conn.prepare(&sql)?;
        let boxed: Vec<Box<dyn ToSql>> = params.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>).collect();
        let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();

        let count: i64 = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }
}
