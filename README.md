# DBSurveyor

## Secure, Offline-First Database Analysis and Documentation Toolchain

**Author**: UncleSp1d3r

Toolchain for surveying database servers, extracting schema and sample data, and generating portable structured output.

## Security & Compliance Guarantees

### Offline-Only Operation

- **NO NETWORK CALLS**: Operates completely offline after initial installation
- **NO TELEMETRY**: Zero data collection, usage tracking, or external reporting
- **NO AUTO-UPDATES**: Manual control over all software updates
- **AIRGAP COMPATIBLE**: Full functionality in air-gapped environments

### Data Security

- **NO CREDENTIALS IN OUTPUTS**: Database credentials never stored in output files
- **AES-GCM ENCRYPTION**: Industry-standard authenticated encryption for sensitive outputs
  - Random 96-bit nonce generation for each encryption operation (never reused)
  - Argon2id key derivation with secure parameters (64 MiB memory, 3 iterations, 4 threads)
  - Embedded KDF parameters and salt in encrypted files for decryption validation
  - 256-bit AES-GCM authenticated encryption with separate authentication tags
  - Comprehensive validation of nonce length, tag length, and algorithm parameters
- **CREDENTIAL SANITIZATION**: All connection strings sanitized in logs and error messages
- **SECURE MEMORY HANDLING**: Automatic zeroing of sensitive data using zeroize crate

### CI Security Controls (Per Pipeline Standard)

- **CodeQL**: Static analysis for security vulnerabilities
- **Syft**: Software Bill of Materials (SBOM) generation
- **Grype**: Vulnerability scanning of dependencies
- **FOSSA**: License compliance verification
- **Rust Quality Gate**: `cargo clippy -- -D warnings` enforced
- **Signed Releases**: All binaries cryptographically signed
- **Supply Chain Security**: SLSA attestation and provenance

### Development and Review Process

- **Code Review**: Primary tool is [CodeRabbit.ai](https://coderabbit.ai) for intelligent, conversational code analysis
- **GitHub Copilot**: Automatic reviews are disabled; CodeRabbit.ai provides superior review capabilities
- **Single Maintainer**: Streamlined development process with direct maintainer access
- **OpenAPI Generator**: Future HTTP client development will use OpenAPI Generator for Rust code generation

## Architecture

DBSurveyor uses a dual-binary architecture for security and flexibility:

### `dbsurveyor-collect` - Database Collection Tool

Connects to databases and extracts comprehensive schema information:

```bash
# Basic schema collection
dbsurveyor-collect postgres://user:pass@localhost/db

# With encryption and compression
dbsurveyor-collect --encrypt --compress --output schema.enc postgres://localhost/db

# Multi-database collection
dbsurveyor-collect --all-databases --exclude-databases system,temp postgres://localhost

# Test connection only
dbsurveyor-collect test postgres://user:pass@localhost/db
```

**Features:**

- PostgreSQL, MySQL, SQLite, MongoDB, SQL Server support (feature-gated)
- Read-only operations with configurable timeouts
- Optional AES-GCM encryption with password-based key derivation
- Zstandard compression for large schemas
- Multi-database server enumeration
- Credential sanitization in all outputs

### `dbsurveyor` - Documentation Generator

Processes collected schema files and generates documentation:

```bash
# Generate Markdown documentation
dbsurveyor generate schema.dbsurveyor.json --format markdown

# Generate HTML report with search
dbsurveyor --format html --output report.html schema.json

# Analyze schema statistics
dbsurveyor analyze schema.json --detailed

# Generate SQL DDL reconstruction
dbsurveyor sql schema.json --dialect postgresql --output schema.sql

# Validate schema file format
dbsurveyor validate schema.enc
```

**Features:**

- Markdown, HTML, JSON, and Mermaid ERD output formats
- Encrypted input file support with password prompts
- Compressed input file support (.json.zst)
- SQL DDL reconstruction for multiple dialects
- Schema validation and analysis tools
- Completely offline operation (no network dependencies)

### Known Security Advisories

The following security advisories are acknowledged and accepted:

- **RUSTSEC-2023-0071** (RSA crate - Marvin Attack): Medium severity timing side-channel vulnerability in RSA implementation
  - **Impact**: Only affects MySQL connections (not enabled by default) and in very specific conditions
  - **Mitigation**: MySQL support is disabled by default; PostgreSQL and SQLite are recommended
  - **Status**: No fix available upstream; tracked for future SQLx updates
  - **Review Date**: 2025-09-01

## Exceptions

### FOSSA License Scanning Integration

- **Rule:** FOSSA GitHub App integration with PR enforcement requirement
- **Status:** Pending - requires GitHub App installation and configuration
- **Rationale:** FOSSA integration requires organization-level GitHub App setup
- **Duration:** Until FOSSA GitHub App is configured for the repository
- **Compensating Controls:**
  - Manual license review via cargo-deny.toml configuration
  - Pre-commit license validation hooks
  - Regular dependency auditing with cargo-audit
  - License information included in generated SBOMs
- **Tracking:** Will be resolved once FOSSA GitHub App is installed

### Migration Status

- **Renovate:** ✅ Configured (replaced Dependabot)
- **Release Please:** ✅ Configured
- **SLSA Provenance:** ✅ Configured
- **Cosign Signing:** ✅ Configured
- **CodeQL Analysis:** ✅ Configured
- **OSSF Scorecard:** ✅ Configured
- **MkDocs Documentation:** ✅ Configured
- **Local GitHub Actions Testing:** ✅ Configured with `act`

## Local Development and Testing

### GitHub Actions Testing with `act`

This project supports local testing of GitHub Actions workflows using [`act`](https://github.com/nektos/act):

```bash
# Setup act for local testing
just setup-act

# Test the entire CI workflow locally
just test-ci-local

# Test specific jobs
just test-lint-local
just test-security-local
just test-build-local

# Test release workflows (dry run)
just test-release-local
just test-release-please-local

# List all available workflows
just list-workflows

# Validate workflow syntax
just validate-workflows
```

## Installation

### From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor

# Install development tools
just install

# Build with default features (PostgreSQL + SQLite)
cargo build --release

# Build with all database support
cargo build --release --all-features

# Build minimal version for airgap environments
cargo build --release --no-default-features --features sqlite
```

### Feature Flags

Control which database engines are compiled in:

```toml
[features]
default = ["postgresql", "sqlite"]
postgresql = ["sqlx/postgres"]  # PostgreSQL support
mysql = ["sqlx/mysql"]          # MySQL support  
sqlite = ["sqlx/sqlite"]        # SQLite support
mongodb = ["mongodb"]           # MongoDB support
mssql = ["tiberius"]           # SQL Server support
compression = ["zstd"]          # Zstandard compression
encryption = ["aes-gcm", "argon2"]  # AES-GCM encryption
```

### Enhanced Testing with Nextest

DBSurveyor uses [cargo-nextest](https://nexte.st/) for faster, more reliable test execution:

```bash
# Run all tests with nextest (default)
just test

# Fast development testing
just test-dev

# CI-optimized testing with verbose output
just test-ci

# Run specific test types
just test-unit           # Unit tests only
just test-integration    # Integration tests only
just test-encryption     # Security/encryption tests
```

**Benefits of Nextest:**

- ⚡ **Faster execution** through intelligent parallel testing
- 🔍 **Better output** with structured results and timing
- 🔄 **Retry mechanisms** for flaky test handling
- 🏗️ **CI-optimized** profiles for different environments

## Usage Examples

### Basic Schema Collection

```bash
# Collect PostgreSQL schema
dbsurveyor-collect postgres://user:password@localhost:5432/mydb

# Collect SQLite schema
dbsurveyor-collect sqlite:///path/to/database.db

# Test connection without collecting
dbsurveyor-collect test postgres://user:pass@localhost/db
```

### Advanced Collection Options

```bash
# Encrypted output with password prompt
dbsurveyor-collect --encrypt postgres://localhost/db

# Compressed output for large schemas
dbsurveyor-collect --compress --output schema.json.zst postgres://localhost/db

# Multi-database collection from server
dbsurveyor-collect --all-databases --exclude-databases postgres,template0 postgres://localhost

# Throttled collection for stealth operations
dbsurveyor-collect --throttle 1000 postgres://localhost/db
```

### Documentation Generation

```bash
# Generate Markdown documentation
dbsurveyor generate schema.dbsurveyor.json

# Generate HTML report with search functionality
dbsurveyor --format html --output report.html schema.json

# Process encrypted schema file
dbsurveyor generate schema.enc  # Will prompt for password

# Generate SQL DDL for different databases
dbsurveyor sql schema.json --dialect mysql --output recreate.sql
```

### Schema Analysis

```bash
# Basic schema analysis
dbsurveyor analyze schema.json

# Detailed analysis with statistics
dbsurveyor analyze schema.json --detailed

# Validate schema file format
dbsurveyor validate schema.dbsurveyor.json
```

## Environment Variables

```bash
# Database connection (alternative to command line)
export DATABASE_URL="postgres://user:pass@localhost/db"
dbsurveyor-collect

# Logging configuration
export RUST_LOG=debug  # Enable debug logging
export RUST_LOG=dbsurveyor_collect=trace  # Trace specific module
```

## Documentation

📚 **[Complete Documentation](https://evilbitlabs.io/dbsurveyor)** - Comprehensive user guide and reference

### Quick Links

- **[Installation Guide](docs/src/installation.md)** - Get DBSurveyor up and running
- **[Quick Start](docs/src/quick-start.md)** - Your first schema collection in minutes
- **[CLI Reference](docs/src/cli-reference.md)** - Complete command-line reference
- **[Security Features](docs/src/security.md)** - Security guarantees and best practices
- **[Database Support](docs/src/database-support.md)** - Supported databases and features
- **[Troubleshooting](docs/src/troubleshooting.md)** - Common issues and solutions

### Key Development Commands

```bash
# Development workflow (format, lint, test, coverage)
just dev

# Security-focused development cycle
just security-full

# Local/CI parity - run the same checks as CI
just ci-check

# Pre-commit validation
just pre-commit

# Build and serve documentation locally
just docs
```
