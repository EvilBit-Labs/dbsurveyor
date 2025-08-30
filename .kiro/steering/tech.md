# DBSurveyor Technology Stack

## Language & Runtime

- **Language**: Rust 2021 Edition (MSRV: 1.77+)
- **Runtime**: Native compiled binaries with no external runtime dependencies
- **Architecture**: Cross-platform support (x86_64, aarch64) for Linux, macOS, Windows

## Core Dependencies

### CLI & Async Framework

- **clap v4+**: Command-line argument parsing with derive macros
- **tokio**: Async runtime for database operations and I/O
- **tracing**: Structured logging (no external reporting)

### Database Connectivity

- **sqlx**: PostgreSQL, MySQL, SQLite support with async drivers and compile-time query verification
- **tiberius**: SQL Server native driver
- **mongodb**: MongoDB official Rust driver
- **Feature flags**: Modular database driver selection to minimize binary size

### Security & Encryption

- **aes-gcm**: AES-GCM authenticated encryption with random nonces
- **ring**: Cryptographic primitives and key derivation
- **zstd**: Optional compression for output files
- **rpassword**: Secure credential input handling

### Serialization & Output

- **serde**: JSON/YAML serialization with security-conscious custom serializers
- **askama**: Template engine for HTML/Markdown report generation
- **markdown**: Markdown processing for documentation output

## Build System & Tools

### Task Runner

- **just**: Primary task runner for development workflows
- **justfile**: Comprehensive recipes for build, test, lint, security validation

### Quality Assurance

- **cargo clippy**: Strict linting with `-- -D warnings` (zero warnings policy)
- **cargo fmt**: Standard Rust formatting (4-space indentation)
- **cargo-llvm-cov**: Code coverage reporting with 75% threshold
- **cargo-audit**: Security vulnerability scanning
- **cargo-deny**: License compliance and dependency auditing

### Testing Framework

- **Built-in Rust testing**: Unit and integration tests
- **testcontainers**: Real database testing with Docker containers
- **criterion**: Performance benchmarking (optional)

### Security Tools

- **Syft**: Software Bill of Materials (SBOM) generation
- **Grype**: Vulnerability scanning of dependencies
- **CodeQL**: Static analysis for security vulnerabilities
- **FOSSA**: License compliance verification (pending GitHub App setup)

## Common Development Commands

### Setup & Installation

```bash
just setup          # Setup development environment
just install         # Install dependencies and security tools
just install-tools   # Install Rust development tools
```

### Development Workflow

```bash
just dev            # Complete development cycle (format, lint, test, coverage)
just format         # Format code with rustfmt
just lint           # Run clippy with strict warnings (-- -D warnings)
just test           # Run all tests with security verification
just check          # Run all linting and formatting checks
```

### Building & Packaging

```bash
just build          # Build debug version
just build-release  # Build optimized release with security flags
just build-minimal  # Build minimal airgap-compatible version
just package-airgap # Create offline deployment package
```

### Security Validation

```bash
just security-full       # Complete security validation suite
just test-encryption     # Test AES-GCM encryption capabilities
just test-offline        # Verify offline-only operation
just test-credential-security  # Verify no credential leakage
just audit              # Run dependency security audit
```

### CI/CD Integration

```bash
just ci-check       # CI-equivalent validation locally
just coverage       # Generate coverage report with 75% threshold
just coverage-ci    # CI-friendly coverage generation
just sbom           # Generate Software Bill of Materials
```

### Documentation

```bash
just doc            # Build Rust documentation
just doc-open       # Build and open documentation
just docs           # Serve MkDocs documentation locally
just docs-build     # Build documentation site
```

### Local GitHub Actions Testing

```bash
just setup-act           # Install and configure act
just test-ci-local       # Test CI workflow locally
just test-lint-local     # Test lint job locally
just test-security-local # Test security scan job locally
just validate-workflows  # Validate GitHub Actions syntax
```

## Feature Flags

```toml
[features]
default = ["postgresql", "sqlite", "mongodb"]
postgresql = ["dep:sqlx", "sqlx/postgres"]
sqlite = ["dep:sqlx", "sqlx/sqlite"]
# mysql = ["dep:sqlx", "sqlx/mysql"]  # Disabled due to RSA vulnerability
mssql = ["tiberius"]
mongodb = ["dep:mongodb"]
compression = ["zstd"]
encryption = ["aes-gcm", "ring"]
```

## Security Configuration

### Workspace Lints

```toml
[workspace.lints.rust]
unsafe_code = "deny"           # No unsafe code allowed
missing_docs = "warn"          # Documentation required
unreachable_pub = "warn"       # No dead public APIs

[workspace.lints.clippy]
all = "deny"                   # Strict linting
arithmetic_side_effects = "deny"  # Prevent integer overflow
panic = "deny"                 # No panic in production code
expect_used = "deny"           # Use proper error handling
unwrap_used = "deny"           # No unwrap in production code
```

### Release Profile (Security Optimized)

```toml
[profile.release]
lto = true              # Link-time optimization
codegen-units = 1       # Better optimization
panic = "abort"         # Smaller binary, immediate failure
strip = "symbols"       # Remove debug symbols
```

## Architecture Patterns

- **Repository Pattern**: Database access abstraction layer
- **Service Pattern**: Business logic encapsulation
- **Factory Pattern**: Database driver instantiation
- **Command Pattern**: CLI command organization using Clap
- **Error Chaining**: Comprehensive error context through call stack

## Performance Considerations

- **Connection Pooling**: Efficient database connection management
- **Streaming Processing**: Memory-efficient handling of large datasets
- **Batch Operations**: Optimized database queries for large schemas
- **Memory Limits**: Configurable limits to prevent excessive resource usage
