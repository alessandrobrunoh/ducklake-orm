//! # ducklake-orm
//!
//! A type-safe, compile-time-checked ORM for [DuckLake] and plain [DuckDB], written
//! in Rust.
//!
//! ## What is DuckLake?
//!
//! [DuckLake](https://ducklake.select) is an open lakehouse format (v1.0, 2026) that
//! stores **metadata** in a SQL catalog (SQLite, PostgreSQL, or DuckDB) and **data**
//! as Parquet files. It is accessed through a DuckDB extension and natively supports
//! snapshot **time travel**, bucket partitioning, and deletion vectors.
//!
//! ## What does this crate do?
//!
//! `ducklake-orm` gives you a Rust-native API to interact with DuckLake and plain
//! DuckDB databases:
//!
//! - **Compile-time column safety** — typos in column names are caught by the Rust
//!   compiler, not at query execution time.
//! - **Fluent query builders** — `SELECT`, `INSERT`, `INSERT (bulk)`, `UPDATE`, and
//!   `DELETE` are each built with chainable methods.
//! - **DuckLake time travel** — query historical snapshots with
//!   [`.at_snapshot()`](query::SelectBuilder::at_snapshot) and
//!   [`.at_timestamp()`](query::SelectBuilder::at_timestamp).
//! - **Connection pool** — powered by [r2d2]; configure size and timeout in a
//!   single `ducklake.toml` file.
//! - **Safety guardrails** — `UPDATE` and `DELETE` without a `WHERE` clause return a
//!   `Result::Err` unless you explicitly call `.update_all()` / `.delete_all()`.
//! - **No `unwrap()`** — every public API returns `Result<_, `[`DuckLakeError`]`>`.
//!
//! ## Quick start
//!
//! ### 1 — Declare a table
//!
//! Apply `#[derive(Table)]` to any struct whose fields map one-to-one to SQL columns,
//! in declaration order. The macro generates:
//!
//! - An implementation of [`DuckLakeTable`] (column list, row deserialisation,
//!   parameter serialisation).
//! - A static column-accessor method for every field (`Sale::amount()`,
//!   `Sale::region()`, …) that returns a [`query::ColumnExpr`].
//!
//! ```rust,ignore
//! use ducklake_orm::Table;
//!
//! #[derive(Table, Debug)]
//! #[ducklake(table = "sales", schema = "main")]
//! pub struct Sale {
//!     #[ducklake(primary_key)]
//!     pub id: i64,
//!     pub amount: f64,
//!     pub region: String,
//! }
//! ```
//!
//! ### 2 — Open a connection and run queries
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConnection, Table};
//!
//! # #[derive(Table, Debug)]
//! # #[ducklake(table = "sales")]
//! # struct Sale { id: i64, amount: f64, region: String }
//! let db = DuckLakeConnection::open("warehouse.duckdb")?;
//!
//! // INSERT
//! db.insert(Sale { id: 1, amount: 250.0, region: "EU".into() }).execute()?;
//!
//! // SELECT with filters
//! let rows: Vec<Sale> = db
//!     .select::<Sale>()
//!     .filter(Sale::amount().gt(100.0))
//!     .filter(Sale::region().eq("EU"))
//!     .order_by(Sale::amount().desc())
//!     .limit(10)
//!     .fetch_all()?;
//!
//! // UPDATE — filter is required or you get an Err
//! db.update::<Sale>()
//!     .set(Sale::region(), "EMEA")
//!     .filter(Sale::id().eq(1i64))
//!     .execute()?;
//!
//! // DELETE — filter is required or you get an Err
//! db.delete::<Sale>()
//!     .filter(Sale::id().eq(1i64))
//!     .execute()?;
//! # Ok::<(), ducklake_orm::DuckLakeError>(())
//! ```
//!
//! ### 3 — Use a connection pool
//!
//! For production workloads that need concurrent access, use [`DuckLakePool`].
//! Configure it in `ducklake.toml` (see [`config`]):
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConfig, DuckLakePool};
//!
//! let config = DuckLakeConfig::from_file("ducklake.toml")?;
//! let pool   = DuckLakePool::from_config(&config)?;
//!
//! // Each call to .get() returns a PooledConnection that is returned to the
//! // pool automatically when it drops.
//! let conn = pool.get()?;
//! conn.execute("SELECT 1")?;
//! # Ok::<(), ducklake_orm::DuckLakeError>(())
//! ```
//!
//! ### 4 — DuckLake time travel
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConnection, Table};
//!
//! # #[derive(Table, Debug)]
//! # #[ducklake(table = "sales")]
//! # struct Sale { id: i64, amount: f64, region: String }
//! let mut db = DuckLakeConnection::open_in_memory()?;
//! db.attach_ducklake("catalog.duckdb", "lake")?;
//!
//! // Read the table as it was at snapshot version 42.
//! let rows: Vec<Sale> = db
//!     .select::<Sale>()
//!     .at_snapshot(42)
//!     .fetch_all()?;
//!
//! // Read the table as it was at a specific point in time.
//! let rows: Vec<Sale> = db
//!     .select::<Sale>()
//!     .at_timestamp("2025-06-01T00:00:00Z")
//!     .fetch_all()?;
//! # Ok::<(), ducklake_orm::DuckLakeError>(())
//! ```
//!
//! ## Module overview
//!
//! | Module | What it contains |
//! |--------|-----------------|
//! | [`config`] | `ducklake.toml` deserialization — [`DuckLakeConfig`], [`config::PoolConfig`], … |
//! | [`connection`] | [`DuckLakeConnection`] — single direct connection |
//! | [`pool`] | [`DuckLakePool`] + [`pool::PooledConnection`] — r2d2-backed pool |
//! | [`error`] | [`DuckLakeError`] — all error variants |
//! | [`schema`] | [`DuckLakeTable`] — the core trait implemented by `#[derive(Table)]` |
//! | [`query`] | All query builders and the filter / ordering DSL |
//! | [`migration`] *(feature `migrations`)* | [`migration::Migrator`], [`migration::SqlMigration`], [`migration::Migration`] |
//!
//! [DuckLake]: https://ducklake.select
//! [DuckDB]: https://duckdb.org
//! [r2d2]: https://docs.rs/r2d2

#![deny(missing_docs)]

pub mod config;
pub mod connection;
pub mod error;
pub mod pool;
pub mod query;
pub mod schema;

#[cfg(feature = "migrations")]
pub mod migration;

pub use config::DuckLakeConfig;
pub use connection::DuckLakeConnection;
pub use error::DuckLakeError;
pub use pool::DuckLakePool;
pub use schema::DuckLakeTable;

/// The `#[derive(Table)]` macro — re-exported from `ducklake-orm-macros` so
/// you only need one dependency in your `Cargo.toml`.
///
/// See the [crate-level documentation](crate) for a full usage example, or the
/// [`ducklake_orm_macros`](ducklake_orm_macros) crate for the full attribute reference.
pub use ducklake_orm_macros::Table;
