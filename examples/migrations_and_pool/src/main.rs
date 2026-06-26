//! Esempio DuckLake ORM: Connection Pool e Migrazioni file-based.
//!
//! Dimostra:
//! - configurazione di un pool r2d2 (`DuckLakePool`),
//! - migrazioni **file-based** caricate da una directory con `Migrator::add_directory`,
//! - convenzione `V<versione>__<descrizione>.up.sql` / `.down.sql`,
//! - operazioni CRUD su una tabella creata tramite migrazione,
//! - rollback di una migration reversibile,
//! - migration irreversibile registrata da codice con `SqlMigration::new_irreversible`.

use std::path::PathBuf;

use ducklake_orm::config::PoolConfig;
use ducklake_orm::migration::{Migrator, SqlMigration};
use ducklake_orm::DuckLakePool;

use models::User;

mod models;

fn print_status(status: &[ducklake_orm::migration::MigrationStatus]) {
    for stat in status {
        let tag = if stat.applied { "[x]" } else { "[ ]" };
        println!("  {tag} v{} - {}", stat.version, stat.description);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Esempio DuckLake ORM: Pool & Migrazioni ===\n");

    // Path della cartella `migrations/` rispetto al crate dell'esempio.
    // V1 e V2 sono reversibili (hanno `.up.sql` e `.down.sql`).
    let migrations_dir: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    println!("Cartella migrazioni: {}", migrations_dir.display());

    // 1. Connection pool r2d2.
    //    NOTA: con DuckDB in-memory ogni connessione del pool è indipendente;
    //    usiamo sempre la stessa `conn` per mantenere lo stato.
    //    In produzione si usa il percorso di un file reale.
    let pool = DuckLakePool::open(
        ":memory:",
        &PoolConfig { size: 5, ..PoolConfig::default() },
    )?;
    let conn = pool.get()?;
    println!("Connessione ottenuta dal pool.");

    // 2. Carica V1 e V2 dalla directory (entrambe reversibili).
    let migrator = Migrator::from_pooled(&conn).add_directory(&migrations_dir)?;

    println!("\n--- Stato iniziale ---");
    print_status(&migrator.status()?);

    // 3. Applica le migrazioni pendenti (idempotente).
    println!("\n--- Applicazione migrazioni da directory ---");
    let applied = migrator.run()?;
    println!("Applicate {applied} migrazioni.");
    print_status(&migrator.status()?);

    // 4. CRUD sulla tabella creata.
    println!("\n--- Inserimento utenti ---");
    conn.insert(User {
        id: 1,
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
    })
    .execute()?;
    conn.insert(User {
        id: 2,
        username: "bob".to_string(),
        email: "bob@example.com".to_string(),
    })
    .execute()?;

    let users: Vec<User> = conn.select::<User>().fetch_all()?;
    println!("Utenti trovati:");
    for u in &users {
        println!("  - {u:?}");
    }

    // 5. Rollback di V2 (reversibile) — successo.
    println!("\n--- Rollback di 1 step (V2 è reversibile) ---");
    let rolled = migrator.rollback(1)?;
    println!("Rollback eseguito per {rolled} migrazioni.");
    print_status(&migrator.status()?);

    // 6. Ora registriamo una migration V3 irreversibile da codice
    //    e mostriamo che il rollback viene rifiutato con un errore chiaro.
    println!("\n--- Migration irreversibile (V3, da codice) ---");
    let migrator = migrator.add(SqlMigration::new_irreversible(
        3,
        // `description` accetta anche `String` owned, non solo `&'static str`:
        format!("drop legacy column ({})", "demo"),
        "ALTER TABLE main.users DROP COLUMN IF EXISTS legacy_flag",
    ));
    let applied = migrator.run()?;
    println!("Applicate {applied} migrazioni.");
    print_status(&migrator.status()?);

    println!("\n--- Tentativo di rollback di V3 (atteso: errore) ---");
    match migrator.rollback(1) {
        Ok(n) => println!("Rollback eseguito per {n} migrazioni (inaspettato)."),
        Err(e) => println!("Rollback bloccato come previsto:\n  {e}"),
    }

    println!("\n=== Esempio completato ===");
    Ok(())
}
