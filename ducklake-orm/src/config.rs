//! Configuration types for `ducklake-orm`, deserialized from `ducklake.toml`.
//!
//! The recommended way to configure the ORM is to place a `ducklake.toml` file
//! in your project root and load it with [`DuckLakeConfig::from_file`]:
//!
//! ```toml
//! # ducklake.toml
//!
//! [database]
//! path = "data/warehouse.duckdb"   # ":memory:" is also valid
//!
//! [pool]
//! size = 8
//! connection_timeout_secs = 30
//!
//! [ducklake]
//! catalog_path = "data/catalog.duckdb"
//! catalog_name = "lake"
//! auto_attach  = true
//! ```
//!
//! Then in your application startup:
//!
//! ```rust,no_run
//! use ducklake_orm::{DuckLakeConfig, DuckLakePool};
//!
//! let config = DuckLakeConfig::from_file("ducklake.toml")?;
//! let pool   = DuckLakePool::from_config(&config)?;
//! # Ok::<(), ducklake_orm::DuckLakeError>(())
//! ```

use serde::Deserialize;

use crate::error::DuckLakeError;

/// Root configuration struct, loaded from `ducklake.toml`.
///
/// Contains three sections:
///
/// - [`database`](DuckLakeConfig::database) — required: the DuckDB file path.
/// - [`pool`](DuckLakeConfig::pool) — optional: pool size and timeout (defaults provided).
/// - [`ducklake`](DuckLakeConfig::ducklake) — optional: DuckLake catalog attachment settings.
///
/// ## Creating programmatically
///
/// If you prefer not to use a TOML file, you can construct this struct directly:
///
/// ```rust
/// use ducklake_orm::config::{DuckLakeConfig, DatabaseConfig, PoolConfig};
///
/// let config = DuckLakeConfig {
///     database: DatabaseConfig { path: ":memory:".into() },
///     pool: PoolConfig::default(),
///     ducklake: None,
/// };
/// ```
#[derive(Debug, Deserialize)]
pub struct DuckLakeConfig {
    /// Settings for the underlying DuckDB database file.
    pub database: DatabaseConfig,

    /// Connection pool settings. All fields have sensible defaults so the
    /// entire `[pool]` section in `ducklake.toml` is optional.
    #[serde(default)]
    pub pool: PoolConfig,

    /// Optional DuckLake catalog attachment. When present and
    /// [`auto_attach`](DuckLakeAttachConfig::auto_attach) is `true`, every
    /// new connection automatically runs `INSTALL ducklake; LOAD ducklake;
    /// ATTACH '…' AS … (TYPE DUCKLAKE)`.
    ///
    /// Set this to `None` (or omit the `[ducklake]` section) when using plain
    /// DuckDB without the lakehouse extension.
    pub ducklake: Option<DuckLakeAttachConfig>,
}

/// Settings for the underlying DuckDB database file.
#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the DuckDB database file.
    ///
    /// Use the special string `":memory:"` to create an in-memory database
    /// that is discarded when the last connection closes. Any other value is
    /// treated as a file path; the file is created automatically if it does
    /// not exist.
    ///
    /// ## Examples
    ///
    /// ```toml
    /// [database]
    /// path = ":memory:"            # in-memory, for tests or ephemeral workloads
    /// # path = "data/warehouse.duckdb"  # persistent file
    /// ```
    pub path: String,
}

/// Connection pool settings — controls concurrency and timeout behaviour.
///
/// All fields provide `serde` defaults so the entire `[pool]` section of
/// `ducklake.toml` is optional.
///
/// ## Defaults
///
/// | Field | Default |
/// |-------|---------|
/// | `size` | `4` |
/// | `connection_timeout_secs` | `30` |
#[derive(Debug, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of DuckDB connections kept open simultaneously.
    ///
    /// DuckDB allows multiple concurrent **readers** on the same file, but
    /// only one **writer** at a time. Size the pool to the expected degree of
    /// read concurrency in your application. A value of `1` is sufficient for
    /// workloads that are entirely single-threaded.
    ///
    /// Defaults to `4` when not specified in `ducklake.toml`.
    #[serde(default = "PoolConfig::default_size")]
    pub size: u32,

    /// How long (in seconds) to wait for an available connection before
    /// returning [`DuckLakeError::Pool`](crate::error::DuckLakeError::Pool).
    ///
    /// If all `size` connections are busy and a new request arrives, the pool
    /// will block for up to this many seconds. If no connection becomes
    /// available within the timeout, the request fails.
    ///
    /// Defaults to `30` when not specified in `ducklake.toml`.
    #[serde(default = "PoolConfig::default_timeout")]
    pub connection_timeout_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            size: Self::default_size(),
            connection_timeout_secs: Self::default_timeout(),
        }
    }
}

impl PoolConfig {
    fn default_size() -> u32 {
        4
    }
    fn default_timeout() -> u64 {
        30
    }
}

/// Settings for automatically attaching a DuckLake catalog on every new connection.
///
/// When this section is present in `ducklake.toml` and
/// [`auto_attach`](Self::auto_attach) is `true`, the pool runs the following
/// SQL on every fresh connection:
///
/// ```sql
/// INSTALL ducklake;
/// LOAD ducklake;
/// ATTACH '<catalog_path>' AS <catalog_name> (TYPE DUCKLAKE);
/// ```
///
/// After attachment, all table references in queries are qualified as
/// `<catalog_name>.<schema>.<table>` — for example `lake.main.sales`.
///
/// ## Requirements
///
/// - DuckDB ≥ 1.5.2 (first release with the stable `ducklake` extension).
/// - Internet access for the initial `INSTALL ducklake` (downloads the extension
///   binary). Subsequent connections use the cached copy.
#[derive(Debug, Deserialize, Clone)]
pub struct DuckLakeAttachConfig {
    /// Path to the DuckLake catalog database file.
    ///
    /// This is the SQLite or DuckDB file that stores DuckLake metadata
    /// (snapshots, table definitions, file references, etc.). It is created
    /// automatically by DuckLake if it does not exist.
    ///
    /// Example: `"data/catalog.duckdb"`
    pub catalog_path: String,

    /// The SQL alias for the attached catalog, used as the first part of
    /// three-part table references (`<catalog_name>.<schema>.<table>`).
    ///
    /// This name must be a valid SQL identifier (no spaces, no special
    /// characters other than `_`).
    ///
    /// Example: `"lake"` → tables are accessed as `lake.main.sales`
    pub catalog_name: String,

    /// When `true`, the pool automatically runs `INSTALL ducklake; LOAD ducklake;
    /// ATTACH …` for every new connection it creates.
    ///
    /// Set to `false` (or omit) if you call
    /// [`DuckLakeConnection::attach_ducklake`](crate::connection::DuckLakeConnection::attach_ducklake)
    /// manually, or if you are managing extensions separately.
    ///
    /// Defaults to `false` when not specified in `ducklake.toml`.
    #[serde(default)]
    pub auto_attach: bool,
}

impl DuckLakeConfig {
    /// Load and parse a `ducklake.toml` file from the given path.
    ///
    /// This is the standard way to configure the ORM in production. The file
    /// must be valid TOML and must contain at least a `[database]` section with
    /// a `path` key. All other sections are optional.
    ///
    /// # Errors
    ///
    /// - [`DuckLakeError::Io`](crate::error::DuckLakeError::Io) — if the file
    ///   cannot be read (not found, permission denied, etc.).
    /// - [`DuckLakeError::Config`](crate::error::DuckLakeError::Config) — if the
    ///   file content is not valid TOML or does not match the expected structure.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ducklake_orm::DuckLakeConfig;
    ///
    /// let config = DuckLakeConfig::from_file("ducklake.toml")?;
    /// println!("DB path: {}", config.database.path);
    /// println!("Pool size: {}", config.pool.size);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, DuckLakeError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse configuration from an in-memory TOML string.
    ///
    /// Useful in tests or when the configuration is generated dynamically
    /// rather than read from a file.
    ///
    /// # Errors
    ///
    /// Returns [`DuckLakeError::Config`](crate::error::DuckLakeError::Config)
    /// if the string is not valid TOML or does not match the expected structure.
    ///
    /// # Example
    ///
    /// ```rust
    /// use ducklake_orm::DuckLakeConfig;
    ///
    /// let toml = r#"
    ///     [database]
    ///     path = ":memory:"
    ///
    ///     [pool]
    ///     size = 2
    /// "#;
    ///
    /// let config = DuckLakeConfig::from_toml(toml)?;
    /// assert_eq!(config.database.path, ":memory:");
    /// assert_eq!(config.pool.size, 2);
    /// # Ok::<(), ducklake_orm::DuckLakeError>(())
    /// ```
    pub fn from_toml(content: &str) -> Result<Self, DuckLakeError> {
        let config: DuckLakeConfig =
            toml::from_str(content).map_err(|e| DuckLakeError::Config(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }
}

impl DuckLakeConfig {
    /// Cross-field validation performed after deserialization.
    ///
    /// Currently checks:
    ///
    /// - `pool.size` is non-zero and within the safety ceiling.
    /// - When `ducklake.auto_attach` is `true`, `catalog_name` is a valid SQL
    ///   identifier (it ends up interpolated unquoted in `ATTACH … AS <name>`).
    ///
    /// Errors are surfaced as [`DuckLakeError::Config`].
    fn validate(&self) -> Result<(), DuckLakeError> {
        if self.pool.size == 0 {
            return Err(DuckLakeError::Config(
                "pool.size must be greater than 0".into(),
            ));
        }
        if self.pool.size > crate::ident::MAX_POOL_SIZE {
            return Err(DuckLakeError::Config(format!(
                "pool.size {} exceeds the safety ceiling of {}",
                self.pool.size,
                crate::ident::MAX_POOL_SIZE
            )));
        }
        if let Some(dl) = &self.ducklake {
            if dl.auto_attach {
                crate::ident::validate_identifier(&dl.catalog_name, "ducklake.catalog_name")?;
            }
        }
        Ok(())
    }
}
