# DBSurveyor

[![License][license-badge]][license]
[![Sponsors][sponsors-badge]][sponsors]

[![CI][ci-badge]][ci]
[![dependency status][deps-badge]][deps]

[![codecov][codecov-badge]][codecov]
[![Issues][issues-badge]][issues]
[![Last Commit][commits-badge]][commits]
[![OpenSSF Scorecard][scorecard-badge]][scorecard]
[![OpenSSF Best Practices][bestpractices-badge]][bestpractices]

## Overview

DBSurveyor is a secure, offline-first database analysis and documentation toolchain. It connects to database servers, extracts schema metadata and sample data, and generates portable structured output -- all without network calls, telemetry, or storing credentials in output files. Built for operators who need auditable database documentation in airgapped or contested environments.

## Quick Start

### Installation

**Pre-built binaries** are available on the [Releases][releases] page for Linux, macOS, and Windows.

**Homebrew** (macOS/Linux):

```bash
brew install EvilBit-Labs/tap/dbsurveyor
```

**From source:**

```bash
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor
cargo build --release
```

### Basic Usage

```bash
# Collect schema from a PostgreSQL database
dbsurveyor-collect postgres://user:pass@localhost:5432/mydb

# Generate Markdown documentation from collected schema
dbsurveyor generate schema.dbsurveyor.json --format markdown
```

## Features

- Read-only database schema collection (tables, columns, indexes, constraints, foreign keys)
- Data sampling with intelligent ordering strategies
- AES-GCM encryption with Argon2id key derivation for sensitive outputs
- Zstandard compression for large schema files
- Credential sanitization in all logs, errors, and output files
- JSON Schema validation for all outputs (v1.0 format)
- Completely offline operation with zero telemetry
- Airgap-compatible deployment

## Architecture

DBSurveyor uses a dual-binary architecture:

- **`dbsurveyor-collect`** -- Connects to databases and extracts schema information, sample data, and metadata. Supports encryption and compression of output files.
- **`dbsurveyor`** -- Processes collected schema files and generates documentation in Markdown, HTML, or SQL DDL formats. Handles encrypted and compressed inputs.

Both binaries are thin wrappers over the shared `dbsurveyor-core` library.

## Usage Examples

### Schema Collection

```bash
# Collect with encryption (prompts for password)
dbsurveyor-collect --encrypt postgres://localhost/db

# Collect with compression
dbsurveyor-collect --compress --output schema.json.zst postgres://localhost/db

# SQLite collection
dbsurveyor-collect sqlite:///path/to/database.db

# Test connection without collecting
dbsurveyor-collect test postgres://user:pass@localhost/db

# Use DATABASE_URL environment variable
export DATABASE_URL="postgres://user:pass@localhost/db"
dbsurveyor-collect
```

### Documentation Generation

```bash
# Markdown documentation
dbsurveyor generate schema.dbsurveyor.json

# Process encrypted schema (prompts for password)
dbsurveyor generate schema.enc

# Schema analysis with statistics
dbsurveyor analyze schema.json --detailed

# Validate schema file format
dbsurveyor validate schema.dbsurveyor.json
```

## Database Support

| Engine     | Status      | Connection Format                          |
|------------|-------------|--------------------------------------------|
| PostgreSQL | Supported   | `postgres://user:pass@host:5432/db`        |
| SQLite     | Supported   | `sqlite:///path/to/database.db`            |
| MySQL      | In Progress | `mysql://user:pass@host:3306/db`           |
| MongoDB    | In Progress | `mongodb://user:pass@host:27017/db`        |
| SQL Server | Planned     | `mssql://user:pass@host:1433/db`           |

PostgreSQL and SQLite are enabled by default. Other engines are feature-gated and can be enabled at build time (e.g., `cargo build --features mysql`).

## Security

- Offline-only operation -- no network calls except to target databases
- Zero telemetry or external reporting
- AES-GCM-256 encryption with Argon2id KDF for data at rest
- Credentials never appear in output files, logs, or error messages
- Secure memory handling with automatic zeroing (zeroize)
- All database operations are strictly read-only

For details, see [docs/src/security.md](docs/src/security.md) and [SECURITY.md](SECURITY.md).

## Documentation

Full documentation is available at **[evilbitlabs.io/dbsurveyor](https://evilbitlabs.io/dbsurveyor)**.

Quick links:
[Installation](docs/src/installation.md) |
[Quick Start](docs/src/quick-start.md) |
[CLI Reference](docs/src/cli-reference.md) |
[Database Support](docs/src/database-support.md) |
[Security](docs/src/security.md) |
[Troubleshooting](docs/src/troubleshooting.md)

## Development

```bash
just dev-setup    # Install tools and dependencies
just fmt          # Format code
just lint         # Run clippy with strict warnings
just test         # Run test suite
just ci-check     # Full CI validation (fmt, clippy, test, doc, deny)
just pre-commit   # Run all pre-commit checks
```

See [AGENTS.md](AGENTS.md) for full development workflow and coding standards.

## Contributing

Contributions are welcome. Please open an issue to discuss proposed changes before submitting a pull request.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

<!-- Badge images -->
[license-badge]: https://img.shields.io/github/license/EvilBit-Labs/dbsurveyor?style=flat-square
[sponsors-badge]: https://img.shields.io/github/sponsors/EvilBit-Labs?style=flat-square
[ci-badge]: https://img.shields.io/github/actions/workflow/status/EvilBit-Labs/dbsurveyor/ci.yml?style=flat-square&label=CI
[deps-badge]: https://deps.rs/repo/github/EvilBit-Labs/dbsurveyor/status.svg?style=flat-square
[codecov-badge]: https://img.shields.io/codecov/c/github/EvilBit-Labs/dbsurveyor?style=flat-square
[issues-badge]: https://img.shields.io/github/issues/EvilBit-Labs/dbsurveyor?style=flat-square
[commits-badge]: https://img.shields.io/github/last-commit/EvilBit-Labs/dbsurveyor?style=flat-square
[scorecard-badge]: https://img.shields.io/ossf-scorecard/github.com/EvilBit-Labs/dbsurveyor?style=flat-square
[bestpractices-badge]: https://www.bestpractices.dev/projects/9872/badge

<!-- Badge links -->
[license]: https://github.com/EvilBit-Labs/dbsurveyor/blob/main/LICENSE
[sponsors]: https://github.com/sponsors/EvilBit-Labs
[ci]: https://github.com/EvilBit-Labs/dbsurveyor/actions/workflows/ci.yml
[deps]: https://deps.rs/repo/github/EvilBit-Labs/dbsurveyor
[codecov]: https://codecov.io/gh/EvilBit-Labs/dbsurveyor
[issues]: https://github.com/EvilBit-Labs/dbsurveyor/issues
[commits]: https://github.com/EvilBit-Labs/dbsurveyor/commits/main
[scorecard]: https://scorecard.dev/viewer/?uri=github.com/EvilBit-Labs/dbsurveyor
[bestpractices]: https://www.bestpractices.dev/projects/9872
[releases]: https://github.com/EvilBit-Labs/dbsurveyor/releases
