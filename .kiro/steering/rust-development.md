---
inclusion: fileMatch
fileMatchPattern: '**/*.rs'
---

# Rust Development Standards for DBSurveyor

## Language Requirements

- **Rust Version**: 1.77+ (MSRV as specified in workspace Cargo.toml)
- **Edition**: 2021
- **Toolchain**: Stable channel preferred

## Code Quality Standards

### Zero Warnings Policy

All code must pass `cargo clippy -- -D warnings` with zero warnings. This is strictly enforced.

### Formatting Standards

- Use `cargo fmt` with default settings (4-space indentation)
- Follow standard Rust formatting conventions
- Maintain consistent style across all files

### Naming Conventions

- **Functions/Variables**: `snake_case`
- **Types/Structs/Enums**: `PascalCase`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **Modules**: `snake_case`
- **Lifetimes**: Single lowercase letters (`'a`, `'b`)

## Error Handling Patterns

Use `thiserror` for custom error types and maintain comprehensive error context:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed to database")]
    ConnectionFailed,

    #[error("Schema discovery failed: {0}")]
    SchemaDiscoveryFailed(String),

    #[error("Query execution failed")]
    QueryFailed(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, DatabaseError>;
```

## Security-Focused Development

### Database Operations

- Use parameterized queries only - NO string concatenation
- All database operations must be read-only
- Never log or expose credentials in error messages
- Implement proper connection pooling with timeouts

### Credential Protection

```rust
// ✅ Secure: No credential exposure
#[derive(Debug)]
pub struct DatabaseConfig {
    host: String,
    port: u16,
    database: String,
    // Never log or display credentials
}

impl std::fmt::Display for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DatabaseConfig({}:{}@{})",
            self.host, self.port, self.database
        )
        // Intentionally omit credentials
    }
}
```

## Async/Await Standards

Use tokio for async operations with proper error handling and timeouts:

```rust
use tokio::time::{timeout, Duration};

async fn collect_with_timeout(url: &str) -> Result<Schema> {
    let timeout_duration = Duration::from_secs(30);

    timeout(timeout_duration, collect_schema_internal(url))
        .await
        .map_err(|_| DatabaseError::ConnectionTimeout)?
        .map_err(DatabaseError::from)
}
```

## Testing Requirements

### Test Organization

- Unit tests: `#[cfg(test)]` modules in source files
- Integration tests: `tests/` directory with testcontainers
- Security tests: Verify no credential leakage
- Performance tests: Use criterion for benchmarks

### Security Testing

```rust
#[tokio::test]
async fn test_no_credentials_in_output() {
    let database_url = "postgres://user:secret@localhost/db";
    let output = generate_schema_doc(database_url).await?;

    // Verify no sensitive data leaks
    assert!(!output.contains("secret"));
    assert!(!output.contains("password"));
    assert!(!output.contains("user:secret"));
}
```

## Documentation Standards

All public APIs must have comprehensive `///` documentation including:

- Security considerations and guarantees
- Error conditions and handling
- Performance characteristics
- Usage examples that compile and run

## Architecture Patterns

- **Repository Pattern**: Database access abstraction
- **Service Pattern**: Business logic encapsulation
- **Factory Pattern**: Database driver instantiation
- **Command Pattern**: CLI organization with Clap
- **Error Chaining**: Comprehensive error context

## Key Principles

1. **Security First**: Every API must be secure by default
2. **Zero Warnings**: All clippy warnings must be addressed
3. **Comprehensive Testing**: Unit, integration, and security tests
4. **Clear Documentation**: All public APIs documented
5. **Performance Aware**: Efficient database operations
6. **Error Context**: Rich error information without exposing credentials
7. **Offline Compatible**: No external dependencies at runtime
