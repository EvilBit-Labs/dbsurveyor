---
inclusion: always
---

# Quality Standards for DBSurveyor

## Core Quality Rules

- **Zero Warnings Policy**: All code must pass `cargo clippy -- -D warnings`
- **Security-First Quality**: Prioritize security in all quality checks
- **80% Test Coverage**: Minimum coverage threshold enforced
- **Documentation Required**: All public APIs must have `///` docs

## Essential Lints (Workspace Configuration)

```toml
[workspace.lints.clippy]
all = "deny"
correctness = "deny"
suspicious = "deny"
complexity = "deny"
perf = "deny"
style = "warn"

# Security-critical lints
await_holding_lock = "deny"
rc_buffer = "deny"
redundant_clone = "deny"

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unused_imports = "deny"
unused_variables = "deny"
```

## Code Organization Template

```rust
//! Module documentation with security guarantees

// Imports: std → external → internal
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Constants and type aliases
const MAX_TIMEOUT: Duration = Duration::from_secs(30);

// Public types first, private second
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicStruct {
    pub field: String,
}

// Implementations with full documentation
impl PublicStruct {
    /// Constructor with security notes if applicable
    pub fn new(field: String) -> Self {
        Self { field }
    }
}

// Tests at end in #[cfg(test)] module
```

## Error Handling Standards

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbSurveyorError {
    /// Connection failed (credentials sanitized)
    #[error("Connection failed: {context}")]
    Connection {
        context: String,  // Never include credentials
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

// Always use Result<T> = std::result::Result<T, DbSurveyorError>
pub async fn collect_schema(url: &str) -> Result<DatabaseSchema> {
    let connection = establish_connection(url)
        .await
        .map_err(|e| DbSurveyorError::Connection {
            context: "Database connection failed".to_string(),
            source: Box::new(e),
        })?;
    Ok(schema)
}
```

## Testing Requirements

### Security Test Pattern

```rust
#[tokio::test]
async fn test_no_credentials_in_output() {
    let schema = collect_schema("postgres://user:secret@host/db").await?;
    let json = serde_json::to_string(&schema)?;
    assert!(!json.contains("secret"));
    assert!(!json.contains("user:secret"));
}
```

### Test Organization

- Use `#[cfg(test)]` modules co-located with code
- Name tests descriptively: `test_postgres_connection_failure_sanitizes_credentials`
- Follow Arrange-Act-Assert pattern
- Test both success and error conditions
- Always test credential sanitization for security-sensitive code

## Quality Commands

```bash
# Primary development workflow
just dev                    # format + lint + test + coverage
just lint                   # clippy with -D warnings
just test                   # full test suite
just coverage              # >80% coverage required

# Security validation
just test-credential-security  # verify no credential leakage
just security-full            # complete security validation
```

## Quality Gates (Pre-commit)

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] Coverage >80%
- [ ] No credentials in logs/outputs
- [ ] All public APIs documented

## Performance Standards

- Connection pooling: 5-10 connections max
- Query timeouts: 30 seconds default
- Stream large datasets (don't load into memory)
- Use `Arc<T>` for shared immutable data
- Monitor memory usage in benchmarks

## Anti-Patterns (Never Do)

```rust
// ❌ NEVER: Expose credentials
log::info!("Connecting to {}", database_url);
#[error("Failed to connect to {url}")]

// ❌ NEVER: Use unwrap in production
let result = operation().unwrap();

// ❌ NEVER: Ignore clippy
#[allow(clippy::all)]

// ❌ NEVER: SQL injection risk
let query = format!("SELECT * FROM {}", table);
```
