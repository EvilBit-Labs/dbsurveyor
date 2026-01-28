# Contributing

We welcome contributions to DBSurveyor! This guide will help you get started with contributing to the project.

## Code of Conduct

DBSurveyor follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and inclusive in all interactions.

## Getting Started

### Prerequisites

- Rust 1.87+ (MSRV)
- Git
- Docker (for integration tests)
- Just task runner

### Development Setup

```bash
# Clone the repository
git clone https://github.com/EvilBit-Labs/dbsurveyor.git
cd dbsurveyor

# Install development tools
just install

# Run initial checks
just dev
```

### Project Structure

```
dbsurveyor/
├── dbsurveyor-core/     # Shared library
├── dbsurveyor-collect/  # Collection binary
├── dbsurveyor/          # Documentation binary
├── docs/                # Documentation source
├── .cursor/rules/       # AI assistant guidelines
└── justfile            # Development tasks
```

## Development Workflow

### Daily Development

```bash
# Format, lint, test, and check coverage
just dev

# Run specific test categories
just test-unit
just test-integration
just test-security

# Security validation
just security-full

# Pre-commit checks
just pre-commit
```

### Code Quality Standards

DBSurveyor enforces strict quality standards:

- **Zero Warnings**: `cargo clippy -- -D warnings` must pass
- **Test Coverage**: >80% coverage required
- **Security First**: All code must pass security validation
- **Documentation**: All public APIs must have `///` documentation

### Testing Requirements

All contributions must include appropriate tests:

```rust
// Unit tests in source files
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_sanitization() {
        let config = ConnectionConfig::new("postgres://user:secret@host/db");
        let safe_display = config.to_safe_string();
        assert!(!safe_display.contains("secret"));
    }
}

// Integration tests in tests/ directory
#[tokio::test]
async fn test_postgres_collection() {
    let docker = testcontainers::clients::Cli::default();
    let postgres = docker.run(testcontainers::images::postgres::Postgres::default());
    // Test implementation
}

// Security tests are mandatory for security-sensitive code
#[tokio::test]
async fn test_no_credentials_in_output() {
    let schema = collect_schema("postgres://user:secret@localhost/db").await?;
    let json = serde_json::to_string(&schema)?;
    assert!(!json.contains("secret"));
}
```

## Contribution Types

### Bug Reports

When reporting bugs, please include:

1. **System Information**: OS, Rust version, DBSurveyor version
2. **Reproduction Steps**: Minimal example that reproduces the issue
3. **Expected vs Actual Behavior**: Clear description of the problem
4. **Debug Information**: Output with `RUST_LOG=debug`

**Security Note**: Never include actual database credentials in bug reports.

### Feature Requests

For new features, please:

1. **Check Existing Issues**: Avoid duplicates
2. **Describe Use Case**: Why is this feature needed?
3. **Propose Implementation**: High-level approach
4. **Consider Security**: How does this maintain security guarantees?

### Code Contributions

#### Pull Request Process

1. **Fork and Branch**: Create a feature branch from `main`
2. **Implement Changes**: Follow coding standards
3. **Add Tests**: Comprehensive test coverage
4. **Update Documentation**: Keep docs in sync
5. **Run Quality Checks**: `just dev` must pass
6. **Submit PR**: Clear description and context

#### Commit Standards

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```bash
# Feature additions
feat(postgres): add connection pooling with timeout handling
feat(security): implement AES-GCM encryption with random nonces

# Bug fixes
fix(mysql): handle connection failures without exposing credentials
fix(core): ensure proper cleanup of sensitive data structures

# Security improvements
security(core): prevent credential leakage in error messages
security(encryption): add key derivation parameter validation

# Documentation
docs(readme): update installation instructions
docs(security): add encryption implementation details
```

## Database Adapter Development

### Adding New Database Support

To add support for a new database:

1. **Create Adapter Module**:

   ```rust
   // dbsurveyor-core/src/adapters/newdb.rs
   pub struct NewDbAdapter {
       config: ConnectionConfig,
   }

   #[async_trait]
   impl DatabaseAdapter for NewDbAdapter {
       async fn test_connection(&self) -> Result<()> { ... }
       async fn collect_schema(&self) -> Result<DatabaseSchema> { ... }
       fn database_type(&self) -> DatabaseType { ... }
       fn supports_feature(&self, feature: AdapterFeature) -> bool { ... }
       fn connection_config(&self) -> ConnectionConfig { ... }
   }
   ```

2. **Add Feature Flag**:

   ```toml
   # Cargo.toml
   [features]
   newdb = ["dep:newdb-driver"]
   ```

3. **Update Factory**:

   ```rust
   // dbsurveyor-core/src/adapters.rs
   match database_type {
       #[cfg(feature = "newdb")]
       DatabaseType::NewDb => {
           let adapter = NewDbAdapter::new(connection_string).await?;
           Ok(Box::new(adapter))
       }
       // ...
   }
   ```

4. **Add Tests**:

   ```rust
   // tests/integration/newdb_tests.rs
   #[tokio::test]
   async fn test_newdb_collection() {
       // Integration test with testcontainers
   }
   ```

### Database Adapter Requirements

All database adapters must:

- **Implement `DatabaseAdapter` trait** completely
- **Use read-only operations** only (SELECT, DESCRIBE, SHOW)
- **Handle connection timeouts** (default: 30 seconds)
- **Sanitize credentials** in all error messages
- **Support connection pooling** where applicable
- **Include comprehensive tests** with testcontainers
- **Document database-specific features** and limitations

### Testing Database Adapters

```bash
# Test specific database adapter
just test-postgres
just test-mysql
just test-sqlite

# Test with real databases using testcontainers
cargo test --test postgres_integration -- --nocapture

# Security testing for new adapters
cargo test --test security_credential_protection
```

## Security Contributions

### Security-First Development

All contributions must maintain DBSurveyor's security guarantees:

1. **No Credential Exposure**: Never log or output credentials
2. **Offline Operation**: No external network calls except to databases
3. **Encryption Security**: Use AES-GCM with random nonces
4. **Memory Safety**: Use `zeroize` for sensitive data

### Security Review Process

Security-sensitive changes require additional review:

1. **Security Tests**: Must include security-specific tests
2. **Threat Model**: Consider impact on threat model
3. **Documentation**: Update security documentation
4. **Review**: Additional security-focused code review

### Security Testing

```rust
// Example security test
#[tokio::test]
async fn test_new_feature_credential_security() {
    // Test that new feature doesn't leak credentials
    let result = new_feature("postgres://user:secret@localhost/db").await?;
    let output = format!("{:?}", result);
    assert!(!output.contains("secret"));
    assert!(!output.contains("user:secret"));
}
```

## Documentation Contributions

### Documentation Standards

- **User-Focused**: Write for the end user
- **Security-Aware**: Highlight security implications
- **Example-Rich**: Include working code examples
- **Up-to-Date**: Keep in sync with code changes

### Documentation Types

1. **API Documentation**: `///` comments in code
2. **User Guide**: Markdown files in `docs/src/`
3. **README**: Project overview and quick start
4. **Security Documentation**: Security features and guarantees

### Building Documentation

```bash
# Build API documentation
cargo doc --all-features --document-private-items --open

# Build user guide
just docs

# Check documentation
just docs-check
```

## Release Process

### Version Management

DBSurveyor uses semantic versioning:

- **Major**: Breaking changes
- **Minor**: New features (backward compatible)
- **Patch**: Bug fixes

### Release Checklist

1. **Update Version**: Bump version in `Cargo.toml`
2. **Update Changelog**: Document all changes
3. **Run Full Tests**: `just security-full`
4. **Update Documentation**: Ensure docs are current
5. **Create Release**: Tag and create GitHub release
6. **Verify Artifacts**: Test release binaries

## Community Guidelines

### Communication

- **GitHub Issues**: Bug reports and feature requests
- **Pull Requests**: Code contributions and discussions
- **Security Issues**: Email [security@evilbitlabs.io](mailto:security@evilbitlabs.io)

### Review Process

1. **Automated Checks**: CI must pass
2. **Code Review**: Maintainer review required
3. **Security Review**: For security-sensitive changes
4. **Documentation Review**: For user-facing changes

### Recognition

Contributors are recognized in:

- `CONTRIBUTORS.md` file
- Release notes
- Git commit history

## Development Environment

### Recommended Tools

- **IDE**: VS Code with Rust Analyzer
- **Git Hooks**: Pre-commit hooks for quality checks
- **Testing**: Nextest for faster test execution
- **Debugging**: `RUST_LOG=debug` for detailed logging

### Environment Variables

```bash
# Development environment
export RUST_LOG=debug
export DATABASE_URL="postgres://dev:dev@localhost/dev_db"

# Testing environment
export RUST_LOG=trace
export DBSURVEYOR_TEST_TIMEOUT=60
```

### Docker Development

```bash
# Start test databases
docker-compose up -d postgres mysql mongodb

# Run integration tests
just test-integration

# Clean up
docker-compose down
```

## Troubleshooting Development Issues

### Common Issues

**Build failures**:

```bash
# Clean and rebuild
cargo clean
cargo build --all-features

# Update toolchain
rustup update
```

**Test failures**:

```bash
# Run specific test with output
cargo test test_name -- --nocapture

# Run with debug logging
RUST_LOG=debug cargo test test_name
```

**Clippy warnings**:

```bash
# Fix automatically where possible
cargo clippy --fix --allow-dirty

# Check specific warnings
cargo clippy -- -D warnings
```

### Getting Help

- **Documentation**: Check existing docs first
- **Issues**: Search existing GitHub issues
- **Code**: Look at similar implementations
- **Community**: Ask questions in GitHub discussions

## License and Legal

### License

DBSurveyor is licensed under the Apache License 2.0. By contributing, you agree to license your contributions under the same license.

### Copyright

All contributions must include appropriate copyright headers:

```rust
// Copyright 2024 EvilBit Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
```

### Contributor License Agreement

By submitting a pull request, you represent that:

1. You have the right to license your contribution
2. You agree to license it under the Apache License 2.0
3. Your contribution is your original work

Thank you for contributing to DBSurveyor! Your contributions help make database documentation more secure and accessible for everyone.
