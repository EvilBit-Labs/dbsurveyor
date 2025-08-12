# User Stories

This document outlines user stories organized by persona for the database surveying tool, with emphasis on operational security, non-destructive behavior, and offline processing capabilities.

**Related Documents**: See [requirements.md](requirements.md) for functional requirements (F000-F016) and [tasks.md](tasks.md) for detailed implementation tasks.

**Terminology Standards**:

- **Collector Binary**: `dbsurveyor-collect` (not `db-collector`)
- **Postprocessor Binary**: `dbsurveyor` (not `db-postprocessor`)
- **Output Files**: `.dbsurveyor.json`, `.dbsurveyor.json.zst`, `.dbsurveyor.enc`
- **Format Version**: `"format_version": "1.0"`
- **Pro Features**: Advanced capabilities requiring license validation

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

## Red Team Operator

### Story 1: Covert Database Collection

**Requirements Mapping**: F001, F003, F004, F005, F015
**Related Tasks**: [Task 1.1](tasks.md#task-11-database-engine-adapters), [Task 1.3](tasks.md#task-13-output-writer-with-multiple-formats)

**As a** Red Team Operator
**I want** to use compromised credentials to survey target databases and export findings to encrypted files
**So that** I can gather intelligence on database schemas without leaving traces or causing disruption

**Acceptance Criteria:**

- [ ] CLI accepts database credentials via environment variables (no command-line exposure)
- [ ] Supports connection throttling with `--rate-limit` flag (queries per second)
- [ ] Exports data to `.dbsurveyor.json`, `.dbsurveyor.json.zst`, or `.dbsurveyor.enc`
- [ ] `--encrypt` flag encrypts output with provided public key
- [ ] `--compress` flag compresses output using zstd
- [ ] Connection timeout configurable via `--timeout` (default: 30s)
- [ ] Maximum concurrent connections limited via `--max-connections` (default: 1)
- [ ] All operations are read-only (SELECT, SHOW, DESCRIBE only)
- [ ] No writes, creates, updates, or deletes performed
- [ ] Graceful handling of connection failures without retry storms

**OPSEC Notes:**

- No telemetry or external network calls
- Minimal logging to avoid detection
- Connection pooling disabled to reduce resource footprint
- Randomized query intervals when rate-limiting active

### Story 2: Offline Intelligence Processing

**As a** Red Team Operator
**I want** to process exported database surveys offline to generate actionable intelligence
**So that** I can analyze findings in a secure environment without network exposure

**Acceptance Criteria:**

- [ ] `dbsurveyor analyze --input survey.json.enc` processes encrypted exports
- [ ] Generates markdown report with schema overview and sensitive data highlights
- [ ] `--diff` flag compares multiple survey files to identify schema changes
- [ ] Identifies potential privilege escalation opportunities (admin tables, stored procedures)
- [ ] Highlights tables likely containing credentials or sensitive data
- [ ] Export includes metadata: collection timestamp, database version, connection details
- [ ] Processing works completely offline with no network dependencies
- [ ] Supports batch processing of multiple survey files

**OPSEC Notes:**

- All processing occurs locally
- No external API calls or data uploads
- Sensitive findings clearly marked in reports
- Option to redact connection details from reports

## Blue Team Analyst

### Story 3: Legacy Database Compliance Audit

**As a** Blue Team Analyst
**I want** to audit legacy databases for PII/PCI compliance violations
**So that** I can generate compliance reports and remediation recommendations

**Acceptance Criteria:**

- [ ] `--compliance-mode` flag enables PII/PCI detection algorithms
- [ ] Classifies fields based on naming patterns (SSN, credit card, email, etc.)
- [ ] Samples data values to confirm classification (configurable sample size)
- [ ] `--redact-samples` flag masks sample values in output
- [ ] Generates compliance report with violation severity levels
- [ ] Supports custom compliance rules via `--rules-file compliance.yaml`
- [ ] Identifies unencrypted sensitive fields
- [ ] Reports on table/field access permissions
- [ ] Export includes compliance score and remediation priorities

**OPSEC Notes:**

- Sample data collection is minimal and configurable
- All sensitive samples redacted by default in reports
- No data transmitted outside the organization
- Audit trail of all accessed tables and fields

### Story 4: SQL Schema Reconstruction (Pro Feature)

**As a** Blue Team Analyst
**I want** to generate complete SQL CREATE statements and ER diagrams from surveyed schemas
**So that** I can document legacy systems and plan migration strategies

**Acceptance Criteria:**

- [ ] `dbsurveyor reconstruct --input survey.json` generates SQL DDL
- [ ] Outputs complete CREATE TABLE statements with constraints
- [ ] Generates CREATE INDEX statements for all discovered indexes
- [ ] `--format` supports multiple outputs: SQL, PlantUML, Mermaid, GraphViz
- [ ] `--diagram` flag generates visual ER diagrams (SVG/PNG)
- [ ] Preserves foreign key relationships and constraints
- [ ] Includes view definitions and stored procedure signatures
- [ ] Supports database-specific SQL dialects (MySQL, PostgreSQL, SQL Server, Oracle)
- [ ] Generates documentation with table descriptions and field metadata

**Pro Feature Notes:**

- Requires license validation (offline license file)
- Advanced diagram generation with relationship visualization
- Export to multiple architectural documentation formats

## Developer (Inherited Environment)

### Story 5: Unknown Schema Discovery

**As a** Developer inheriting an unknown system
**I want** to quickly survey and document database schemas
**So that** I can understand the data model and accelerate development

**Acceptance Criteria:**

- [ ] `dbsurveyor discover --database mydb` provides comprehensive schema overview
- [ ] Generates human-readable documentation in markdown format
- [ ] Identifies table relationships and foreign key constraints
- [ ] `--sample-queries` generates example SELECT statements for each table
- [ ] Produces data dictionary with field types and constraints
- [ ] `--onboarding` flag creates developer-friendly documentation
- [ ] Estimates table sizes and row counts
- [ ] Identifies potentially important tables (users, auth, config)
- [ ] Export includes connection examples and query templates

**Developer-Focused Features:**

- Integration examples for common ORMs (SQLAlchemy, Django, Entity Framework)
- API endpoint suggestions based on table structures
- Mock data generation templates
- Database seeding script examples

### Story 6: Portable Documentation Artifacts

**As a** Developer
**I want** to generate portable documentation artifacts that can be shared with my team
**So that** we can collaborate effectively on database-driven applications

**Acceptance Criteria:**

- [ ] `--format html` generates standalone HTML documentation
- [ ] `--bundle` creates self-contained documentation package (HTML + assets)
- [ ] Searchable documentation with field and table search
- [ ] Interactive schema browser (JavaScript-based, works offline)
- [ ] `--export-models` generates model classes for popular frameworks
- [ ] Documentation includes connection instructions and examples
- [ ] Shareable via file system, no web server required
- [ ] Version control friendly (deterministic output, meaningful diffs)

## System Administrator

### Story 7: Multi-Instance Schema Validation

**As a** System Administrator
**I want** to validate schema consistency across multiple database instances
**So that** I can ensure environment parity and identify configuration drift

**Acceptance Criteria:**

- [ ] `dbsurveyor validate --instances prod,staging,dev` compares multiple databases
- [ ] Identifies schema differences between environments
- [ ] Reports missing tables, fields, indexes, and constraints
- [ ] `--drift-report` generates detailed diff report with remediation steps
- [ ] Validates data types, field lengths, and constraint consistency
- [ ] Checks stored procedures and view definitions for differences
- [ ] `--ignore-data` flag focuses on schema structure only
- [ ] Configurable tolerance levels for acceptable differences
- [ ] Supports connection to multiple database types simultaneously

**Infrastructure Focus:**

- Connection pooling for efficient multi-instance scanning
- Parallel processing of multiple database connections
- Detailed logging for troubleshooting connection issues
- Integration with infrastructure monitoring tools

### Story 8: Standardized Reporting

**As a** System Administrator
**I want** to generate standardized reports across all database instances
**So that** I can maintain consistent documentation and monitoring

**Acceptance Criteria:**

- [ ] `--template` flag applies consistent report formatting
- [ ] `--metrics` includes performance and sizing information
- [ ] Generates inventory reports with version and configuration details
- [ ] `--schedule` supports automated periodic surveys (via cron integration)
- [ ] Reports include health checks (connectivity, permissions, version)
- [ ] `--format csv` for integration with spreadsheet tools
- [ ] `--format json` for integration with monitoring systems
- [ ] Standardized field naming across all database types
- [ ] Historical tracking of schema changes over time

**Operational Features:**

- Integration with configuration management tools
- Alert generation for critical schema changes
- Backup verification (ensuring schema matches backup contents)
- Compliance reporting for audit requirements

## Cross-Cutting Stories

### Story 9: Secure Credential Management

**As any persona**
**I want** to securely manage database credentials
**So that** I can survey databases without exposing sensitive authentication information

**Acceptance Criteria:**

- [ ] Supports credential files (JSON, YAML) with restricted permissions
- [ ] Environment variable support for all connection parameters
- [ ] `--credential-store` integration with system keystores
- [ ] `--vault-integration` for HashiCorp Vault, AWS Secrets Manager
- [ ] No credentials logged or stored in temporary files
- [ ] Connection string parsing with credential extraction
- [ ] Support for certificate-based authentication
- [ ] Kerberos and LDAP authentication support where available

### Story 10: Air-Gapped Operation

**As any persona**
**I want** to operate the tool in completely air-gapped environments
**So that** I can survey databases in secure, isolated networks

**Acceptance Criteria:**

- [ ] No external dependencies or network calls during operation
- [ ] Self-contained binary with embedded database drivers
- [ ] Offline license validation (for Pro features)
- [ ] Local help system with full documentation
- [ ] Export/import of configuration and templates
- [ ] Portable license files for air-gapped license management
- [ ] Local update mechanism via file transfer
- [ ] Complete functionality without internet connectivity

**Security Guarantees:**

- No telemetry or usage analytics
- No automatic update checks
- No external API dependencies
- All processing occurs locally
- Configurable logging levels (including complete silence)
- Secure deletion of temporary files
- Memory-safe operation (no credential exposure in memory dumps)
