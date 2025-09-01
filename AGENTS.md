# DBSurveyor - AI Coding Assistant Rules

This document outlines the coding standards, architectural patterns, and project layout preferences for the DBSurveyor project. It serves as a comprehensive guide for AI coding assistants to ensure consistency, maintainability, and adherence to established best practices.

## 1. Core Philosophy

- **Security-First Principle**: Always prioritize security considerations in design and implementation. Trust the framework's built-in security mechanisms over custom solutions.
- **Operator-Centric Design**: Projects are built for operators, by operators. This means prioritizing workflows that are efficient, auditable, and functional in contested or airgapped environments.
- **Offline-First Architecture**: All functionality must work without internet connectivity. No telemetry, external reporting, or network dependencies in production.
- **Database-Agnostic Design**: Support for multiple database engines (PostgreSQL, MySQL, SQLite, MongoDB) with unified interfaces and consistent behavior across both SQL and NoSQL databases.

## 2. Project Structure and Layout

The project follows a Rust workspace structure with clear separation of concerns:

```text
/
├── bin/
│   ├── collector/             # Database collection binary
│   └── postprocessor/         # Data processing binary
├── crates/
│   └── shared/                # Shared library code
├── .cursor/
│   └── rules/                 # Cursor AI rules
├── .github/
│   ├── workflows/             # GitHub Actions CI/CD
│   └── dependabot.yml         # Dependency management
├── project_specs/             # Project specifications
├── justfile                   # Task runner configuration
├── Cargo.toml                 # Workspace configuration
├── cargo-deny.toml            # Security policy
└── AGENTS.md                  # This file
```

## 3. Technology Stack

The preferred technology stack is consistent across the project:

| Layer             | Technology                                | Notes                                            |
| ----------------- | ----------------------------------------- | ------------------------------------------------ |
| **Language**      | Rust 2021 Edition                         | Modern Rust with idiomatic patterns              |
| **CLI**           | Clap v4 with derive macros                | For clean, user-friendly command-line interfaces |
| **Async**         | Tokio runtime                             | For async database operations                    |
| **Database**      | SQLx with async drivers                   | Type-safe database access                        |
| **Serialization** | Serde with JSON support                   | For data interchange and file I/O                |
| **Encryption**    | AES-GCM with random nonce                 | For secure data at rest                          |
| **Testing**       | Built-in test framework + testcontainers  | For unit and integration testing                 |
| **CI/CD**         | GitHub Actions                            | For automated testing, linting, and releases     |
| **Tooling**       | `cargo` for deps, `just` for task running | `cargo clippy -- -D warnings` for quality        |

## 4. Coding Standards and Conventions

### Rust

- **Formatting**: `cargo fmt` using standard Rust formatting
- **Linting**: `cargo clippy -- -D warnings` to enforce strict zero-warning policy
- **File Organization**: Single-purpose files strictly enforced - one type of code per file, maximum 600 lines preferred, break large files into smaller focused modules
- **Naming**: Follow standard Rust conventions - `snake_case` for variables/functions, `PascalCase` for types
- **Error Handling**: Use `Result<T, E>` types and `?` operator. Create custom error types when needed
- **Documentation**: Comprehensive `///` doc comments for all public APIs
- **Testing**: Unit tests co-located with code, integration tests in separate files
- **Security**: `unsafe` code is denied at the workspace level

### Database Operations Standards

- **Connection Management**: Use connection pooling for performance and resource management
- **Query Safety**: Use parameterized queries only - no string concatenation
- **Transaction Safety**: Proper transaction boundaries with rollback on errors
- **Schema Discovery**: Read-only operations only - no schema modifications
- **Credential Handling**: Never log or output credentials in any form

### Commit Messages

- **Conventional Commits**: All commit messages must adhere to the [Conventional Commits](https://www.conventionalcommits.org) specification
  - **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`
  - **Scopes**: `(collector)`, `(processor)`, `(shared)`, `(security)`, `(cli)`, etc.
  - **Breaking Changes**: Indicated with `!` in the header or `BREAKING CHANGE:` in the footer

## 5. Security Requirements

### Critical Security Guarantees

1. **Offline-Only Operation**: No network calls except to target databases
2. **No Telemetry**: Zero data collection or external reporting mechanisms
3. **Credential Protection**: Database credentials never appear in any output files
4. **Encryption**: AES-GCM with random nonce, embedded KDF params, authenticated headers
5. **Airgap Compatibility**: Full functionality in air-gapped environments

### Security Implementation Standards

- **No hardcoded secrets**: Use environment variables or secure configuration
- **Input validation**: Validate all inputs before processing
- **Secure defaults**: Default to secure configurations
- **Error handling**: Don't expose sensitive information in error messages
- **Dependencies**: Regular security auditing with `cargo audit` and `cargo deny`

## 6. Database Support Standards

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

## 7. EvilBit Labs Standards Integration

### Brand Principles

- **Trust the Operator**: Full control, no black boxes
- **Polish Over Scale**: Quality over feature-bloat
- **Offline First**: Built for where the internet isn't
- **Sane Defaults**: Clean outputs, CLI help that's actually helpful
- **Ethical Constraints**: No dark patterns, spyware, or telemetry

### Quality Assurance

1. **Code Quality**: All code must pass `cargo clippy -- -D warnings` with zero warnings
2. **Type Safety**: Comprehensive type safety through Rust's type system
3. **Testing**: Unit and integration tests with database fixtures
4. **Documentation**: Clear documentation for all user-facing functionality
5. **Performance**: Benchmark-driven development with regression detection

## 8. AI Assistant Behavior and Rules of Engagement

### Core Development Rules

- **Clippy Strict Enforcement**: Always use `cargo clippy -- -D warnings` to enforce zero-warning policy
- **Security-First**: All code changes must maintain security guarantees
- **TERM=dumb Support**: Ensure terminal output respects `TERM="dumb"` environment variable for CI/automation
- **CodeRabbit.ai Integration**: Prefer coderabbit.ai for code review over GitHub Copilot auto-reviews
- **Single Maintainer Workflow**: Configure for single maintainer (UncleSp1d3r) with no second reviewer requirement
- **No Auto-commits**: Never commit code on behalf of maintainer without explicit permission

### Assistant Guidelines

- **Clarity and Precision**: Be direct, professional, and context-aware in all interactions
- **Adherence to Standards**: Strictly follow the defined rules for code style and project structure
- **Tool Usage**: Use `cargo` for Rust development, `just` for task execution
- **Security Awareness**: Always consider security implications of changes
- **Database Safety**: Ensure all database operations are read-only and safe
- **Testing Requirements**: All new functionality must include appropriate tests

### Code Generation Requirements

- Generated code must conform to all established patterns
- Include comprehensive type safety through Rust's type system
- Use proper error handling with context preservation
- Follow architectural patterns (Repository, Service, Factory)
- Include appropriate documentation and testing
- Maintain security guarantees (offline-only, no telemetry, credential protection)

## 9. Development Workflow

### Common Commands

```bash
# Development setup
just dev-setup               # Install tools and dependencies

# Quality assurance
just lint                    # Run clippy with strict warnings
just format                  # Format code
just test                    # Run test suite
just pre-commit              # Run all pre-commit checks

# Security validation
just security-audit          # Run security audit and SBOM generation
just test-encryption         # Verify encryption capabilities
just test-offline            # Test offline operation
just security-full           # Full security validation suite

# Building
just build                   # Build release version
just build-minimal           # Build minimal airgap-compatible version
just package-airgap          # Create airgap deployment package
```

### Testing Strategy

- **Unit Tests**: Test individual functions and modules
- **Integration Tests**: Test database adapters with real databases using testcontainers
- **Security Tests**: Verify encryption, credential handling, offline operation
- **Performance Tests**: Benchmark database operations and memory usage

## 10. Architecture Patterns

- **Repository Pattern**: Database access abstraction layer
- **Service Pattern**: Business logic encapsulation
- **Factory Pattern**: Database driver instantiation
- **Command Pattern**: CLI command organization
- **Error Chaining**: Comprehensive error context through the call stack

## 11. Common Commands and Workflows

### Development Commands

- `just dev-setup` - Install dependencies and tools
- `just lint` - Run strict clippy linting
- `just test` - Run complete test suite
- `just build` - Build optimized release version
- `just security-full` - Run complete security validation

### Quality Assurance Commands

- `cargo clippy -- -D warnings` - Strict linting (zero warnings)
- `cargo fmt --check` - Check code formatting
- `cargo audit` - Security vulnerability scan
- `cargo test --all-features` - Run all tests

### Security Commands

- `just security-audit` - Generate SBOM and vulnerability reports
- `just test-encryption` - Verify AES-GCM encryption
- `just test-offline` - Test airgap compatibility
- `just package-airgap` - Create offline deployment package

## 12. Project-Specific Notes

### DBSurveyor

- **Primary Purpose**: Database schema documentation and analysis
- **Security Focus**: Offline-only operation with encrypted outputs
- **Database Support**: PostgreSQL (primary), MySQL, SQLite, MongoDB (NoSQL)
- **Deployment**: Self-contained binaries with no runtime dependencies
- **Output Formats**: JSON, Markdown, encrypted bundles

### Critical Constraints

- **No Network Access**: Except to target databases for schema collection
- **No Telemetry**: Zero data collection or external reporting
- **Credential Security**: Database credentials never stored or logged
- **Airgap Ready**: Full functionality in disconnected environments
- **Read-Only**: All database operations are strictly read-only

## 13. Key Reminders

1. **Security First**: Every change must maintain security guarantees
2. **Rust Quality Gate**: Zero warnings policy with `cargo clippy -- -D warnings`
3. **Offline Operation**: No external dependencies at runtime
4. **Database Safety**: Read-only operations with proper connection handling
5. **Operator Focus**: Build for security professionals and database administrators
6. **Documentation**: Comprehensive docs for all public APIs and CLI usage

This document serves as the authoritative guide for AI assistants working on the DBSurveyor project, ensuring consistent, secure, and high-quality development practices.
