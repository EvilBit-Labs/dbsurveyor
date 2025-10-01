# Database Adapters

This directory contains unified database adapter implementations for DBSurveyor's schema collection functionality.

## Overview

The adapter system provides a trait-based interface (`SchemaCollector`) that enables consistent schema metadata collection across diverse database engines while maintaining strict security guarantees.

## Supported Databases

| Database | Feature Flag | Status | Connection String Example |
|----------|--------------|--------|---------------------------|
| PostgreSQL | `postgresql` | ✅ Full Support | `postgresql://user:pass@host:5432/db` |
| SQLite | `sqlite` | ✅ Full Support | `sqlite:///path/to/database.db` |
| MongoDB | `mongodb` | ✅ Full Support | `mongodb://user:pass@host:27017/db` |

## Architecture

### Core Components

1. **`SchemaCollector` Trait** - Unified interface for all database adapters
2. **Connection Pooling** - Configurable connection pools with timeout management
3. **Error Sanitization** - All errors are sanitized to prevent credential leakage
4. **Feature Flags** - Compile-time feature selection for minimal builds

### Security Guarantees

All adapter implementations enforce these security requirements:

- ✅ **Zero Credential Storage** - Credentials never stored in memory after connection
- ✅ **Sanitized Error Messages** - No credential leakage in errors or logs
- ✅ **Read-Only Operations** - All database operations are strictly read-only
- ✅ **Connection String Protection** - URLs never logged after connection establishment
- ✅ **Safe Descriptions** - Safe descriptions exclude all sensitive information

## Usage Examples

### PostgreSQL Adapter

```rust
use dbsurveyor_collect::adapters::{
    postgresql::PostgresAdapter,
    ConnectionConfig,
    SchemaCollector,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure connection pool
    let config = ConnectionConfig {
        max_connections: 10,
        min_idle_connections: 2,
        connect_timeout: std::time::Duration::from_secs(30),
        ..Default::default()
    };

    // Create adapter (credentials consumed here)
    let adapter = PostgresAdapter::new(
        "postgresql://user:password@localhost:5432/mydb",
        config
    ).await?;

    // Test connection
    adapter.test_connection().await?;

    // Collect complete database metadata
    let metadata = adapter.collect_metadata().await?;

    println!("Database: {}", metadata.database_type);
    println!("Version: {}", metadata.version.unwrap_or_default());
    println!("Schemas: {}", metadata.schemas.len());

    // Safe description never includes credentials
    println!("Connection: {}", adapter.safe_description());

    Ok(())
}
```

### SQLite Adapter

```rust
use dbsurveyor_collect::adapters::{
    sqlite::SqliteAdapter,
    ConnectionConfig,
    SchemaCollector,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConnectionConfig::default();

    // Connect to file-based database
    let adapter = SqliteAdapter::new(
        "sqlite:///path/to/database.db",
        config
    ).await?;

    // Or use in-memory database
    let adapter = SqliteAdapter::new(
        "sqlite::memory:",
        config
    ).await?;

    let metadata = adapter.collect_metadata().await?;

    // SQLite has a single schema named "main"
    for schema in metadata.schemas {
        println!("Schema: {}", schema.name);
        for table in schema.tables {
            println!("  Table: {} ({} columns)", table.name, table.columns.len());
        }
    }

    Ok(())
}
```

### MongoDB Adapter

```rust
use dbsurveyor_collect::adapters::{
    mongodb::MongoAdapter,
    ConnectionConfig,
    SchemaCollector,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConnectionConfig::default();

    let adapter = MongoAdapter::new(
        "mongodb://user:password@localhost:27017/mydb",
        config
    ).await?;

    adapter.test_connection().await?;

    let metadata = adapter.collect_metadata().await?;

    // MongoDB treats collections as tables
    for schema in metadata.schemas {
        println!("Database: {}", schema.name);
        for collection in schema.tables {
            println!("  Collection: {} ({} fields inferred)",
                     collection.name,
                     collection.columns.len());
        }
    }

    Ok(())
}
```

## Configuration

### Connection Pool Configuration

```rust
use std::time::Duration;
use dbsurveyor_collect::adapters::ConnectionConfig;

let config = ConnectionConfig {
    max_connections: 10,           // Maximum pool size
    min_idle_connections: 2,       // Minimum idle connections
    connect_timeout: Duration::from_secs(30),    // Connection timeout
    acquire_timeout: Duration::from_secs(30),    // Pool acquire timeout
    idle_timeout: Duration::from_secs(600),      // Idle connection timeout (10 min)
    max_lifetime: Duration::from_secs(3600),     // Max connection lifetime (1 hour)
};
```

### Feature Flags

Build with specific database support:

```bash
# PostgreSQL and SQLite only (default)
cargo build --features postgresql,sqlite

# All databases
cargo build --features postgresql,sqlite,mongodb

# Minimal build (no databases)
cargo build --no-default-features

# Single database
cargo build --no-default-features --features postgresql
```

## Error Handling

All adapters use the `AdapterError` enum which provides sanitized error messages:

```rust
use dbsurveyor_collect::adapters::{AdapterError, AdapterResult};

async fn safe_operation() -> AdapterResult<()> {
    // All errors are automatically sanitized
    let adapter = PostgresAdapter::new("postgresql://...", config).await?;
    
    adapter.test_connection().await?;
    
    Ok(())
}
```

Common error types:
- `ConnectionFailed` - Connection could not be established (no details leaked)
- `ConnectionTimeout` - Connection attempt timed out
- `QueryFailed` - Query execution failed (no query details leaked)
- `InvalidParameters` - Invalid connection parameters
- `PoolExhausted` - Connection pool has no available connections

## Testing

### Unit Tests

```bash
# Run all adapter unit tests
cargo test --lib --features postgresql,sqlite,mongodb

# Test specific adapter
cargo test --lib --features postgresql postgres
```

### Integration Tests

Integration tests use testcontainers to spin up real database instances:

```bash
# Run integration tests (requires Docker)
cargo test --test integration_tests --features postgresql,sqlite

# Run specific integration test
cargo test --test integration_tests postgres_connection
```

### Security Tests

Verify credential protection:

```bash
# Run all security tests
cargo test --test security_tests --features postgresql,sqlite,mongodb

# Verify no credential leakage
cargo test --test security_tests credential_security
```

## Performance Considerations

### Connection Pooling

- Default pool size: 10 connections
- Recommended for production: 5-20 connections depending on workload
- Idle connections automatically closed after 10 minutes
- Connections recycled after 1 hour maximum lifetime

### Memory Usage

- PostgreSQL adapter: ~50-100 MB with default pool
- SQLite adapter: ~10-50 MB depending on database size
- MongoDB adapter: ~50-150 MB depending on collection count

### Large Schemas

For databases with >1000 tables:
- Schema collection may take several minutes
- Consider increasing timeouts in ConnectionConfig
- Monitor memory usage during collection

## Security Best Practices

1. **Never Log Connection Strings**
   ```rust
   // ❌ WRONG - logs credentials
   println!("Connecting to {}", database_url);
   
   // ✅ CORRECT - use safe description
   println!("Connection: {}", adapter.safe_description());
   ```

2. **Use Environment Variables**
   ```bash
   export DATABASE_URL="postgresql://user:pass@host/db"
   ```

3. **Validate Before Collection**
   ```rust
   adapter.test_connection().await?;
   let metadata = adapter.collect_metadata().await?;
   ```

4. **Immediate Credential Consumption**
   ```rust
   // Connection string immediately consumed during adapter creation
   let adapter = PostgresAdapter::new(&connection_string, config).await?;
   // connection_string can now be safely dropped/zeroed
   ```

## Troubleshooting

### Connection Failures

```
Error: ConnectionFailed
```
- Verify database is running and accessible
- Check network connectivity
- Validate credentials (but don't log them!)
- Check firewall rules

### Pool Exhaustion

```
Error: PoolExhausted
```
- Increase `max_connections` in ConnectionConfig
- Check for connection leaks (ensure connections are properly closed)
- Reduce concurrent operations

### Query Timeouts

```
Error: QueryFailed
```
- Increase timeout in ConnectionConfig
- Check database performance
- Verify read permissions on tables

## Contributing

When adding new database adapters:

1. Implement the `SchemaCollector` trait
2. Ensure zero credential storage
3. Sanitize all error messages
4. Add comprehensive unit tests
5. Add integration tests with testcontainers
6. Add security tests for credential protection
7. Update this README with usage examples

## License

Apache 2.0 - See LICENSE file for details
