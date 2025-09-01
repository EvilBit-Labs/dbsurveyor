---
inclusion: always
---

# Documentation Standards for DBSurveyor

## Core Principles

- **Security-First**: Document security guarantees and credential handling
- **Offline-First**: All documentation must work without internet
- **Example-Driven**: Include working, testable code examples
- **Error-Aware**: Document all error conditions and security implications

## Rust Documentation Requirements

### Module Documentation Template

````rust
//! Database schema collection for [DATABASE_TYPE].
//!
//! # Security Guarantees
//! - No credentials stored or logged
//! - AES-256-GCM encryption for sensitive data
//! - Read-only database operations only
//! - Zero external network dependencies
//!
//! # Example
//! ```rust
//! use dbsurveyor_core::collectors::PostgresCollector;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let collector = PostgresCollector::new("postgres://user:pass@localhost/db").await?;
//! let schema = collector.collect_schema().await?;
//! println!("Found {} tables", schema.tables.len());
//! # Ok(())
//! # }
//! ```
````

### Function Documentation Template

````rust
/// Brief description of what the function does.
///
/// # Security
/// - Credentials never logged or stored
/// - All operations are read-only
/// - Connection strings sanitized in errors
///
/// # Arguments
/// * `param` - Description with security implications if relevant
///
/// # Errors
/// Returns error if:
/// - Connection fails or times out
/// - Insufficient privileges
/// - Unsupported database features
///
/// # Example
/// ```rust
/// let result = function_name(param).await?;
/// assert!(!result.is_empty());
/// ```
pub async fn function_name(param: Type) -> Result<ReturnType, Error> {
    // Implementation
}
````

### Error Type Documentation

```rust
/// Errors during database schema collection.
///
/// # Security
/// All error messages are sanitized to prevent credential leakage.
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    /// Database connection failed (credentials sanitized)
    #[error("Database connection failed: {context}")]
    ConnectionFailed { context: String },

    /// Insufficient privileges for schema access
    #[error("Insufficient privileges: {required}")]
    InsufficientPrivileges { required: String },
}
```

## Documentation Commands

```bash
# Generate and test documentation
cargo doc --all-features --document-private-items
cargo test --doc --all-features

# Check for missing docs and broken links
cargo doc --all-features 2>&1 | grep -i warning
```

## Quality Requirements

### Mandatory Documentation

- All public APIs must have `///` documentation
- Include security implications for credential-handling functions
- Provide working examples that pass `cargo test --doc`
- Document error conditions with security context
- Include performance notes for expensive operations

### Style Guidelines

- Use present tense: "returns" not "will return"
- Be specific about security guarantees and constraints
- Include practical, compilable examples
- Document all possible error conditions
- Use consistent terminology across codebase

### Security Documentation Rules

- **ALWAYS** document credential handling behavior
- **ALWAYS** specify if function logs or stores sensitive data
- **ALWAYS** document encryption/decryption operations
- **NEVER** include actual credentials in examples
- **ALWAYS** mention sanitization of error messages
