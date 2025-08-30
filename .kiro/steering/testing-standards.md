---
inclusion: fileMatch
fileMatchPattern: '**/*.rs'
---

# Testing Standards for DBSurveyor

## Testing Philosophy

- **Security-First Testing**: All tests must verify security guarantees
- **Comprehensive Coverage**: >80% test coverage with `cargo llvm-cov`
- **Real Database Integration**: Use testcontainers for authentic testing
- **Zero Warnings**: All test code must pass `cargo clippy -- -D warnings`

## Test Organization

### Test Categories

- **Unit Tests**: `#[cfg(test)]` modules in each source file
- **Integration Tests**: `tests/` directory with testcontainers
- **Security Tests**: Credential protection and encryption validation
- **Performance Tests**: `benches/` directory with criterion
- **Documentation Tests**: Examples in `///` documentation

### Test File Naming

- Unit tests: Co-located with source code
- Integration tests: `tests/integration_*.rs`
- Security tests: `tests/security_*.rs`
- Database tests: `tests/database_*.rs`
- Benchmarks: `benches/*.rs`

## Integration Testing with Testcontainers

### PostgreSQL Integration Tests

```rust
use testcontainers::{clients, images};
use dbsurveyor_shared::collectors::PostgresCollector;

#[tokio::test]
async fn test_postgres_schema_collection() {
    let docker = clients::Cli::default();
    let postgres = docker.run(images::postgres::Postgres::default());

    let port = postgres.get_host_port_ipv4(5432);
    let database_url = format!(
        "postgres://postgres:postgres@localhost:{}/postgres",
        port
    );

    let collector = PostgresCollector::new(&database_url).await
        .expect("Failed to create collector");

    let schema = collector.collect_schema().await
        .expect("Failed to collect schema");

    // Verify basic schema structure
    assert!(!schema.tables.is_empty());
    assert!(schema.tables.iter().any(|t| t.name == "information_schema"));
}
```

### MySQL Integration Tests

```rust
#[tokio::test]
async fn test_mysql_schema_collection() {
    let docker = clients::Cli::default();
    let mysql = docker.run(
        images::mysql::Mysql::default()
            .with_root_password("testpass")
            .with_database("testdb")
    );

    let port = mysql.get_host_port_ipv4(3306);
    let database_url = format!(
        "mysql://root:testpass@localhost:{}/testdb",
        port
    );

    // Wait for MySQL to be ready
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let collector = MySqlCollector::new(&database_url).await
        .expect("Failed to create MySQL collector");

    let schema = collector.collect_schema().await
        .expect("Failed to collect MySQL schema");

    assert!(!schema.tables.is_empty());
}
```

### SQLite Integration Tests

```rust
use tempfile::tempdir;

#[tokio::test]
async fn test_sqlite_schema_collection() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test database
    let conn = sqlx::SqlitePool::connect(&format!("sqlite:{}", db_path.display())).await?;

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&conn).await?;

    conn.close().await;

    // Test schema collection
    let database_url = format!("sqlite:{}", db_path.display());
    let collector = SqliteCollector::new(&database_url).await
        .expect("Failed to create SQLite collector");

    let schema = collector.collect_schema().await
        .expect("Failed to collect SQLite schema");

    assert!(schema.tables.iter().any(|t| t.name == "users"));
}
```

## Security Testing Requirements

### Credential Protection Tests

```rust
#[tokio::test]
async fn test_no_credentials_in_schema_output() {
    let database_url = "postgres://testuser:secret123@localhost:5432/testdb";

    let mock_schema = create_mock_schema();
    let json_output = serde_json::to_string(&mock_schema).unwrap();

    // Verify no sensitive data is present
    assert!(!json_output.contains("secret123"));
    assert!(!json_output.contains("testuser:secret123"));
    assert!(!json_output.contains("password"));
    assert!(!json_output.contains("secret"));
}

#[test]
fn test_database_config_display() {
    let config = DatabaseConfig::new("postgres://user:secret@host:5432/db");
    let display_output = format!("{}", config);

    // Should show connection info but not credentials
    assert!(display_output.contains("host:5432"));
    assert!(display_output.contains("db"));
    assert!(!display_output.contains("secret"));
    assert!(!display_output.contains("user:secret"));
}
```

### Encryption Tests

```rust
#[tokio::test]
async fn test_aes_gcm_encryption_randomness() {
    let data = b"test database schema data";

    let encrypted1 = encrypt_schema_data(data).await?;
    let encrypted2 = encrypt_schema_data(data).await?;

    // Same data should produce different ciphertext due to random nonce
    assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    assert_ne!(encrypted1.nonce, encrypted2.nonce);

    // Both should decrypt to same plaintext
    let decrypted1 = decrypt_schema_data(&encrypted1).await?;
    let decrypted2 = decrypt_schema_data(&encrypted2).await?;

    assert_eq!(decrypted1, data);
    assert_eq!(decrypted2, data);
}
```

### Offline Operation Tests

```rust
#[tokio::test]
async fn test_airgap_compatibility() {
    // Simulate airgap environment by testing without network access
    let schema_data = include_bytes!("fixtures/sample_schema.json");
    let schema: DatabaseSchema = serde_json::from_slice(schema_data).unwrap();

    // All processing should work offline
    let documentation = generate_documentation(&schema, OutputFormat::Markdown).await?;
    assert!(!documentation.is_empty());

    let json_export = generate_documentation(&schema, OutputFormat::Json).await?;
    assert!(!json_export.is_empty());
}
```

## Performance Testing

### Benchmark Structure

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_schema_serialization(c: &mut Criterion) {
    let schema = create_large_test_schema(1000); // 1000 tables

    c.bench_function("schema_to_json", |b| {
        b.iter(|| serde_json::to_string(black_box(&schema)))
    });

    c.bench_function("schema_to_markdown", |b| {
        b.iter(|| generate_markdown_documentation(black_box(&schema)))
    });
}

criterion_group!(benches, bench_schema_serialization);
criterion_main!(benches);
```

## Test Data and Fixtures

### Test Data Generation

```rust
pub fn create_test_schema() -> DatabaseSchema {
    DatabaseSchema {
        database_name: "test_db".to_string(),
        database_type: DatabaseType::PostgreSQL,
        tables: vec![
            create_test_table("users"),
            create_test_table("orders"),
            create_test_table("products"),
        ],
        indexes: vec![],
        constraints: vec![],
        created_at: chrono::Utc::now(),
    }
}

pub fn create_test_table(name: &str) -> Table {
    Table {
        name: name.to_string(),
        schema: "public".to_string(),
        table_type: TableType::BaseTable,
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                is_nullable: false,
                column_default: None,
                is_primary_key: true,
            },
        ],
        row_count: Some(100),
        size_bytes: Some(8192),
    }
}
```

## Testing Commands

### Essential Test Commands

```bash
# Run all tests
just test

# Run unit tests only
cargo test --lib

# Run integration tests
cargo test --test '*'

# Run specific database tests
just test-postgres
just test-mysql
just test-sqlite

# Run security tests
just test-credential-security
just test-encryption
just test-offline

# Generate coverage report
just coverage
just coverage-html

# Run benchmarks
cargo bench
```

## Test Quality Standards

### Test Requirements

- **Comprehensive**: Test happy path, error cases, and edge conditions
- **Isolated**: Tests should not depend on external services (except testcontainers)
- **Deterministic**: Tests must produce consistent results
- **Fast**: Unit tests should complete in milliseconds
- **Secure**: No credentials or sensitive data in test code

### Test Coverage Goals

- **Unit Tests**: >90% coverage for business logic
- **Integration Tests**: Cover all database adapters
- **Security Tests**: Verify all security guarantees
- **Performance Tests**: Establish baseline metrics

### Common Testing Patterns

- Use `Result<(), Box<dyn std::error::Error>>` for test functions
- Test both `Ok` and `Err` cases for functions returning `Result`
- Use `assert_matches!` for pattern matching in tests
- Use `tempfile` for temporary files and directories
- Mock external dependencies appropriately
