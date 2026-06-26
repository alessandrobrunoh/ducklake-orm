//! Schema migration management.
//!
//! This module is only available when the `migrations` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! ducklake-orm = { version = "0.1", features = ["migrations"] }
//! ```
//!
//! ## Overview
//!
//! The migration system lets you version-control your DuckLake / DuckDB schema
//! changes. Each migration has a unique integer version, a description, and
//! two SQL scripts (or arbitrary Rust code): `up` to apply and `down` to reverse.
//!
//! Applied migrations are tracked in a table named `main._ducklake_migrations`,
//! which is created automatically on first use.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConnection, DuckLakeError};
//! use ducklake_orm::migration::{Migrator, SqlMigration};
//!
//! let db = DuckLakeConnection::open("warehouse.duckdb")?;
//!
//! let migrator = Migrator::new(&db)
//!     .add(SqlMigration::new(
//!         1,
//!         "create sales table",
//!         // up
//!         "CREATE TABLE IF NOT EXISTS main.sales (
//!              id     BIGINT PRIMARY KEY,
//!              amount DOUBLE NOT NULL,
//!              region VARCHAR NOT NULL
//!          )",
//!         // down
//!         "DROP TABLE IF EXISTS main.sales",
//!     ))
//!     .add(SqlMigration::new(
//!         2,
//!         "add sold_at column",
//!         "ALTER TABLE main.sales ADD COLUMN sold_at TIMESTAMP",
//!         "ALTER TABLE main.sales DROP COLUMN sold_at",
//!     ));
//!
//! // Apply all pending migrations (idempotent — safe to call on every startup).
//! let n = migrator.run()?;
//! println!("{n} migration(s) applied");
//!
//! // Roll back the last migration if needed:
//! // migrator.rollback(1)?;
//!
//! // Inspect status:
//! for s in migrator.status()? {
//!     println!(
//!         "v{:<4} [{:^7}] {}",
//!         s.version,
//!         if s.applied { "applied" } else { "pending" },
//!         s.description,
//!     );
//! }
//! # Ok::<(), DuckLakeError>(())
//! ```
//!
//! ## Custom migrations
//!
//! Implement the [`Migration`] trait to write migration logic in Rust:
//!
//! ```rust,ignore
//! use ducklake_orm::{DuckLakeError, migration::Migration};
//!
//! struct SeedData;
//!
//! impl Migration for SeedData {
//!     fn version(&self) -> i64 { 3 }
//!     fn description(&self) -> &str { "seed initial data" }
//!
//!     fn up(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
//!         conn.execute_batch(
//!             "INSERT INTO main.sales VALUES (1, 100.0, 'EU');
//!              INSERT INTO main.sales VALUES (2, 200.0, 'US');"
//!         )?;
//!         Ok(())
//!     }
//!
//!     fn down(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
//!         conn.execute_batch("DELETE FROM main.sales WHERE id IN (1, 2)")?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Atomicity guarantee
//!
//! Each migration (both `up` and `down`) is executed inside a
//! `BEGIN` / `COMMIT` transaction together with the update to
//! `_ducklake_migrations`. If either the schema change or the tracking update
//! fails, the entire transaction is rolled back, leaving the database in a
//! consistent state.

pub mod migration;
pub mod migrator;

pub use migration::{Migration, MigrationStatus, SqlMigration};
pub use migrator::Migrator;
