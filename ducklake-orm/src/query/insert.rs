//! Fluent INSERT builders — single-record and bulk.

use std::marker::PhantomData;

use crate::{
    connection::DuckLakeConnection, error::DuckLakeError, pool::PooledConnection,
    query::filter::ColumnExpr, schema::DuckLakeTable,
};

// ── OnConflict ────────────────────────────────────────────────────────────────

/// Conflict resolution strategy for `INSERT` statements.
///
/// Set via [`InsertBuilder::or_replace`] or [`InsertBuilder::or_ignore`].
/// The default when constructing an [`InsertBuilder`] is [`OnConflict::Abort`]
/// (standard SQL behaviour — the statement fails on a constraint violation).
///
/// ## Generated SQL
///
/// | Variant | SQL keyword |
/// |---------|------------|
/// | `Abort` | `INSERT INTO …` |
/// | `Replace` | `INSERT OR REPLACE INTO …` |
/// | `Ignore` | `INSERT OR IGNORE INTO …` |
#[derive(Debug, Clone, Default)]
pub enum OnConflict {
    /// Standard SQL: fail the statement on any constraint violation (default).
    #[default]
    Abort,
    /// Replace the existing row with the new data if a `PRIMARY KEY` or
    /// `UNIQUE` constraint is violated.
    ///
    /// Equivalent to `DELETE` + `INSERT` — all columns are overwritten.
    Replace,
    /// Silently skip the row if a `PRIMARY KEY` or `UNIQUE` constraint is
    /// violated.
    Ignore,
}

// ── Single INSERT ─────────────────────────────────────────────────────────────

/// Fluent builder for inserting a **single record** into a table.
///
/// Create one with [`DuckLakeConnection::insert`](crate::connection::DuckLakeConnection::insert)
/// or [`PooledConnection::insert`](crate::pool::PooledConnection::insert), then call
/// [`.execute()`](Self::execute) to send the statement.
///
/// The generated SQL looks like:
///
/// ```sql
/// INSERT INTO [catalog.]schema.table (col1, col2, …) VALUES ($1, $2, …)
/// ```
///
/// Column names and parameter count come from [`DuckLakeTable::column_names`]
/// and [`DuckLakeTable::to_params`] respectively.
///
/// ## When to use `InsertBuilder` vs `BulkInsertBuilder`
///
/// | Scenario | Recommended builder |
/// |----------|-------------------|
/// | One record at a time | [`InsertBuilder`] |
/// | Many records (hundreds / thousands) | [`BulkInsertBuilder`] |
///
/// `BulkInsertBuilder` wraps all rows in a single `BEGIN` / `COMMIT`
/// transaction, which is significantly faster than thousands of individual
/// auto-commit `INSERT`s.
///
/// ## Example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// let rows_affected = db
///     .insert(Sale { id: 1, amount: 99.9, region: "EU".into() })
///     .execute()?;
/// assert_eq!(rows_affected, 1);
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct InsertBuilder<'conn, T: DuckLakeTable> {
    raw: &'conn duckdb::Connection,
    catalog: Option<String>,
    record: T,
    on_conflict: OnConflict,
}

impl<'conn, T: DuckLakeTable> InsertBuilder<'conn, T> {
    pub(crate) fn new(src: &'conn DuckLakeConnection, record: T) -> Self {
        Self {
            raw: src.inner(),
            catalog: src.catalog().map(str::to_owned),
            record,
            on_conflict: OnConflict::Abort,
        }
    }

    pub(crate) fn new_pooled(src: &'conn PooledConnection<'_>, record: T) -> Self {
        use std::ops::Deref;
        Self {
            raw: src.inner.deref(),
            catalog: src.catalog.clone(),
            record,
            on_conflict: OnConflict::Abort,
        }
    }

    fn build_sql(&self) -> String {
        let cols = T::column_names();
        let tref = table_ref::<T>(self.catalog.as_deref());
        let col_list = cols.join(", ");
        let placeholders = cols
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let keyword = match self.on_conflict {
            OnConflict::Abort => "INSERT INTO",
            OnConflict::Replace => "INSERT OR REPLACE INTO",
            OnConflict::Ignore => "INSERT OR IGNORE INTO",
        };
        format!("{keyword} {tref} ({col_list}) VALUES ({placeholders})")
    }

    /// On a `PRIMARY KEY` or `UNIQUE` conflict, **replace** the existing row.
    ///
    /// Generates `INSERT OR REPLACE INTO …`. The old row is deleted and the
    /// new one inserted, so all columns are overwritten (even those not
    /// included in the conflict).
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // Upsert: insert or overwrite:
    /// db.insert(Sale { id: 1, amount: 999.0, region: "EU".into() })
    ///   .or_replace()
    ///   .execute()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn or_replace(mut self) -> Self {
        self.on_conflict = OnConflict::Replace;
        self
    }

    /// On a `PRIMARY KEY` or `UNIQUE` conflict, **silently skip** the row.
    ///
    /// Generates `INSERT OR IGNORE INTO …`. No error is returned; the existing
    /// row is left unchanged.
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// // Insert only if not already present:
    /// db.insert(Sale { id: 1, amount: 100.0, region: "EU".into() })
    ///   .or_ignore()
    ///   .execute()?;
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn or_ignore(mut self) -> Self {
        self.on_conflict = OnConflict::Ignore;
        self
    }

    /// Execute the `INSERT` statement.
    ///
    /// # Returns
    ///
    /// The number of rows inserted (always `1` on success; `0` when
    /// `.or_ignore()` was set and the row was skipped).
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if DuckDB rejects the statement —
    /// for example, a `PRIMARY KEY` or `UNIQUE` constraint violation when
    /// the default [`OnConflict::Abort`] strategy is in use.
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        let sql = self.build_sql();
        let params_owned = self.record.to_params();
        let refs = params_to_refs_boxed(&params_owned);
        Ok(self.raw.execute(&sql, refs.as_slice())?)
    }

    /// Execute the `INSERT … RETURNING col` statement and return the value of
    /// the specified column from the inserted row.
    ///
    /// This is the standard way to retrieve a server-generated value (such as
    /// an auto-increment primary key or a `DEFAULT` expression) without a
    /// second `SELECT` round-trip.
    ///
    /// `col` should be a column accessor from the same table (e.g.
    /// `Sale::id()`), but any valid column name is accepted.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Duckdb`] — SQL error or type mismatch between the
    ///   column value and `R`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::{DuckLakeConnection, Table};
    ///
    /// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
    /// # let db = DuckLakeConnection::open_in_memory().unwrap();
    /// let inserted_id: i64 = db
    ///     .insert(Sale { id: 42, amount: 99.9, region: "EU".into() })
    ///     .execute_returning(Sale::id())?;
    /// assert_eq!(inserted_id, 42);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn execute_returning<R: duckdb::types::FromSql>(
        self,
        col: ColumnExpr,
    ) -> Result<R, DuckLakeError> {
        let mut sql = self.build_sql();
        sql.push_str(&format!(" RETURNING {}", col.name));
        let params_owned = self.record.to_params();
        let refs = params_to_refs_boxed(&params_owned);
        let mut stmt = self.raw.prepare(&sql)?;
        let val: R = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
        Ok(val)
    }
}

// ── Bulk INSERT ───────────────────────────────────────────────────────────────

/// Fluent builder for inserting **multiple records** in a single transaction.
///
/// Create one with
/// [`DuckLakeConnection::insert_many`](crate::connection::DuckLakeConnection::insert_many)
/// or [`PooledConnection::insert_many`](crate::pool::PooledConnection::insert_many),
/// then call [`.execute()`](Self::execute).
///
/// All rows are wrapped in `BEGIN` / `COMMIT`, so the entire batch either
/// succeeds or is rolled back as a unit. This is substantially faster than
/// calling [`InsertBuilder::execute`] in a loop, because DuckDB pays the
/// transaction overhead only once.
///
/// ## Example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// # #[derive(Table, Debug)] #[ducklake(table = "sales")] struct Sale { id: i64, amount: f64, region: String }
/// # let db = DuckLakeConnection::open_in_memory().unwrap();
/// let records = vec![
///     Sale { id: 1, amount: 10.0, region: "EU".into() },
///     Sale { id: 2, amount: 20.0, region: "US".into() },
///     Sale { id: 3, amount: 30.0, region: "APAC".into() },
/// ];
/// let inserted = db.insert_many(records).execute()?;
/// assert_eq!(inserted, 3);
/// # Ok::<(), ducklake_orm::DuckLakeError>(())
/// ```
pub struct BulkInsertBuilder<'conn, T: DuckLakeTable> {
    raw: &'conn duckdb::Connection,
    catalog: Option<String>,
    records: Vec<T>,
    _marker: PhantomData<T>,
}

impl<'conn, T: DuckLakeTable> BulkInsertBuilder<'conn, T> {
    pub(crate) fn new(src: &'conn DuckLakeConnection, records: Vec<T>) -> Self {
        Self {
            raw: src.inner(),
            catalog: src.catalog().map(str::to_owned),
            records,
            _marker: PhantomData,
        }
    }

    pub(crate) fn new_pooled(src: &'conn PooledConnection<'_>, records: Vec<T>) -> Self {
        use std::ops::Deref;
        Self {
            raw: src.inner.deref(),
            catalog: src.catalog.clone(),
            records,
            _marker: PhantomData,
        }
    }

    /// Execute all `INSERT` statements inside a single transaction.
    ///
    /// Runs `BEGIN` before the first row and `COMMIT` after the last. If any
    /// individual `INSERT` fails, the entire batch is rolled back.
    ///
    /// # Returns
    ///
    /// The total number of rows successfully inserted.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Duckdb`] if any row causes a constraint
    /// violation or any other SQL error. The transaction is rolled back
    /// automatically by DuckDB when the error occurs.
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        let tref = table_ref::<T>(self.catalog.as_deref());
        let cols = T::column_names();
        let col_list = cols.join(", ");
        let placeholders = cols
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO {tref} ({col_list}) VALUES ({placeholders})");

        self.raw.execute_batch("BEGIN")?;
        let mut total = 0_usize;
        for record in &self.records {
            let params_owned = record.to_params();
            let refs = params_to_refs_boxed(&params_owned);
            total += self.raw.execute(&sql, refs.as_slice())?;
        }
        self.raw.execute_batch("COMMIT")?;
        Ok(total)
    }
}

// ── internal helpers ──────────────────────────────────────────────────────────

/// Build the fully-qualified `[catalog.]schema.table` reference string.
pub(crate) fn table_ref<T: DuckLakeTable>(catalog: Option<&str>) -> String {
    let schema = T::schema_name();
    let table = T::table_name();
    match catalog {
        Some(cat) => format!("{cat}.{schema}.{table}"),
        None => format!("{schema}.{table}"),
    }
}

/// Convert a slice of boxed [`duckdb::types::ToSql`] trait objects into a
/// `Vec` of references suitable for DuckDB's `execute` / `query_map` API.
pub(crate) fn params_to_refs_boxed<'a>(
    boxed: &'a [Box<dyn duckdb::types::ToSql>],
) -> Vec<&'a dyn duckdb::types::ToSql> {
    boxed.iter().map(|b| b.as_ref()).collect()
}
