---
inclusion: always
---

# Architecture Patterns for DBSurveyor

## Core Principles

DBSurveyor follows these non-negotiable architectural principles:

1. **Security-First**: Every design decision prioritizes security and privacy
2. **Offline-Capable**: Zero external dependencies after database connection
3. **Database-Agnostic**: Unified interface across all supported databases

## Workspace Structure

DBSurveyor uses a multi-crate workspace with clear separation of concerns:

- **`dbsurveyor-collect/`**: Database collection binary (CLI + collectors)
- **`dbsurveyor/`**: Postprocessor binary (report generation)
- **`dbsurveyor-core/`**: Shared library (models, encryption, utilities)

## Required Design Patterns

### Repository Pattern (REQUIRED)

Use for all database access with this trait structure:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub trait SchemaCollector: Send + Sync {
    fn collect_schema<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<DatabaseSchema, Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>>;
    fn test_connection<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>>;
}

// Concrete implementations provide async constructors
impl PostgresCollector {
    pub async fn new(connection_string: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Implementation
    }
}
```

### Factory Pattern (REQUIRED)

Use for database-specific collector instantiation:

```rust
use std::sync::Arc;

pub async fn create_collector(
    database_type: DatabaseType,
    connection_string: &str,
) -> Result<Arc<dyn SchemaCollector + Send + Sync>, Box<dyn std::error::Error + Send + Sync>> {
    match database_type {
        DatabaseType::PostgreSQL => {
            let collector = PostgresCollector::new(connection_string).await?;
            Ok(Arc::new(collector))
        }
        DatabaseType::MySQL => {
            let collector = MySqlCollector::new(connection_string).await?;
            Ok(Arc::new(collector))
        }
        DatabaseType::SQLite => {
            let collector = SqliteCollector::new(connection_string).await?;
            Ok(Arc::new(collector))
        }
    }
}
```

### Command Pattern (REQUIRED)

Use clap derive macros for CLI structure:

```rust
#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
```

## Data Flow Requirements

### Collection Pipeline (MANDATORY STEPS)

1. **Connection**: Sanitize credentials immediately, never log connection strings
2. **Introspection**: Use read-only queries with 30s timeouts
3. **Processing**: Normalize to unified `DatabaseSchema` struct
4. **Security**: Apply credential sanitization and optional encryption
5. **Output**: Generate multiple formats (JSON, Markdown, encrypted)

## Error Handling Rules

### Error Type Structure (REQUIRED)

Use `thiserror::Error` with this hierarchy:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DbSurveyorError {
    #[error("Database connection failed")]
    Connection(#[from] ConnectionError),
    #[error("Schema collection failed")]
    Collection(#[from] CollectionError),
    #[error("Encryption operation failed")]
    Encryption(#[from] EncryptionError),
}
```

### Security Requirements (MANDATORY)

- **NEVER** include credentials in error messages
- **ALWAYS** sanitize connection strings in logs
- **ALWAYS** provide generic error messages for authentication failures

## Security Architecture (NON-NEGOTIABLE)

### Credential Management Rules

- **NEVER** store credentials in structs or configuration
- **ALWAYS** separate connection config from credentials
- **ALWAYS** use well-known crates for memory zeroing: prefer `Zeroizing<T>` wrappers or derive/implement the `Zeroize` trait from the zeroize crate to ensure reliable memory zeroing
- **ALWAYS** parse credentials separately from connection config

```rust
use zeroize::{Zeroize, Zeroizing};

#[derive(Zeroize)]
#[zeroize(drop)]
struct Credentials {
    username: Zeroizing<String>,
    password: Zeroizing<Option<String>>,
}

// Or use Zeroizing wrapper for automatic zeroing
struct ConnectionConfig {
    host: String,
    port: u16,
    database: String,
    // Credentials handled separately and immediately consumed
}
```

### Encryption Requirements

- **MUST** use AES-GCM-256 with random nonces
- **MUST** include authentication tags
- **MUST** support key derivation parameters
- **NEVER** reuse nonces

```rust
pub struct EncryptedData {
    pub algorithm: String,     // "AES-GCM-256"
    pub nonce: Vec<u8>,        // 96-bit random nonce
    pub ciphertext: Vec<u8>,   // Raw encrypted payload
    pub auth_tag: Vec<u8>,     // Separate authentication tag
    pub kdf: String,           // Key derivation function (e.g., "Argon2id")
    pub kdf_params: KdfParams, // KDF parameters (salt, iterations, memory, etc.)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub salt: Vec<u8>,    // 32-byte random salt
    pub memory_cost: u32, // Memory cost in KiB (e.g., 65536)
    pub time_cost: u32,   // Time cost iterations (e.g., 3)
    pub parallelism: u32, // Parallelism factor (e.g., 4)
}
```

## Performance Requirements

### Connection Pooling (REQUIRED)

- Use SQLx connection pools with max 5-10 connections
- Implement 30-second connection timeouts
- Cache pools by sanitized connection string (no credentials)

### Memory Management (REQUIRED)

- Stream large result sets instead of loading into memory
- Use batch processing for databases with >1000 tables
- Implement explicit `drop()` for large data structures
- Monitor memory usage in benchmarks

## Configuration Rules

### Configuration Structure (REQUIRED)

- Use hierarchical config with `serde` derive macros
- Support environment variables with `DBSURVEYOR_` prefix
- **NEVER** store credentials in configuration files
- Load from: defaults → system config → user config → env vars

### Key Constraints

- All database operations must be read-only
- All network calls must timeout within 30 seconds
- All sensitive data must be sanitized before logging
- All encryption must use AES-GCM with random nonces
