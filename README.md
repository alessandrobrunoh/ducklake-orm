# ducklake-orm

**Type-safe Rust ORM for [DuckLake](https://ducklake.select) — compile-time query validation, connection pools, DuckLake-native time travel, and schema migrations.**

[![Crates.io](https://img.shields.io/crates/v/ducklake-orm)](https://crates.io/crates/ducklake-orm)
[![Docs.rs](https://docs.rs/ducklake-orm/badge.svg)](https://docs.rs/ducklake-orm)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

---

## What is DuckLake?

[DuckLake](https://ducklake.select) (v1.0) is an open lakehouse format that stores metadata in SQL and data as Parquet files. It is accessed via a DuckDB extension and supports **time travel** (snapshot versioning), partitioning, deletion vectors, and more.

`ducklake-orm` gives you a Rust-native, type-safe API on top of DuckLake and plain DuckDB — without the overhead of an async runtime or the limitations of existing ORMs that cannot be extended to new backends.

---

## Features

- **Compile-time safety** — column names and types are checked by the Rust compiler via `#[derive(Table)]`
- **Fluent query builder** — `SELECT`, `INSERT`, `UPDATE`, `DELETE` with chainable methods
- **DuckLake time travel** — `.at_snapshot(n)` and `.at_timestamp("…")` out of the box
- **Connection pool** — powered by `r2d2`; configure via `ducklake.toml`
- **Analytics** — `GROUP BY`, `HAVING`, `ORDER BY`, and aggregate functions (`SUM`, `AVG`, `MIN`, `MAX`)
- **Rich filter DSL** — comparisons, `LIKE`, `BETWEEN`, `IN`, `NOT IN`, `IS NULL`, compound `AND`/`OR`
- **Upsert** — `INSERT OR REPLACE` and `INSERT OR IGNORE` with `.or_replace()` / `.or_ignore()`
- **`RETURNING`** — retrieve server-generated values after an insert without a second query
- **Raw SQL escape hatch** — `select_raw::<T>()` for window functions, CTEs, and complex joins
- **Schema migrations** *(opt-in feature)* — versioned, transactional, with `up`/`down` support
- **Safety guardrails** — `UPDATE` and `DELETE` require explicit filters (or an explicit `_all` variant)
- **No `unwrap()`** — all public APIs return `Result<_, DuckLakeError>`

---

## Installation

```toml
[dependencies]
ducklake-orm = "0.1"
```

To use the migration system, enable the `migrations` feature:

```toml
[dependencies]
ducklake-orm = { version = "0.1", features = ["migrations"] }
```

If DuckDB is **not** installed system-wide, enable `bundled` to compile it from source:

```toml
[dependencies]
ducklake-orm = { version = "0.1", features = ["bundled"] }
```

### System requirement (without `bundled`)

DuckDB ≥ 1.5.2 must be installed and discoverable by the linker.

```bash
# macOS
brew install duckdb

# Linux — download from https://github.com/duckdb/duckdb/releases
```

If you installed DuckDB in a non-standard path, set `DUCKDB_LIB_DIR`:

```bash
DUCKDB_LIB_DIR=/opt/duckdb/lib cargo build
```

Or pin it permanently in `.cargo/config.toml`:

```toml
[env]
DUCKDB_LIB_DIR = "/opt/homebrew/lib"
```

---

## Quick start

### 1. Define a table

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

The `#[derive(Table)]` macro generates:
- `DuckLakeTable` implementation (column list, row deserialization, parameter serialization)
- Type-safe column accessor methods (`Sale::id()`, `Sale::amount()`, `Sale::region()`)

### 2. Connect and query

```rust
use ducklake_orm::DuckLakeConnection;

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

// UPDATE (filter required — no accidental full-table writes)
db.update::<Sale>()
    .set(Sale::region(), "EMEA")
    .filter(Sale::id().eq(1i64))
    .execute()?;

// DELETE (filter required)
db.delete::<Sale>()
    .filter(Sale::id().eq(1i64))
    .execute()?;
```

### 3. DuckLake time travel

```rust
let mut db = DuckLakeConnection::open_in_memory()?;
db.attach_ducklake("catalog.duckdb", "lake")?;

// Read as of snapshot version 42
let snapshot: Vec<Sale> = db
    .select::<Sale>()
    .at_snapshot(42)
    .fetch_all()?;

// Read as of a point in time
let historical: Vec<Sale> = db
    .select::<Sale>()
    .at_timestamp("2025-01-01T00:00:00Z")
    .fetch_all()?;
```

---

## Configuration file

Create a `ducklake.toml` in your project root:

```toml
[database]
path = "data/warehouse.duckdb"   # or ":memory:"

[pool]
size = 8
connection_timeout_secs = 30

[ducklake]
catalog_path = "data/catalog.duckdb"
catalog_name = "lake"
auto_attach  = true              # INSTALL + LOAD + ATTACH on every connection
```

Load it at startup:

```rust
use ducklake_orm::{DuckLakeConfig, DuckLakePool};

let config = DuckLakeConfig::from_file("ducklake.toml")?;
let pool   = DuckLakePool::from_config(&config)?;
```

---

## Connection pool

```rust
use ducklake_orm::{DuckLakeConfig, DuckLakePool};

let config = DuckLakeConfig::from_file("ducklake.toml")?;
let pool   = DuckLakePool::from_config(&config)?;

// Check out a connection — returned to pool automatically on drop
let conn = pool.get()?;

let rows: Vec<Sale> = conn
    .select::<Sale>()
    .filter(Sale::region().eq("EU"))
    .fetch_all()?;
```

`PooledConnection` exposes the same `select`, `insert`, `insert_many`, `update`, `delete`, and `select_raw` methods as `DuckLakeConnection`.

---

## Filter API

| Method | SQL |
|--------|-----|
| `.eq(val)` | `col = $n` |
| `.ne(val)` | `col != $n` |
| `.gt(val)` | `col > $n` |
| `.gte(val)` | `col >= $n` |
| `.lt(val)` | `col < $n` |
| `.lte(val)` | `col <= $n` |
| `.like(pattern)` | `col LIKE $n` |
| `.between(lo, hi)` | `col BETWEEN $lo AND $hi` |
| `.in_(iter)` | `col IN ($1, $2, …)` |
| `.not_in(iter)` | `col NOT IN ($1, $2, …)` |
| `.is_null()` | `col IS NULL` |
| `.is_not_null()` | `col IS NOT NULL` |
| `.and(other)` | `(left AND right)` |
| `.or(other)` | `(left OR right)` |

```rust
// Compound predicates
let rows: Vec<Sale> = db
    .select::<Sale>()
    .filter(Sale::region().in_(["EU", "UK", "US"]))
    .filter(Sale::amount().gt(50.0).and(Sale::amount().lte(500.0)))
    .fetch_all()?;
```

---

## Analytics

### GROUP BY / HAVING

```rust
#[derive(Table, Debug)]
#[ducklake(table = "sales", schema = "main")]
pub struct RegionGroup {
    pub region: String,
}

let groups: Vec<RegionGroup> = db
    .select::<RegionGroup>()
    .group_by("region")
    .order_by(RegionGroup::region().asc())
    .fetch_all()?;
```

### Aggregate functions

`SUM`, `AVG`, `MIN`, and `MAX` are terminal methods on `SelectBuilder`. They return `Option<R>` — `None` when no rows match.

```rust
let total:   Option<f64> = db.select::<Sale>().sum(Sale::amount())?;
let average: Option<f64> = db.select::<Sale>().avg(Sale::amount())?;
let minimum: Option<f64> = db.select::<Sale>().min(Sale::amount())?;
let maximum: Option<f64> = db.select::<Sale>().max(Sale::amount())?;

// With filters
let eu_total: Option<f64> = db
    .select::<Sale>()
    .filter(Sale::region().eq("EU"))
    .sum(Sale::amount())?;
```

---

## Upsert

Use `.or_replace()` to overwrite a conflicting row, or `.or_ignore()` to silently skip it:

```rust
// INSERT OR REPLACE — old row is deleted and new one inserted
db.insert(Sale { id: 1, amount: 999.0, region: "EMEA".into() })
    .or_replace()
    .execute()?;

// INSERT OR IGNORE — keeps the existing row if id = 1 already exists
let affected = db
    .insert(Sale { id: 1, amount: 0.0, region: "APAC".into() })
    .or_ignore()
    .execute()?;
// affected == 0 if the row was skipped
```

---

## RETURNING

Retrieve a column value from the inserted row without a second `SELECT`:

```rust
let inserted_id: i64 = db
    .insert(Sale { id: 42, amount: 99.9, region: "EU".into() })
    .execute_returning(Sale::id())?;

assert_eq!(inserted_id, 42);
```

---

## Raw SQL escape hatch

For queries the builders cannot express (window functions, CTEs, complex JOINs), use `select_raw`:

```rust
use ducklake_orm::query::SqlValue;

let rows: Vec<Sale> = db.select_raw::<Sale>(
    "SELECT id, amount, region
     FROM main.sales
     WHERE region = $1
     ORDER BY amount DESC
     LIMIT 5",
    &[SqlValue::Text("EU".into())],
)?;
```

Column order in the SQL must match the struct field order.

---

## Safety guardrails

`UPDATE` and `DELETE` without a filter return `Err(DuckLakeError::Query(…))`:

```rust
// ❌ Returns Err — filter is required
db.update::<Sale>().set(Sale::amount(), 0.0).execute()?;

// ✅ Explicit opt-in for full-table operations
db.update::<Sale>().set(Sale::amount(), 0.0).update_all()?;
db.delete::<Sale>().delete_all()?;
```

---

## Schema migrations

Enable the `migrations` feature in `Cargo.toml`:

```toml
[dependencies]
ducklake-orm = { version = "0.1", features = ["migrations"] }
```

Define migrations with `SqlMigration` (or implement the `Migration` trait for Rust-based logic):

```rust
use ducklake_orm::migration::{Migrator, SqlMigration};

let migrator = Migrator::new(&db)
    .add(SqlMigration::new(
        1,
        "create sales table",
        // up
        "CREATE TABLE IF NOT EXISTS main.sales (
             id     BIGINT PRIMARY KEY,
             amount DOUBLE NOT NULL,
             region VARCHAR NOT NULL
         )",
        // down
        "DROP TABLE IF EXISTS main.sales",
    ))
    .add(SqlMigration::new(
        2,
        "add sold_at column",
        "ALTER TABLE main.sales ADD COLUMN sold_at TIMESTAMP",
        "ALTER TABLE main.sales DROP COLUMN sold_at",
    ));

// Apply all pending migrations — safe to call on every startup
let applied = migrator.run()?;
println!("{applied} migration(s) applied");

// Inspect status
for s in migrator.status()? {
    println!(
        "v{:<4} [{:^7}] {}",
        s.version,
        if s.applied { "applied" } else { "pending" },
        s.description,
    );
}

// Roll back the last migration
migrator.rollback(1)?;
```

### How it works

- Applied migrations are tracked in `main._ducklake_migrations` (created automatically).
- Each migration runs inside a `BEGIN` / `COMMIT` transaction — if it fails, the transaction is rolled back and the migration is not recorded.
- `run()` is **idempotent**: calling it multiple times applies only the pending migrations.
- Duplicate version numbers registered in the same `Migrator` are detected before any SQL runs.

### Custom migrations (Rust logic)

```rust
use ducklake_orm::{DuckLakeError, migration::Migration};

struct SeedData;

impl Migration for SeedData {
    fn version(&self) -> i64 { 3 }
    fn description(&self) -> &str { "seed initial data" }

    fn up(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
        conn.execute_batch(
            "INSERT INTO main.sales VALUES (1, 100.0, 'EU');
             INSERT INTO main.sales VALUES (2, 200.0, 'US');"
        )?;
        Ok(())
    }

    fn down(&self, conn: &duckdb::Connection) -> Result<(), DuckLakeError> {
        conn.execute_batch("DELETE FROM main.sales WHERE id IN (1, 2)")?;
        Ok(())
    }
}
```

---

## Examples

We provide two complete example crates under `examples/` directory to help you understand how `ducklake-orm` works in real-world scenarios:

1. **`simple_crud`**: Demonstrates the basic CRUD operations.
   - Defines a `Product` table model using `#[derive(Table)]`.
   - Opens an in-memory DuckDB connection.
   - Inserts records individually and using bulk/batch inserts.
   - Queries data using the typed DSL with filters, orderings, and limits.
   - Updates and deletes records safely.
   - Runs with:
     ```bash
     cargo run --bin simple_crud
     ```

2. **`migrations_and_pool`**: Demonstrates connection pooling and schema migrations (requires `migrations` feature).
   - Initializes an `r2d2` connection pool (`DuckLakePool`) with a custom configuration.
   - Handles versioned database migrations dynamically using the `Migrator` and `SqlMigration`.
   - Illustrates migration application and transactional rollback.
   - Runs with:
     ```bash
     cargo run --bin migrations_and_pool
     ```

---

## Workspace layout

```
ducklake-orm/
├── Cargo.toml                    # workspace root
├── ducklake.toml                 # example config (copy to your project)
├── ducklake-orm/                 # main library crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config.rs             # DuckLakeConfig (ducklake.toml deserialization)
│   │   ├── connection.rs         # DuckLakeConnection
│   │   ├── pool.rs               # DuckLakePool + PooledConnection
│   │   ├── error.rs              # DuckLakeError
│   │   ├── schema/table.rs       # DuckLakeTable trait
│   │   ├── migration/            # migrations feature
│   │   │   ├── migration.rs      # Migration trait + SqlMigration + MigrationStatus
│   │   │   └── migrator.rs       # Migrator runner
│   │   └── query/
│   │       ├── filter.rs         # ColumnExpr, FilterExpr, SqlValue, OrderExpr
│   │       ├── select.rs         # SelectBuilder (+ SUM/AVG/MIN/MAX)
│   │       ├── insert.rs         # InsertBuilder (+ upsert + RETURNING), BulkInsertBuilder
│   │       ├── update.rs         # UpdateBuilder
│   │       └── delete.rs         # DeleteBuilder
│   └── tests/integration.rs      # 39 integration tests
└── ducklake-orm-macros/          # proc-macro crate (#[derive(Table)])
```

---

## Cargo features

| Feature | Default | Description |
|---------|---------|-------------|
| `bundled` | No | Compile DuckDB from source (no system install needed) |
| `migrations` | No | Enable `ducklake_orm::migration` — versioned schema migrations |

---

## Roadmap

### Query builder
- [ ] **Transaction API** — `db.transaction(|tx| { tx.insert(…)?; tx.update(…)?; Ok(()) })` for grouping multiple ORM operations into one atomic `BEGIN`/`COMMIT`
- [ ] **Type-safe `JOIN`** — fluent join builder between two `DuckLakeTable` types with compile-time column safety
- [ ] **Window functions** — `ROW_NUMBER`, `RANK`, `LAG`, `LEAD`, `SUM OVER (PARTITION BY …)`, etc.
- [ ] **Column selection** — `.columns([Sale::amount(), Sale::region()])` to `SELECT` a subset of columns instead of always `SELECT *`
- [ ] **`DISTINCT`** — `.distinct()` modifier on `SelectBuilder`
- [ ] **`NOT` operator** — `.not(expr)` to negate any `FilterExpr`
- [ ] **`ON CONFLICT DO UPDATE SET`** — targeted upsert that updates only specific columns (more precise than `INSERT OR REPLACE` which deletes + reinserts)
- [ ] **Pagination helper** — `.paginate(page, per_page)` as a shorthand for `.limit(n).offset(page * n)`

### Schema & codegen
- [ ] **`create_table_sql()`** — static method generated by `#[derive(Table)]` that returns the `CREATE TABLE IF NOT EXISTS …` SQL for the struct, useful for test setup and initial schema bootstrapping
- [ ] **Struct codegen from existing schema** — CLI / proc-macro that reads an existing DuckDB or DuckLake catalog and generates annotated Rust structs, lowering the adoption barrier for existing databases
- [ ] **Composite primary keys** — support multiple `#[ducklake(primary_key)]` fields on the same struct
- [ ] **Automatic timestamps** — `#[ducklake(created_at)]` / `#[ducklake(updated_at)]` attributes that auto-fill `CURRENT_TIMESTAMP` on insert/update

### DuckLake-specific
- [ ] **`COPY TO` / `COPY FROM`** — export query results to Parquet/CSV and import from files directly through the ORM, leveraging DuckDB's native `COPY` statement
- [ ] **Soft deletes** — `#[ducklake(soft_delete)]` struct attribute that adds a `deleted_at` column and transparently filters it out of all queries unless explicitly opted in with `.include_deleted()`
- [ ] **Snapshot diff** — compare two snapshot versions of a table and return added/removed/changed rows

### Developer experience
- [ ] **Migration CLI** — `ducklake-orm migrate up / down / status` binary that picks up migrations from a Rust file or a `migrations/` SQL directory
- [ ] **Async support** — `tokio::task::spawn_blocking` wrapper so `select`, `insert`, etc. can be `.await`-ed in async runtimes without blocking the executor

---

## License

MIT — see [LICENSE](LICENSE).
