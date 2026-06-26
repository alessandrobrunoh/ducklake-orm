pub mod connection;
pub mod error;
pub mod query;
pub mod schema;

pub use connection::DuckLakeConnection;
pub use error::DuckLakeError;
pub use schema::DuckLakeTable;

// Re-export the derive macro so users only need one `use`/dependency.
pub use ducklake_orm_macros::Table;

