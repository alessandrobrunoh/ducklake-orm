# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and test commands

```bash
# Build the workspace
cargo build

# Run all integration tests
cargo test

# Run a single test by name
cargo test <test_name>
# e.g. cargo test filter_gt_amount

# Check compilation without linking
cargo check

# Lint
cargo clippy

# Format
cargo fmt
```

The `.cargo/config.toml` sets `DUCKDB_LIB_DIR=/opt/homebrew/lib` (Homebrew DuckDB on macOS). If DuckDB isn't installed at that path, either install it with `brew install duckdb` or switch to the `bundled` feature and remove that config entry.

## Workspace layout

```
ducklake-orm/           # main library crate (the public API)
  src/
    config.rs           # DuckLakeConfig — ducklake.toml deserialization
    connection.rs       # DuckLakeConnection — single direct connection
    pool.rs             # DuckLakePool + PooledConnection — r2d2 pool
    error.rs            # DuckLakeError — all error variants
    schema/             # DuckLakeTable trait
    query/
      filter.rs         # ColumnExpr, FilterExpr, OrderExpr, SqlValue
      select.rs         # SelectBuilder + SnapshotRef
      insert.rs         # InsertBuilder, BulkInsertBuilder
      update.rs         # UpdateBuilder
      delete.rs         # DeleteBuilder
  tests/integration.rs  # all tests run against in-memory DuckDB

ducklake-orm-macros/    # proc-macro crate (not used directly by consumers)
  src/lib.rs            # #[derive(Table)] implementation
```

## Architecture

### The `#[derive(Table)]` macro

Applying `#[derive(Table)]` to a named-field struct generates:

1. An `impl DuckLakeTable` with `table_name()`, `schema_name()`, `column_names()`, `from_row()` (positional `row.get(N)`), and `to_params()`.
2. A static column accessor method per field (e.g. `Sale::amount()`) that returns a `ColumnExpr`.

**Critical invariant**: struct field order must exactly match the `CREATE TABLE` column order because `from_row` uses positional index access.

Struct-level attributes: `#[ducklake(table = "name", schema = "name")]` (defaults: snake_case of struct name, `"main"`).  
Field-level: `#[ducklake(primary_key)]` (documentation only for now).

### Query builders

All builders are obtained from `DuckLakeConnection` or `pool::PooledConnection`:

| Method | Builder | SQL |
|--------|---------|-----|
| `db.select::<T>()` | `SelectBuilder` | `SELECT … FROM … WHERE … GROUP BY … ORDER BY … LIMIT …` |
| `db.insert(record)` | `InsertBuilder` | `INSERT INTO … VALUES (…)` |
| `db.insert_many(vec)` | `BulkInsertBuilder` | Same, wrapped in `BEGIN`/`COMMIT` |
| `db.update::<T>()` | `UpdateBuilder` | `UPDATE … SET … WHERE …` |
| `db.delete::<T>()` | `DeleteBuilder` | `DELETE FROM … WHERE …` |

**Safety guardrails**: `UpdateBuilder::execute()` and `DeleteBuilder::execute()` return `Err(DuckLakeError::Query(…))` if no `.filter()` was set. To intentionally affect all rows, call `.update_all()` / `.delete_all()` instead.

### Filter DSL

`ColumnExpr` (from `Sale::amount()`) → `FilterExpr` via comparison methods:
- `.eq(v)`, `.ne(v)`, `.gt(v)`, `.gte(v)`, `.lt(v)`, `.lte(v)`
- `.like(pattern)`, `.between(lo, hi)`, `.is_null()`, `.is_not_null()`
- `.and(other)` / `.or(other)` on `FilterExpr` to combine predicates

`ColumnExpr::asc()` / `::desc()` → `OrderExpr` passed to `SelectBuilder::order_by()`.

`SqlValue` is the internal scalar type; all scalar Rust primitives (`i16/i32/i64`, `f32/f64`, `String`/`&str`, `bool`) convert via `Into<SqlValue>`.

### DuckLake time travel

After `DuckLakeConnection::attach_ducklake(catalog_path, catalog_name)`, `SelectBuilder` gains:
- `.at_snapshot(n: u64)` — query at a specific snapshot version
- `.at_timestamp("2025-06-01T00:00:00Z")` — query at a point in time

Table references become three-part: `<catalog_name>.<schema>.<table>`.

### Connection pool

`DuckLakePool` wraps r2d2. `pool.get()` returns a `PooledConnection` that auto-returns on drop. `PooledConnection` exposes the same `insert`, `select`, `update`, `delete`, `execute` API as `DuckLakeConnection`.

### Configuration

`ducklake.toml` (see `ducklake.toml` at repo root for a full example) is loaded with `DuckLakeConfig::from_file(path)`. Required: `[database] path`. Optional: `[pool]` (defaults: size=4, timeout=30s) and `[ducklake]` (catalog attachment).
