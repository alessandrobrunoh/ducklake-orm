/// Core trait implemented by the `#[derive(Table)]` macro.
///
/// Provides the ORM with all static metadata needed to build SQL queries
/// and map result rows back to Rust structs.
pub trait DuckLakeTable: Sized {
    /// SQL table name (without schema/catalog prefix).
    fn table_name() -> &'static str;

    /// Schema that contains the table (default: "main").
    fn schema_name() -> &'static str;

    /// Ordered list of column names — must match field declaration order.
    fn column_names() -> &'static [&'static str];

    /// Deserialize a DuckDB result row into Self.
    fn from_row(row: &duckdb::Row<'_>) -> duckdb::Result<Self>;

    /// Serialize Self into positional SQL parameters for INSERT/UPDATE.
    fn to_params(&self) -> Vec<Box<dyn duckdb::types::ToSql>>;
}
