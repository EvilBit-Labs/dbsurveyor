---
inclusion: always
---


# Architecture Patterns for DBSurveyor

## Core Architecture Philosophy

DBSurveyor follows three fundamental principles:

1. **Security-First**: Every design decision prioritizes security and privacy
2. **Offline-Capable**: Zero external dependencies after database connection
3. **Database-Agnostic**: Unified interface across PostgreSQL, MySQL, and SQLite

## High-Level Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                          DBSurveyor                            │
│                                                                 │
│  ┌─────────────┐    ┌──────────────────┐    ┌─────────────────┐ │
│  │     CLI     │───▶│  Schema Collector │───▶│ Output Generator│ │
│  │             │    │                  │    │                 │ │
│  │ • Args      │    │ • PostgreSQL     │    │ • Markdown      │ │
│  │ • Config    │    │ • MySQL          │    │ • JSON          │ │
│  │ • Security  │    │ • SQLite         │    │ • Encrypted     │ │
│  └─────────────┘    └──────────────────┘    └─────────────────┘ │
│                                │                                │
│                                ▼                                │
│                     ┌──────────────────┐                       │
│                     │  Security Layer   │                       │
│                     │                  │                       │
│                     │ • Encryption     │                       │
│                     │ • Sanitization   │                       │
│                     │ • Access Control │                       │
│                     └──────────────────┘                       │
└─────────────────────────────────────────────────────────────────┘
```

## Module Organization

### Crate Structure

```text
dbsurveyor/
├── Cargo.toml                 # Workspace configuration
├── dbsurveyor-collect/        # Database collection binary
│   ├── src/
│   │   ├── main.rs           # CLI entry point
│   │   ├── collectors/       # Database-specific collectors
│   │   └── config.rs         # Configuration management
│   └── Cargo.toml
├── dbsurveyor/               # Postprocessor binary
│   ├── src/
│   │   ├── main.rs           # Report generation entry
│   │   ├── output/           # Documentation generators
│   │   └── templates/        # Output templates
│   └── Cargo.toml
└── dbsurveyor-core/          # Shared library
    ├── src/
    │   ├── lib.rs            # Public API exports
    │   ├── models/           # Data structures
    │   ├── encryption/       # Security and encryption
    │   └── error.rs          # Error types
    └── Cargo.toml
```

## Design Patterns

### Repository Pattern

Database access abstraction layer:

```rust
#[async_trait]
pub trait SchemaCollector {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn new(connection_string: &str) -> Result<Self, Self::Error>
    where
        Self: Sized;

    async fn collect_schema(&self) -> Result<DatabaseSchema, Self::Error>;

    async fn test_connection(&self) -> Result<(), Self::Error>;

    async fn get_metadata(&self) -> Result<DatabaseMetadata, Self::Error>;
}
```

### Service Pattern

Business logic encapsulation:

```rust
pub struct SchemaService {
    collector: Box<dyn SchemaCollector>,
    config: ServiceConfig,
}

impl SchemaService {
    pub async fn collect_and_process(&self) -> Result<ProcessedSchema> {
        let raw_schema = self.collector.collect_schema().await?;
        let processed = self.process_schema(raw_schema)?;
        Ok(processed)
    }

    fn process_schema(&self, schema: DatabaseSchema) -> Result<ProcessedSchema> {
        // Business logic for schema processing
    }
}
```

### Factory Pattern

Database driver instantiation:

```rust
pub struct CollectorFactory;

impl CollectorFactory {
    pub async fn create_collector(
        database_type: DatabaseType,
        connection_string: &str,
    ) -> Result<Box<dyn SchemaCollector>> {
        match database_type {
            DatabaseType::PostgreSQL => {
                Ok(Box::new(PostgresCollector::new(connection_string).await?))
            }
            DatabaseType::MySQL => {
                Ok(Box::new(MySqlCollector::new(connection_string).await?))
            }
            DatabaseType::SQLite => {
                Ok(Box::new(SqliteCollector::new(connection_string).await?))
            }
        }
    }
}
```

### Command Pattern

CLI command organization:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dbsurveyor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Collect(CollectCommand),
    Process(ProcessCommand),
    Encrypt(EncryptCommand),
}

#[derive(Parser)]
pub struct CollectCommand {
    #[arg(long)]
    pub database_url: String,
    
    #[arg(long)]
    pub output: PathBuf,
    
    #[arg(long)]
    pub encrypt: bool,
}
```

## Data Flow Architecture

### Schema Collection Pipeline

```text
1. Connection Establishment
   ┌─────────────────────┐
   │ Connection String   │ → Sanitize credentials
   │ "postgres://..."    │   Remove from logs/errors
   └─────────────────────┘
            │
            ▼
2. Database Introspection
   ┌─────────────────────┐
   │ System Tables       │ → Read-only queries
   │ information_schema  │   Minimal privileges
   │ pg_catalog, etc.    │   Timeout protection
   └─────────────────────┘
            │
            ▼
3. Schema Processing
   ┌─────────────────────┐
   │ Raw Schema Data     │ → Normalize structure
   │ Tables, columns     │   Validate data types
   │ Indexes, constraints│   Calculate metadata
   └─────────────────────┘
            │
            ▼
4. Security Layer
   ┌─────────────────────┐
   │ Unified Schema      │ → Optional encryption
   │ DatabaseSchema      │   Credential sanitization
   │ struct              │   Access control
   └─────────────────────┘
            │
            ▼
5. Output Generation
   ┌─────────────────────┐
   │ Documentation       │ → Multiple formats
   │ Markdown, JSON      │   Template-based
   │ Encrypted binary    │   Offline-capable
   └─────────────────────┘
```

## Error Handling Architecture

### Comprehensive Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum DbSurveyorError {
    #[error("Database connection failed")]
    Connection(#[from] ConnectionError),

    #[error("Schema collection failed")]
    Collection(#[from] CollectionError),

    #[error("Encryption operation failed")]
    Encryption(#[from] EncryptionError),

    #[error("Output generation failed")]
    Output(#[from] OutputError),
}
```

### Security-Conscious Error Messages

```rust
impl ConnectionError {
    pub fn sanitized_message(&self) -> String {
        match self {
            ConnectionError::DatabaseUnreachable { host, port, .. } => {
                format!("Cannot connect to database at {}:{}", host, port)
            }
            ConnectionError::AuthenticationFailed => {
                "Invalid credentials".to_string() // No specifics
            }
            ConnectionError::Timeout { duration } => {
                format!("Connection timed out after {:?}", duration)
            }
        }
    }
}
```

## Security Architecture

### Credential Management

```rust
pub struct ConnectionConfig {
    host: String,
    port: u16,
    database: String,
    // Credentials are never stored in structs
}

impl ConnectionConfig {
    pub fn from_url(url: &str) -> Result<(Self, Credentials), ConnectionError> {
        let parsed = Url::parse(url)?;

        let config = Self {
            host: parsed.host_str().unwrap_or("localhost").to_string(),
            port: parsed.port().unwrap_or(5432),
            database: parsed.path().trim_start_matches('/').to_string(),
        };

        let credentials = Credentials {
            username: parsed.username().to_string(),
            password: parsed.password().map(String::from),
        };

        Ok((config, credentials))
    }
}

impl Drop for Credentials {
    fn drop(&mut self) {
        // Zero out sensitive data
        self.username.zeroize();
        if let Some(ref mut password) = self.password {
            password.zeroize();
        }
    }
}
```

### Encryption Architecture

```rust
pub struct EncryptedData {
    pub algorithm: String,           // "AES-GCM-256"
    pub nonce: Vec<u8>,             // 96-bit random nonce
    pub ciphertext: Vec<u8>,        // Encrypted data
    pub tag: Vec<u8>,               // Authentication tag
    pub kdf_params: Option<KdfParams>, // Key derivation parameters
}

pub async fn encrypt_schema_data(
    plaintext: &[u8],
    key: Option<&EncryptionKey>,
) -> Result<EncryptedData, EncryptionError> {
    let encryption_key = match key {
        Some(k) => k.clone(),
        None => EncryptionKey::derive_from_entropy().await?,
    };

    let nonce = generate_random_nonce()?;
    let cipher = Aes256Gcm::new(&encryption_key.as_bytes());
    let ciphertext = cipher.encrypt(&nonce, plaintext)?;

    Ok(EncryptedData {
        algorithm: "AES-GCM-256".to_string(),
        nonce: nonce.to_vec(),
        ciphertext,
        tag: vec![], // Included in ciphertext for AES-GCM
        kdf_params: Some(encryption_key.kdf_params()),
    })
}
```

## Performance Architecture

### Connection Pooling

```rust
pub struct CollectorPool {
    postgres_pools: HashMap<String, PgPool>,
    mysql_pools: HashMap<String, MySqlPool>,
    sqlite_pools: HashMap<String, SqlitePool>,
    config: PoolConfig,
}

impl CollectorPool {
    pub async fn get_postgres_collector(
        &mut self,
        connection_string: &str,
    ) -> Result<PostgresCollector, ConnectionError> {
        let pool = match self.postgres_pools.get(connection_string) {
            Some(pool) => pool.clone(),
            None => {
                let pool = PgPoolOptions::new()
                    .max_connections(self.config.max_connections)
                    .connect_timeout(self.config.connect_timeout)
                    .connect(connection_string)
                    .await?;

                self.postgres_pools.insert(connection_string.to_string(), pool.clone());
                pool
            }
        };

        Ok(PostgresCollector::from_pool(pool))
    }
}
```

### Memory Management

```rust
pub struct StreamingCollector<T> {
    inner: T,
    batch_size: usize,
    memory_limit: usize,
}

impl<T: SchemaCollector> StreamingCollector<T> {
    pub async fn collect_schema_streaming(
        &self,
        mut writer: impl AsyncWrite + Unpin,
    ) -> Result<(), T::Error> {
        let table_count = self.inner.count_tables().await?;
        let batch_count = (table_count + self.batch_size - 1) / self.batch_size;

        for batch_index in 0..batch_count {
            let offset = batch_index * self.batch_size;
            let batch_tables = self.inner
                .collect_tables_batch(offset, self.batch_size)
                .await?;

            let batch_json = serde_json::to_vec(&batch_tables)?;
            writer.write_all(&batch_json).await?;

            drop(batch_tables); // Force garbage collection
        }

        Ok(())
    }
}
```

## Configuration Architecture

### Hierarchical Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSurveyorConfig {
    pub database: DatabaseConfig,
    pub security: SecurityConfig,
    pub output: OutputConfig,
    pub performance: PerformanceConfig,
}

impl DbSurveyorConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Config::builder()
            .add_source(config::File::from_str(DEFAULT_CONFIG, FileFormat::Toml))
            .add_source(config::File::with_name("/etc/dbsurveyor/config").required(false))
            .add_source(config::File::with_name("~/.config/dbsurveyor/config").required(false))
            .add_source(config::File::with_name("dbsurveyor.toml").required(false))
            .add_source(config::Environment::with_prefix("DBSURVEYOR"))
            .build()?;

        config.try_deserialize()
    }
}
```

This architecture ensures DBSurveyor maintains its security-first principles while providing a clean, maintainable, and extensible codebase.
