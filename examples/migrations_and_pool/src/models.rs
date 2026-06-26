//! Modello che mappa la tabella `main.users`, creata tramite migrazioni.

use ducklake_orm::Table;

#[derive(Table, Debug, Clone)]
#[ducklake(table = "users", schema = "main")]
pub struct User {
    #[ducklake(primary_key)]
    pub id: i64,
    pub username: String,
    pub email: String,
}
