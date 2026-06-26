//! Esempio DuckLake ORM: CRUD Semplice.
//!
//! Dimostra insert (singolo + bulk), select con filtri/ordinamento,
//! update e delete usando il builder DSL tipizzato.

mod models;
mod ops;
mod setup;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Esempio DuckLake ORM: CRUD Semplice ===\n");

    let db = setup::setup()?;
    println!("Connessione in memoria aperta, tabella 'main.products' creata.\n");

    ops::demo_insert(&db)?;
    ops::demo_select(&db)?;
    ops::demo_update(&db)?;
    ops::demo_delete(&db)?;

    println!("\n=== Esempio completato con successo ===");
    Ok(())
}
