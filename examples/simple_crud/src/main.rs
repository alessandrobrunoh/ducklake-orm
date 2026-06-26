use ducklake_orm::{DuckLakeConnection, Table};

// Definiamo un modello che mappa una tabella nel database DuckDB.
// L'ordine dei campi nella struct Rust deve corrispondere esattamente all'ordine delle colonne nella tabella SQL.
#[derive(Table, Debug, Clone)]
#[ducklake(table = "products", schema = "main")]
pub struct Product {
    #[ducklake(primary_key)]
    pub id: i64,
    pub name: String,
    pub price: f64,
    pub active: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Esempio DuckLake ORM: CRUD Semplice ===");

    // 1. Apriamo una connessione a un database DuckDB in-memory
    let db = DuckLakeConnection::open_in_memory()?;
    println!("Connessione in memoria aperta con successo.");

    // 2. Creiamo la tabella corrispondente al nostro modello
    db.execute(
        "CREATE TABLE main.products (id BIGINT PRIMARY KEY, name VARCHAR, price DOUBLE, active BOOLEAN)"
    )?;
    println!("Tabella 'main.products' creata.");

    // 3. Inserimento di record singoli
    println!("\n--- Inserimento di record singoli ---");
    let p1 = Product {
        id: 1,
        name: "Rust Book".to_string(),
        price: 45.99,
        active: true,
    };
    db.insert(p1.clone()).execute()?;
    println!("Inserito: {:?}", p1);

    // 4. Inserimento bulk (molti record alla volta)
    println!("\n--- Inserimento in blocco (Bulk Insert) ---");
    let bulk_products = vec![
        Product {
            id: 2,
            name: "Mechanical Keyboard".to_string(),
            price: 129.50,
            active: true,
        },
        Product {
            id: 3,
            name: "Ergonomic Mouse".to_string(),
            price: 79.99,
            active: false,
        },
        Product {
            id: 4,
            name: "USB-C Hub".to_string(),
            price: 29.99,
            active: true,
        },
    ];
    let inserted_count = db.insert_many(bulk_products).execute()?;
    println!("Inseriti {} prodotti in blocco.", inserted_count);

    // 5. Query di selezione (Select)
    println!("\n--- Selezione di tutti i prodotti ---");
    let all_products: Vec<Product> = db.select::<Product>().fetch_all()?;
    for p in &all_products {
        println!(" - {:?}", p);
    }

    // 6. Filtrare con la DSL tipizzata
    println!("\n--- Selezione con filtri (prezzo > 40 e attivo) ---");
    let filtered_products: Vec<Product> = db
        .select::<Product>()
        .filter(Product::price().gt(40.0).and(Product::active().eq(true)))
        .fetch_all()?;
    for p in &filtered_products {
        println!(" - {:?}", p);
    }

    // 7. Ordinamento e limite
    println!("\n--- Ordinamento per prezzo decrescente e limite di 2 ---");
    let ordered_products: Vec<Product> = db
        .select::<Product>()
        .order_by(Product::price().desc())
        .limit(2)
        .fetch_all()?;
    for p in &ordered_products {
        println!(" - {:?}", p);
    }

    // 8. Aggiornamento (Update)
    println!("\n--- Aggiornamento del prezzo di un prodotto ---");
    let updated_count = db
        .update::<Product>()
        .set(Product::price(), 39.99)
        .filter(Product::id().eq(1i64))
        .execute()?;
    println!("Aggiornati {} record.", updated_count);

    // Verifichiamo l'aggiornamento
    let updated_p1 = db
        .select::<Product>()
        .filter(Product::id().eq(1i64))
        .fetch_one()?;
    println!("Nuovo stato prodotto 1: {:?}", updated_p1);

    // 9. Eliminazione (Delete)
    println!("\n--- Eliminazione di un prodotto non attivo ---");
    let deleted_count = db
        .delete::<Product>()
        .filter(Product::active().eq(false))
        .execute()?;
    println!("Eliminati {} record.", deleted_count);

    // Contiamo i prodotti rimasti
    let remaining_count = db.select::<Product>().count()?;
    println!("Prodotti totali rimasti nel database: {}", remaining_count);

    println!("\n=== Esempio completato con successo ===");
    Ok(())
}
