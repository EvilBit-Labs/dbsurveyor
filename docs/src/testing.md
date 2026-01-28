# Testing

DBSurveyor uses a comprehensive testing strategy to ensure security, reliability, and correctness. This guide covers the testing framework, practices, and how to run tests.

## Testing Philosophy

- **Security-First Testing**: All tests must verify security guarantees
- **Comprehensive Coverage**: >80% test coverage with `cargo llvm-cov`
- **Real Database Integration**: Use testcontainers for authentic testing
- **Zero Warnings**: All test code must pass `cargo clippy -- -D warnings`

## Test Organization

### Test Categories

DBSurveyor organizes tests into distinct categories:

```mermaid
graph TD
    TESTS[Test Suite] --> UNIT[Unit Tests]
    TESTS --> INTEGRATION[Integration Tests]
    TESTS --> SECURITY[Security Tests]
    TESTS --> PERFORMANCE[Performance Tests]
    
    UNIT --> MODELS[Model Tests]
    UNIT --> ADAPTERS[Adapter Unit Tests]
    UNIT --> UTILS[Utility Tests]
    
    INTEGRATION --> POSTGRES[PostgreSQL Tests]
    INTEGRATION --> MYSQL[MySQL Tests]
    INTEGRATION --> SQLITE[SQLite Tests]
    INTEGRATION --> MONGODB[MongoDB Tests]
    
    SECURITY --> CREDENTIALS[Credential Protection]
    SECURITY --> ENCRYPTION[Encryption Tests]
    SECURITY --> OFFLINE[Offline Operation]
    
    PERFORMANCE --> BENCHMARKS[Criterion Benchmarks]
    PERFORMANCE --> MEMORY[Memory Tests]
    PERFORMANCE --> CONCURRENCY[Concurrency Tests]
```

### Test File Structure

```
dbsurveyor/
├── dbsurveyor-core/
│   └── src/
│       ├── lib.rs              # Unit tests in #[cfg(test)]
│       ├── models.rs           # Model unit tests
│       └── security.rs         # Security unit tests
├── tests/
│   ├── integration/
│   │   ├── postgres_tests.rs   # PostgreSQL integration
│   │   ├── mysql_tests.rs      # MySQL integration
│   │   └── sqlite_tests.rs     # SQLite integration
│   ├── security/
│   │   ├── credential_tests.rs # Credential protection
│   │   ├── encryption_tests.rs # Encryption validation
│   │   └── offline_tests.rs    # Offline operation
│   └── fixtures/
│       ├── sample_schemas/     # Test schema files
│       └── test_data/          # Test databases
└── benches/
    ├── collection.rs           # Collection benchmarks
    └── documentation.rs        # Documentation benchmarks
```

## Running Tests

### Basic Test Commands

```bash
# Run all tests with nextest (default)
just test

# Run specific test categories
just test-unit           # Unit tests only
just test-integration    # Integration tests only
just test-security       # Security tests only

# Run tests for specific databases
just test-postgres       # PostgreSQL tests
just test-mysql          # MySQL tests
just test-sqlite         # SQLite tests
```

### Advanced Test Options

```bash
# Run tests with different profiles
just test-dev            # Development profile (fast)
just test-ci             # CI profile (comprehensive)

# Run with verbose output
just test-verbose

# Run specific test by name
cargo nextest run test_postgres_connection

# Run tests with debug logging
RUST_LOG=debug cargo nextest run
```

### Coverage Testing

```bash
# Generate coverage report (>80% required)
just coverage

# Generate HTML coverage report
just coverage-html

# CI-friendly coverage
just coverage-ci
```

## Unit Testing

### Test Structure

Unit tests are co-located with source code using `#[cfg(test)]` modules:

```rust
// dbsurveyor-core/src/models.rs
impl DatabaseSchema {
    pub fn new(database_info: DatabaseInfo) -> Self {
        // Implementation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_schema_creation() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        let schema = DatabaseSchema::new(db_info);
        
        assert_eq!(schema.format_version, "1.0");
        assert_eq!(schema.database_info.name, "test_db");
        assert_eq!(schema.object_count(), 0);
    }

    #[test]
    fn test_add_warning() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        let mut schema = DatabaseSchema::new(db_info);
        
        schema.add_warning("Test warning".to_string());
        
        assert_eq!(schema.collection_metadata.warnings.len(), 1);
        assert_eq!(schema.collection_metadata.warnings[0], "Test warning");
    }
}
```

### Security Unit Tests

All security-sensitive code must include security-focused unit tests:

```rust
#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn test_credential_sanitization() {
        let url = "postgres://user:secret123@localhost:5432/db";
        let sanitized = redact_database_url(url);
        
        // Verify credentials are sanitized
        assert!(!sanitized.contains("secret123"));
        assert!(sanitized.contains("user:****"));
        assert!(sanitized.contains("localhost:5432/db"));
    }

    #[test]
    fn test_connection_config_display() {
        let config = ConnectionConfig::new("localhost".to_string())
            .with_port(5432)
            .with_database("testdb".to_string());
        
        let display_output = format!("{}", config);
        
        // Should show connection info but never credentials
        assert!(display_output.contains("localhost:5432"));
        assert!(display_output.contains("testdb"));
        assert!(!display_output.contains("password"));
        assert!(!display_output.contains("secret"));
    }
}
```

## Integration Testing

### Database Integration Tests

Integration tests use testcontainers to spin up real databases:

```rust
// tests/integration/postgres_tests.rs
use testcontainers::{clients, images};
use dbsurveyor_core::adapters::create_adapter;

#[tokio::test]
async fn test_postgres_schema_collection() {
    let docker = clients::Cli::default();
    let postgres = docker.run(images::postgres::Postgres::default());

    let port = postgres.get_host_port_ipv4(5432);
    let database_url = format!(
        "postgres://postgres:postgres@localhost:{}/postgres",
        port
    );

    // Wait for PostgreSQL to be ready
    wait_for_postgres(&database_url).await;

    // Create test schema
    let pool = sqlx::PgPool::connect(&database_url).await
        .expect("Failed to connect to PostgreSQL");

    sqlx::query("CREATE TABLE IF NOT EXISTS users (id INT PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .expect("Failed to create test table");

    // Test schema collection
    let adapter = create_adapter(&database_url).await
        .expect("Failed to create adapter");

    let schema = adapter.collect_schema().await
        .expect("Failed to collect schema");

    // Verify schema was collected correctly
    assert!(!schema.tables.is_empty());
    
    let users_table = schema.tables.iter()
        .find(|t| t.name == "users")
        .expect("Users table not found");

    assert_eq!(users_table.name, "users");
    assert_eq!(users_table.columns.len(), 2);

    // Clean up
    pool.close().await;
}

async fn wait_for_postgres(database_url: &str) {
    let max_attempts = 30;
    let mut attempts = 0;

    while attempts < max_attempts {
        match sqlx::PgPool::connect(database_url).await {
            Ok(pool) => {
                pool.close().await;
                return;
            }
            Err(_) => {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }

    panic!("PostgreSQL failed to become ready after {} attempts", max_attempts);
}
```

### MySQL Integration Tests

```rust
// tests/integration/mysql_tests.rs
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

    // Wait for MySQL to be ready with exponential backoff
    wait_for_mysql(&database_url).await;

    let adapter = create_adapter(&database_url).await
        .expect("Failed to create MySQL adapter");

    let schema = adapter.collect_schema().await
        .expect("Failed to collect MySQL schema");

    assert!(!schema.tables.is_empty());
}
```

### SQLite Integration Tests

```rust
// tests/integration/sqlite_tests.rs
use tempfile::tempdir;

#[tokio::test]
async fn test_sqlite_schema_collection() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test database
    let conn = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display())).await?;

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&conn).await?;

    conn.close().await;

    // Test schema collection
    let database_url = format!("sqlite://{}", db_path.display());
    let adapter = create_adapter(&database_url).await
        .expect("Failed to create SQLite adapter");

    let schema = adapter.collect_schema().await
        .expect("Failed to collect SQLite schema");

    assert!(schema.tables.iter().any(|t| t.name == "users"));
}
```

## Security Testing

### Credential Protection Tests

Security tests verify that credentials never appear in outputs:

```rust
// tests/security/credential_tests.rs
#[tokio::test]
async fn test_no_credentials_in_schema_output() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = "postgres://testuser:secret123@localhost:5432/testdb";

    // Create mock schema (in real test, would collect from database)
    let db_info = DatabaseInfo::new("testdb".to_string());
    let schema = DatabaseSchema::new(db_info);
    
    let json_output = serde_json::to_string(&schema)?;

    // Verify no sensitive data is present
    assert!(!json_output.contains("secret123"));
    assert!(!json_output.contains("testuser:secret123"));
    assert!(!json_output.contains("password"));
    assert!(!json_output.contains("secret"));

    Ok(())
}

#[test]
fn test_error_message_sanitization() {
    let connection_url = "postgres://user:secret@host:5432/db";
    
    // Simulate connection error
    let error = DbSurveyorError::connection_failed("Connection timeout");
    let error_message = format!("{}", error);
    
    // Error message should not contain credentials
    assert!(!error_message.contains("secret"));
    assert!(!error_message.contains("user:secret"));
}
```

### Encryption Tests

Comprehensive encryption testing ensures security guarantees:

```rust
// tests/security/encryption_tests.rs
#[cfg(feature = "encryption")]
mod encryption_tests {
    use dbsurveyor_core::security::encryption::{encrypt_data, decrypt_data};

    #[tokio::test]
    async fn test_encryption_roundtrip() {
        let original_data = b"sensitive database schema data";
        let password = "test_password_123";

        // Encrypt the data
        let encrypted = encrypt_data(original_data, password).unwrap();

        // Verify encrypted data structure
        assert_eq!(encrypted.algorithm, "AES-GCM-256");
        assert_eq!(encrypted.nonce.len(), 12); // 96 bits
        assert_eq!(encrypted.auth_tag.len(), 16); // 128 bits
        assert!(!encrypted.ciphertext.is_empty());

        // Decrypt the data
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        // Verify roundtrip
        assert_eq!(original_data, &decrypted[..]);
    }

    #[tokio::test]
    async fn test_nonce_uniqueness() {
        let data = b"test data for nonce uniqueness";
        let password = "same_password";

        // Generate multiple encryptions
        let encrypted1 = encrypt_data(data, password).unwrap();
        let encrypted2 = encrypt_data(data, password).unwrap();
        let encrypted3 = encrypt_data(data, password).unwrap();

        // Nonces should be different (random)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted2.nonce, encrypted3.nonce);
        assert_ne!(encrypted1.nonce, encrypted3.nonce);

        // All should decrypt to same plaintext
        let decrypted1 = decrypt_data(&encrypted1, password).unwrap();
        let decrypted2 = decrypt_data(&encrypted2, password).unwrap();
        let decrypted3 = decrypt_data(&encrypted3, password).unwrap();

        assert_eq!(data, &decrypted1[..]);
        assert_eq!(data, &decrypted2[..]);
        assert_eq!(data, &decrypted3[..]);
    }

    #[tokio::test]
    async fn test_wrong_password_fails() {
        let data = b"secret data";
        let correct_password = "correct_password";
        let wrong_password = "wrong_password";

        let encrypted = encrypt_data(data, correct_password).unwrap();

        // Decryption with wrong password should fail
        let result = decrypt_data(&encrypted, wrong_password);
        assert!(result.is_err());
    }
}
```

### Offline Operation Tests

Verify that DBSurveyor works completely offline:

```rust
// tests/security/offline_tests.rs
#[tokio::test]
async fn test_airgap_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate airgap environment by testing without network access
    let schema_data = include_bytes!("../fixtures/sample_schema.json");
    let schema: DatabaseSchema = serde_json::from_slice(schema_data)?;

    // All processing should work offline
    let documentation = generate_documentation(&schema, OutputFormat::Markdown).await?;
    assert!(!documentation.is_empty());

    let json_export = generate_documentation(&schema, OutputFormat::Json).await?;
    assert!(!json_export.is_empty());

    Ok(())
}
```

## Performance Testing

### Benchmarks

Performance benchmarks use Criterion:

```rust
// benches/collection.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dbsurveyor_core::models::DatabaseSchema;

fn bench_schema_serialization(c: &mut Criterion) {
    let schema = create_large_test_schema(1000); // 1000 tables

    c.bench_function("schema_to_json", |b| {
        b.iter(|| serde_json::to_string(black_box(&schema)))
    });

    c.bench_function("schema_to_markdown", |b| {
        b.iter(|| generate_markdown_documentation(black_box(&schema)))
    });
}

fn create_large_test_schema(table_count: usize) -> DatabaseSchema {
    let db_info = DatabaseInfo::new("benchmark_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);
    
    for i in 0..table_count {
        let table = create_test_table(&format!("table_{}", i));
        schema.tables.push(table);
    }
    
    schema
}

criterion_group!(benches, bench_schema_serialization);
criterion_main!(benches);
```

### Memory Testing

```rust
#[tokio::test]
async fn test_memory_usage_large_schema() {
    let initial_memory = get_memory_usage();
    
    // Process large schema
    let large_schema = create_schema_with_tables(10000);
    let _documentation = generate_documentation(&large_schema, OutputFormat::Html).await?;
    
    let final_memory = get_memory_usage();
    let memory_increase = final_memory - initial_memory;
    
    // Ensure memory usage is reasonable (< 100MB for 10k tables)
    assert!(memory_increase < 100 * 1024 * 1024);
}
```

## Test Configuration

### Nextest Configuration

DBSurveyor uses nextest for enhanced test execution:

```toml
# .config/nextest.toml
[profile.default]
retries = 2
test-threads = 4
failure-output = "immediate"
success-output = "never"

# Test groups for different types of tests
[[profile.default.overrides]]
filter = "test(integration)"
test-group = "integration"
max-threads = 2  # Limit concurrent container tests

[[profile.default.overrides]]
filter = "test(security)"
test-group = "security"
max-threads = 1  # Security tests run sequentially

[[profile.default.overrides]]
filter = "test(unit)"
test-group = "unit"
max-threads = 8  # Unit tests can run in parallel
```

### CI Test Configuration

```yaml
# .github/workflows/test.yml
- name: Run Tests
  run: |
    # Run tests with CI profile
    cargo nextest run --profile ci --workspace --all-features
    
    # Generate coverage
    cargo llvm-cov --lcov --output-path lcov.info
    
    # Security validation
    just security-full
```

## Test Data and Fixtures

### Test Data Generation

```rust
// tests/fixtures/mod.rs
pub fn create_test_schema() -> DatabaseSchema {
    let db_info = DatabaseInfo::new("test_db".to_string());
    let mut schema = DatabaseSchema::new(db_info);
    
    schema.tables = vec![
        create_test_table("users"),
        create_test_table("orders"),
        create_test_table("products"),
    ];
    
    schema
}

pub fn create_test_table(name: &str) -> Table {
    Table {
        name: name.to_string(),
        schema: Some("public".to_string()),
        columns: vec![
            Column {
                name: "id".to_string(),
                data_type: UnifiedDataType::Integer { bits: 32, signed: true },
                is_nullable: false,
                is_primary_key: true,
                is_auto_increment: true,
                default_value: None,
                comment: None,
                ordinal_position: 1,
            }
        ],
        primary_key: Some(PrimaryKey {
            name: Some(format!("{}_pkey", name)),
            columns: vec!["id".to_string()],
        }),
        foreign_keys: vec![],
        indexes: vec![],
        constraints: vec![],
        comment: None,
        row_count: Some(100),
    }
}
```

### Fixture Files

```
tests/fixtures/
├── sample_schemas/
│   ├── postgres_sample.json
│   ├── mysql_sample.json
│   └── sqlite_sample.json
├── test_databases/
│   ├── small_db.sql
│   ├── medium_db.sql
│   └── large_db.sql
└── encrypted_samples/
    ├── encrypted_schema.enc
    └── compressed_schema.zst
```

## Testing Best Practices

### Test Quality Standards

- **Comprehensive**: Test happy path, error cases, and edge conditions
- **Isolated**: Tests should not depend on external services (except testcontainers)
- **Deterministic**: Tests must produce consistent results
- **Fast**: Unit tests should complete in milliseconds
- **Secure**: No real credentials in test code; use explicit dummy values only

### Common Testing Patterns

```rust
// Use Result<(), Box<dyn std::error::Error>> for test functions
#[tokio::test]
async fn test_function() -> Result<(), Box<dyn std::error::Error>> {
    // Test implementation
    Ok(())
}

// Test both Ok and Err cases
#[test]
fn test_validation() {
    // Test success case
    assert!(validate_input("valid").is_ok());
    
    // Test failure cases
    assert!(validate_input("").is_err());
    assert!(validate_input("invalid").is_err());
}

// Use assert_matches! for pattern matching
#[test]
fn test_error_types() {
    let result = operation_that_fails();
    assert_matches!(result, Err(DbSurveyorError::Configuration { .. }));
}
```

### Security Testing Requirements

Every security-sensitive function must have tests that verify:

1. **No credential leakage** in any output
2. **Proper error sanitization** in all error paths
3. **Secure memory handling** with automatic cleanup
4. **Offline operation** without external dependencies

## Continuous Integration

### GitHub Actions Integration

```yaml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        
      - name: Install tools
        run: just install
        
      - name: Run tests
        run: just test-ci
        
      - name: Security validation
        run: just security-full
        
      - name: Coverage
        run: just coverage-ci
```

### Test Reporting

- **Coverage Reports**: Uploaded to Codecov
- **Test Results**: JUnit XML format for CI integration
- **Performance**: Criterion benchmark results
- **Security**: Security test results and audit reports

This comprehensive testing strategy ensures DBSurveyor maintains its security guarantees while providing reliable functionality across all supported platforms and databases.
