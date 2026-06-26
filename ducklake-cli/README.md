# ducklake-cli

CLI base per il workspace `ducklake-orm`.

## Stato attuale

Attualmente la CLI espone un solo comando:

```bash
cargo run -p ducklake-cli -- hello
```

Output:

```text
Hello World
```

## Obiettivo della crate

Questa crate nasce come base semplice, ordinata ed estensibile per i futuri comandi CLI del progetto.

## Struttura

```text
ducklake-cli/
├── Cargo.toml
├── README.md
└── src/
    ├── app.rs
    ├── main.rs
    └── commands/
        ├── mod.rs
        └── hello.rs
```

## Come estenderla

### 1. Aggiungere un nuovo comando

- creare un nuovo file in `src/commands/`
- registrare il comando in `src/commands/mod.rs`
- gestire il dispatch in `src/app.rs`

### 2. Possibili comandi futuri

- `init` — crea una configurazione `ducklake.toml`
- `connect` — verifica la connessione al database
- `migrate` — esegue migration up/down
- `query` — esegue query SQL raw
- `schema` — mostra tabelle e colonne
- `pool` — testa la configurazione del pool
- `version` — mostra versione CLI e crate collegate

## Possibili miglioramenti futuri

- parser argomenti con `clap`
- supporto flag globali (`--help`, `--version`, `--verbose`)
- output strutturato (`table`, `json`)
- gestione errori con enum dedicato
- test per parsing e dispatch dei comandi

## Sviluppo

Verifica compilazione:

```bash
cargo check -p ducklake-cli
```
