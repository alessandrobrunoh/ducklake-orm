//! Modello che mappa la tabella `main.products` nel database DuckDB.
//!
//! L'ordine dei campi nella struct Rust deve corrispondere esattamente
//! all'ordine delle colonne nella tabella SQL.

use ducklake_orm::Table;

#[derive(Table, Debug, Clone)]
#[ducklake(table = "products", schema = "main")]
pub struct Product {
    #[ducklake(primary_key)]
    pub id: i64,
    pub name: String,
    pub price: f64,
    pub active: bool,
}

impl Product {
    /// Crea un nuovo prodotto con i valori forniti.
    pub fn new(id: i64, name: &str, price: f64, active: bool) -> Self {
        Self {
            id,
            name: name.to_string(),
            price,
            active,
        }
    }
}
