# ducklake-orm-macros

[![crates.io](https://img.shields.io/crates/v/ducklake-orm-macros.svg)](https://crates.io/crates/ducklake-orm-macros)
[![docs.rs](https://docs.rs/ducklake-orm-macros/badge.svg)](https://docs.rs/ducklake-orm-macros)
[![license](https://img.shields.io/crates/l/ducklake-orm-macros.svg)](LICENSE)

Procedural macros for [`ducklake-orm`](https://crates.io/crates/ducklake-orm).

**This crate is not meant to be used directly.** Add `ducklake-orm` to your dependencies instead — the `#[derive(Table)]` macro is re-exported from there.

```toml
[dependencies]
ducklake-orm = "0.1"
```

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

See the [`ducklake-orm` documentation](https://docs.rs/ducklake-orm) for full usage.

## License

MIT
