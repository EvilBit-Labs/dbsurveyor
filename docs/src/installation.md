# Installation

## From Source (Recommended)

### Prerequisites

- Rust 1.87+ (MSRV)
- Git

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor

# Install development tools
just install

# Build with default features (PostgreSQL + SQLite)
cargo build --release

# Binaries will be available in target/release/
ls target/release/dbsurveyor*
```

### Feature Selection

Control which database engines are compiled in using feature flags:

```bash
# Build with all database support
cargo build --release --all-features

# Build with specific databases only
cargo build --release --features postgresql,mysql,encryption

# Build minimal version for airgap environments
cargo build --release --no-default-features --features sqlite
```

## Feature Flags

| Feature       | Description           | Dependencies              |
| ------------- | --------------------- | ------------------------- |
| `postgresql`  | PostgreSQL support    | sqlx with postgres driver |
| `mysql`       | MySQL support         | sqlx with mysql driver    |
| `sqlite`      | SQLite support        | sqlx with sqlite driver   |
| `mongodb`     | MongoDB support       | mongodb crate             |
| `mssql`       | SQL Server support    | tiberius crate            |
| `compression` | Zstandard compression | zstd crate                |
| `encryption`  | AES-GCM encryption    | aes-gcm, argon2 crates    |

### Default Features

```toml
default = ["postgresql", "sqlite"]
```

The default build includes PostgreSQL and SQLite support, which covers the most common use cases while maintaining a reasonable binary size.

## Binary Variants

DBSurveyor provides two main binaries:

### `dbsurveyor-collect`

The database collection tool that connects to databases and extracts schema information.

**Default Features**: `postgresql`, `sqlite`
**Optional Features**: `mysql`, `mongodb`, `mssql`, `compression`, `encryption`

### `dbsurveyor`

The documentation generator that processes collected schema files.

**Default Features**: None (minimal dependencies)
**Optional Features**: `compression`, `encryption`

## Development Setup

For development work, install the complete toolchain:

```bash
# Install development dependencies
just install

# This installs:
# - Rust toolchain components (clippy, rustfmt)
# - Cargo tools (audit, deny, llvm-cov, nextest)
# - Security tools (syft for SBOM generation)
# - Documentation tools (mdbook and plugins)
```

## Verification

Verify your installation:

```bash
# Check binary versions
./target/release/dbsurveyor-collect --version
./target/release/dbsurveyor --version

# Test with SQLite (no external database required)
echo "CREATE TABLE test (id INTEGER);" | sqlite3 test.db
./target/release/dbsurveyor-collect sqlite://test.db
./target/release/dbsurveyor schema.dbsurveyor.json

# Clean up
rm test.db schema.dbsurveyor.json schema.md
```

## Airgap Installation

For air-gapped environments:

1. **Prepare on connected system**:

   ```bash
   # Download dependencies
   cargo fetch

   # Create vendor directory
   cargo vendor vendor

   # Build minimal version
   cargo build --release --no-default-features --features sqlite
   ```

2. **Transfer to airgap system**:

   - Copy entire project directory including `vendor/`
   - Copy built binaries from `target/release/`

3. **Use offline**:

   ```bash
   # Use vendored dependencies
   cargo build --release --offline --no-default-features --features sqlite
   ```

## Troubleshooting

### Common Issues

**Compilation fails with missing dependencies**:

- Ensure you have the latest Rust toolchain: `rustup update`
- Check feature flags match your requirements

**Database driver compilation errors**:

- Install system dependencies for your target databases
- For PostgreSQL: `libpq-dev` (Ubuntu) or `postgresql-devel` (RHEL)
- For MySQL: `libmysqlclient-dev` (Ubuntu) or `mysql-devel` (RHEL)

**Permission errors**:

- Ensure you have write permissions to the target directory
- Use `cargo install --root ~/.local` for user-local installation

### Getting Help

- Check the [Troubleshooting](./troubleshooting.md) section
- Review [GitHub Issues](https://github.com/EvilBit-Labs/dbsurveyor/issues)
- Consult the [CLI Reference](./cli-reference.md) for command-specific help
