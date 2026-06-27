# ducklake-orm

[![crates.io](https://img.shields.io/crates/v/ducklake-orm.svg)](https://crates.io/crates/ducklake-orm)
[![docs.rs](https://docs.rs/ducklake-orm/badge.svg)](https://docs.rs/ducklake-orm)
[![license](https://img.shields.io/crates/l/ducklake-orm.svg)](LICENSE)

A type-safe, compile-time-checked ORM for [DuckLake](https://ducklake.select) and plain [DuckDB](https://duckdb.org), written in Rust.

## Features

- **Compile-time column safety** — typos in column names are caught by the Rust compiler, not at runtime
- **Fluent query builders** — `SELECT`, `INSERT`, bulk `INSERT`, `UPDATE`, and `DELETE` with chainable methods
- **DuckLake time travel** — query historical snapshots with `.at_snapshot()` and `.at_timestamp()`
- **Connection pool** — powered by [r2d2](https://docs.rs/r2d2); configure size and timeout in `ducklake.toml`
- **Safety guardrails** — `UPDATE` and `DELETE` without a `WHERE` clause return `Err` unless you call `.update_all()` / `.delete_all()`
- **No `unwrap()`** — every public API returns `Result<_, DuckLakeError>`

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
ducklake-orm = "0.1"
```

### 1 — Declare a table

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

### 2 — Open a connection and run queries

```rust
use ducklake_orm::{DuckLakeConnection, Table};

let db = DuckLakeConnection::open("warehouse.duckdb")?;

// INSERT
db.insert(Sale { id: 1, amount: 250.0, region: "EU".into() }).execute()?;

// SELECT with filters
let rows: Vec<Sale> = db
    .select::<Sale>()
    .filter(Sale::amount().gt(100.0))
    .filter(Sale::region().eq("EU"))
    .order_by(Sale::amount().desc())
    .limit(10)
    .fetch_all()?;

// UPDATE — filter required, or returns Err
db.update::<Sale>()
    .set(Sale::region(), "EMEA")
    .filter(Sale::id().eq(1i64))
    .execute()?;

// DELETE — filter required, or returns Err
db.delete::<Sale>()
    .filter(Sale::id().eq(1i64))
    .execute()?;
```

### 3 — Connection pool

```rust
use ducklake_orm::{DuckLakeConfig, DuckLakePool};

let config = DuckLakeConfig::from_file("ducklake.toml")?;
let pool   = DuckLakePool::from_config(&config)?;

let conn = pool.get()?;
conn.execute("SELECT 1")?;
```

### 4 — DuckLake time travel

```rust
let mut db = DuckLakeConnection::open_in_memory()?;
db.attach_ducklake("catalog.duckdb", "lake")?;

// Snapshot version
let rows: Vec<Sale> = db.select::<Sale>().at_snapshot(42).fetch_all()?;

// Point in time
let rows: Vec<Sale> = db.select::<Sale>().at_timestamp("2025-06-01T00:00:00Z").fetch_all()?;
```

## `#[derive(Table)]` attributes

### Struct-level

| Attribute | Default | Description |
|-----------|---------|-------------|
| `table = "name"` | struct name in `snake_case` | SQL table name |
| `schema = "name"` | `"main"` | SQL schema name |
| `rename_all = "…"` | no renaming | Case conversion for all field names |

Supported `rename_all` values: `snake_case`, `SCREAMING_SNAKE_CASE`, `kebab-case`, `camelCase`, `PascalCase`, `lowercase`, `UPPERCASE`.

### Field-level

| Attribute | Effect |
|-----------|--------|
| `primary_key` | Documentation marker |
| `column = "…"` | Override the SQL column name for this field |
| `skip_insert` | Exclude from `INSERT` (keeps in `SELECT`) — for DB-generated columns |
| `skip` | Fully virtual field: excluded from `SELECT` and `INSERT` |

## License

MIT
