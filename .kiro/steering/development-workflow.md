---
inclusion: always
---

# Development Workflow for DBSurveyor

## Pre-Development Checklist

Before making any changes:

- [ ] Review `project_specs/requirements.md` for context
- [ ] Check existing code patterns in similar components
- [ ] Verify security implications of planned changes
- [ ] Ensure changes align with offline-first architecture

## Development Process

### 1. Code Development

Start with a clean environment:

```bash
# Format and lint first
just format
just lint

# Implement changes following Rust patterns
# - Use Result<T, E> for error handling
# - Add comprehensive /// documentation
# - Follow repository/service patterns
# - Implement security-first database access
```

### 2. Testing Requirements

All new code must include appropriate tests:

```bash
# Unit tests (required for all new code)
cargo test --lib

# Integration tests with testcontainers
cargo test --test integration_tests

# Security validation
just test-credential-security
just test-encryption
just test-offline

# Database-specific tests
just test-postgres
just test-mysql
just test-sqlite
```

### 3. Quality Assurance

Run comprehensive quality checks:

```bash
# Code quality gates
just format-check
just lint                # cargo clippy -- -D warnings
just coverage           # >80% threshold required

# Security audit
just security-audit     # SBOM + vulnerability scan
just security-full      # Complete security validation

# CI validation
just ci-check           # Full CI equivalent
```

## Code Review Checklist

### Security Review

- [ ] **No credential exposure**: Database URLs not logged or output
- [ ] **Read-only operations**: All database queries are SELECT/DESCRIBE only
- [ ] **Offline compatibility**: No external network calls except to databases
- [ ] **Encryption compliance**: AES-GCM with random nonce if applicable
- [ ] **Input validation**: All user inputs properly validated and sanitized

### Rust Code Review

- [ ] **Zero warnings**: `cargo clippy -- -D warnings` passes
- [ ] **Error handling**: All `Result` types properly handled with `?` operator
- [ ] **Documentation**: All public items have `///` documentation
- [ ] **Testing**: Unit tests for logic, integration tests for database operations
- [ ] **Type safety**: Leverages Rust's type system and SQLx compile-time checks

### Architecture Review

- [ ] **Pattern compliance**: Follows Repository/Service patterns
- [ ] **Separation of concerns**: Database logic separated from business logic
- [ ] **Async/await**: Proper async handling with Tokio
- [ ] **Resource management**: Proper connection pooling and cleanup
- [ ] **Configuration**: Secure configuration management without hardcoded values

## Essential Commands

### Development Commands

```bash
# Primary development workflow
just dev                 # Run development checks (format, lint, test, coverage)
just install            # Install dependencies and setup environment
just build              # Complete build with security optimizations

# Code quality
just format             # Format code with cargo fmt
just lint               # Run linting with strict warnings
just check              # Run pre-commit hooks and comprehensive checks
just ci-check           # Run CI-equivalent checks locally

# Testing
just test               # Run the full test suite with security verification
just test-postgres      # Test PostgreSQL adapter specifically
just test-mysql         # Test MySQL adapter specifically
just test-sqlite        # Test SQLite adapter specifically
just coverage           # Run test coverage with >80% threshold
just coverage-html      # Generate HTML coverage report

# Security validation
just security-full      # Complete security validation suite
just test-encryption    # Verify AES-GCM encryption with random nonce
just test-offline       # Test airgap compatibility
just test-credential-security  # Verify no credential leakage
just security-audit     # Generate SBOM and vulnerability reports

# Building and packaging
just build-release      # Build optimized release version
just build-minimal      # Build minimal airgap-compatible version
just package-airgap     # Create offline deployment package
```

### Usage Examples

```bash
# Primary use cases - Database schema collection and documentation
cargo run --bin dbsurveyor-collect -- --database-url postgres://user:pass@localhost/db --output schema.json
cargo run --bin dbsurveyor -- --input schema.json --format markdown --output schema.md

# Security-focused operation
export DATABASE_URL="postgres://user:pass@localhost/db"
cargo run --bin dbsurveyor-collect -- --output encrypted_schema.bin --encrypt
cargo run --bin dbsurveyor -- --input encrypted_schema.bin --decrypt --format json
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name_expected_behavior() {
        // Arrange
        let input = create_test_input();

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected_output);
    }

    #[test]
    fn test_credential_sanitization() {
        let config = DatabaseConfig::new("postgres://user:secret@host/db");
        let output = config.safe_display();
        assert!(!output.contains("secret"));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_postgres_integration() {
    let docker = testcontainers::clients::Cli::default();
    let postgres = docker.run(testcontainers::images::postgres::Postgres::default());

    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres",
                              postgres.get_host_port_ipv4(5432));

    let collector = PostgresCollector::new(&database_url).await?;
    let schema = collector.collect_schema().await?;

    assert!(!schema.tables.is_empty());
}
```

### Security Tests

```rust
#[tokio::test]
async fn test_no_credentials_in_output() {
    let database_url = "postgres://testuser:secret@localhost/testdb";
    let schema = collect_schema(database_url).await?;
    let json_output = serde_json::to_string(&schema)?;

    assert!(!json_output.contains("secret"));
    assert!(!json_output.contains("testuser:secret"));
}
```

## Performance Guidelines

### Database Operations

- Use connection pooling with reasonable limits (5-10 connections)
- Implement query timeouts (30 seconds default)
- Stream large result sets instead of loading into memory
- Use prepared statements for repeated queries

### Memory Management

- Avoid cloning large data structures unnecessarily
- Use `Arc<T>` for shared immutable data
- Stream processing for large files
- Monitor memory usage with benchmarks

## Documentation Standards

### Code Documentation

```rust
/// Collects comprehensive schema information from a PostgreSQL database.
///
/// This function connects to the database using the provided connection string
/// and extracts table, column, index, and constraint information. All operations
/// are read-only and safe for production environments.
///
/// # Arguments
///
/// * `database_url` - PostgreSQL connection string (credentials will not be logged)
///
/// # Returns
///
/// Returns `Ok(Schema)` containing the complete database schema, or an error
/// if the connection fails or schema collection encounters issues.
///
/// # Security
///
/// - Database credentials are never logged or stored
/// - All database operations are read-only
/// - Connection strings are sanitized in error messages
///
/// # Examples
///
/// ```rust,no_run
/// use dbsurveyor_shared::collect_postgres_schema;
///
/// let schema = collect_postgres_schema("postgres://user:pass@localhost/db").await?;
/// println!("Found {} tables", schema.tables.len());
/// ```
pub async fn collect_postgres_schema(database_url: &str) -> Result<Schema> {
    // Implementation
}
```

## Final Checklist

Before reporting completion:

- [ ] All tests pass (`just test`)
- [ ] Security validation passes (`just security-full`)
- [ ] Code coverage >80% (`just coverage`)
- [ ] No clippy warnings (`just lint`)
- [ ] Documentation updated for new features
- [ ] Offline operation verified
- [ ] No credentials exposed in any output or logs

## Common Pitfalls to Avoid

### Security Pitfalls

- ❌ Logging database connection strings
- ❌ Exposing credentials in error messages
- ❌ Making external network calls
- ❌ Storing credentials in configuration files

### Rust Pitfalls

- ❌ Ignoring clippy warnings
- ❌ Using `.unwrap()` in production code
- ❌ Missing error handling with `?` operator
- ❌ Inadequate testing coverage

### Database Pitfalls

- ❌ Not using connection pooling
- ❌ Forgetting query timeouts
- ❌ Not handling connection failures gracefully
- ❌ Performing write operations (this is a read-only tool)
