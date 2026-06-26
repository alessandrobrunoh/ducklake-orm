//! Operazioni CRUD di esempio: insert, select, update, delete.

use ducklake_orm::DuckLakeConnection;

use crate::models::Product;

/// Inserisce un singolo record e poi un blocco di record multipli.
pub fn demo_insert(
    db: &DuckLakeConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Inserimento di record singoli ---");
    let p1 = Product::new(1, "Rust Book", 45.99, true);
    db.insert(p1.clone()).execute()?;
    println!("Inserito: {p1:?}");

    println!("\n--- Inserimento in blocco (Bulk Insert) ---");
    let bulk_products = vec![
        Product::new(2, "Mechanical Keyboard", 129.50, true),
        Product::new(3, "Ergonomic Mouse", 79.99, false),
        Product::new(4, "USB-C Hub", 29.99, true),
    ];
    let inserted_count = db.insert_many(bulk_products).execute()?;
    println!("Inseriti {inserted_count} prodotti in blocco.");

    Ok(())
}

/// Esempio di `SELECT`: recupero completo, con filtro DSL, con ordinamento.
pub fn demo_select(
    db: &DuckLakeConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Selezione di tutti i prodotti ---");
    let all: Vec<Product> = db.select::<Product>().fetch_all()?;
    for p in &all {
        println!(" - {p:?}");
    }

    println!("\n--- Selezione con filtri (prezzo > 40 e attivo) ---");
    let filtered: Vec<Product> = db
        .select::<Product>()
        .filter(Product::price().gt(40.0).and(Product::active().eq(true)))
        .fetch_all()?;
    for p in &filtered {
        println!(" - {p:?}");
    }

    println!("\n--- Ordinamento per prezzo decrescente e limite di 2 ---");
    let ordered: Vec<Product> = db
        .select::<Product>()
        .order_by(Product::price().desc())
        .limit(2)
        .fetch_all()?;
    for p in &ordered {
        println!(" - {p:?}");
    }

    Ok(())
}

/// Esempio di `UPDATE`: aggiorna il prezzo di un prodotto.
pub fn demo_update(
    db: &DuckLakeConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Aggiornamento del prezzo di un prodotto ---");
    let updated = db
        .update::<Product>()
        .set(Product::price(), 39.99)
        .filter(Product::id().eq(1i64))
        .execute()?;
    println!("Aggiornati {updated} record.");

    let p1 = db
        .select::<Product>()
        .filter(Product::id().eq(1i64))
        .fetch_one()?;
    println!("Nuovo stato prodotto 1: {p1:?}");

    Ok(())
}

/// Esempio di `DELETE`: rimuove i prodotti non attivi.
pub fn demo_delete(
    db: &DuckLakeConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Eliminazione di un prodotto non attivo ---");
    let deleted = db
        .delete::<Product>()
        .filter(Product::active().eq(false))
        .execute()?;
    println!("Eliminati {deleted} record.");

    let remaining = db.select::<Product>().count()?;
    println!("Prodotti totali rimasti nel database: {remaining}");

    Ok(())
}
