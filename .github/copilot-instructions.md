# GitHub Copilot Instructions for DBSurveyor

## Project Overview

DBSurveyor is a security-focused database documentation tool written in Rust that generates comprehensive schema documentation for both SQL (PostgreSQL, MySQL, SQLite) and NoSQL (MongoDB) databases. **Offline-first design** - all operations work without internet connectivity except for database connections.

## Core Principles

- **Security-First**: Offline-only operation with encrypted outputs and no telemetry
- **Database-Agnostic**: Unified interface across both SQL (PostgreSQL, MySQL, SQLite) and NoSQL (MongoDB) databases
- **Operator-Centric**: Efficient, auditable workflows for security professionals
- **Zero-Warning Policy**: `cargo clippy -- -D warnings` must pass without any warnings

## Critical Security Rules (NON-NEGOTIABLE)

1. **OFFLINE-ONLY OPERATION**: No network calls except to target databases for schema collection
2. **NO TELEMETRY**: Zero data collection or external reporting mechanisms
3. **CREDENTIAL PROTECTION**: Database credentials never appear in any output files, logs, or artifacts
4. **ENCRYPTION**: AES-GCM with random nonce, Argon2id KDF (min 64MB memory, 3 iterations, 4 parallelism), embedded deterministic test vectors, authenticated headers
5. **AIRGAP COMPATIBILITY**: Full functionality in air-gapped environments
6. **NO HTTP(S) EGRESS**: No HTTP(S) egress in CI/tests except to approved DB targets

## Technology Stack

| Layer             | Technology                               | Notes                                               |
|-------------------|------------------------------------------|-----------------------------------------------------|
| **Language**      | Rust 2021 Edition                        | Modern Rust with idiomatic patterns                 |
| **CLI**           | Clap v4 with derive macros               | Clean, user-friendly command-line interface         |
| **Async Runtime** | Tokio                                    | For async database operations                       |
| **Database**      | SQLx with async drivers                  | Type-safe database access                           |
| **Serialization** | Serde with JSON support                  | Data interchange and file I/O                       |
| **Encryption**    | AES-GCM with Argon2id KDF                | Secure data at rest with deterministic test vectors |
| **Testing**       | Built-in test framework + testcontainers | Unit and integration testing                        |

## Rust Coding Standards

### Code Quality Requirements

- **Formatting**: `cargo fmt` using standard Rust formatting
- **Linting**: `cargo clippy -- -D warnings` - ZERO warnings policy enforced
- **Naming**: Follow Rust conventions - `snake_case` for variables/functions, `PascalCase` for types
- **Error Handling**: Use `Result<T, E>` types and `?` operator, create custom error types with `thiserror`
- **Documentation**: Comprehensive `///` doc comments for all public APIs
- **Testing**: Unit tests co-located with code, integration tests in separate files
- **Safety**: `unsafe` code is denied at the workspace level

### Database Operations Standards

- **Connection Management**: Use connection pooling for performance and resource management
- **Query Safety**: Use parameterized queries only - NO string concatenation
- **Transaction Safety**: Proper transaction boundaries with rollback on errors
- **Schema Discovery**: Read-only operations only - NO schema modifications
- **Credential Handling**: Never log or output credentials in any form

## Project Architecture

### Workspace Structure

```text
/
├── bin/
│   ├── collector/             # Database collection binary
│   └── postprocessor/         # Data processing binary
├── crates/
│   └── shared/                # Shared library code
├── .github/workflows/         # GitHub Actions CI/CD
├── project_specs/             # Project specifications
├── justfile                   # Task runner configuration
├── Cargo.toml                 # Workspace configuration
└── cargo-deny.toml            # Security policy
```

### Architecture Patterns

- **Repository Pattern**: Database access abstraction layer
- **Service Pattern**: Business logic encapsulation
- **Factory Pattern**: Database driver instantiation
- **Command Pattern**: CLI command organization
- **Error Chaining**: Comprehensive error context through call stack

## Database Support

### Supported Engines

- **PostgreSQL**: Primary target with full feature support
- **MySQL**: Secondary target with core functionality
- **SQLite**: Minimal target for local development and testing
- **MongoDB**: NoSQL target for document database support (required for initial release)

### Database Operations

- **Read-Only**: All database operations are strictly read-only
- **Schema Discovery**: Automated discovery of tables, columns, indexes, constraints
- **Metadata Collection**: Gather statistics and metadata without modifying data
- **Connection Security**: Use TLS/SSL when available, validate certificates

## Common Patterns

### Error Handling Pattern

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {context}")]
    ConnectionFailed { context: String },

    #[error("Query execution failed")]
    QueryFailed(#[from] sqlx::Error),

    #[error("Schema not found: {schema}")]
    SchemaNotFound { schema: String },
}

type Result<T> = std::result::Result<T, DatabaseError>;
```

### Database Repository Pattern

```rust
#[async_trait]
pub trait SchemaRepository {
    async fn get_tables(&self) -> Result<Vec<Table>>;
    async fn get_columns(&self, table: &str) -> Result<Vec<Column>>;
    async fn get_indexes(&self, table: &str) -> Result<Vec<Index>>;
}
```

### CLI Command Pattern

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dbsurveyor")]
#[command(about = "Secure database schema documentation tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Collect {
        #[arg(short, long)]
        database_url: String,

        #[arg(short, long)]
        output: PathBuf,
    },
    Process {
        #[arg(short, long)]
        input: PathBuf,

        #[arg(short, long)]
        format: OutputFormat,
    },
}
```

## Testing Standards

### Test Organization

- **Unit Tests**: Test individual functions and modules
- **Integration Tests**: Test database adapters with real databases using testcontainers
- **Security Tests**: Verify encryption, credential handling, offline operation
- **Performance Tests**: Benchmark database operations and memory usage

### Test Example Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::{clients, images};

    #[tokio::test]
    async fn test_postgres_schema_collection() {
        let docker = clients::Cli::default();
        let postgres = docker.run(images::postgres::Postgres::default());

        let database_url = format!(
            "postgres://postgres:postgres@localhost:{}/postgres",
            postgres.get_host_port_ipv4(5432)
        );

        let repo = PostgresRepository::new(&database_url).await?;
        let tables = repo.get_tables().await?;

        assert!(!tables.is_empty());
    }
}
```

## Security Requirements

### Critical Security Checks

1. **No Network Access**: Except to target databases for schema collection
2. **No Telemetry**: Zero external data collection or reporting
3. **Credential Security**: Database credentials never stored, logged, or output
4. **Encryption**: Proper AES-GCM implementation with Argon2id KDF (min 64MB memory, 3 iterations, 4 parallelism) and deterministic test vectors
5. **Offline Ready**: Full functionality in air-gapped environments
6. **No HTTP(S) Egress**: No HTTP(S) egress in CI/tests except to approved DB targets

### Security Testing

```rust
#[tokio::test]
async fn test_no_credentials_in_output() {
    let sensitive_data = "password123";
    let output = generate_report(&database_url).await?;

    assert!(!output.contains(sensitive_data));
    assert!(!output.contains("password"));
    assert!(!output.contains("secret"));
}

#[tokio::test]
async fn test_encryption_with_random_nonce() {
    let data = b"test data";
    let encrypted = encrypt_data(data)?;
    let encrypted2 = encrypt_data(data)?;

    // Same data should produce different ciphertext due to random nonce
    assert_ne!(encrypted, encrypted2);

    // But both should decrypt to same plaintext
    assert_eq!(decrypt_data(&encrypted)?, data);
    assert_eq!(decrypt_data(&encrypted2)?, data);
}
```

## Commit Standards

All commit messages must follow [Conventional Commits](https://www.conventionalcommits.org):

- **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`
- **Scopes**: `(collector)`, `(processor)`, `(shared)`, `(security)`, `(cli)`, etc.
- **Format**: `<type>(<scope>): <description>`
- **Breaking Changes**: Indicated with `!` in header

Examples:

- `feat(collector): add PostgreSQL schema discovery`
- `fix(security): prevent credential leakage in error messages`
- `docs(readme): update installation instructions`

## Key Reminders

1. **Security First**: Every change must maintain security guarantees
2. **Zero Warnings**: `cargo clippy -- -D warnings` must pass
3. **Offline Only**: No external dependencies at runtime
4. **Database Safety**: Read-only operations with proper connection handling
5. **Operator Focus**: Build for security professionals and database administrators
6. **Documentation**: Comprehensive docs for all public APIs and CLI usage
7. **Testing**: Include unit, integration, and security tests for all changes

## Issue Resolution

When encountering problems:

- Identify the specific issue clearly
- Explain the problem in ≤ 5 lines
- Propose a concrete path forward
- Don't proceed without resolving security blockers
- Always maintain security guarantees in solutions

This document provides guidance for GitHub Copilot when working on the DBSurveyor project, ensuring all contributions align with security requirements and coding standards.
