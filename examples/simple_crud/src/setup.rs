//! Inizializzazione della connessione e dello schema.

use ducklake_orm::DuckLakeConnection;

/// Apre un database in-memory e crea la tabella `main.products`.
pub fn setup() -> Result<DuckLakeConnection, Box<dyn std::error::Error>> {
    let db = DuckLakeConnection::open_in_memory()?;
    db.execute(
        "CREATE TABLE main.products (\
         id BIGINT PRIMARY KEY, \
         name VARCHAR, \
         price DOUBLE, \
         active BOOLEAN\
         )",
    )?;
    Ok(db)
}
