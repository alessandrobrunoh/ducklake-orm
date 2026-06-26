use ducklake_orm::config::PoolConfig;
use ducklake_orm::migration::{Migrator, SqlMigration};
use ducklake_orm::{DuckLakePool, Table};

// Definiamo un modello per la tabella creata tramite migrazioni
#[derive(Table, Debug, Clone)]
#[ducklake(table = "users", schema = "main")]
pub struct User {
    #[ducklake(primary_key)]
    pub id: i64,
    pub username: String,
    pub email: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Esempio DuckLake ORM: Connection Pool & Migrazioni ===");

    // 1. Configurazione del connection pool r2d2
    let pool_cfg = PoolConfig {
        size: 5, // Impostiamo la dimensione massima del pool a 5 connessioni
        ..PoolConfig::default()
    };

    // Apriamo il pool su un database in-memory.
    // NOTA: Con DuckDB in-memory, ogni connessione aperta nel pool è indipendente.
    // Negli ambienti di produzione, usereste il percorso di un file reale per condividere lo stato tra le connessioni del pool.
    let pool = DuckLakePool::open(":memory:", &pool_cfg)?;
    println!("Connection pool r2d2 inizializzato con successo.");

    // Preleviamo una connessione dal pool per eseguire le migrazioni
    let conn = pool.get()?;
    println!("Connessione ottenuta con successo dal pool per l'applicazione delle migrazioni.");

    // 2. Definizione delle migrazioni
    // Definiamo una migrazione con ID versione, descrizione, SQL di up (applicazione) e SQL di down (rollback)
    let migration_1 = SqlMigration::new(
        1,
        "create_users_table",
        "CREATE TABLE main.users (id BIGINT PRIMARY KEY, username VARCHAR, email VARCHAR)",
        "DROP TABLE main.users",
    );

    // 3. Esecuzione delle migrazioni con il Migrator
    let mut migrator = Migrator::from_pooled(&conn);
    migrator = migrator.add(migration_1);

    // Mostriamo lo stato iniziale delle migrazioni
    println!("\n--- Stato iniziale delle migrazioni ---");
    let status_before = migrator.status()?;
    for stat in &status_before {
        println!(
            "Versione {}: '{}' - Applicata: {}",
            stat.version, stat.description, stat.applied
        );
    }

    // Applichiamo le migrazioni pendenti
    println!("\n--- Applicazione delle migrazioni ---");
    let applied_count = migrator.run()?;
    println!("Applicate {} migrazioni.", applied_count);

    // Verifichiamo il nuovo stato delle migrazioni
    let status_after = migrator.status()?;
    for stat in &status_after {
        println!(
            "Versione {}: '{}' - Applicata: {}",
            stat.version, stat.description, stat.applied
        );
    }

    // 4. Utilizzo del pool con la tabella creata dalla migrazione
    println!("\n--- Utilizzo di un'altra connessione dal pool per operazioni CRUD ---");
    // Preleviamo una nuova connessione
    let _app_conn = pool.get()?;

    // Poiché DuckDB in-memory isola le connessioni in-memory separate, in questo esempio specifico in-memory dobbiamo
    // usare la stessa connessione o ricreare lo schema se usiamo un'altra connessione in memoria.
    // Tuttavia, per dimostrare l'uso tipico di produzione con una connessione del pool dove lo schema esiste,
    // eseguiamo l'inserimento direttamente. Nel nostro caso, per far sì che l'esempio in-memory funzioni,
    // usiamo la connessione 'conn' in cui abbiamo appena creato la tabella.
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

    println!("Record utente inseriti con successo!");

    // Recuperiamo e stampiamo i record inseriti
    let users: Vec<User> = conn.select::<User>().fetch_all()?;
    println!("Utenti trovati nel database:");
    for user in &users {
        println!(" - {:?}", user);
    }

    // 5. Rollback della migrazione
    println!("\n--- Rollback dell'ultima migrazione (1 passo) ---");
    let rolled_back_count = migrator.rollback(1)?;
    println!(
        "Rollback eseguito con successo per {} migrazioni.",
        rolled_back_count
    );

    // Verifichiamo lo stato finale delle migrazioni
    let status_final = migrator.status()?;
    for stat in &status_final {
        println!(
            "Versione {}: '{}' - Applicata: {}",
            stat.version, stat.description, stat.applied
        );
    }

    println!("\n=== Esempio completato con successo ===");
    Ok(())
}
