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
- [ ] 2.1 Set up PostgreSQL connection pooling infrastructure
  - Replace placeholder `collect_schema` method in PostgresAdapter
  - Implement sqlx::PgPool configuration with proper connection limits
  - Add connection timeout and query timeout handling (30s defaults)
  - Create connection pool builder with security-focused defaults
  - Add connection string validation and sanitization
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 2.2 Implement basic schema enumeration queries
  - Add schema enumeration queries using information_schema.schemata
  - Query information_schema.tables for table metadata
  - Extract basic table information (name, type, schema)
  - Implement error handling for insufficient privileges
  - Add query logging with credential sanitization
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 2.3 Implement table and column introspection
  - Query information_schema.columns for column metadata
  - Extract column names, data types, nullability, and defaults
  - Implement proper UnifiedDataType mapping from PostgreSQL types
  - Handle PostgreSQL-specific types (arrays, JSON, custom types)
  - Add column ordering and position information
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 2.4 Add constraint and index collection
  - Query information_schema.table_constraints for constraints
  - Extract primary keys, foreign keys, unique constraints, and check constraints
  - Query pg_catalog.pg_indexes for index information
  - Map constraint and index metadata to unified schema format
  - Handle complex constraints and partial indexes
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 2.5 Implement foreign key relationship mapping
  - Query information_schema.referential_constraints for FK relationships
  - Extract foreign key column mappings and reference tables
  - Build relationship graph between tables
  - Handle self-referencing tables and circular references
  - Add cascade action information (ON DELETE, ON UPDATE)
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 2.6 Add comprehensive PostgreSQL adapter testing
  - Set up testcontainers for PostgreSQL integration testing
  - Test connection pooling with various configurations
  - Test schema collection with different PostgreSQL versions
  - Add tests for edge cases (empty schemas, special characters)
  - Test error handling for connection failures and timeouts
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
- [ ] 5.1 Implement advanced connection pooling configuration
  - Add configurable pool limits: max_connections (default: 10), min_idle_connections (default: 2)
  - Implement connection timeouts: connect_timeout (default: 30s), acquire_timeout (default: 30s)
  - Add idle connection management: idle_timeout (default: 10min), max_lifetime (default: 1hour)
  - Support pool configuration via environment variables and config files
  - Add runtime pool parameter validation and adjustment
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 5.2 Add comprehensive connection pool testing
  - Test connection pool exhaustion scenarios (max_connections + 1)
  - Add timeout validation under load testing
  - Test pool parameter configuration with various settings
  - Validate connection lifecycle management (acquire, release, cleanup)
  - Add performance testing with concurrent schema collection
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 5.3 Increase test coverage to meet 70% threshold
  - Audit current test coverage for dbsurveyor-core crate
  - Identify untested code paths and edge cases
  - Develop additional unit tests to meet 70% coverage requirement
  - Add integration tests using testcontainers where appropriate
  - Implement benchmarks for performance-critical code paths
  - _Requirements: 1.1, 1.2, 1.7_

- [ ] 6. Add intelligent data sampling to PostgreSQL adapter
- [ ] 6.1 Implement ordering strategy detection
  - Detect primary key columns for optimal ordering
  - Identify timestamp columns for chronological sampling
  - Find auto-increment columns for sequential sampling
  - Implement fallback strategies for tables without clear ordering
  - Add strategy selection logic based on table characteristics
  - _Requirements: 11.1, 11.2, 11.3_

- [ ] 6.2 Create configurable data sampling infrastructure
  - Implement SamplingConfig structure with rate limiting parameters
  - Add configurable rate limits: queries/sec and rows/sec
  - Implement exponential backoff for rate limit violations
  - Create sample_data method with configurable sampling strategies
  - Add randomized jitter between batches to avoid thundering herd
  - _Requirements: 11.1, 11.2, 11.3, 11.4_

- [ ] 6.3 Implement safe query execution with timeouts
  - Add PostgreSQL statement_timeout configuration
  - Implement per-query execution controls and monitoring
  - Use indexed ordering (PK/timestamp) for efficient sampling
  - Implement paginated key-range scans to avoid full-table scans
  - Add query batching with small LIMIT sizes for memory efficiency
  - _Requirements: 11.1, 11.3, 11.4_

- [ ] 6.4 Add sensitive data detection and logging
  - Implement sensitive data detection patterns (PII, credentials, etc.)
  - Add log-only warnings for potentially sensitive data (no redaction)
  - Create configurable sensitivity detection rules
  - Ensure collector never redacts data, only warns
  - Add comprehensive logging for sampling operations
  - _Requirements: 11.5, 11.6_

- [ ] 6.5 Test data sampling with various scenarios
  - Test sampling with different table structures and sizes
  - Validate rate limiting and backoff behavior under load
  - Test with various PostgreSQL data types and edge cases
  - Add performance benchmarks for sampling operations
  - Ensure 70% test coverage threshold is maintained
  - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6_

- [ ] 7. Implement multi-database collection for PostgreSQL
- [ ] 7.1 Add database enumeration capabilities
  - Implement list_databases method to query pg_database
  - Filter accessible databases based on user privileges
  - Handle system database filtering (postgres, template0, template1)
  - Add configurable exclusion patterns for database names
  - Implement privilege detection for each database
  - _Requirements: 12.1, 12.2, 12.4, 12.5_

- [ ] 7.2 Implement per-database connection management
  - Create connect_to_database method for specific database connections
  - Handle connection string modification for different databases
  - Implement connection pooling per database with resource limits
  - Add error handling for database connection failures
  - Ensure proper connection cleanup and resource management
  - _Requirements: 12.1, 12.2, 12.3_

- [ ] 7.3 Create server-level schema collection orchestration
  - Implement collect_all_databases method with parallel collection
  - Add progress reporting for multi-database operations
  - Handle partial failures gracefully (continue with accessible databases)
  - Implement configurable concurrency limits for database collection
  - Add comprehensive error reporting and logging
  - _Requirements: 12.1, 12.3, 12.6_

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
- [ ] 14.1 Create redaction configuration infrastructure
  - ✅ Add redaction modes to CLI: Conservative, Balanced, Minimal, None
  - ✅ Create CLI flags for redaction control (--no-redact, --redact-mode)
  - Create RedactionConfig structure with pattern matching rules
  - Implement RedactionPattern enum for different sensitivity levels
  - Add user override options for custom redaction rules
  - _Requirements: 4.1, 4.2, 8.6_

- [ ] 14.2 Implement redaction logic for different sensitivity levels
  - Implement Conservative mode: redact all potentially sensitive data
  - Implement Balanced mode: redact obvious PII/credentials, preserve structure
  - Implement Minimal mode: redact only clear credentials and secrets
  - Add None mode: no redaction, preserve all original data
  - Ensure redaction preserves data structure and relationships
  - _Requirements: 4.1, 4.2, 11.5_

- [ ] 14.3 Add pattern-based sensitive data detection
  - Create regex patterns for common PII (SSN, credit cards, emails)
  - Add patterns for database credentials and connection strings
  - Implement field name heuristics (password, ssn, credit_card, etc.)
  - Add configurable custom pattern support
  - Ensure collector never redacts data, only postprocessor performs redaction
  - _Requirements: 4.1, 4.2, 8.6, 11.5_

- [ ] 15. Add SQL reconstruction and schema documentation
- [ ] 15.1 Implement SQL DDL generation infrastructure
  - Create SqlDialect enum for database-specific SQL generation
  - Implement DDL generator trait with database-specific implementations
  - Add PostgreSQL DDL generation with proper type mapping
  - Generate CREATE TABLE statements with columns and data types
  - Handle database-specific syntax and quoting rules
  - _Requirements: 3.1, 3.3_

- [ ] 15.2 Add constraint and index DDL generation
  - Generate PRIMARY KEY and UNIQUE constraint statements
  - Implement FOREIGN KEY constraint generation with references
  - Add CHECK constraint generation from metadata
  - Generate CREATE INDEX statements for all indexes
  - Handle complex constraints and partial indexes
  - _Requirements: 3.1, 3.3_

- [ ] 15.3 Create comprehensive Markdown report generation
  - Replace placeholder documentation generators with askama templates
  - Implement Markdown template for schema overview and table of contents
  - Add detailed table documentation with column descriptions
  - Generate relationship sections showing foreign key connections
  - Include statistics and metadata summaries
  - _Requirements: 3.1, 6.1_

- [ ] 15.4 Add visual relationship diagrams
  - Implement Mermaid.js entity relationship diagram generation
  - Create table relationship graphs showing foreign key connections
  - Add configurable diagram complexity (simple vs. detailed)
  - Generate separate diagrams for large schemas (per-schema or per-module)
  - Include diagram legends and documentation
  - _Requirements: 6.1_

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
- [ ] 19.1 Configure parallel test execution with nextest
  - Add nextest configuration to Cargo.toml and .config/nextest.toml
  - Configure test partitioning and parallel execution settings
  - Set up test filtering and grouping for different test types
  - Add nextest integration to justfile development tasks
  - Configure test output formatting and reporting
  - _Requirements: 1.1, 1.2_

- [ ] 19.2 Set up testcontainers for database integration testing
  - Add testcontainers-modules dependencies for PostgreSQL, MySQL, MongoDB
  - Create database test fixtures with known schema structures
  - Implement test database initialization and cleanup
  - Add helper functions for container lifecycle management
  - Configure container resource limits and timeouts
  - _Requirements: 1.1, 1.2, 2.1_

- [ ] 19.3 Create comprehensive integration test suite
  - Implement integration tests for PostgreSQL adapter with real database
  - Add MySQL adapter integration tests with testcontainers
  - Create SQLite adapter tests with temporary file databases
  - Add MongoDB adapter tests with document collections
  - Test cross-database consistency and output format compatibility
  - _Requirements: 1.1, 1.2, 2.1_

- [ ] 19.4 Add property-based testing for edge cases
  - Set up proptest for generating random database schemas
  - Test schema collection with various table and column configurations
  - Add property tests for data type mapping and serialization
  - Test error handling with malformed inputs and edge cases
  - Validate security properties with property-based credential tests
  - _Requirements: 1.1, 1.2, 2.2_

- [ ] 19.5 Implement comprehensive security testing
  - Test credential sanitization in all error messages and logs
  - Verify no database credentials appear in output files
  - Test schema collection with malicious table/column names
  - Add tests for SQL injection resistance in schema queries
  - Validate memory cleanup for sensitive data structures
  - _Requirements: 2.1, 2.2_

- [ ] 22. Implement core security testing suite
- [ ] 22.1 Test credential protection and sanitization
  - Test database connection strings never appear in logs, error messages, or output files
  - Verify password fields are `zeroized` in memory after use
  - Test credential sanitization in all error paths and logging statements
  - Validate that serialized output never contains connection credentials
  - Test memory cleanup for sensitive data structures
  - _Requirements: 1.5, 2.1, 2.2, 2.3_

- [ ] 22.2 Test SQL injection resistance and malicious input handling
  - Test schema collection with malicious table/column names containing SQL injection payloads
  - Validate parameterized query usage prevents SQL injection
  - Test handling of special characters and Unicode in database identifiers
  - Test malicious file paths, connection strings, and configuration values
  - Verify input validation and sanitization for all user inputs
  - _Requirements: 2.1, 2.2, 2.4_

- [ ] 22.3 Test offline operation and network isolation
  - Test complete functionality without internet connectivity
  - Verify no external network calls except to target databases
  - Test airgap compatibility with all features enabled
  - Validate that documentation generation works completely offline
  - Test with network interfaces disabled or firewalled
  - _Requirements: 2.3, 2.4_

- [ ] 22.4 Test cryptographic security implementation
  - Test AES-GCM encryption with random nonce generation for uniqueness
  - Verify Argon2id KDF parameters meet security requirements
  - Test encryption/decryption roundtrip with various data sizes
  - Validate cryptographic constants and parameter validation
  - Test key derivation performance and security properties
  - _Requirements: 2.7, 9.3, 9.4, 9.5_

- [ ] 22.5 Set up security-focused database testing
  - Set up testcontainers with custom security profiles and privilege configurations
  - Test with minimal database privileges (read-only access)
  - Validate behavior with restricted database permissions
  - Test connection timeout and resource limit enforcement
  - Add security-focused integration tests with real databases
  - _Requirements: 1.5, 2.1, 2.2_

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
- [ ] 25.1 Set up comprehensive rustdoc API documentation
  - Set up comprehensive rustdoc with examples and security notes (Requirement 14.1)
  - Document all public APIs with security implications and usage examples
  - Add module-level documentation with architecture overviews
  - Include security guarantees and credential handling notes in all relevant APIs
  - Configure rustdoc with proper cross-references and navigation
  - _Requirements: 14.1_

- [ ] 25.2 Create user-facing mdbook documentation
  - Create mdbook user guide with installation and usage instructions (Requirement 14.2)
  - Document all CLI options and configuration parameters with examples
  - Add security best practices and operational guidelines
  - Include troubleshooting guide and FAQ section for common issues
  - Create getting started tutorial with step-by-step examples
  - _Requirements: 14.2_

- [ ] 25.3 Add practical usage examples and scenarios
  - Add practical examples for red team, compliance, and development scenarios (Requirement 14.3)
  - Create database-specific usage examples (PostgreSQL, MySQL, SQLite, MongoDB)
  - Document encryption and compression workflows with security considerations
  - Add multi-database collection examples and best practices
  - Include performance tuning and optimization guides
  - _Requirements: 14.3_

- [ ] 25.4 Create architecture and development documentation
  - Create architecture and plugin development guides (Requirement 14.4)
  - Document the dual-binary architecture and design decisions
  - Add plugin development guide with WASM integration examples
  - Include contribution guidelines and development setup instructions
  - Document security architecture and threat model
  - _Requirements: 3.1, 10.1, 14.4_

- [ ] 25.5 Set up automated documentation deployment and testing
  - Set up automated documentation deployment (Requirement 14.5)
  - Ensure all examples are tested for accuracy (Requirement 14.6)
  - Configure GitHub Pages or similar for documentation hosting
  - Add documentation build and deployment to CI pipeline
  - Implement example testing to ensure documentation accuracy
  - _Requirements: 14.5, 14.6_

- [ ] 20. Set up comprehensive cross-platform CI testing with GitHub Actions
- [ ] 20.1 Configure platform-specific CI matrix testing
  - ✅ Create matrix-based CI workflow for macOS, Windows, and Linux platforms (existing .github/workflows)
  - Configure macOS and Windows runners for build validation with SQLite-only testing
  - Set up Linux runners for comprehensive testing with all database types
  - Add platform-specific test exclusions and feature flag handling
  - Configure cross-compilation testing for different architectures
  - _Requirements: 1.1, 1.2, 7.1, 7.2_

- [ ] 20.2 Implement comprehensive security scanning
  - ✅ Configure security scanning with CodeQL, cargo-audit, cargo-deny, and Grype vulnerability checks
  - ✅ Add clippy linting with zero-warnings policy across all platforms (justfile enforces this)
  - ✅ Configure SBOM generation and security attestation for release artifacts (justfile includes sbom task)
  - Add dependency license compliance checking
  - Implement secret scanning for commits and pull requests
  - _Requirements: 2.1, 2.2_

- [ ] 20.3 Set up database integration testing in CI
  - Implement testcontainers integration for realistic database testing on Linux
  - Configure PostgreSQL, MySQL, and MongoDB containers for CI testing
  - Add database version matrix testing (multiple PostgreSQL/MySQL versions)
  - Implement test data seeding and cleanup for consistent CI runs
  - Add timeout and resource limit configuration for CI containers
  - _Requirements: 1.1, 1.2, 2.1_

- [ ] 20.4 Configure test coverage and performance monitoring
  - Configure test coverage reporting with cargo-llvm-cov and codecov integration
  - Add performance regression testing with Criterion benchmarks
  - Implement coverage threshold enforcement (70% minimum)
  - Add performance baseline tracking and regression detection
  - Configure coverage reporting for different test types (unit, integration, security)
  - _Requirements: 1.1, 1.2_

- [ ] 20.5 Optimize CI performance and caching
  - Implement artifact caching for dependencies and build outputs
  - Configure Rust toolchain caching and incremental compilation
  - Add selective test execution based on changed files
  - Create separate workflows for PR validation, nightly builds, and release testing
  - Implement parallel job execution and dependency optimization
  - _Requirements: 7.1, 7.2_

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
