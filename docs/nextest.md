# Nextest Integration for DBSurveyor

DBSurveyor uses [cargo-nextest](https://nexte.st/) as the default test runner for enhanced test execution with better performance, parallel execution, and improved reporting.

## Why Nextest?

- **Faster execution**: Parallel test execution with intelligent scheduling
- **Better output**: Clear, structured test results with timing information
- **Reliability**: Improved test isolation and retry mechanisms
- **CI-friendly**: Better integration with CI/CD pipelines

## Installation

Nextest is automatically installed when you run:

```bash
just install
```

Or install manually:

```bash
cargo install cargo-nextest
```

## Usage

### Basic Commands

```bash
# Run all tests (default)
just test

# Run tests with development profile (faster feedback)
just test-dev

# Run tests with CI profile (more verbose, suitable for CI)
just test-ci

# Run tests with verbose output
just test-verbose
```

### Direct Nextest Commands

```bash
# Run all tests
cargo nextest run

# Run specific package tests
cargo nextest run --package dbsurveyor-core

# Run tests matching a pattern
cargo nextest run encryption

# Run with specific profile
cargo nextest run --profile ci
```

## Test Profiles

### Default Profile

- **Retries**: 2
- **Threads**: 4
- **Output**: Immediate failures, no success output
- **Use case**: Local development

### Dev Profile

- **Retries**: 1
- **Threads**: 8
- **Output**: Immediate failures, no success output
- **Use case**: Fast local development feedback

### CI Profile

- **Retries**: 3
- **Threads**: 2
- **Output**: Immediate failures, final success output
- **Use case**: CI/CD environments

## Test Groups

Tests are automatically organized into groups based on their characteristics:

### Security Group

- **Max threads**: 1 (sequential execution)
- **Tests**: Encryption, credential protection, security validation
- **Reason**: Security tests need isolation to avoid interference

### Integration Group

- **Max threads**: 2 (limited parallelism)
- **Tests**: Database integration tests with testcontainers
- **Reason**: Database containers need resource management

### Unit Group

- **Max threads**: 8 (high parallelism)
- **Tests**: Fast unit tests without external dependencies
- **Reason**: Can run safely in parallel

## Configuration

Nextest configuration is stored in `.config/nextest.toml` and includes:

- Test execution profiles (default, dev, ci)
- Test group definitions
- Retry policies
- Output formatting
- Timeout settings

## Integration with Just

All test commands in the `justfile` use nextest by default:

```bash
# These commands use nextest internally
just test           # All tests
just test-unit      # Unit tests only
just test-integration  # Integration tests only
just test-postgres  # PostgreSQL-specific tests
just test-encryption   # Encryption tests
```

## Comparison with Cargo Test

| Feature | cargo test | cargo nextest |
|---------|------------|---------------|
| Parallel execution | Limited | Full parallel |
| Test isolation | Basic | Enhanced |
| Output formatting | Basic | Rich, structured |
| Retry mechanism | None | Configurable |
| CI integration | Basic | Optimized |
| Performance | Slower | Faster |

## Troubleshooting

### Tests Not Running

If nextest reports "no tests to run":

- Check that the package has tests
- Verify feature flags are correct
- Use `--no-tests` flag if intentional

### Slow Tests

For tests that take a long time:

- Check if they're in the correct test group
- Consider if they need sequential execution
- Review timeout settings in profiles

### CI Issues

For CI-specific problems:

- Use the `ci` profile: `cargo nextest run --profile ci`
- Check retry settings
- Verify thread limits for resource constraints

## Security Considerations

Nextest maintains DBSurveyor's security guarantees:

- **Offline operation**: No external network calls during test execution
- **Credential protection**: Security tests verify no credential leakage
- **Isolation**: Security tests run sequentially to prevent interference
- **Reproducibility**: Consistent test execution across environments

## Performance Benefits

Typical performance improvements with nextest:

- **Unit tests**: 2-3x faster due to parallelization
- **Integration tests**: Better resource utilization
- **CI pipelines**: Reduced execution time and better reporting
- **Developer feedback**: Faster local test cycles

For more information, see the [official nextest documentation](https://nexte.st/).
