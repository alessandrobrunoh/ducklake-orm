//! # ducklake-orm-macros
//!
//! Procedural macros for `ducklake-orm`. This crate is not meant to be used
//! directly — import it via the re-export in `ducklake-orm`:
//!
//! ```toml
//! [dependencies]
//! ducklake-orm = "0.1"
//! ```
//!
//! Then use the derive macro:
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

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derive macro that implements [`DuckLakeTable`](ducklake_orm::schema::DuckLakeTable) for a
/// struct and generates type-safe column accessor methods used in query builders.
///
/// # What it generates
///
/// Given a struct:
///
/// ```rust,ignore
/// #[derive(Table, Debug)]
/// #[ducklake(table = "sales", schema = "main")]
/// pub struct Sale {
///     #[ducklake(primary_key)]
///     pub id: i64,
///     pub amount: f64,
///     pub region: String,
/// }
/// ```
///
/// The macro emits:
///
/// 1. **`DuckLakeTable` implementation** with:
///    - `table_name()` → `"sales"`
///    - `schema_name()` → `"main"`
///    - `column_names()` → `&["id", "amount", "region"]`
///    - `from_row(row)` — deserialises a `duckdb::Row` by positional index
///    - `to_params(&self)` — serialises the struct's fields as DuckDB parameters
///
/// 2. **Static column accessor methods** on the struct itself:
///    - `Sale::id()` → [`ColumnExpr`](ducklake_orm::query::ColumnExpr)
///    - `Sale::amount()` → [`ColumnExpr`](ducklake_orm::query::ColumnExpr)
///    - `Sale::region()` → [`ColumnExpr`](ducklake_orm::query::ColumnExpr)
///
///    These are the entry points for building type-safe filter and ordering
///    expressions:
///
///    ```rust,ignore
///    Sale::amount().gt(100.0)      // FilterExpr::Gt
///    Sale::region().eq("EU")       // FilterExpr::Eq
///    Sale::amount().desc()         // OrderExpr (DESC)
///    ```
///
/// # Attributes
///
/// ## Struct-level: `#[ducklake(…)]`
///
/// | Attribute | Type | Default | Description |
/// |-----------|------|---------|-------------|
/// | `table = "name"` | string literal | struct name in `snake_case` | SQL table name |
/// | `schema = "name"` | string literal | `"main"` | SQL schema name |
///
/// ```rust,ignore
/// // Use defaults: table = "sale", schema = "main"
/// #[derive(Table)]
/// pub struct Sale { … }
///
/// // Override both:
/// #[derive(Table)]
/// #[ducklake(table = "sales_2024", schema = "analytics")]
/// pub struct Sale { … }
/// ```
///
/// ## Field-level: `#[ducklake(primary_key)]`
///
/// Marks a field as the primary key. Currently used for documentation purposes
/// and may influence future code generation (e.g., `fetch_by_pk` helpers).
///
/// ```rust,ignore
/// #[derive(Table)]
/// pub struct Sale {
///     #[ducklake(primary_key)]
///     pub id: i64,     // ← primary key
///     pub amount: f64,
/// }
/// ```
///
/// # Column order contract
///
/// The order of fields in the struct **must exactly match** the column order
/// in the DuckDB `CREATE TABLE` statement. This is because `from_row` uses
/// positional (`row.get(0)`, `row.get(1)`, …) access and `to_params` produces
/// parameters in declaration order.
///
/// If the column order in the database differs from the struct field order,
/// you will get incorrect values or type errors at runtime.
///
/// # Type compatibility
///
/// Any field type that implements both:
/// - `duckdb::types::ToSql` (for INSERT / UPDATE parameters)
/// - `duckdb::types::FromSql` (for SELECT row deserialisation)
///
/// is supported. Common Rust types and their DuckDB equivalents:
///
/// | Rust type | DuckDB type |
/// |-----------|------------|
/// | `i64` | `BIGINT` |
/// | `i32` | `INTEGER` |
/// | `f64` | `DOUBLE` |
/// | `String` | `VARCHAR` |
/// | `bool` | `BOOLEAN` |
/// | `Option<T>` | nullable column |
/// | `chrono::DateTime<Utc>` | `TIMESTAMPTZ` (requires `chrono` feature) |
///
/// # Errors at compile time
///
/// The macro emits a compile-time error if applied to:
/// - An `enum` or `union` (only structs are supported)
/// - A struct with tuple or unit fields (only named fields are supported)
///
/// ```text
/// error: #[derive(Table)] only supports structs with named fields
/// ```
///
/// # Full example
///
/// ```rust,no_run
/// use ducklake_orm::{DuckLakeConnection, Table};
///
/// #[derive(Table, Debug, Clone)]
/// #[ducklake(table = "sales", schema = "main")]
/// pub struct Sale {
///     #[ducklake(primary_key)]
///     pub id: i64,
///     pub amount: f64,
///     pub region: String,
/// }
///
/// let db = DuckLakeConnection::open_in_memory().unwrap();
/// db.execute(
///     "CREATE TABLE main.sales (id BIGINT, amount DOUBLE, region VARCHAR)",
///     [],
/// ).unwrap();
///
/// // Insert
/// db.insert(Sale { id: 1, amount: 250.0, region: "EU".into() })
///   .execute()
///   .unwrap();
///
/// // Query with type-safe filters
/// let rows: Vec<Sale> = db
///     .select::<Sale>()
///     .filter(Sale::amount().gt(100.0))
///     .fetch_all()
///     .unwrap();
/// ```
#[proc_macro_derive(Table, attributes(ducklake))]
pub fn derive_table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    let default_table = to_snake_case(&struct_name.to_string());
    let mut table_name = default_table;
    let mut schema_name = "main".to_string();

    for attr in &input.attrs {
        if attr.path().is_ident("ducklake") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("table") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    table_name = value.value();
                } else if meta.path.is_ident("schema") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    schema_name = value.value();
                }
                Ok(())
            });
        }
    }

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new(
                    Span::call_site(),
                    "#[derive(Table)] only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new(Span::call_site(), "#[derive(Table)] only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let field_idents: Vec<_> = named_fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();

    let field_name_strs: Vec<String> = field_idents.iter().map(|f| f.to_string()).collect();

    let field_indices: Vec<usize> = (0..field_idents.len()).collect();

    let col_methods: Vec<_> = field_idents
        .iter()
        .map(|ident| {
            let col_name = ident.to_string();
            quote! {
                /// Returns a [`ColumnExpr`](::ducklake_orm::query::ColumnExpr) handle for this
                /// column, which can be used to build filter and ordering expressions.
                ///
                /// ```rust,ignore
                /// Sale::amount().gt(100.0)   // WHERE amount > 100.0
                /// Sale::amount().desc()      // ORDER BY amount DESC
                /// ```
                pub fn #ident() -> ::ducklake_orm::query::ColumnExpr {
                    ::ducklake_orm::query::ColumnExpr::new(#col_name)
                }
            }
        })
        .collect();

    let from_row_fields = field_idents
        .iter()
        .zip(field_indices.iter())
        .map(|(ident, idx)| {
            quote! { #ident: row.get(#idx)? }
        });

    let to_params_fields = field_idents.iter().map(|ident| {
        quote! {
            Box::new(self.#ident.clone()) as Box<dyn ::duckdb::types::ToSql>
        }
    });

    let expanded = quote! {
        impl ::ducklake_orm::schema::DuckLakeTable for #struct_name {
            fn table_name() -> &'static str {
                #table_name
            }

            fn schema_name() -> &'static str {
                #schema_name
            }

            fn column_names() -> &'static [&'static str] {
                &[#(#field_name_strs),*]
            }

            fn from_row(row: &::duckdb::Row<'_>) -> ::duckdb::Result<Self> {
                Ok(Self {
                    #(#from_row_fields),*
                })
            }

            fn to_params(&self) -> Vec<Box<dyn ::duckdb::types::ToSql>> {
                vec![#(#to_params_fields),*]
            }
        }

        impl #struct_name {
            #(#col_methods)*
        }
    };

    TokenStream::from(expanded)
}

/// Convert a `PascalCase` or `camelCase` identifier to `snake_case`.
///
/// Used to derive the default SQL table name from the struct name when no
/// `#[ducklake(table = "…")]` attribute is present.
///
/// ## Examples
///
/// | Input | Output |
/// |-------|--------|
/// | `Sale` | `sale` |
/// | `SaleItem` | `sale_item` |
/// | `HTTPRequest` | `h_t_t_p_request` |
fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
