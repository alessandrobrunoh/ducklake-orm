//! The [`DuckLakeTable`] trait — the core abstraction that every ORM-mapped
//! struct must implement.
//!
//! You almost never implement this trait by hand. Instead, derive it with the
//! `#[derive(Table)]` macro provided by the `ducklake-orm-macros` crate (and
//! re-exported from this crate as [`crate::Table`]):
//!
//! ```rust,ignore
//! use ducklake_orm::Table;
//!
//! #[derive(Table, Debug)]
//! #[ducklake(table = "orders", schema = "main")]
//! pub struct Order {
//!     pub id: i64,
//!     pub total: f64,
//!     pub status: String,
//! }
//! ```
//!
//! The derive macro inspects the struct at compile time and generates:
//!
//! 1. An implementation of [`DuckLakeTable`] with the correct column names,
//!    row-deserialisation logic (`from_row`), and parameter-serialisation
//!    logic (`to_params`).
//! 2. A static column-accessor method for every field — e.g. `Order::id()`,
//!    `Order::total()`, `Order::status()` — each returning a
//!    [`crate::query::ColumnExpr`] that can be passed to `.filter()`,
//!    `.set()`, `.order_by()`, etc.

mod table;

pub use table::DuckLakeTable;
