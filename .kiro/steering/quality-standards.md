---
inclusion: always
---

# Quality Standards for DBSurveyor

## Quality Philosophy

- **Zero Warnings Policy**: All code must compile with `cargo clippy -- -D warnings`
- **Security-First Quality**: Quality checks emphasize security vulnerabilities
- **Automated Enforcement**: Quality gates prevent low-quality code from merging
- **Consistent Standards**: All contributors follow identical quality standards

## Rust Code Quality

### Clippy Configuration

Essential clippy lints that must be enforced:

```toml
# Workspace level clippy configuration
[workspace.lints.clippy]
# Mandatory lints - treat as errors
all = "deny"
correctness = "deny"
suspicious = "deny"
complexity = "deny"
perf = "deny"
style = "warn"
pedantic = "warn"

# Security-specific lints
await_holding_lock = "deny"
await_holding_refcell_ref = "deny"
large_stack_arrays = "deny"
rc_buffer = "deny"
rc_mutex = "deny"
redundant_clone = "deny"
suspicious_else_formatting = "deny"

# Performance lints
inefficient_to_string = "deny"
large_enum_variant = "deny"
large_types_passed_by_value = "warn"
linkedlist = "deny"
mutex_atomic = "deny"
or_fun_call = "deny"
slow_vector_initialization = "deny"

# Security and correctness
clone_on_ref_ptr = "deny"
cmp_null = "deny"
drop_copy = "deny"
drop_ref = "deny"
forget_copy = "deny"
forget_ref = "deny"
mem_forget = "deny"
transmute_int_to_float = "deny"
undropped_manually_drops = "deny"
unused_self = "deny"

[workspace.lints.rust]
unsafe_code = "forbid"             # No unsafe code allowed
missing_docs = "warn"              # Document public APIs
dead_code = "warn"                 # Clean up unused code
unused_imports = "deny"            # Remove unused imports
unused_variables = "deny"          # Clean variable usage
```

### Formatting Standards

Use `cargo fmt` with these key settings:

- **Max Width**: 100 characters
- **Tab Spaces**: 4 spaces (no hard tabs)
- **Newline Style**: Unix
- **Imports Layout**: Mixed with crate grouping
- **Trailing Comma**: Vertical only

### Code Organization Standards

```rust
// File organization template
//! Module-level documentation
//!
//! Security considerations and guarantees
//! Usage examples and patterns

// Standard library imports first
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::time::Duration;

// External crate imports second, grouped by crate
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};
use thiserror::Error;
use tokio::time::timeout;

// Internal imports last
use crate::error::{CollectorError, Result};
use crate::models::{DatabaseSchema, Table};

// Constants and type aliases
const MAX_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
type ConnectionPool = Pool<Postgres>;

// Public types first
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicStruct {
    pub field: String,
}

// Private types second
#[derive(Debug)]
struct PrivateStruct {
    field: String,
}

// Implementations
impl PublicStruct {
    /// Public constructor with comprehensive documentation
    pub fn new(field: String) -> Self {
        Self { field }
    }
}

// Tests at the end
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_struct_creation() {
        let instance = PublicStruct::new("test".to_string());
        assert_eq!(instance.field, "test");
    }
}
```

## Error Handling Quality

### Error Type Standards

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbSurveyorError {
    /// Database connection errors with sanitized messages
    #[error("Connection failed: {context}")]
    Connection {
        context: String,  // Never include credentials
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// IO errors with context
    #[error("IO operation failed: {operation}")]
    Io {
        operation: String,
        #[source]
        source: std::io::Error,
    },

    /// Encryption errors (no sensitive details)
    #[error("Encryption operation failed")]
    Encryption {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

// Security-conscious error message sanitization
impl DbSurveyorError {
    /// Get sanitized error message safe for logging
    pub fn sanitized_message(&self) -> String {
        match self {
            DbSurveyorError::Connection { context, .. } => {
                // Remove any potential credential fragments
                let sanitized = context
                    .split('@')
                    .last()
                    .unwrap_or("unknown")
                    .to_string();
                format!("Connection failed to {}", sanitized)
            }
            _ => self.to_string(),
        }
    }
}
```

### Result Type Patterns

```rust
// Consistent Result type usage
pub type Result<T> = std::result::Result<T, DbSurveyorError>;

// Function signature patterns
pub async fn collect_schema(
    connection_string: &str,
    options: CollectionOptions,
) -> Result<DatabaseSchema> {
    let connection = establish_connection(connection_string)
        .await
        .map_err(|e| DbSurveyorError::Connection {
            context: "Failed to establish database connection".to_string(),
            source: Box::new(e),
        })?;

    let schema = extract_schema(&connection)
        .await
        .map_err(|e| DbSurveyorError::Collection {
            context: "Failed to extract schema information".to_string(),
            source: Box::new(e),
        })?;

    Ok(schema)
}
```

## Testing Quality Standards

### Test Structure and Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test helper for creating mock database schemas
    fn create_test_schema() -> DatabaseSchema {
        DatabaseSchema {
            database_name: "test_db".to_string(),
            database_type: DatabaseType::PostgreSQL,
            tables: vec![create_test_table("users")],
            created_at: chrono::Utc::now(),
        }
    }

    /// Test basic functionality with clear naming
    #[tokio::test]
    async fn test_postgres_schema_collection_success() {
        // Arrange
        let connection_string = "postgres://test:test@localhost/test";
        let collector = PostgresCollector::new(connection_string).await?;

        // Act
        let result = collector.collect_schema().await;

        // Assert
        assert!(result.is_ok());
        let schema = result.unwrap();
        assert_eq!(schema.database_type, DatabaseType::PostgreSQL);
        assert!(!schema.tables.is_empty());
    }

    /// Test error conditions thoroughly
    #[tokio::test]
    async fn test_postgres_connection_failure_sanitizes_credentials() {
        // Arrange
        let connection_string = "postgres://admin:supersecret@nonexistent:5432/db";

        // Act
        let result = PostgresCollector::new(connection_string).await;

        // Assert
        assert!(result.is_err());
        let error_message = format!("{}", result.unwrap_err());
        assert!(!error_message.contains("supersecret"));
        assert!(!error_message.contains("admin:supersecret"));
    }

    /// Property-based testing for edge cases
    #[tokio::test]
    async fn test_schema_serialization_roundtrip() {
        let original_schema = create_test_schema();

        // Test JSON roundtrip
        let json = serde_json::to_string(&original_schema)?;
        let deserialized: DatabaseSchema = serde_json::from_str(&json)?;
        assert_eq!(original_schema, deserialized);

        // Test encrypted roundtrip
        let encrypted = encrypt_schema_data(&json.into_bytes()).await?;
        let decrypted = decrypt_schema_data(&encrypted).await?;
        let decrypted_schema: DatabaseSchema = serde_json::from_slice(&decrypted)?;
        assert_eq!(original_schema, decrypted_schema);
    }
}
```

### Benchmark Quality Standards

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_schema_collection(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("schema_collection");

    // Test different database sizes
    for table_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("postgres", table_count),
            table_count,
            |b, &table_count| {
                b.to_async(&runtime).iter(|| async {
                    let schema = create_mock_schema_with_tables(table_count);
                    black_box(schema)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_schema_collection);
criterion_main!(benches);
```

## Quality Assurance Process

### Quality Check Commands

```bash
#!/bin/bash
# Comprehensive quality validation

set -euo pipefail

echo "🔍 Running comprehensive quality checks..."

# 1. Formatting check
echo "📏 Checking code formatting..."
cargo fmt --all -- --check

# 2. Clippy with zero warnings
echo "🔧 Running Clippy with strict linting..."
cargo clippy --all-features --all-targets -- -D warnings

# 3. Test execution
echo "🧪 Running all tests..."
cargo test --all-features --verbose

# 4. Documentation tests
echo "📚 Running documentation tests..."
cargo test --doc --all-features

# 5. Security audit
echo "🛡️  Running security audit..."
cargo audit

# 6. Coverage check
echo "📊 Checking test coverage..."
cargo llvm-cov --all-features --workspace --fail-under-lines 80

# 7. Documentation generation
echo "📖 Generating documentation..."
cargo doc --all-features --no-deps

echo "✅ All quality checks passed!"
```

### Pre-commit Quality Hooks

Essential quality hooks for development:

- **Rust formatting**: `cargo fmt --all -- --check`
- **Rust clippy**: `cargo clippy --all-features --all-targets -- -D warnings`
- **Rust tests**: `cargo test --all-features`
- **Documentation tests**: `cargo test --doc --all-features`
- **Security audit**: `cargo audit`
- **Coverage check**: `cargo llvm-cov --all-features --workspace --fail-under-lines 80`

## Quality Metrics and Monitoring

### Code Quality Metrics

- **Cyclomatic Complexity**: Maximum 10 per function
- **Test Coverage**: Minimum 80% line coverage
- **Documentation Coverage**: 100% for public APIs
- **Clippy Warnings**: Zero warnings policy
- **Security Vulnerabilities**: Zero known vulnerabilities
- **Performance Regressions**: <5% performance degradation per release

### Quality Dashboard Configuration

```toml
# Quality measurement tools
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
cargo-llvm-cov = "0.5"
cargo-audit = "0.18"
cargo-outdated = "0.13"
cargo-machete = "0.6"  # Find unused dependencies

[package.metadata.coverage]
min-coverage = 80
exclude-files = ["tests/*", "benches/*", "examples/*"]

[package.metadata.audit]
ignore = []  # List of CVE IDs to ignore (with justification)
```

## Performance Quality Standards

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

This comprehensive quality framework ensures that DBSurveyor maintains the highest standards of code quality, security, and maintainability throughout its development lifecycle.
