# Requirements Document

## Introduction

This feature implements the core database schema collection and documentation functionality for dbsurveyor. The system provides secure, offline-capable database introspection with comprehensive metadata extraction and structured documentation generation across multiple database engines. The toolchain consists of two primary executables: a collector (dbsurveyor-collect) that enumerates databases and outputs structured files, and a postprocessor (dbsurveyor) that processes output files offline to generate reports, diagrams, and reconstructed SQL.

**Data Integrity Principle**: The collector NEVER alters, redacts, or modifies data during collection. All sample data is stored exactly as retrieved from the database. If fields cannot serialize directly to JSON (e.g., binary data), they are encoded in appropriate formats (e.g., base64) to ensure exact reconstruction. Redaction and privacy controls are exclusively handled by the postprocessor during output generation.

## Requirements

### Requirement 1

**User Story:** As a database administrator, I want to collect comprehensive schema information from multiple database types including relational (PostgreSQL, MySQL, SQLite, SQL Server, Oracle) and NoSQL (MongoDB), so that I can generate complete documentation across diverse data platforms.

#### Acceptance Criteria

1. WHEN I provide a valid database connection string THEN the system SHALL establish a secure connection using appropriate drivers for the detected database type
2. WHEN the connection is established THEN the system SHALL extract all available metadata including tables/collections, columns/fields, data types, constraints, indexes, and database-specific objects
3. WHEN schema collection is complete THEN the system SHALL generate a structured .dbsurveyor.json output file with unified schema representation
4. IF the database contains more than 1000 tables/collections THEN the system SHALL complete collection in under 10 seconds
5. WHEN processing any database type THEN the system SHALL operate in read-only mode with no write operations
6. WHEN encountering NoSQL databases THEN the system SHALL infer schema from document structure and field patterns
7. WHEN processing columnar databases THEN the system SHALL extract partition information and column statistics

**Note:** Cassandra, ClickHouse, and BigQuery support are planned for future releases and will be implemented as Pro-tier features.

### Requirement 2

**User Story:** As a security analyst, I want the tool to operate with minimal network connectivity and zero telemetry, so that I can use it in air-gapped environments without security concerns.

#### Acceptance Criteria

1. WHEN the collector tool is running THEN it SHALL only initiate network connections to the target database for schema collection
2. WHEN the postprocessor is running THEN it SHALL make zero network connections and operate entirely on local files
3. WHEN processing is complete THEN the system SHALL have collected zero telemetry or usage data
4. WHEN database credentials are provided THEN they SHALL never appear in output files or logs
5. WHEN the collector serializes data THEN it SHALL use only approved formats (.dbsurveyor.json, .dbsurveyor.json.zst, .dbsurveyor.enc)
6. WHEN operating in any environment THEN the postprocessor SHALL function without internet connectivity
7. IF encryption is requested THEN the system SHALL use AES-GCM with random nonces

### Requirement 3

**User Story:** As a developer, I want to generate human-readable documentation from collected schema data, so that I can understand inherited database systems quickly.

#### Acceptance Criteria

1. WHEN I provide a .dbsurveyor.json file THEN the system SHALL generate comprehensive Markdown documentation
2. WHEN generating documentation THEN it SHALL include table-of-contents navigation and relationship diagrams
3. WHEN processing schema data THEN the system SHALL reconstruct CREATE TABLE statements with proper syntax
4. IF the schema contains foreign keys THEN the documentation SHALL visualize table relationships
5. WHEN documentation is generated THEN it SHALL be completely self-contained and offline-readable

### Requirement 4

**User Story:** As a compliance officer, I want to identify potentially sensitive data fields, so that I can assess PII/PCI compliance risks.

#### Acceptance Criteria

1. WHEN analyzing schema data THEN the system SHALL classify fields based on naming patterns and data types
2. WHEN sensitive fields are detected THEN they SHALL be flagged with confidence scores
3. WHEN sample data is collected THEN it SHALL be stored exactly as collected from database with no alteration, redaction, or modification
4. IF compliance mode is enabled THEN the system SHALL generate audit-ready reports
5. WHEN processing sensitive data THEN the collector SHALL store raw sample values without modification; redaction SHALL only occur in the postprocessor
6. WHEN serializing data to JSON THEN the system SHALL encode non-serializable fields (e.g., binary data) in appropriate formats (e.g., base64) to ensure exact reconstruction

### Requirement 5

**User Story:** As a red team operator, I want to throttle database queries and encrypt outputs, so that I can collect intelligence covertly without detection.

#### Acceptance Criteria

1. WHEN rate limiting is configured THEN the system SHALL throttle queries to the specified rate
2. WHEN encryption is requested THEN output files SHALL be encrypted with AES-GCM
3. WHEN operating covertly THEN the system SHALL minimize logging and resource usage
4. IF connection fails THEN the system SHALL implement a bounded retry policy with maximum 3 attempts, exponential backoff (base delay 500ms), randomized jitter (±100-300ms), and hard cap of 5 seconds total retry time to avoid detection
5. WHEN collection is complete THEN all operations SHALL have been read-only and non-intrusive

### Requirement 6

**User Story:** As a database administrator, I want the ability to recreate database structures from collected metadata, so that I can replicate schemas in new environments when explicitly directed.

#### Acceptance Criteria

1. WHEN rehydration mode is enabled THEN the postprocessor SHALL generate executable DDL statements from collected metadata (uses data from Requirement 1)
2. WHEN creating new database structures THEN the system SHALL only execute when explicitly directed by the user (maintains read-only principle from Requirement 1)
3. WHEN rehydrating schemas THEN the system SHALL preserve all constraints, indexes, and relationships from the original (preserves metadata from Requirement 1)
4. IF rehydration is requested THEN the user SHALL provide explicit confirmation before any write operations (security control extending Requirement 2)
5. WHEN rehydration is complete THEN the system SHALL provide a detailed report of created objects (documentation extending Requirement 3)

### Requirement 7

**User Story:** As a developer, I want to easily extend the tool to support new database types through a plugin architecture, so that the system can evolve with emerging database technologies.

#### Acceptance Criteria

1. WHEN implementing a new database adapter THEN it SHALL conform to a standardized Rust trait interface
2. WHEN a plugin is loaded THEN the system SHALL validate its compatibility and security
3. WHEN multiple database adapters are available THEN they SHALL be selectable via feature flags or runtime configuration
4. IF a database type is unsupported THEN the system SHALL provide clear guidance on plugin development with feature flag hints
5. WHEN plugins are compiled THEN they SHALL maintain the same security guarantees as built-in adapters
6. WHEN new adapters are added THEN they SHALL support the unified .dbsurveyor.json output format
7. WHEN using WASM plugins THEN they SHALL be loaded via wasmtime with sandboxed execution
8. WHEN using stdio plugins THEN they SHALL communicate via stable JSON contract over stdin/stdout

### Requirement 8

**User Story:** As a security operator, I want configurable data sampling and throttling capabilities, so that I can control the collection process for operational security.

#### Acceptance Criteria

1. WHEN sampling data THEN the system SHALL collect the most recent N rows per table using best-effort ordering (extends metadata collection from Requirement 1)
2. WHEN throttling is configured THEN the collector SHALL control the rate of database queries to reduce detection risk (supports covert operation from Requirement 5)
3. WHEN sampling is enabled THEN the system SHALL provide clear warnings about potentially sensitive data in samples (security awareness extending Requirement 2)
4. IF no ordering is available THEN the system SHALL sample using available methods (timestamp, primary key, etc.) (fallback for Requirement 1 metadata)
5. WHEN collection is complete THEN the system SHALL have operated without triggering slow query logs (stealth operation supporting Requirement 5)
6. WHEN redaction is requested THEN the postprocessor SHALL apply redaction to sample values during output generation while preserving original data in source files (privacy control extending Requirement 4)

**Note**: The collector and postprocessor have distinct responsibilities: the collector samples and stores data exactly as retrieved from the database, while the postprocessor handles all data transformation, redaction, and output formatting. This separation ensures data integrity and allows the same collected data to be processed multiple times with different privacy settings.

### Requirement 9

**User Story:** As a system administrator, I want multiple output formats with optional compression and encryption, so that I can securely store and transport database metadata.

#### Acceptance Criteria

1. WHEN collection is complete THEN the system SHALL generate .dbsurveyor.json with format_version "1.0" (structured output from Requirement 1)
2. WHEN compression is requested THEN the system SHALL output .dbsurveyor.json.zst using Zstandard compression (efficiency for large schemas from Requirement 1)
3. WHEN encryption is requested THEN the system SHALL output .dbsurveyor.enc using AES-GCM with 96-bit (12-byte) unique nonces per file (security extending Requirement 2)
4. WHEN encrypting data THEN the system SHALL use Argon2id as KDF with memory_size >= 64MB and iterations >= 3, require salt length >= 16 bytes, and embed version, KDF parameters, salt, and associated-data metadata in authenticated headers (security implementation for Requirement 2)
5. WHEN providing encryption keys THEN the system SHALL support --key, --key-file, or stdin with TTY echo disabled, and zeroize derived keys and secret material in memory after use (credential security from Requirement 2)
6. WHEN generating output THEN files SHALL remain under 10MB when possible for typical workloads (performance constraint supporting Requirement 1)

### Requirement 10

**User Story:** As a compliance auditor, I want Pro-tier features for advanced analysis and reporting, so that I can generate comprehensive compliance documentation.

#### Acceptance Criteria

1. WHEN Pro features are enabled THEN the system SHALL generate Mermaid.js or D2 visual schema diagrams (extends Requirement 3)
2. WHEN classification is requested THEN the system SHALL tag likely PII/PCI fields based on regex and naming heuristics (extends Requirement 4)
3. WHEN HTML output is requested THEN the system SHALL generate standalone styled HTML reports with search/filter capabilities (extends Requirement 3)
4. IF Pro features are used THEN the system SHALL not enforce DRM or cloud license checks (maintains Requirement 2 security principles)
5. WHEN Pro binaries are distributed THEN they SHALL operate on honor system or static key validation (maintains offline-first from Requirement 2)

### Requirement 11

**User Story:** As a database administrator, I want intelligent data sampling that identifies the most recent records, so that I can understand current data patterns without manual query construction.

#### Acceptance Criteria

1. WHEN sampling data THEN the system SHALL automatically identify primary keys for optimal ordering (supports Requirement 1 metadata collection)
2. WHEN no primary key exists THEN the system SHALL detect timestamp columns (created_at, updated_at, etc.) for chronological ordering
3. WHEN no timestamp columns exist THEN the system SHALL fall back to auto-increment columns or system row IDs
4. IF no reliable ordering is available THEN the system SHALL use random sampling with appropriate warnings
5. WHEN collecting samples THEN the collector SHALL store data exactly as stored in database with zero alteration, redaction, or modification (maintains data integrity from Requirement 2)
6. WHEN configuring sampling THEN users SHALL be able to specify sample size, throttling, and exclusion patterns

### Requirement 12

**User Story:** As a security operator with superuser privileges, I want to collect schemas from all accessible databases on a server, so that I can perform comprehensive database enumeration.

#### Acceptance Criteria

1. WHEN provided with server-level credentials THEN the system SHALL enumerate all accessible databases automatically
2. WHEN connection string omits database name THEN the system SHALL attempt multi-database discovery
3. WHEN collecting multiple databases THEN the system SHALL respect user privileges and only access permitted databases (security control from Requirement 2)
4. IF system databases exist THEN they SHALL be excluded by default but includable via configuration flag
5. WHEN multi-database collection occurs THEN the system SHALL provide progress reporting and error handling per database
6. WHEN enumeration completes THEN the output SHALL include server-level metadata and per-database collection status

### Requirement 13

**User Story:** As an operator with limited bandwidth or storage, I want database-specific collector binaries, so that I can deploy minimal tools for specific database types.

#### Acceptance Criteria

1. WHEN distributing collectors THEN the system SHALL provide specialized binaries for each database type (postgres, mysql, sqlite, mongodb, mssql)
2. WHEN using specialized collectors THEN binary size SHALL be minimized through selective compilation
3. WHEN database type is unsupported THEN the collector SHALL provide clear error messages with alternative binary suggestions (user experience from Requirement 3)
4. IF universal support is needed THEN a full-featured collector SHALL be available with all database drivers
5. WHEN postprocessor is used THEN it SHALL work with outputs from any collector variant (maintains unified format from Requirement 9)
6. WHEN installing collectors THEN users SHALL be able to choose appropriate binary for their specific use case

### Requirement 14

**User Story:** As a developer and user, I want comprehensive documentation using modern Rust tooling, so that I can effectively use and contribute to the project.

#### Acceptance Criteria

1. WHEN accessing API documentation THEN it SHALL be generated with rustdoc including security notes and examples
2. WHEN reading user guides THEN they SHALL be created with mdbook including installation, usage, and troubleshooting
3. WHEN viewing documentation THEN it SHALL include practical examples for all major use cases (red team, compliance, development scenarios)
4. IF contributing to the project THEN architecture and plugin development guides SHALL be available
5. WHEN documentation is built THEN it SHALL be automatically deployed and kept current with releases
6. WHEN examples are provided THEN they SHALL be tested to ensure accuracy and functionality

## Future / Roadmap

The following database engines are planned for future releases and will be implemented as Pro-tier features:

- **Cassandra**: NoSQL wide-column database support with CQL schema extraction
- **ClickHouse**: Columnar database support with partition and compression analysis
- **BigQuery**: Cloud data warehouse integration with project/dataset discovery

These features will maintain the same security guarantees and offline-first architecture as the baseline database engines.
