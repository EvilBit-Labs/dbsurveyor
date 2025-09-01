# Requirements Document: dbsurveyor

| Field            | Value       |
|------------------|-------------|
| **Project Name** | dbsurveyor  |
| **Version**      | 1.0         |
| **Status**       | Draft       |
| **Author(s)**    | UncleSp1d3r |
| **Created**      | 2024-12-19  |
| **Modified**     | 2024-12-19  |

## Table of Contents

[TOC]

## 1. Introduction & Scope

### 1.1 Project Description and Purpose

dbsurveyor is a two-part toolchain designed to quickly survey and map the contents of unknown or unfamiliar database systems. It provides security operators, penetration testers, and system administrators with fast visibility into the structure and contents of database servers for which they have access credentials. The tool is split into a collector and a postprocessor to support offline workflows and red team use cases.

The system provides comprehensive database metadata extraction, documentation generation, and schema analysis capabilities without requiring persistent network connectivity. The toolchain consists of two independent binaries that work together to collect database metadata and generate structured output files for further processing and documentation.

#### CRITICAL SECURITY GUARANTEES

- **OFFLINE-ONLY OPERATION**: Zero network calls after installation; no external dependencies during runtime
- **NO TELEMETRY**: Absolutely no data collection, usage tracking, or external reporting mechanisms
- **NO CREDENTIALS IN OUTPUTS**: Database credentials never appear in any output files, logs, or debug information
- **AIRGAP COMPATIBILITY**: Full functionality in air-gapped environments for both binaries and all outputs

### 1.2 Project Goals and Objectives

The primary objectives of dbsurveyor are:

- **Offline-First Operation**: All functionality must work without internet connectivity after initial installation
- **Operator-Focused Design**: Built for database administrators, data analysts, and software developers who need reliable, comprehensive database documentation
- **Comprehensive Coverage**: Support major database systems (PostgreSQL, MySQL, SQLite, SQL Server, Oracle, MongoDB) with full metadata extraction
- **NoSQL Database Support**: MongoDB support is required for the initial release to ensure coverage of both relational and non-relational database paradigms
- **Structured Output**: Generate portable, machine-readable output formats for integration with other tools and workflows
- **Cross-Platform Compatibility**: Native support for Linux, macOS, and Windows environments
- **Security-Conscious**: Zero telemetry, secure credential handling, and configurable data anonymization

### 1.3 Target Audience and Stakeholders

#### Primary Users

- **Red team operators**: Performing post-compromise database enumeration in contested environments
- **Blue team analysts and defenders**: Auditing unknown systems and inherited databases
- **System administrators**: Exploring inherited or legacy databases with minimal documentation
- **Developers**: Working in inherited environments with minimal documentation
- **Database Administrators (DBAs)**: Comprehensive documentation, schema analysis, and compliance reporting
- **Data Analysts**: Data discovery, quality assessment, and relationship understanding

#### Secondary Users

- **Compliance Officers**: Audit-ready documentation and sensitive data cataloging
- **Security Analysts**: Database security assessment and risk identification

### 1.4 Project Boundaries and Limitations

#### In Scope

- Metadata extraction from supported database systems
- Schema documentation and visualization
- Data profiling and quality assessment
- Offline operation capabilities
- Cross-platform binary distribution

#### Out of Scope

- Real-time monitoring or alerting
- Database modification or migration execution
- Performance tuning recommendations
- Web-based user interfaces
- Cloud-specific integrations

## 2. Scope Definition

### 2.1 In-scope Features and Functionality

#### Core Collection Features

- Database connectivity for PostgreSQL, MySQL, SQLite, SQL Server, Oracle
- Comprehensive metadata extraction (tables, views, indexes, procedures, functions, triggers)
- Data profiling and statistics collection
- Incremental collection with delta updates
- Progress monitoring and error recovery

#### Documentation Generation

- HTML documentation with navigation and search
- Entity Relationship Diagrams (ERD)
- Data dictionaries with business-friendly formatting
- Schema comparison reports
- Multi-format output (HTML, PDF, Markdown)

#### Analysis Capabilities

- Schema complexity analysis
- Data quality assessment
- Relationship discovery and validation
- Migration impact analysis
- Compliance reporting features

### 2.2 Out-of-scope Items

- Database backup or restore functionality
- Query execution or data manipulation
- Real-time monitoring capabilities
- Network-based operation modes
- Plugin architectures or extensibility frameworks
- GUI applications

### 2.3 Success Criteria and Acceptance Criteria

#### Technical Success Criteria

- Successfully processes databases with 10,000+ tables
- Memory usage under 1GB for typical workloads
- Cross-platform compatibility (Linux, macOS, Windows)
- Complete offline functionality
- Zero data leakage or telemetry

#### User Success Criteria

- Reduces database documentation time by 90%
- Provides audit-ready compliance documentation
- Enables effective schema change management
- Supports air-gapped and secure environments

### 2.4 Timeline and Milestones

Detailed task breakdown and implementation phases are documented in [tasks.md](tasks.md).

#### Milestone Naming Strategy

**Convention**: Milestones are named by version number (e.g., `v0.1`, `v0.2`, `v0.3`, `v1.0`) with contextual descriptions that explain the milestone's purpose and scope.

#### v0.1 - Collector MVP

- Database connectivity and metadata extraction
- Multi-engine support (PostgreSQL, MySQL, SQLite, MongoDB)
- Basic schema collection functionality
- Structured output generation

#### v0.2 - Postprocessor MVP

- Documentation generation from collected metadata
- Markdown and HTML report generation
- SQL reconstruction capabilities
- Privacy controls and redaction features

#### v0.3 - Pro Features

- Advanced schema diagramming (Mermaid/D2)
- Data classification and compliance reporting
- Interactive HTML exports with search
- Plugin system architecture

#### v1.0 - Production Release

- Cross-platform packaging and distribution
- Comprehensive documentation and polish
- Security hardening and audit completion
- Performance optimization and tuning

## 3. Context and Background

### 3.1 Business Context and Justification

- Red teams often find database credentials but lack visibility into contents
- Blue teams and devs frequently inherit systems with undocumented schemas
- Existing tools are heavy, invasive, or not suited to airgapped use
- dbsurveyor fills this niche with a polished, low-friction survey/report pipeline

### 3.2 Previous Work and Dependencies

- Inspired by ad hoc scripts and tools like sqlmap, schemaSpy, and internal red team enumeration tools
- No runtime external dependencies — static builds only

### 3.3 Assumptions and Constraints

#### Assumptions

- Requires valid DB credentials and host info
- Collector requires runtime DB driver availability (statically linked or embedded)
- Output file must be portable and self-contained
- Users are comfortable with command-line interfaces

#### Constraints

- Must operate without internet connectivity
- Zero telemetry or external reporting
- Minimal external dependencies
- Cross-platform compatibility requirements

### 3.4 Risk Assessment Overview

#### Medium Risk

- Driver support and edge-case schemas (e.g., no primary keys)
- DB-specific quirks and non-standard metadata conventions
- Cross-platform build and distribution complexity

#### Low Risk

- CLI-focused design minimizes complexity
- Core Rust ecosystem stability
- File I/O and basic system operations

## 4. Functional Requirements

### 4.1 Functional Requirements (F000-F023)

Updated to align with complete business requirements from dbsurveyor_requirements.md

#### System Architecture (F000-F002)

- **F000**: Dual-binary architecture with independent collector (dbsurveyor-collect) and postprocessor (dbsurveyor) executables
- **F001**: Multi-database connection support via Rust traits for PostgreSQL, MySQL, SQLite, SQL Server, and MongoDB
- **F002**: Feature flag-based database driver selection to minimize binary size and dependencies

#### Authentication and Security (F003-F005)

- **F003**: Multiple authentication methods including username/password, certificates, and environment variables
- **F004**: AES-GCM encryption of output files with random nonces and embedded KDF parameters
- **F005**: Zero telemetry operation with no credential storage in outputs or logs

#### Metadata Collection (F006-F010)

- **F006**: Comprehensive schema discovery and enumeration across all supported database engines
- **F007**: Table structure analysis including columns, data types, constraints, indexes, and foreign keys
- **F008**: Database-specific object extraction (views, stored procedures, functions, triggers, user-defined types)
- **F009**: Incremental collection with change detection and delta updates for large databases
- **F010**: Connection pooling, timeout handling, and graceful degradation on partial failures

#### Data Profiling and Analysis (F011-F013)

- **F011**: Statistical analysis including row counts, table sizes, and column value distributions
- **F012**: Data quality metrics (completeness, consistency, uniqueness) with configurable thresholds
- **F013**: Configurable sample data extraction with privacy controls and pattern-based redaction

#### Core Processing Features (F014-F016)

- **F014**: Generate .dbsurveyor.json output — fully portable and structured with "format_version": "1.0" specification
- **F015**: Postprocessor that loads .dbsurveyor.json and generates markdown/JSON reports
- **F016**: SQL reconstruction mode — outputs CREATE TABLE definitions based on discovered schema

#### Reporting and Visualization (F017-F019)

- **F017**: Report mode — renders a browsable table-of-contents style markdown document
- **F018**: Diagram mode (Pro) — generates Mermaid.js or D2 visual schema diagrams
- **F019**: Classify mode (Pro) — tags likely PII/PCI fields based on regex or naming heuristics

#### Pro Features and Advanced Capabilities (F020-F023)

- **F020**: HTML output (Pro) — standalone styled HTML report generation with search/filter
- **F021**: Offline mode only — all features must function without network access
- **F022**: Support pluggable DB engines via a Rust trait-based adapter system (adapters may be compiled-in by feature flag or loaded as WASM/stdio child-process plugins using stable JSON contract)
- **F023**: Additional capabilities: Allow user to redact sensitive sample values in postprocessor; MVP must include at least one NoSQL database with schema-like inspection support (e.g., MongoDB); Support configurable throttling rate in collector to reduce detection risk and avoid slow logs; Support optional output compression (e.g., gzip) and encryption (e.g., AES-GCM) via CLI flags

### 4.2 User Stories and Use Cases

Comprehensive user stories are detailed in the [user_stories.md](user_stories.md) document. Key use cases include:

- **Database Documentation**: Automated generation of comprehensive database documentation (See [US-DBA-001](user_stories.md#us-dba-001-basic-database-survey), [US-DBA-005](user_stories.md#us-dba-005-html-documentation-generation))
- **Schema Analysis**: Understanding table relationships and data dependencies (See [US-DA-004](user_stories.md#us-da-004-relationship-discovery))
- **Compliance Reporting**: Audit-ready documentation for regulatory requirements (See [US-CO-001](user_stories.md#us-co-001-compliance-documentation), [US-CO-002](user_stories.md#us-co-002-sensitive-data-identification))
- **Change Management**: Tracking and comparing schema evolution over time (See [US-DBA-007](user_stories.md#us-dba-007-schema-comparison-reports), [US-DEV-001](user_stories.md#us-dev-001-schema-version-tracking))
- **Data Discovery**: Helping analysts understand available data sources (See [US-DA-001](user_stories.md#us-da-001-data-dictionary-generation))

### 4.3 Feature Priority Matrix

Aligned with business requirements priority:

| Priority   | Features                                      | Justification                                                                                                                           |
|------------|-----------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------|
| **High**   | F000–F007, F014, F015, F021, F022, F023       | Core functionality: dual-binary architecture, database survey, portable output, offline mode, pluggable engines, throttling/compression |
| **Medium** | F016–F019, F013                               | Processing features: SQL reconstruction, report/diagram modes, Pro features, data sampling with privacy controls                        |
| **Low**    | F020, advanced diagrams, anonymized redaction | HTML output (Pro), advanced visualizations, enhanced privacy features                                                                   |

### 4.4 Performance Requirements (from Business Document)

- **CLI startup in < 100ms**
- **Collector must complete < 10s for DBs with < 1000 tables**
- **Output files should remain < 10MB when possible**
- **Postprocessor should operate in < 500ms on small/medium DBs**
- **Memory Usage**: Efficient processing for typical workloads
- **File Formats**: .dbsurveyor.json (JSON), .dbsurveyor.json.zst (compressed), .dbsurveyor.enc (encrypted)
- **Cross-Platform Performance**: Consistent performance across Linux, macOS, and Windows

## 5. User Interface Requirements

### 5.1 CLI Flags and Subcommands

#### Primary Commands (dbsurveyor-collect)

```bash
dbsurveyor-collect [OPTIONS] --database-url <URL>    # Metadata collection
```

#### Primary Commands (dbsurveyor)

```bash
dbsurveyor report [OPTIONS] --input <FILE>         # Generate comprehensive reports
dbsurveyor reconstruct [OPTIONS] --input <FILE>   # Reconstruct database schemas
dbsurveyor diagram [OPTIONS] --input <FILE>       # Generate ERD and visual diagrams
dbsurveyor classify [OPTIONS] --input <FILE>      # Data classification (Pro feature)
dbsurveyor html [OPTIONS] --input <FILE>          # Interactive HTML export (Pro feature)
```

#### Core Collection Flags

- `--out, -o <PATH>`: Output file path for collected metadata
- `--sample <N>`: Number of sample rows to extract per table (default: 100)
- `--throttle <MS>`: Throttle delay between operations in milliseconds
- `--compress`: Enable zstd compression for output (.dbsurveyor.json.zst)
- `--encrypt`: Enable AES-GCM encryption for output (.dbsurveyor.enc)
- `--database-url <URL>`: Database connection string
- `--config, -c <FILE>`: Configuration file path
- `--include <PATTERN>`: Include objects matching pattern
- `--exclude <PATTERN>`: Exclude objects matching pattern
- `--incremental`: Perform incremental collection
- `--no-data`: Skip data sampling entirely

#### Global Flags (Both Binaries)

- `--verbose, -v`: Increase output verbosity (can be used multiple times)
- `--quiet, -q`: Suppress non-essential output
- `--help, -h`: Display help information
- `--version, -V`: Display version information

### 5.2 Output Modes

- **JSON**: Machine-readable structured output
- **YAML**: Human-readable structured output
- **HTML**: Interactive web documentation
- **PDF**: Printable documentation format
- **Markdown**: Version-control-friendly documentation

### 5.3 Accessibility Notes

- All CLI output respects `NO_COLOR` environment variable
- Progress indicators work with screen readers
- Clear, descriptive error messages with actionable suggestions
- Comprehensive help system with examples

## 6. Technical Specifications

### 6.1 Language and Runtime

- **Language**: Rust (version 1.89.0, minimum supported version 1.77)
- **Runtime**: Native compiled binaries with no external runtime dependencies
- **Architecture**: Cross-platform support (x86_64, aarch64)

### 6.2 Core Crates and Dependencies

- **CLI Framework**: clap v4+ for argument parsing and subcommands
- **Async Runtime**: tokio for async I/O and task management
- **Parallel Processing**: rayon for CPU-intensive operations
- **Serialization**: serde ecosystem (serde, serde_json) for data structures
- **Database Connectivity**:
  - sqlx (PostgreSQL, MySQL, SQLite support with async drivers)
  - tiberius (SQL Server native driver)
  - mongodb (MongoDB official Rust driver)
- **Compression**: zstd for efficient data compression
- **Encryption**: aes-gcm and ring for secure file encryption with random nonces
- **Template Engines**: askama and/or tera for documentation generation
- **Markdown Processing**: comrak for Markdown parsing and rendering
- **Logging**: tracing ecosystem for structured logging and diagnostics

### 6.3 Build and Release

- **Build System**: cargo-dist for cross-platform distribution
- **Cross-Compilation**: cross or cargo-zigbuild for target platform coverage
- **MSRV**: Minimum Supported Rust Version 1.77+
- **CI/CD Pipeline**: GitHub Actions with comprehensive security scanning
- **Quality Gates**:
  - `cargo clippy -- -D warnings` (strict linting enforcement)
  - `cargo fmt --check` (formatting validation)
  - cargo-nextest for enhanced testing experience
- **Release Automation**:
  - Semantic versioning with Release Please
  - Signed releases with Cosign/SLSA attestation
  - Automated dependency updates via Renovate
- **Security Scanning**:
  - CodeQL for static analysis
  - Syft for SBOM generation
  - Grype for vulnerability scanning
  - FOSSA for license compliance
- **Code Coverage**: Codecov integration with coverage reporting
- **Task Automation**: just recipes for consistent development workflows

### 6.4 Feature Flags

```toml
[features]
default = ["postgresql", "mysql", "sqlite"]
postgresql = ["sqlx/postgres"]
mysql = ["sqlx/mysql"]
sqlite = ["sqlx/sqlite"]
mssql = ["sqlx/mssql"]
oracle = ["oracle-driver"]
full = ["postgresql", "mysql", "sqlite", "mssql", "oracle"]
```

### 6.5 Output Format Schema

Standardized JSON schema for metadata exchange between collector and postprocessor binaries, including versioning for backward compatibility.

### 6.6 CI/CD & Testing

Following Pipeline Standard with just recipes:

- `just test`: Run all tests including unit and integration
- `just lint`: Run cargo clippy with strict warnings (`-- -D warnings`)
- `just format`: Run cargo fmt for consistent code styling
- `just build-release`: Cross-platform release builds
- `just package`: Create distribution packages

## 7. Security Requirements

### 7.1 Offline-Only Operation

- No internet connectivity required after installation
- All processing occurs locally
- No external API calls or service dependencies

### 7.2 No Telemetry

- Zero data collection or usage tracking
- No external reporting mechanisms
- No automatic update checks or notifications

### 7.3 Encryption and Data Security

- **AES-GCM Encryption**: Industry-standard authenticated encryption for output files
  - Random nonce generation for each encryption operation
  - Embedded Key Derivation Function (KDF) parameters in encrypted files
  - Authenticated headers to prevent tampering and ensure data integrity
  - 256-bit keys derived from user-provided passwords using PBKDF2 or Argon2
- **No Credentials in Outputs**: Absolute prohibition of database credentials in any output files
- **Pattern-Based Redaction**: Configurable sensitive data patterns (SSNs, credit cards, emails)
- **Environment Variable Support**: Secure credential sourcing from environment variables

### 7.4 File Security and Permissions

- **Secure File Permissions**: Restrictive file permissions (0600) for sensitive outputs
- **Encrypted Output Files**: Optional .dbsurveyor.enc format with authenticated encryption
- **Audit Logging**: Comprehensive logging of all file operations and access attempts
- **Temporary File Cleanup**: Secure deletion of temporary files and intermediate data

### 7.5 Runtime Security

- **Memory Protection**: Secure memory handling for sensitive data with zeroing on deallocation
- **Error Message Sanitization**: No sensitive information leaked in error messages or stack traces
- **Credential Exclusion**: Database credentials never written to log files or debug output
- **Network Isolation**: No network calls after initial database connection (offline-first)

## 8. System Architecture

### 8.1 Two-Binary Architecture

The dbsurveyor system consists of two independent executables that work together through structured file interchange:

#### dbsurveyor-collect (Collector Binary)

- **Purpose**: Database connectivity, metadata extraction, and structured data collection
- **Responsibilities**:
  - Database connection management across supported engines
  - Comprehensive metadata extraction via database-specific adapters
  - Data profiling and statistics collection
  - Progress monitoring and error recovery
  - Structured output generation (.dbsurveyor.json, .dbsurveyor.json.zst, .dbsurveyor.enc)
- **Engine Support**: PostgreSQL, MySQL, SQLite, SQL Server, MongoDB
- **Adapter Architecture**: Rust traits for consistent database abstraction

#### dbsurveyor (Postprocessor Binary)

- **Purpose**: Documentation generation, analysis, and reporting from collected metadata
- **Responsibilities**:
  - Input validation and parsing of collector output
  - Template-driven documentation generation (HTML, Markdown)
  - Entity Relationship Diagram (ERD) generation
  - Schema comparison and analysis
  - Multi-format output rendering
- **Pro Features** (behind feature flags):
  - Advanced Mermaid/D2 diagram generation
  - Data classification heuristics
  - HTML export with interactive features

### 8.2 Database Engine Support

Supported database engines are implemented via Rust traits with feature flags for modular compilation:

```toml
[features]
default = ["postgresql", "mysql", "sqlite"]
postgresql = ["sqlx/postgres"]
mysql = ["sqlx/mysql"]
sqlite = ["sqlx/sqlite"]
mssql = ["tiberius"]
mongodb = ["mongodb"]
pro = ["mermaid", "classification", "html-export"]
tui = ["ratatui"]                                  # Optional TUI preview (out-of-scope for full GUI)
```

### 8.3 Plugin Architecture (Optional)

- **WASM Plugins**: Optional WebAssembly plugin support for custom analyzers
- **Stdio JSON Plugins**: External process communication via JSON over stdin/stdout
- **Note**: Plugin architecture is optional and may be implemented in future versions

### 8.4 Data Flow and Output Formats

#### Collection Phase

1. **Database Connection**: Connect using sqlx, tiberius, or mongodb drivers
2. **Metadata Extraction**: Comprehensive schema and statistics collection
3. **Output Generation**: Structured files with "format_version": "1.0"
   - `.dbsurveyor.json`: Uncompressed JSON metadata
   - `.dbsurveyor.json.zst`: Zstandard compressed JSON
   - `.dbsurveyor.enc`: AES-GCM encrypted JSON with embedded KDF parameters

#### Processing Phase

1. **Input Validation**: Parse and validate collector output format
2. **Template Processing**: Apply Askama/Tera templates for documentation
3. **Analysis**: Generate insights, diagrams, and reports
4. **Output Rendering**: Multi-format documentation generation

### 8.5 Cross-Platform Support

**Target Platforms**:

- macOS: x86_64, aarch64 (Apple Silicon)
- Linux: x86_64, aarch64
- Windows: x86_64

**Driver Matrix**: Clear error messages when database drivers are disabled via feature flags

### 8.6 Deployment Architecture

- **Standalone Binaries**: Independent executables with minimal dependencies
- **Configuration-Driven**: TOML/YAML configuration files
- **File-Based Communication**: Structured JSON interchange between binaries
- **Container Support**: Optional Docker images for containerized deployment

## 9. Compliance with EvilBit Labs Standards

### 9.1 Standard Compliance Table

| Standard                    | Requirement            | Compliance Status | Implementation Notes                      |
|-----------------------------|------------------------|-------------------|-------------------------------------------|
| **Pipeline Standard**       | GitHub Actions CI/CD   | ✅ Compliant       | Using just recipes for task automation    |
| **Security Standard**       | No telemetry           | ✅ Compliant       | Zero external communication               |
| **Security Standard**       | Credential protection  | ✅ Compliant       | AES-GCM encryption, no plaintext storage  |
| **Documentation Standard**  | User documentation     | ✅ Compliant       | Comprehensive CLI help and user guide     |
| **Documentation Standard**  | API documentation      | ✅ Compliant       | Code documentation and schema definitions |
| **Testing Standard**        | Unit test coverage     | ✅ Compliant       | Minimum 80% code coverage target          |
| **Testing Standard**        | Integration testing    | ✅ Compliant       | Database integration test suite           |
| **Release Standard**        | Semantic versioning    | ✅ Compliant       | Following SemVer specification            |
| **Release Standard**        | Signed releases        | ✅ Compliant       | GPG-signed release artifacts              |
| **Offline Standard**        | Air-gap operation      | ✅ Compliant       | Complete offline functionality            |
| **Cross-Platform Standard** | Multi-platform support | ✅ Compliant       | Linux, macOS, Windows binaries            |

### 9.2 Standard Deviations

None identified. This project fully complies with all applicable EvilBit Labs standards.

### 9.3 Standard Compliance Verification

- **Automated Checks**: CI pipeline validates compliance requirements
- **Manual Review**: Regular compliance audits during development
- **Documentation Updates**: Standards compliance updated with each release

---
*This document follows the EvilBit Requirements Standard v2.1. For questions or clarifications, contact the author or project maintainers.*
