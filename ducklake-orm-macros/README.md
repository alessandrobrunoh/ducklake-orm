# ducklake-orm-macros

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
