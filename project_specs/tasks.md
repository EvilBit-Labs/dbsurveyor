# DBSurveyor Development Tasks

## Overview

DBSurveyor is a database schema discovery and documentation tool organized into phased releases with clear milestones. This document provides the complete work breakdown structure with acceptance criteria, dependencies, and testing requirements.

**Related Documents**: See [requirements.md](requirements.md) for detailed functional requirements (F000-F016) and [user_stories.md](user_stories.md) for comprehensive user stories.

**Author**: UncleSp1d3r
**Single-Maintainer Model**: Streamlined development process with direct maintainer access

### Milestone Naming Convention

Milestones follow version-based naming (v0.1, v0.2, v0.3, v1.0) with contextual descriptions explaining the milestone's purpose and scope.

## SECURITY & COMPLIANCE GUARANTEES

### Offline-Only Operation

- **NO NETWORK CALLS**: Zero external communication after installation
- **NO TELEMETRY**: Absolutely no data collection or external reporting
- **AIRGAP COMPATIBLE**: Full functionality in air-gapped environments

### Data Security

- **NO CREDENTIALS IN OUTPUTS**: Database credentials never stored in output files
- **AES-GCM ENCRYPTION**: Random nonce generation, embedded KDF parameters, authenticated headers
- **SENSITIVE DATA WARNINGS**: Explicit warnings about sensitive data in samples

### CI Security Controls (Pipeline Standard)

- **CodeQL**: Static analysis for security vulnerabilities
- **Syft**: Software Bill of Materials (SBOM) generation
- **Grype**: Vulnerability scanning of dependencies
- **FOSSA**: License compliance verification

WARNING: Sample data may contain sensitive information. Review outputs before sharing.

---

## v0.1 — Collector MVP

**Goal**: Basic schema collection functionality with multi-engine support and structured output.

### Task 1.1: Database Engine Adapters

**Requirements Mapping**: F001, F002, F003, F010
**Related User Stories**: [US-DBA-003](user_stories.md#us-dba-003-multiple-database-support), [US-SA-002](user_stories.md#us-sa-002-cross-platform-support)

**Acceptance Criteria**:

- Support for PostgreSQL, MySQL/MariaDB, SQLite, and MongoDB (F001)
- Unified adapter interface with connection handling (F001)
- Proper error handling for connection failures and timeouts (F010)
- Connection pooling and resource cleanup (F010)
- Configuration via connection strings or structured config (F003)
- Feature flag-based database driver selection (F002)

**Owner**: Single-maintainer

**Dependencies**:

- Feature flags: `postgres`, `mysql`, `sqlite`, `mongodb` (default: all enabled)
- External crates: `tokio-postgres`, `sqlx`, `rusqlite`, `mongodb`

**Testing Notes**:

- Unit tests for each adapter interface
- Integration tests with Testcontainers for PostgreSQL, MySQL, MongoDB
- SQLite tests with in-memory databases
- Connection failure scenarios and timeout handling
- Performance budget: Connection establishment < 5s, query timeout < 30s

### Task 1.2: Schema Enumeration and Sampling Logic

**Requirements Mapping**: F006, F007, F008, F011, F013
**Related User Stories**: [US-DBA-001](user_stories.md#us-dba-001-basic-database-survey), [US-DBA-002](user_stories.md#us-dba-002-progress-monitoring-for-large-databases)

**Acceptance Criteria**:

- Enumerate databases, schemas, tables, views, procedures, functions (F006, F008)
- Column metadata: name, type, constraints, defaults, nullable (F007)
- Index information and foreign key relationships (F007)
- Best-effort ordering: system objects last, user objects first (F006)
- Configurable sampling limits (rows per table, max tables) (F013)
- Handle large schemas gracefully with progress reporting (F011)

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 1.1 (Database Engine Adapters)
- Feature flags: `sampling` (default: enabled)

**Testing Notes**:

- Unit tests for enumeration logic per engine
- Integration tests with complex schemas (100+ tables)
- Sampling accuracy tests with known datasets
- Performance budget: 1000 tables < 2 minutes, progress updates every 10s

### Task 1.3: Output Writer with Multiple Formats

**Requirements Mapping**: F004, F014, F015
**Related User Stories**: [US-ALL-001](user_stories.md#us-all-001-secure-credential-management), [US-ALL-002](user_stories.md#us-all-002-data-privacy-protection)

**Acceptance Criteria**:

- JSON output with structured schema representation (F014, F015)
- Compressed output (.dbsurveyor.json.zst) for large schemas (F015)
- Encrypted output (.dbsurveyor.enc) with AES-256-GCM (F004, F015)
- Include `format_version` field for backward compatibility (F014)
- Atomic writes with temporary files and rename (F004)
- Human-readable JSON formatting option (F015)

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 1.2 (Schema Enumeration)
- Feature flags: `compression`, `encryption` (default: enabled)
- External crates: `zstd`, `ring` or `aes-gcm`

**Testing Notes**:

- Unit tests for each output format
- Round-trip tests (write → read → validate)
- Compression ratio validation
- Encryption/decryption with test keys
- File corruption handling
- Performance budget: 100MB schema < 30s write/read

### Task 1.4: CLI Interface and Configuration

**Acceptance Criteria**:

- CLI with subcommands: `collect`, `version`, `help`
- Connection string parsing and validation
- Throttling options: `--max-connections`, `--query-delay`
- Output format selection: `--format json|json.zst|enc`
- Verbose/quiet logging levels
- Configuration file support (TOML)
- Progress bars and status reporting

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: All previous tasks
- External crates: `clap`, `tracing`, `indicatif`

**Testing Notes**:

- CLI integration tests with various flag combinations
- Configuration file parsing tests
- Help text and error message validation
- Progress reporting accuracy
- Performance budget: CLI overhead < 1s

### Task 1.5: Logging and Observability

**Acceptance Criteria**:

- Structured logging via `tracing` crate
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Connection events, query execution, errors
- JSON log format option for structured analysis
- Performance metrics: query count, duration, throughput
- Optional OpenTelemetry integration

**Owner**: Single-maintainer

**Dependencies**:

- Feature flags: `telemetry` (default: disabled)
- External crates: `tracing`, `tracing-subscriber`, optional `opentelemetry`

**Testing Notes**:

- Log output validation in tests
- Performance impact measurement
- Telemetry integration tests (if enabled)
- Log rotation and file size limits

### Task 1.6: Basic Test Suite

**Acceptance Criteria**:

- Unit tests for all core modules (>80% coverage)
- Integration tests with Testcontainers
- Docker Compose setup for test databases
- CI-friendly test execution (parallel, isolated)
- Test data fixtures for complex scenarios
- Performance regression tests

**Owner**: Single-maintainer

**Dependencies**:

- External crates: `testcontainers`, `tokio-test`

**Testing Notes**:

- Test execution time < 10 minutes
- All tests pass in CI environment
- Flaky test detection and remediation
- Test data cleanup and isolation

---

## v0.2 — Postprocessor MVP

**Goal**: Transform collected schema data into human-readable documentation and analysis.

### Task 2.1: JSON Schema Loader and Validator

**Acceptance Criteria**:

- Load and parse `.dbsurveyor.json` files
- Validate format version compatibility
- Handle compressed and encrypted inputs
- Migration support for older format versions
- Error reporting for corrupted or invalid files
- Memory-efficient loading for large schemas

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: v0.1 output formats
- Feature flags: `validation` (default: enabled)

**Testing Notes**:

- Validation tests with various format versions
- Corruption handling and recovery
- Memory usage tests with large files (>1GB)
- Performance budget: 500MB file < 30s load time

### Task 2.2: Markdown Report Generation

**Acceptance Criteria**:

- Generate comprehensive markdown documentation
- Database overview with statistics
- Table-by-table documentation with relationships
- Index and constraint documentation
- Data type summaries and patterns
- Customizable report templates
- Table of contents with navigation links

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 2.1 (JSON Schema Loader)
- External crates: `askama` or `tera`

**Testing Notes**:

- Markdown syntax validation
- Template rendering tests
- Link integrity checks
- Performance budget: 1000 tables < 5 minutes

### Task 2.3: SQL Reconstruction Engine

**Acceptance Criteria**:

- Generate CREATE statements for tables, indexes, views
- Preserve original column ordering and constraints
- Handle engine-specific syntax differences
- Foreign key relationship reconstruction
- Stored procedure and function definitions
- Data type mapping accuracy
- Optional schema migration scripts

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 2.1 (JSON Schema Loader)
- Feature flags: `sql-generation` (default: enabled)

**Testing Notes**:

- SQL syntax validation per database engine
- Round-trip tests (schema → JSON → SQL → schema)
- Complex constraint handling
- Performance budget: 1000 tables < 2 minutes

### Task 2.4: Redaction and Privacy Controls

**Acceptance Criteria**:

- Configurable field redaction rules
- Pattern-based sensitive data detection
- Redaction of table/column names and sample data
- Anonymization options (hashing, masking)
- Compliance templates (GDPR, PCI, HIPAA)
- Audit trail for redaction actions
- Reversible redaction with encryption keys

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 2.1 (JSON Schema Loader)
- Feature flags: `redaction` (default: enabled)

**Testing Notes**:

- Redaction accuracy tests
- Pattern matching validation
- Compliance template verification
- Audit trail integrity

### Task 2.5: CLI Subcommands and Help System

**Acceptance Criteria**:

- Subcommands: `process`, `redact`, `export`, `validate`
- Comprehensive help documentation
- Command-specific options and flags
- Interactive prompts for sensitive operations
- Shell completion support (bash, zsh, fish)
- Configuration inheritance and overrides

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: All v0.2 tasks
- External crates: `clap_complete`

**Testing Notes**:

- CLI integration tests for all subcommands
- Help text accuracy and completeness
- Shell completion validation
- Interactive prompt testing

---

## v0.3 — Pro Features

**Goal**: Advanced analysis, visualization, and extensibility features.

### Task 3.1: Schema Diagramming

**Acceptance Criteria**:

- Generate Mermaid ERD diagrams
- Optional D2 diagram support
- Relationship visualization with cardinality
- Configurable diagram layouts and themes
- SVG/PNG export capability
- Interactive diagram navigation
- Large schema handling with clustering

**Owner**: Single-maintainer

**Dependencies**:

- Feature flags: `mermaid` (default: enabled), `d2` (optional)
- External tools: `mermaid-cli`, optional `d2`

**Testing Notes**:

- Diagram syntax validation
- Visual regression tests for layout
- Performance with large schemas (1000+ tables)
- Export format quality tests

### Task 3.2: Field Classification and Heuristics

**Acceptance Criteria**:

- PII detection: names, emails, phone numbers, addresses
- PCI compliance: credit card patterns, payment data
- Sensitive data classification with confidence scores
- Custom classification rules and patterns
- False positive/negative reporting
- Machine learning-based classification (optional)
- Compliance reporting templates

**Owner**: Single-maintainer

**Dependencies**:

- Feature flags: `classification`, `ml-classification` (optional)
- Optional external crates: `candle` or `tch` for ML

**Testing Notes**:

- Classification accuracy tests with known datasets
- False positive rate < 5%
- Performance with large schemas
- Compliance template validation

### Task 3.3: HTML Export with Search and Filtering

**Acceptance Criteria**:

- Self-contained HTML reports (no external assets)
- Client-side search and filtering
- Responsive design for mobile devices
- Interactive table navigation
- Bookmark-able URLs for specific sections
- Print-friendly CSS styles
- Accessibility compliance (WCAG 2.1)
- Dark/light theme toggle

**Owner**: Single-maintainer

**Dependencies**:

- Depends on: Task 2.2 (Markdown Report Generation)
- External crates: `comrak` for markdown-to-HTML conversion

**Testing Notes**:

- Cross-browser compatibility testing
- Search functionality accuracy
- Accessibility audit with automated tools
- Mobile responsiveness testing
- Performance with large reports

### Task 3.4: Plugin System Architecture

**Acceptance Criteria**:

- WASM plugin support for custom processors
- STDIO plugin interface for external tools
- Stable JSON contract for plugin communication
- Plugin discovery and loading mechanism
- Sandboxed execution environment
- Plugin configuration and dependencies
- Error isolation and recovery
- Plugin marketplace preparation

**Owner**: Single-maintainer

**Dependencies**:

- Feature flags: `plugins`, `wasm-plugins` (default: disabled)
- External crates: `wasmtime`, `serde_json`

**Testing Notes**:

- Plugin isolation and security tests
- Performance impact measurement
- Contract stability validation
- Error handling and recovery tests

---

## v1.0 — Packaging, Polish, Cross-platform Release

**Goal**: Production-ready release with comprehensive documentation and distribution.

### Task 4.1: Cross-platform Build System

**Acceptance Criteria**:

- cargo-dist configuration for automated releases
- Cross-compilation for: Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), Windows (x86_64)
- Static linking where possible
- Binary size optimization
- Reproducible builds with checksums
- Release automation via GitHub Actions
- Homebrew formula generation

**Owner**: Single-maintainer

**Dependencies**:

- External tools: `cargo-dist`, GitHub Actions

**Testing Notes**:

- Cross-platform binary validation
- Smoke tests on all target platforms
- Binary size regression monitoring
- Installation testing via package managers

### Task 4.2: Database Driver Matrix and Feature Gating

**Acceptance Criteria**:

- Optional feature compilation for database drivers
- Minimal binary with core functionality only
- Driver-specific feature flags and documentation
- Compatibility matrix documentation
- Runtime driver detection and error messages
- Performance optimization per driver
- Memory usage optimization

**Owner**: Single-maintainer

**Dependencies**:

- Cargo feature system optimization

**Testing Notes**:

- Feature compilation validation
- Runtime driver detection tests
- Performance benchmarking per driver
- Memory leak detection

### Task 4.3: Comprehensive Documentation

**Acceptance Criteria**:

- MkDocs Material documentation site
- Installation guides per platform
- Usage tutorials with examples
- API reference documentation
- Configuration reference
- Troubleshooting guide
- Offline documentation bundle
- Security best practices guide

**Owner**: Single-maintainer

**Dependencies**:

- External tools: MkDocs Material, `cargo doc`

**Testing Notes**:

- Documentation accuracy validation
- Link integrity checking
- Example code execution tests
- Offline bundle completeness

### Task 4.4: Security Hardening and Audit

**Acceptance Criteria**:

- Security audit of all dependencies
- Input validation hardening
- Cryptographic implementation review
- Secure default configurations
- Rate limiting and DoS protection
- Security vulnerability reporting process
- SBOM (Software Bill of Materials) generation
- Supply chain security measures

**Owner**: Single-maintainer

**Dependencies**:

- Security audit tools: `cargo audit`, `cargo deny`

**Testing Notes**:

- Penetration testing scenarios
- Fuzzing tests for input handling
- Cryptographic correctness validation
- Dependency vulnerability scanning

### Task 4.5: Performance Tuning and Optimization

**Acceptance Criteria**:

- Large schema performance optimization (10,000+ tables)
- Memory usage profiling and optimization
- Query batching and parallel processing
- Connection pooling optimization
- Caching strategies for repeated operations
- Benchmark suite with regression detection
- Performance documentation and tuning guides

**Owner**: Single-maintainer

**Dependencies**:

- Profiling tools: `cargo flamegraph`, `valgrind`

**Testing Notes**:

- Performance regression tests
- Memory leak detection
- Stress testing with large datasets
- Performance budget validation

---

## Ongoing Platform/Quality Tracks

### Track A: Continuous Integration and Quality

**Tasks**:

- **A1**: Implement `cargo clippy -- -D warnings` enforcement (Rust Quality Gate)
- **A2**: Code formatting with `cargo fmt --check`
- **A3**: Test execution with `cargo nextest`
- **A4**: Security scanning with CodeQL, Syft, Grype
- **A5**: License compliance with FOSSA
- **A6**: Code coverage reporting with Codecov
- **A7**: CodeRabbit.ai integration for automated code reviews (preferred over GitHub Copilot)
- **A8**: Single-maintainer workflow optimization

**Acceptance Criteria**:

- Zero warnings in CI pipeline (cargo clippy -- -D warnings enforced)
- 90%+ test coverage maintenance
- All security scans pass
- License compliance verification
- Automated quality gates
- CodeRabbit.ai provides conversational code review
- GitHub Copilot automatic reviews disabled
- Single-maintainer workflow documented

**Owner**: Single-maintainer

**Testing Notes**:

- CI pipeline execution time < 15 minutes
- Quality gate enforcement
- Historical trend monitoring

### Track B: Release Management and Security

**Tasks**:

- **B1**: Release Please configuration for automated releases
- **B2**: Cosign binary signing implementation
- **B3**: SLSA provenance generation
- **B4**: Supply chain security hardening
- **B5**: Security vulnerability disclosure process

**Acceptance Criteria**:

- Automated semantic versioning
- All releases cryptographically signed
- SLSA Level 3 compliance
- Vulnerability response SLA: 48 hours

**Owner**: Single-maintainer

**Testing Notes**:

- Release automation validation
- Signature verification tests
- Provenance chain validation

### Track C: Dependency Management and Maintenance

**Tasks**:

- **C1**: Renovate weekly dependency updates
- **C2**: Justfile task runner implementation
- **C3**: CI job parity with local development
- **C4**: Automated security updates
- **C5**: Breaking change impact assessment

**Acceptance Criteria**:

- Weekly dependency update PRs
- All CI jobs executable locally via justfile
- Zero-day security update capability
- Breaking change documentation

**Owner**: Single-maintainer

**Testing Notes**:

- Dependency update testing
- Local/CI environment parity validation
- Breaking change impact measurement

---

## Performance Budgets Summary

| Component           | Budget   | Measurement                    |
| ------------------- | -------- | ------------------------------ |
| Database Connection | < 5s     | Time to establish connection   |
| Schema Enumeration  | < 2 min  | 1000 tables discovery          |
| JSON Output         | < 30s    | 100MB schema serialization     |
| Report Generation   | < 5 min  | 1000 tables to markdown        |
| HTML Export         | < 10s    | Client-side search response    |
| Binary Size         | < 50MB   | Statically linked executable   |
| Memory Usage        | < 1GB    | Processing 10,000 table schema |
| CI Pipeline         | < 15 min | Full test suite execution      |

---

## Risk Mitigation

### Technical Risks

- **Database compatibility issues**: Extensive integration testing with multiple engine versions
- **Performance degradation**: Continuous benchmarking and performance regression tests
- **Security vulnerabilities**: Regular security audits and automated scanning

### Resource Risks

- **Single maintainer burden**: Comprehensive automation and clear documentation
- **Community support**: Early feedback collection and issue tracking
- **Long-term maintenance**: Sustainable development practices and technical debt management

---

## Success Metrics

- **v0.1**: Successfully processes schemas from all 4 database engines
- **v0.2**: Generates publication-ready documentation
- **v0.3**: Advanced features adopted by power users
- **v1.0**: Production deployments with enterprise users
- **Ongoing**: Zero security incidents, \<24h issue response time
