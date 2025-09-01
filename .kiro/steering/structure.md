# DBSurveyor Project Structure

## Workspace Organization

DBSurveyor follows a Rust workspace pattern with multiple crates organized for security, maintainability, and clear separation of concerns.

```text
dbsurveyor/
├── Cargo.toml                    # Workspace configuration with security lints
├── Cargo.lock                    # Dependency lock file
├── justfile                      # Task runner with comprehensive recipes
├── cargo-deny.toml               # Security policy and license compliance
├── cargo-dist.toml               # Cross-platform distribution config
├── mkdocs.yml                    # Documentation site configuration
├── lcov.info                     # Coverage report output
│
├── dbsurveyor/                   # Postprocessor binary crate
│   ├── Cargo.toml               # CLI processor dependencies
│   └── src/
│       └── main.rs              # Report generation and analysis
│
├── dbsurveyor-collect/          # Collector binary crate
│   ├── Cargo.toml               # Database connectivity dependencies
│   └── src/
│       └── main.rs              # Database schema collection
│
├── dbsurveyor-core/             # Shared library crate
│   ├── Cargo.toml               # Core data structures and utilities
│   └── src/
│       └── lib.rs               # Common types and encryption
│
├── project_specs/               # Project documentation
│   ├── requirements.md          # Comprehensive requirements specification
│   ├── tasks.md                 # Implementation tasks and milestones
│   └── user_stories.md          # User stories with acceptance criteria
│
├── .cursor/                     # Cursor AI configuration
│   └── rules/                   # AI assistant coding standards
│       ├── README.md            # Rules overview
│       ├── ai-assistant/        # AI workflow guidelines
│       ├── core/                # Core concepts and commit standards
│       ├── project/             # DBSurveyor-specific architecture
│       ├── quality/             # Code quality standards
│       └── rust/                # Rust development standards
│
├── .github/                     # GitHub Actions CI/CD
│   └── workflows/               # Automated testing and security scanning
│
├── .kiro/                       # Kiro IDE configuration
│   └── steering/                # AI steering rules (this directory)
│
└── target/                      # Build artifacts (gitignored)
```

## Crate Responsibilities

### `dbsurveyor-core` (Shared Library)

**Purpose**: Common data structures, utilities, and security functions shared between binaries.

**Key Components**:

- Database schema data models (tables, columns, indexes, constraints)
- Encryption/decryption utilities (AES-GCM with random nonces)
- Serialization helpers with credential sanitization
- Error types and result handling
- Configuration management structures

**Dependencies**: Minimal - only serde, crypto, and utility crates

### `dbsurveyor-collect` (Collector Binary)

**Purpose**: Database connectivity and metadata extraction.

**Key Components**:

- Database adapters for PostgreSQL, MySQL, SQLite, SQL Server, MongoDB
- Connection management and pooling
- Schema discovery and introspection
- Data profiling and statistics collection
- Secure credential handling
- Output generation (.dbsurveyor.json, compressed, encrypted formats)

**Dependencies**: Database drivers (sqlx, tiberius, mongodb), async runtime (tokio)

### `dbsurveyor` (Postprocessor Binary)

**Purpose**: Documentation generation and analysis from collected metadata.

**Key Components**:

- Input validation and parsing of collector output
- Template-driven report generation (Markdown, HTML)
- SQL reconstruction from schema metadata
- Entity Relationship Diagram (ERD) generation
- Data classification and analysis
- Multi-format output rendering

**Dependencies**: Template engines (askama), markdown processing, analysis libraries

## Configuration Files

### Security & Quality

- `cargo-deny.toml`: Dependency security policy, license compliance, vulnerability scanning
- `.pre-commit-config.yaml`: Pre-commit hooks for code quality
- `.editorconfig`: Consistent editor settings across platforms
- `.markdownlint-cli2.jsonc`: Markdown linting configuration
- `.mdformat.toml`: Markdown formatting standards

### CI/CD & Automation

- `.github/workflows/`: GitHub Actions for CI, security scanning, release automation
- `.actrc`: Local GitHub Actions testing configuration
- `renovate.json`: Automated dependency updates
- `justfile`: Comprehensive task automation with security focus

### Documentation

- `mkdocs.yml`: Documentation site configuration
- `README.md`: Project overview and quick start guide
- `CHANGELOG.md`: Version history and release notes
- `SECURITY.md`: Security policy and vulnerability reporting
- `CONTRIBUTORS.md`: Contribution guidelines and acknowledgments

## File Naming Conventions

### Rust Files

- `main.rs`: Binary entry points
- `lib.rs`: Library root modules
- `mod.rs`: Module declarations
- `error.rs`: Error type definitions
- Snake_case for module files: `database_adapter.rs`, `schema_collector.rs`

### Configuration Files

- Lowercase with hyphens: `cargo-deny.toml`, `.pre-commit-config.yaml`
- Dotfiles for tool configuration: `.editorconfig`, `.gitignore`
- Uppercase for project docs: `README.md`, `CHANGELOG.md`, `LICENSE`

### Output Files

- `.dbsurveyor.json`: Standard JSON metadata output
- `.dbsurveyor.json.zst`: Compressed JSON output
- `.dbsurveyor.enc`: Encrypted JSON output with AES-GCM

## Directory Conventions

### Source Code Organization

```text
src/
├── lib.rs                    # Public API exports and module declarations
├── models/                   # Data structures and schema definitions
│   ├── mod.rs               # Module exports
│   ├── database.rs          # Database metadata structures
│   ├── schema.rs            # Table/column/index structures
│   └── security.rs          # Security-related data types
├── collectors/              # Database-specific collection logic
│   ├── mod.rs               # Collector trait and common functionality
│   ├── postgres.rs          # PostgreSQL schema collection
│   ├── mysql.rs             # MySQL schema collection
│   ├── sqlite.rs            # SQLite schema collection
│   └── mongodb.rs           # MongoDB schema collection
├── encryption/              # Security and encryption utilities
│   ├── mod.rs               # Encryption API
│   ├── aes_gcm.rs          # AES-GCM implementation
│   └── key_derivation.rs   # Key derivation functions
├── output/                  # Documentation generation
│   ├── mod.rs               # Output format trait
│   ├── markdown.rs          # Markdown report generation
│   ├── json.rs              # JSON output formatting
│   └── html.rs              # HTML report generation
└── error.rs                 # Comprehensive error types
```

### Test Organization

```text
tests/
├── integration/             # End-to-end integration tests
│   ├── postgres_tests.rs   # PostgreSQL integration tests
│   ├── mysql_tests.rs      # MySQL integration tests
│   └── common/              # Shared test utilities
├── security/                # Security-focused test suite
│   ├── credential_tests.rs  # Credential protection validation
│   ├── encryption_tests.rs  # Cryptography validation
│   └── offline_tests.rs     # Network isolation verification
└── fixtures/                # Test data and sample schemas
```

## Build Artifacts

### Target Directory Structure

```text
target/
├── debug/                   # Debug build artifacts
├── release/                 # Optimized release builds
├── llvm-cov/               # Coverage reports and data
│   ├── html/               # HTML coverage reports
│   └── lcov.info           # LCOV format for CI integration
└── doc/                    # Generated Rust documentation
```

### Generated Files (Gitignored)

- `target/`: All build artifacts
- `lcov.info`: Coverage report output
- `sbom.json`, `sbom.spdx.json`: Software Bill of Materials
- `grype-report.json`: Vulnerability scan results
- `.secrets`: Local secrets for GitHub Actions testing

## Security Considerations

### File Permissions

- Configuration files: 644 (readable by owner and group)
- Executable binaries: 755 (executable by all, writable by owner)
- Sensitive outputs: 600 (readable/writable by owner only)
- Private keys/secrets: 600 (owner access only)

### Credential Handling

- Never store credentials in source code or configuration files
- Use environment variables or secure credential stores
- Sanitize all file paths and connection strings in logs
- Implement secure cleanup of temporary files

### Output Security

- Generated documentation files exclude all credential information
- Encrypted outputs use AES-GCM with random nonces
- Compressed outputs maintain security properties
- File integrity verification for critical outputs

## Development Workflow

### File Modification Patterns

1. **Core Library Changes**: Modify `dbsurveyor-core/src/` for shared functionality
2. **Collector Changes**: Modify `dbsurveyor-collect/src/` for database connectivity
3. **Postprocessor Changes**: Modify `dbsurveyor/src/` for report generation
4. **Documentation Updates**: Update relevant `.md` files and `mkdocs.yml`
5. **Configuration Changes**: Update `Cargo.toml`, `justfile`, or security configs

### Testing Strategy

- Unit tests co-located with source code in `#[cfg(test)]` modules
- Integration tests in separate `tests/` directory
- Security tests validate encryption, credential handling, offline operation
- Performance tests ensure efficient resource usage

This structure ensures clear separation of concerns, maintainable code organization, and adherence to security-first principles throughout the project.
