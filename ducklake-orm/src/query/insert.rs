use duckdb::types::ToSql;

use crate::{
    connection::DuckLakeConnection,
    error::DuckLakeError,
    schema::DuckLakeTable,
};

/// Fluent INSERT builder — created via `DuckLakeConnection::insert(record)`.
pub struct InsertBuilder<'conn, T: DuckLakeTable> {
    conn: &'conn DuckLakeConnection,
    record: T,
}

impl<'conn, T: DuckLakeTable> InsertBuilder<'conn, T> {
    pub(crate) fn new(conn: &'conn DuckLakeConnection, record: T) -> Self {
        Self { conn, record }
    }

    fn build_sql(&self) -> String {
        let cols = T::column_names();
        let schema = T::schema_name();
        let table = T::table_name();

        let table_ref = match self.conn.catalog() {
            Some(catalog) => format!("{catalog}.{schema}.{table}"),
            None => format!("{schema}.{table}"),
        };

        let col_list = cols.join(", ");
        let placeholders = cols
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        format!("INSERT INTO {table_ref} ({col_list}) VALUES ({placeholders})")
    }

    /// Execute the INSERT statement.
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        let sql = self.build_sql();
        let boxed: Vec<Box<dyn ToSql>> = self.record.to_params();
        let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();

        let rows = self.conn.inner().execute(&sql, refs.as_slice())?;
        Ok(rows)
    }
}

/// Fluent bulk INSERT builder.
pub struct BulkInsertBuilder<'conn, T: DuckLakeTable> {
    conn: &'conn DuckLakeConnection,
    records: Vec<T>,
}

impl<'conn, T: DuckLakeTable> BulkInsertBuilder<'conn, T> {
    pub(crate) fn new(conn: &'conn DuckLakeConnection, records: Vec<T>) -> Self {
        Self { conn, records }
    }

    /// Execute all INSERTs inside a single transaction.
    pub fn execute(self) -> Result<usize, DuckLakeError> {
        let schema = T::schema_name();
        let table = T::table_name();
        let table_ref = match self.conn.catalog() {
            Some(catalog) => format!("{catalog}.{schema}.{table}"),
            None => format!("{schema}.{table}"),
        };

        let cols = T::column_names();
        let col_list = cols.join(", ");
        let placeholders = cols
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!("INSERT INTO {table_ref} ({col_list}) VALUES ({placeholders})");
        let conn = self.conn.inner();

        conn.execute_batch("BEGIN")?;
        let mut total = 0;
        for record in &self.records {
            let boxed: Vec<Box<dyn ToSql>> = record.to_params();
            let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
            total += conn.execute(&sql, refs.as_slice())?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(total)
    }
}
