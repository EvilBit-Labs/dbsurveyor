# Implementation Plan

Convert the feature design into a series of prompts for a code-generation LLM that will implement each step in a test-driven manner. Prioritize best practices, incremental progress, and early testing, ensuring no big jumps in complexity at any stage. Make sure that each prompt builds on the previous prompts, and ends with wiring things together. There should be no hanging or orphaned code that isn't integrated into a previous step. Focus ONLY on tasks that involve writing, modifying, or testing code.

## Task List

- [x] 1. Implement actual AES-GCM encryption and Argon2id key derivation ✅ **COMPLETED**

  - ✅ Replace placeholder encryption functions in `dbsurveyor-core/src/security.rs`
  - ✅ Implement `encrypt_data` function using aes-gcm crate with random 96-bit nonces
  - ✅ Implement `decrypt_data` function with proper nonce and tag validation
  - ✅ Use Argon2id for key derivation with exact settings: 16-byte salt, version 1.3, time cost 3, memory 64 MiB, parallelism 4
  - ✅ Embed KDF parameters and salt in EncryptedData structure for decryption validation
  - ✅ Add comprehensive tests for encryption/decryption roundtrip and nonce uniqueness
  - ✅ **BONUS**: Added cryptographic constants, validation helper functions, and optimized test performance
  - _Requirements: 2.7, 9.3, 9.4, 9.5_

- [ ] 2. Implement PostgreSQL schema collection with real database queries

  - Replace placeholder `collect_schema` method in PostgresAdapter
  - Implement connection pooling with sqlx::PgPool and proper configuration
  - Add schema enumeration queries using information_schema and pg_catalog
  - Extract tables, columns, data types, constraints, indexes, and foreign keys
  - Implement proper UnifiedDataType mapping from PostgreSQL types
  - Add connection timeout and query timeout handling
  - Test with real PostgreSQL database using testcontainers
  - _Requirements: 1.1, 1.2, 1.7_

- [x] 3. Set up project structure and core interfaces

  - ✅ Create Cargo workspace with three crates: dbsurveyor-core, dbsurveyor-collect, dbsurveyor
  - ✅ Define core traits: DatabaseAdapter, Connection with object-safe design
  - ✅ Implement basic error types with credential sanitization
  - ✅ Set up feature flags for database drivers (postgres, mysql, sqlite, mongodb, mssql)
  - ✅ Configure workspace-level lints and security settings
  - ✅ Implement comprehensive CLI frameworks for both binaries
  - ✅ Set up justfile with security-focused development tasks
  - _Requirements: 1.1, 1.2, 7.1, 7.2_

- [x] 4. Implement unified data models and serialization

  - ✅ Create DatabaseSchema, Table, Column, and related data structures
  - ✅ Implement UnifiedDataType enum for cross-database type mapping
  - ✅ Add serde serialization with format_version "1.0"
  - ✅ Create SamplingConfig and CollectionConfig structures
  - ✅ Implement credential sanitization in error messages
  - ✅ Add comprehensive security utilities with credential protection
  - _Requirements: 1.1, 1.3, 2.1, 9.1_

- [ ] 5. Implement comprehensive PostgreSQL adapter testing and advanced features

  - Add comprehensive unit tests with testcontainers including:
    - Connection pool exhaustion scenarios (max_connections + 1)
    - Timeout validation under load
    - Pool parameter configuration testing
    - Connection lifecycle management validation
    - Performance testing with concurrent schema collection
  - Implement advanced connection pooling configuration:
    - Configurable pool limits: max_connections (default: 10), min_idle_connections (default: 2)
    - Connection timeouts: connect_timeout (default: 30s), acquire_timeout (default: 30s)
    - Idle connection management: idle_timeout (default: 10min), max_lifetime (default: 1hour)
    - Pool configuration via environment variables and config files
    - Runtime pool parameter validation and adjustment
  - Increase minimum test coverage threshold to 70% for dbsurveyor-core
    - If it does not meet the minimum threshold, develop additional test cases to meet the requirement
    - Ensure all major functionality is covered by unit tests, benchmarks, and testcontainer-based integration tests (where appropriate)
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 6. Add intelligent data sampling to PostgreSQL adapter

  - Implement ordering strategy detection (primary key, timestamp, auto-increment)
  - Create sample_data method with configurable sampling and rate limiting
  - Add configurable rate limits: queries/sec and rows/sec with exponential backoff
  - Implement query batching with small LIMIT sizes and randomized jitter between batches
  - Add safe default timeouts (statement_timeout) and per-query execution controls
  - Use indexed ordering (PK/timestamp) or paginated key-range scans to avoid full-table scans
  - Add sensitive data detection patterns (warnings only, no redaction) with log-only sensitive-data warnings
  - Implement configurable parameters: sampling frequency, max concurrent queries, jitter, backoff policy, timeout values
  - Test with various table structures and data types
  - Increase minimum test coverage threshold to 70% for dbsurveyor-collector and dbsurveyor-core; do not lower it again
  - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6_

- [ ] 7. Implement multi-database collection for PostgreSQL

  - Add list_databases method to enumerate all accessible databases
  - Implement connect_to_database for specific database connections
  - Create collect_all_databases method with server-level schema collection
  - Add privilege detection and access level assessment
  - Handle system database filtering and exclusion patterns
  - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.6_

- [ ] 8. Create MySQL adapter with unified interface

  - Implement MySqlAdapter following the same patterns as PostgreSQL
  - Add MySQL-specific schema queries using information_schema
  - Implement data sampling with MySQL-specific ordering strategies (Requirement 11)
  - Add multi-database collection for MySQL servers (Requirement 12)
  - Ensure consistent output format with PostgreSQL adapter
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6, 12.1-12.6_

- [ ] 9. Create SQLite adapter with file-based handling

  - Implement SqliteAdapter for single-file databases
  - Use sqlite_master table for schema introspection
  - Handle SQLite-specific data types and constraints
  - Implement data sampling with ROWID-based ordering (Requirement 11)
  - Add file path validation and error handling
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6_

- [ ] 10. Create MongoDB adapter for NoSQL support

  - Implement MongoAdapter with mongodb crate
  - Add collection introspection and document schema inference
  - Implement field statistics and occurrence rate calculation
  - Create unified schema representation for NoSQL structures
  - Add document sampling with configurable limits (Requirement 11)
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6_

- [x] 11. Implement encryption and compression for output files (structure complete)

  - ✅ Create encryption module structure with placeholder functions
  - ✅ Add zstd compression support for .dbsurveyor.json.zst format in CLI
  - ✅ Create EncryptedData structure with authenticated headers
  - ✅ Add CLI integration for encryption and compression options
  - Note: Actual encryption implementation moved to Task 1 for priority
  - _Requirements: 2.1, 2.3, 9.3, 9.4, 9.5_

- [x] 12. Build collector CLI with clap integration

  - ✅ Create collector binary with comprehensive CLI using clap derive
  - ✅ Implement database type detection from connection strings
  - ✅ Add support for all output formats and encryption options
  - ✅ Implement multi-database collection CLI flags
  - ✅ Add progress reporting and verbose logging with tracing
  - ✅ Implement credential sanitization in all logging and error output
  - _Requirements: 1.1, 8.1, 8.2, 9.1_

- [x] 13. Create postprocessor core with template engine

  - ✅ Implement postprocessor binary for offline documentation generation
  - ✅ Set up askama template engine dependency for Markdown and HTML generation
  - ✅ Create input validation and format version checking
  - ✅ Add support for loading compressed and encrypted inputs (with placeholder implementations)
  - ✅ Implement basic report generation from collected metadata (placeholder implementations)
  - ✅ Add comprehensive CLI with multiple output formats and redaction modes
  - _Requirements: 3.1, 3.3, 6.1_

- [ ] 14. Implement data redaction in postprocessor

  - ✅ Add redaction modes to CLI: Conservative, Balanced, Minimal, None
  - ✅ Create CLI flags for redaction control (--no-redact, --redact-mode)
  - Create RedactionConfig and RedactionPattern structures
  - Implement configurable redaction with user override options
  - Implement actual redaction logic for different sensitivity levels
  - Ensure collector never redacts data, only postprocessor (Requirement 11.5)
  - _Requirements: 4.1, 4.2, 8.6, 11.5_

- [ ] 15. Add SQL reconstruction and schema documentation

  - Implement SQL DDL generation from collected metadata
  - Create database-specific SQL dialect support
  - Generate CREATE TABLE statements with constraints and indexes
  - Add comprehensive Markdown report generation with table of contents
  - Include relationship diagrams using Mermaid syntax
  - Replace placeholder documentation generators with actual askama templates
  - _Requirements: 3.1, 3.3, 6.1_

- [ ] 16. Implement Pro-tier features for advanced analysis

  - Add Mermaid.js and D2 visual schema diagram generation (Requirement 10.1)
  - Implement PII/PCI field classification with regex and naming heuristics (Requirement 10.2)
  - Create standalone HTML reports with search and filter capabilities (Requirement 10.3)
  - Ensure Pro features operate without DRM or cloud license checks (Requirement 10.4)
  - Implement honor system or static key validation for Pro binaries (Requirement 10.5)
  - Add feature flags for Pro functionality with graceful degradation
  - _Requirements: 10.1-10.5_

- [ ] 17. Implement plugin architecture with WASM support

  - Create PluginManager with static and WASM adapter support
  - Implement object-safe trait design for dynamic dispatch
  - Add wasmtime integration for sandboxed plugin execution
  - Create plugin loading and validation mechanisms
  - Add feature flag gating for WASM plugin support
  - _Requirements: 7.1, 7.2, 7.6, 7.8_

- [ ] 18. Create specialized collector binaries

  - Configure cargo-dist for multiple binary variants (Requirement 13)
  - Create database-specific collectors (dbsurveyor-collect-postgres, etc.)
  - Implement conditional compilation with feature flags
  - Add clear error messages for unsupported database types (Requirement 13.3)
  - Optimize binary sizes through selective compilation (Requirement 13.2)
  - Ensure postprocessor works with all collector variants (Requirement 13.5)
  - _Requirements: 7.1, 7.2, 10.1, 13.1-13.6_

- [ ] 19. Set up comprehensive testing framework

  - Configure nextest for parallel test execution
  - Set up testcontainers-modules for realistic database testing
  - Create integration tests for all database adapters
  - Add property-based testing with proptest for edge cases
  - Implement security tests for credential sanitization
  - _Requirements: 1.1, 1.2, 2.1, 2.2_

- [ ] 22. Implement core security testing suite

  - Test database connection strings never appear in logs, error messages, or output files
  - Verify password fields are `zeroized` in memory after use
  - Test schema collection with malicious table/column names containing SQL injection payloads
  - Test complete functionality without internet connectivity
  - Test AES-GCM encryption with random nonce generation for uniqueness
  - Verify Argon2id KDF parameters meet security requirements
  - Test malicious file paths, connection strings, and configuration values
  - Set up testcontainers with custom security profiles and privilege configurations
  - _Requirements: 1.5, 2.1, 2.2, 2.3, 2.4, 2.7, 9.3, 9.4, 9.5_

- [ ] 23. Add CLI snapshot testing with insta

  - Create snapshot tests for all CLI help output
  - Test error message formatting and credential sanitization
  - Add snapshot tests for generated documentation formats
  - Test multi-database collection output formatting
  - Ensure consistent CLI behavior across all collector variants
  - _Requirements: 1.1, 2.1, 8.1_

- [ ] 24. Implement performance benchmarking

  - Create Criterion benchmarks for schema collection performance
  - Benchmark different database sizes and complexity levels
  - Add benchmarks for output format generation and compression
  - Test encryption performance with different key derivation settings
  - Monitor memory usage and connection pool efficiency
  - _Requirements: 1.1, 9.6_

- [ ] 25. Create documentation with rustdoc and mdbook

  - Set up comprehensive rustdoc with examples and security notes (Requirement 14.1)
  - Create mdbook user guide with installation and usage instructions (Requirement 14.2)
  - Document all CLI options and configuration parameters
  - Add security best practices and operational guidelines
  - Include troubleshooting guide and FAQ section
  - Add practical examples for red team, compliance, and development scenarios (Requirement 14.3)
  - Create architecture and plugin development guides (Requirement 14.4)
  - Set up automated documentation deployment (Requirement 14.5)
  - Ensure all examples are tested for accuracy (Requirement 14.6)
  - _Requirements: 3.1, 10.1, 14.1-14.6_

- [x] 20. Set up comprehensive cross-platform CI testing with GitHub Actions

  - ✅ Create matrix-based CI workflow for macOS, Windows, and Linux platforms (existing .github/workflows)
  - ✅ Configure security scanning with CodeQL, cargo-audit, cargo-deny, and Grype vulnerability checks
  - ✅ Add clippy linting with zero-warnings policy across all platforms (justfile enforces this)
  - ✅ Configure SBOM generation and security attestation for release artifacts (justfile includes sbom task)
  - Configure macOS and Windows runners for build validation with SQLite-only testing
  - Set up Linux runners for comprehensive testing with all database types (PostgreSQL, MySQL, SQLite, MongoDB)
  - Implement testcontainers integration for realistic database testing on Linux
  - Configure test coverage reporting with cargo-llvm-cov and codecov integration
  - Add performance regression testing with Criterion benchmarks
  - Implement artifact caching for dependencies and build outputs
  - Create separate workflows for PR validation, nightly builds, and release testing
  - _Requirements: 1.1, 1.2, 2.1, 2.2, 7.1, 7.2_

- [x] 21. Configure distribution and release automation

  - ✅ Set up cargo-dist configuration (dist-workspace.toml exists)
  - ✅ Configure GitHub Actions for automated testing and releases (existing workflows)
  - ✅ Add CodeQL static analysis for security vulnerabilities (existing in CI)
  - ✅ Add Grype vulnerability scanning for dependencies (justfile includes security-audit)
  - Set up cargo-dist for cross-platform binary distribution with specialized collector variants
  - Enable cargo-dist SBOM generation with cargo-cyclonedx
  - Configure cargo-dist GitHub attestations for automatic build provenance and artifact signing
  - Create installation scripts and package manager integration (shell, PowerShell, Homebrew)
  - Configure per-platform binary overrides for specialized collectors
  - _Requirements: 7.1, 7.2, 10.1, 13.1-13.6_
