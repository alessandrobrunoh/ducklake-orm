# ducklake-orm

[![crates.io](https://img.shields.io/crates/v/ducklake-orm.svg)](https://crates.io/crates/ducklake-orm)
[![docs.rs](https://docs.rs/ducklake-orm/badge.svg)](https://docs.rs/ducklake-orm)
[![license](https://img.shields.io/crates/l/ducklake-orm.svg)](LICENSE)

A type-safe, compile-time-checked ORM for [DuckLake](https://ducklake.select) and plain [DuckDB](https://duckdb.org), written in Rust.

Typos in column names are caught at **compile time**. Query builders are chainable and fully typed. Every public API returns `Result` — no panics, no `unwrap()`.

---

## Features

| | |
|---|---|
| **Compile-time column safety** | Column names are Rust method calls — typos are compiler errors, not runtime surprises |
| **Fluent query builders** | `SELECT`, `INSERT`, bulk `INSERT`, `UPDATE`, `DELETE` — all chainable |
| **DuckLake time travel** | Query historical snapshots with `.at_snapshot()` and `.at_timestamp()` |
| **Connection pool** | r2d2-backed pool configured via `ducklake.toml` |
| **Safety guardrails** | `UPDATE`/`DELETE` without a filter return `Err` — no accidental full-table mutations |
| **No `unwrap()`** | Every public API returns `Result<_, DuckLakeError>` |

---

## Installation

```toml
[dependencies]
ducklake-orm = "0.1"
```

---

## Usage

### Declare a table

Apply `#[derive(Table)]` to any struct whose fields map to SQL columns:

```rust
use ducklake_orm::Table;

#[derive(Table, Debug)]
#[ducklake(table = "sales", schema = "main")]
pub struct Sale {
    #[ducklake(primary_key)]
    pub id: i64,
    pub amount: f64,
    pub region: String,
}
```

The macro generates:
- An implementation of `DuckLakeTable` (column list, row deserialization, parameter serialization)
- A static column accessor per field — `Sale::amount()`, `Sale::region()`, … — usable in filters, ordering, and updates

### CRUD

```rust
use ducklake_orm::{DuckLakeConnection, Table};

let db = DuckLakeConnection::open("warehouse.duckdb")?;

// INSERT
db.insert(Sale { id: 1, amount: 250.0, region: "EU".into() }).execute()?;

// Bulk INSERT (wrapped in a transaction)
db.insert_many(vec![
    Sale { id: 2, amount: 100.0, region: "US".into() },
    Sale { id: 3, amount: 300.0, region: "APAC".into() },
]).execute()?;

// SELECT with filters, ordering, and limit
let rows: Vec<Sale> = db
    .select::<Sale>()
    .filter(Sale::amount().gt(100.0))
    .filter(Sale::region().eq("EU"))
    .order_by(Sale::amount().desc())
    .limit(10)
    .fetch_all()?;

// UPDATE — a filter is required or you get Err
db.update::<Sale>()
    .set(Sale::region(), "EMEA")
    .filter(Sale::id().eq(1i64))
    .execute()?;

// DELETE — a filter is required or you get Err
db.delete::<Sale>()
    .filter(Sale::id().eq(1i64))
    .execute()?;
```

### Connection pool

```rust
use ducklake_orm::{DuckLakeConfig, DuckLakePool};

let config = DuckLakeConfig::from_file("ducklake.toml")?;
let pool   = DuckLakePool::from_config(&config)?;

// PooledConnection is returned to the pool automatically on drop
let conn = pool.get()?;
conn.execute("SELECT 1")?;
```

### DuckLake time travel

```rust
let mut db = DuckLakeConnection::open_in_memory()?;
db.attach_ducklake("catalog.duckdb", "lake")?;

// Query at a specific snapshot version
let rows: Vec<Sale> = db
    .select::<Sale>()
    .at_snapshot(42)
    .fetch_all()?;

// Query at a point in time
let rows: Vec<Sale> = db
    .select::<Sale>()
    .at_timestamp("2025-06-01T00:00:00Z")
    .fetch_all()?;
```

### Filter DSL

Column accessors return a `ColumnExpr` that supports:

```rust
Sale::amount().gt(100.0)          // >
Sale::amount().gte(100.0)         // >=
Sale::amount().lt(500.0)          // <
Sale::amount().lte(500.0)         // <=
Sale::amount().eq(250.0)          // =
Sale::amount().ne(0.0)            // !=
Sale::amount().between(100.0, 500.0)
Sale::region().like("E%")
Sale::region().is_null()
Sale::region().is_not_null()

// Combine predicates
Sale::amount().gt(100.0).and(Sale::region().eq("EU"))
Sale::region().eq("EU").or(Sale::region().eq("US"))
```

---

## `#[derive(Table)]` reference

### Struct-level attributes

| Attribute | Default | Description |
|-----------|---------|-------------|
| `table = "name"` | struct name in `snake_case` | SQL table name |
| `schema = "name"` | `"main"` | SQL schema name |
| `rename_all = "…"` | no renaming | Case conversion applied to all field names |

Supported `rename_all` values: `snake_case`, `SCREAMING_SNAKE_CASE`, `kebab-case`, `camelCase`, `PascalCase`, `lowercase`, `UPPERCASE`.

### Field-level attributes

| Attribute | Effect |
|-----------|--------|
| `primary_key` | Documentation marker |
| `column = "…"` | Override the SQL column name for this field |
| `skip_insert` | Exclude from `INSERT` but keep in `SELECT` — for DB-generated columns |
| `skip` | Fully virtual field: excluded from both `SELECT` and `INSERT` |

---

## What is DuckLake?

[DuckLake](https://ducklake.select) is an open lakehouse format (v1.0, 2026) that stores **metadata** in a SQL catalog (SQLite, PostgreSQL, or DuckDB) and **data** as Parquet files. It natively supports snapshot time travel, bucket partitioning, and deletion vectors — all accessed through a DuckDB extension.

---

## License

MIT
