# Implementation Plan

Convert the feature design into a series of prompts for a code-generation LLM that will implement each step in a test-driven manner. Prioritize best practices, incremental progress, and early testing, ensuring no big jumps in complexity at any stage. Make sure that each prompt builds on the previous prompts, and ends with wiring things together. There should be no hanging or orphaned code that isn't integrated into a previous step. Focus ONLY on tasks that involve writing, modifying, or testing code.

## Task List

- [ ] 1. Set up project structure and core interfaces

  - Create Cargo workspace with three crates: dbsurveyor-core, dbsurveyor-collect, dbsurveyor
  - Define core traits: DatabaseAdapter, Connection with object-safe design
  - Implement basic error types with credential sanitization
  - Set up feature flags for database drivers (postgres, mysql, sqlite, mongodb, mssql)
  - Configure workspace-level lints and security settings
  - _Requirements: 1.1, 1.2, 7.1, 7.2_

- [ ] 2. Implement unified data models and serialization

  - Create DatabaseSchema, Table, Column, and related data structures
  - Implement UnifiedDataType enum for cross-database type mapping
  - Add serde serialization with format_version "1.0"
  - Create SamplingConfig and CollectionConfig structures
  - Implement credential sanitization in error messages
  - _Requirements: 1.1, 1.3, 2.1, 9.1_

- [ ] 3. Create PostgreSQL adapter with schema collection

  - Implement PostgresAdapter with sqlx integration
  - Add connection management with comprehensive connection pooling:
    - Configurable pool limits: max_connections (default: 10), min_idle_connections (default: 2)
    - Connection timeouts: connect_timeout (default: 30s), acquire_timeout (default: 30s)
    - Idle connection management: idle_timeout (default: 10min), max_lifetime (default: 1hour)
    - Pool configuration via environment variables and config files
    - Runtime pool parameter validation and adjustment
  - Implement schema enumeration using information_schema and pg_catalog
  - Extract tables, columns, indexes, constraints, and foreign keys
  - Add comprehensive unit tests with testcontainers including:
    - Connection pool exhaustion scenarios (max_connections + 1)
    - Timeout validation under load
    - Pool parameter configuration testing
    - Connection lifecycle management validation
    - Performance testing with concurrent schema collection
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 4. Add intelligent data sampling to PostgreSQL adapter

  - Implement ordering strategy detection (primary key, timestamp, auto-increment)
  - Create sample_data method with configurable sampling and rate limiting
  - Add configurable rate limits: queries/sec and rows/sec with exponential backoff
  - Implement query batching with small LIMIT sizes and randomized jitter between batches
  - Add safe default timeouts (statement_timeout) and per-query execution controls
  - Use indexed ordering (PK/timestamp) or paginated key-range scans to avoid full-table scans
  - Add sensitive data detection patterns (warnings only, no redaction) with log-only sensitive-data warnings
  - Implement configurable parameters: sampling frequency, max concurrent queries, jitter, backoff policy, timeout values
  - Test with various table structures and data types
  - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6_

- [ ] 5. Implement multi-database collection for PostgreSQL

  - Add list_databases method to enumerate all accessible databases
  - Implement connect_to_database for specific database connections
  - Create collect_all_databases method with server-level schema collection
  - Add privilege detection and access level assessment
  - Handle system database filtering and exclusion patterns
  - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.6_

- [ ] 6. Create MySQL adapter with unified interface

  - Implement MySqlAdapter following the same patterns as PostgreSQL
  - Add MySQL-specific schema queries using information_schema
  - Implement data sampling with MySQL-specific ordering strategies (Requirement 11)
  - Add multi-database collection for MySQL servers (Requirement 12)
  - Ensure consistent output format with PostgreSQL adapter
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6, 12.1-12.6_

- [ ] 7. Create SQLite adapter with file-based handling

  - Implement SqliteAdapter for single-file databases
  - Use sqlite_master table for schema introspection
  - Handle SQLite-specific data types and constraints
  - Implement data sampling with ROWID-based ordering (Requirement 11)
  - Add file path validation and error handling
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6_

- [ ] 8. Create MongoDB adapter for NoSQL support

  - Implement MongoAdapter with mongodb crate
  - Add collection introspection and document schema inference
  - Implement field statistics and occurrence rate calculation
  - Create unified schema representation for NoSQL structures
  - Add document sampling with configurable limits (Requirement 11)
  - _Requirements: 1.1, 1.2, 1.7, 11.1-11.6_

- [ ] 9. Implement encryption and compression for output files

  - Create encryption module using aes-gcm and ring crates
  - Implement AES-GCM with random nonces and embedded Argon2id KDF parameters
  - Use exact Argon2id settings: 16-byte salt, Argon2id version 1.3, time cost 3, memory 64 MiB (65536 KiB), parallelism 4, output key length 32 bytes
  - Embed KDF parameters and per-file salt in encrypted file header for decryption validation
  - Implement parameter validation and version checking during decryption
  - Add zstd compression support for .dbsurveyor.json.zst format
  - Create EncryptedOutput structure with authenticated headers
  - Support multiple output formats: .json, .json.zst, .enc
  - _Requirements: 2.1, 2.3, 9.3, 9.4, 9.5_

- [ ] 10. Build collector CLI with clap integration

  - Create collector binary with comprehensive CLI using clap derive
  - Implement database type detection from connection strings
  - Add support for all output formats and encryption options
  - Implement multi-database collection CLI flags
  - Add progress reporting and verbose logging with tracing
  - _Requirements: 1.1, 8.1, 8.2, 9.1_

- [ ] 11. Create postprocessor core with template engine

  - Implement postprocessor binary for offline documentation generation
  - Set up askama template engine for Markdown and HTML generation
  - Create input validation and format version checking
  - Add support for loading compressed and encrypted inputs
  - Implement basic report generation from collected metadata
  - _Requirements: 3.1, 3.3, 6.1_

- [ ] 12. Implement data redaction in postprocessor

  - Create RedactionConfig and RedactionPattern structures
  - Implement configurable redaction with user override options
  - Add redaction modes: Conservative, Balanced, Minimal, None
  - Create CLI flags for redaction control (--no-redact, --redact-mode)
  - Ensure collector never redacts data, only postprocessor (Requirement 11.5)
  - _Requirements: 4.1, 4.2, 8.6, 11.5_

- [ ] 13. Add SQL reconstruction and schema documentation

  - Implement SQL DDL generation from collected metadata
  - Create database-specific SQL dialect support
  - Generate CREATE TABLE statements with constraints and indexes
  - Add comprehensive Markdown report generation with table of contents
  - Include relationship diagrams using Mermaid syntax
  - _Requirements: 3.1, 3.3, 6.1_

- [ ] 13.1. Implement Pro-tier features for advanced analysis

  - Add Mermaid.js and D2 visual schema diagram generation (Requirement 10.1)
  - Implement PII/PCI field classification with regex and naming heuristics (Requirement 10.2)
  - Create standalone HTML reports with search and filter capabilities (Requirement 10.3)
  - Ensure Pro features operate without DRM or cloud license checks (Requirement 10.4)
  - Implement honor system or static key validation for Pro binaries (Requirement 10.5)
  - Add feature flags for Pro functionality with graceful degradation
  - _Requirements: 10.1-10.5_

- [ ] 14. Implement plugin architecture with WASM support

  - Create PluginManager with static and WASM adapter support
  - Implement object-safe trait design for dynamic dispatch
  - Add wasmtime integration for sandboxed plugin execution
  - Create plugin loading and validation mechanisms
  - Add feature flag gating for WASM plugin support
  - _Requirements: 7.1, 7.2, 7.6, 7.8_

- [ ] 15. Create specialized collector binaries

  - Configure cargo-dist for multiple binary variants (Requirement 13)
  - Create database-specific collectors (dbsurveyor-collect-postgres, etc.)
  - Implement conditional compilation with feature flags
  - Add clear error messages for unsupported database types (Requirement 13.3)
  - Optimize binary sizes through selective compilation (Requirement 13.2)
  - Ensure postprocessor works with all collector variants (Requirement 13.5)
  - _Requirements: 7.1, 7.2, 10.1, 13.1-13.6_

- [ ] 16. Set up comprehensive testing framework

  - Configure nextest for parallel test execution
  - Set up testcontainers-modules for realistic database testing
  - Create integration tests for all database adapters
  - Add property-based testing with proptest for edge cases
  - Implement security tests for credential sanitization
  - _Requirements: 1.1, 1.2, 2.1, 2.2_

- [ ] 16.1. Implement credential sanitization and secrets protection tests

  - Test database connection strings never appear in logs, error messages, or output files
  - Verify password fields are zeroized in memory after use
  - Test connection string parsing without credential persistence
  - Tooling: Custom test utilities, memory inspection tools
  - Acceptance: Zero credential leakage in any output format, memory zeroization verified
  - _Requirements: 2.4, 9.5_

- [ ] 16.2. Implement SQL/NoSQL injection prevention tests

  - Test schema collection with malicious table/column names containing SQL injection payloads
  - Verify NoSQL injection attempts in MongoDB collection/field names
  - Test edge cases with special characters, unicode, and control sequences
  - Tooling: testcontainers, proptest for property-based testing, custom injection payloads
  - Acceptance: All injection attempts fail safely, no code execution, proper error handling
  - _Requirements: 1.5, 2.1, 2.2_

- [ ] 16.3. Implement access control and privilege escalation prevention tests

  - Test schema collection with minimal database privileges (SELECT only)
  - Verify no privilege escalation attempts during connection or query execution
  - Test behavior with revoked permissions and restricted access patterns
  - Tooling: testcontainers with custom privilege configurations, database user management
  - Acceptance: Operations fail gracefully with insufficient privileges, no privilege escalation
  - _Requirements: 1.5, 2.1, 2.3, 12.3_

- [ ] 16.4. Implement secrets leakage prevention and memory isolation tests

  - Test that no sensitive data appears in generated documentation or output files
  - Verify encryption keys and derived secrets are never logged or persisted
  - Test memory isolation between different database connections
  - Tooling: Memory analysis tools, custom test harnesses, encryption validation
  - Acceptance: Zero secrets in any output, proper memory isolation, encryption compliance
  - _Requirements: 2.4, 9.5, 9.6_

- [ ] 16.5. Implement stealth operation and evasion detection tests

  - Test rate limiting compliance with configured throttling parameters
  - Verify query timing patterns don't trigger slow query logs or monitoring alerts
  - Test retry policies with exponential backoff and jitter for detection avoidance
  - Test resource usage patterns (CPU, memory, network) for covert operation
  - Tooling: testcontainers with monitoring, custom timing analysis, resource profiling
  - Acceptance: Rate limits enforced, retry policies work correctly, minimal resource footprint
  - _Requirements: 5.1, 5.4, 8.2, 8.5_

- [ ] 16.6. Implement offline operation and airgap compatibility tests

  - Test complete functionality without internet connectivity
  - Verify no external API calls or telemetry collection
  - Test airgap compatibility with all database types
  - Tooling: Network isolation tools, dependency analysis, offline environment simulation
  - Acceptance: Full functionality in airgapped environments, zero external dependencies
  - _Requirements: 2.1, 2.2, 2.3, 2.6_

- [ ] 16.7. Implement encryption and cryptographic security validation tests

  - Test AES-GCM encryption with random nonce generation for uniqueness
  - Verify Argon2id KDF parameters meet security requirements
  - Test key derivation and `zeroization` processes
  - Test encrypted output format compliance and decryption accuracy
  - Tooling: Cryptographic validation libraries, custom encryption test suites
  - Acceptance: Encryption meets AES-GCM standards, KDF parameters compliant, keys properly `zeroized`
  - _Requirements: 2.7, 9.3, 9.4, 9.5_

- [ ] 16.8. Implement input validation and malicious input handling tests

  - Test malicious file paths, connection strings, and configuration values
  - Verify proper handling of malformed database responses and edge cases
  - Test boundary conditions with extremely large schemas and data volumes
  - Tooling: proptest for property-based testing, fuzzing tools, custom test generators
  - Acceptance: All malicious inputs rejected safely, proper error handling, no crashes
  - _Requirements: 1.1, 2.1, 2.2, 2.3_

- [ ] 16.9. Implement plugin security and WASM sandboxing tests

  - Test WASM plugin loading and execution in sandboxed environments
  - Verify plugin isolation and resource limits
  - Test malicious plugin detection and rejection
  - Tooling: wasmtime integration, custom plugin test harnesses, security scanners
  - Acceptance: Plugins execute safely in sandbox, malicious plugins rejected, proper isolation
  - _Requirements: 7.2, 7.7, 7.8_

- [ ] 16.10. Implement compliance and audit trail validation tests

  - Test PII/PCI field detection accuracy and confidence scoring
  - Verify audit logging and compliance reporting functionality
  - Test data sampling compliance with privacy requirements
  - Tooling: Compliance validation tools, custom test datasets, audit log analysis
  - Acceptance: Accurate sensitive data detection, proper audit trails, compliance reporting
  - _Requirements: 4.1, 4.2, 4.5, 10.2_

- [ ] 16.11. Implement performance and resource security tests

  - Test memory usage patterns for potential memory exhaustion attacks
  - Verify connection pool security and resource isolation
  - Test large schema handling without resource exhaustion
  - Tooling: Memory profiling tools, resource monitoring, stress testing frameworks
  - Acceptance: Bounded memory usage, secure resource isolation, graceful degradation
  - _Requirements: 1.4, 9.6, 11.6_

- [ ] 16.12. Set up security testing framework and infrastructure

  - Configure testcontainers with custom security profiles and privilege configurations
  - Set up memory analysis and profiling tools for credential leakage detection
  - Integrate cryptographic validation libraries for encryption testing
  - Configure network isolation tools for offline operation testing
  - Set up custom test harnesses for security-specific test scenarios
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.7_

- [ ] 16.13. Create security testing data and malicious test fixtures

  - Create malicious test datasets with SQL injection payloads and edge cases
  - Build test databases with sensitive data patterns for PII/PCI detection testing
  - Generate large schema datasets for performance and resource security testing
  - Create privilege escalation test scenarios with restricted database users
  - Develop stealth operation test cases with monitoring and detection tools
  - _Requirements: 1.1, 1.5, 2.1, 2.3, 4.1, 4.2, 5.1, 5.4_

- [ ] 16.14. Integrate security testing into CI/CD pipeline

  - Integrate security tests into CI/CD pipeline with automated execution
  - Set up security test reporting and failure analysis tools
  - Configure security test coverage metrics and reporting
  - Implement security test result caching and incremental testing
  - Add security test failure alerts and notification systems
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.7_

- [ ] 17. Add CLI snapshot testing with insta

  - Create snapshot tests for all CLI help output
  - Test error message formatting and credential sanitization
  - Add snapshot tests for generated documentation formats
  - Test multi-database collection output formatting
  - Ensure consistent CLI behavior across all collector variants
  - _Requirements: 1.1, 2.1, 8.1_

- [ ] 18. Implement performance benchmarking

  - Create Criterion benchmarks for schema collection performance
  - Benchmark different database sizes and complexity levels
  - Add benchmarks for output format generation and compression
  - Test encryption performance with different key derivation settings
  - Monitor memory usage and connection pool efficiency
  - _Requirements: 1.1, 9.6_

- [ ] 19. Create documentation with rustdoc and mdbook

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

- [ ] 20. Set up comprehensive cross-platform CI testing with GitHub Actions

  - Create matrix-based CI workflow for macOS, Windows, and Linux platforms
  - Configure macOS and Windows runners for build validation with SQLite-only testing
  - Set up Linux runners for comprehensive testing with all database types (PostgreSQL, MySQL, SQLite, MongoDB)
  - Implement testcontainers integration for realistic database testing on Linux
  - Add security scanning with CodeQL, cargo-audit, cargo-deny, and Grype vulnerability checks
  - Configure test coverage reporting with cargo-llvm-cov and codecov integration
  - Add performance regression testing with Criterion benchmarks
  - Implement artifact caching for dependencies and build outputs
  - Create separate workflows for PR validation, nightly builds, and release testing
  - Add clippy linting with zero-warnings policy across all platforms
  - Configure SBOM generation and security attestation for release artifacts
  - _Requirements: 1.1, 1.2, 2.1, 2.2, 7.1, 7.2_

- [ ] 21. Configure distribution and release automation

  - Set up cargo-dist for cross-platform binary distribution with specialized collector variants
  - Configure GitHub Actions for automated testing and releases
  - Enable cargo-dist SBOM generation with cargo-cyclonedx
  - Add CodeQL static analysis for security vulnerabilities
  - Add Grype vulnerability scanning for dependencies (cargo-dist doesn't handle this)
  - Configure cargo-dist GitHub attestations for automatic build provenance and artifact signing
  - Create installation scripts and package manager integration (shell, PowerShell, Homebrew)
  - Configure per-platform binary overrides for specialized collectors
  - _Requirements: 7.1, 7.2, 10.1, 13.1-13.6_
