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
#[async_trait]
pub trait SchemaCollector {
    type Error: std::error::Error + Send + Sync + 'static;
    
    async fn new(connection_string: &str) -> Result<Self, Self::Error> where Self: Sized;
    async fn collect_schema(&self) -> Result<DatabaseSchema, Self::Error>;
    async fn test_connection(&self) -> Result<(), Self::Error>;
}
```

### Factory Pattern (REQUIRED)

Use for database-specific collector instantiation:

```rust
pub async fn create_collector(
    database_type: DatabaseType,
    connection_string: &str,
) -> Result<Box<dyn SchemaCollector>>
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
- **ALWAYS** implement `Drop` trait to zero out sensitive data
- **ALWAYS** parse credentials separately from connection config

### Encryption Requirements

- **MUST** use AES-GCM-256 with random nonces
- **MUST** include authentication tags
- **MUST** support key derivation parameters
- **NEVER** reuse nonces

```rust
pub struct EncryptedData {
    pub algorithm: String,    // Always "AES-GCM-256"
    pub nonce: Vec<u8>,      // 96-bit random nonce
    pub ciphertext: Vec<u8>, // Encrypted data + auth tag
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
