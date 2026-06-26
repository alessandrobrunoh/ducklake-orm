use thiserror::Error;

#[derive(Debug, Error)]
pub enum DuckLakeError {
    #[error("DuckDB error: {0}")]
    Duckdb(#[from] duckdb::Error),

    #[error("No rows found")]
    NotFound,

    #[error("Query error: {0}")]
    Query(String),
}
