use crate::{
    error::DuckLakeError,
    query::{BulkInsertBuilder, InsertBuilder, SelectBuilder},
    schema::DuckLakeTable,
};

/// The main entry point for the ORM.
///
/// Wraps a `duckdb::Connection` and, optionally, an attached DuckLake catalog.
///
/// # Basic usage (plain DuckDB)
/// ```no_run
/// use ducklake_orm::DuckLakeConnection;
///
/// let db = DuckLakeConnection::open_in_memory().unwrap();
/// db.execute("CREATE TABLE main.users (id BIGINT, name VARCHAR)").unwrap();
/// ```
///
/// # DuckLake usage
/// ```no_run
/// use ducklake_orm::DuckLakeConnection;
///
/// let mut db = DuckLakeConnection::open_in_memory().unwrap();
/// db.attach_ducklake("my_catalog.duckdb", "lake").unwrap();
/// ```
pub struct DuckLakeConnection {
    inner: duckdb::Connection,
    catalog: Option<String>,
}

impl DuckLakeConnection {
    /// Open or create a DuckDB database file at `path`.
    pub fn open(path: &str) -> Result<Self, DuckLakeError> {
        let inner = duckdb::Connection::open(path)?;
        Ok(Self { inner, catalog: None })
    }

    /// Open an in-memory DuckDB database (data is lost when the connection closes).
    pub fn open_in_memory() -> Result<Self, DuckLakeError> {
        let inner = duckdb::Connection::open_in_memory()?;
        Ok(Self { inner, catalog: None })
    }

    /// Install + load the `ducklake` DuckDB extension and ATTACH a DuckLake catalog.
    ///
    /// Requires DuckDB ≥ 1.5.2 and network access for the initial `INSTALL`.
    pub fn attach_ducklake(
        &mut self,
        catalog_path: &str,
        catalog_name: &str,
    ) -> Result<(), DuckLakeError> {
        self.inner
            .execute_batch("INSTALL ducklake; LOAD ducklake;")?;
        self.inner.execute(
            &format!(
                "ATTACH '{catalog_path}' AS {catalog_name} (TYPE DUCKLAKE)"
            ),
            [],
        )?;
        self.catalog = Some(catalog_name.to_string());
        Ok(())
    }

    /// Execute arbitrary SQL (DDL, raw statements, etc.).
    pub fn execute(&self, sql: &str) -> Result<(), DuckLakeError> {
        self.inner.execute_batch(sql)?;
        Ok(())
    }

    /// Begin a SELECT query for table `T`.
    pub fn select<T: DuckLakeTable>(&self) -> SelectBuilder<'_, T> {
        SelectBuilder::new(self)
    }

    /// Begin an INSERT of a single record of type `T`.
    pub fn insert<T: DuckLakeTable>(&self, record: T) -> InsertBuilder<'_, T> {
        InsertBuilder::new(self, record)
    }

    /// Begin a bulk INSERT of multiple records of type `T` in a single transaction.
    pub fn insert_many<T: DuckLakeTable>(&self, records: Vec<T>) -> BulkInsertBuilder<'_, T> {
        BulkInsertBuilder::new(self, records)
    }

    pub(crate) fn catalog(&self) -> Option<&str> {
        self.catalog.as_deref()
    }

    pub(crate) fn inner(&self) -> &duckdb::Connection {
        &self.inner
    }
}
