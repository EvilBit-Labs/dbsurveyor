# DBSurveyor Epic Brief - Secure Database Schema Collection

## Summary

DBSurveyor provides security-first database schema collection and documentation for professionals operating in air-gapped, contested, or security-critical environments. The tool addresses the fundamental gap in existing database documentation solutions: they require internet connectivity, send telemetry, expose credentials in outputs, or lack the security guarantees needed for sensitive operations. DBSurveyor implements a dual-binary architecture (collector + postprocessor) that enables secure schema collection with zero external dependencies, comprehensive credential protection, and offline-capable documentation generation. The v1.0 release focuses on core SQL database coverage (PostgreSQL, MySQL, SQLite) with essential security features, intelligent data sampling, and production-ready quality standards achievable by a solo developer.

## Context & Problem

### Who's Affected

**Primary Users:**
- **Red Team Operators**: Need covert database intelligence gathering with rate limiting, encryption, and minimal detection footprint
- **Database Administrators**: Require comprehensive schema documentation across multiple database instances without security compromises
- **Security Analysts**: Must operate in air-gapped environments with zero telemetry and complete offline capability
- **System Administrators**: Need reliable schema backup, replication, and disaster recovery documentation

**Secondary Users:**
- **Compliance Officers**: Require PII/PCI detection and audit-ready reporting (lower priority for v1.0)
- **Developers**: Need to understand inherited database systems quickly (lower priority for v1.0)

### Current Pain Points

**Security Gaps in Existing Tools:**
- Commercial database tools require internet connectivity and send usage telemetry
- Open-source alternatives expose database credentials in error messages and logs
- No existing tools provide encryption-at-rest for collected schema metadata
- Most tools cannot operate in air-gapped or disconnected environments
- Credential handling is insecure, with passwords appearing in process lists and outputs

**Operational Limitations:**
- Database documentation tools require write access or schema modifications
- No unified interface across different database engines (PostgreSQL, MySQL, SQLite, MongoDB, etc.)
- Existing tools lack intelligent data sampling with configurable rate limiting
- Multi-database enumeration requires manual scripting and custom tooling
- Documentation generation requires external services or internet-connected tools

**Quality and Trust Issues:**
- No comprehensive testing with real database instances (testcontainers)
- Security guarantees are not validated or documented
- Offline operation is not a first-class design principle
- No clear separation between data collection and data processing (privacy concerns)

### Where in the Product

This Epic addresses the **entire product foundation** for DBSurveyor, establishing:

1. **Core Architecture**: Dual-binary design separating collection (dbsurveyor-collect) from processing (dbsurveyor)
2. **Security Foundation**: Credential protection, AES-GCM encryption, Argon2id key derivation, zero telemetry
3. **Database Adapters**: Unified interface for PostgreSQL, MySQL, and SQLite with feature parity
4. **Data Collection**: Intelligent sampling strategies, multi-database enumeration, rate limiting
5. **Documentation Generation**: Offline-capable Markdown and SQL reconstruction from collected metadata
6. **Quality Assurance**: 70% test coverage with testcontainers, security-focused testing, comprehensive documentation

### Success Criteria for v1.0

**Must Have:**
- PostgreSQL adapter with 100% feature completion (including multi-database collection)
- MySQL and SQLite adapters with core schema collection
- Multi-database selection via **flag-driven filters** with glob support (pg_dump-like include/exclude)
- Partial-failure behavior that preserves usable outputs **and** records machine-actionable failure metadata
- Automation-friendly exit codes (default success on partial success; optional strict mode to fail on any partial failure)
- Postprocessor generating offline Markdown documentation and SQL DDL reconstruction
- AES-GCM encryption with Argon2id key derivation for secure outputs
- Zero telemetry and complete offline operation
- 70% test coverage with testcontainers integration
- Comprehensive rustdoc and mdbook documentation

**Deferred to Post-v1.0:**
- MongoDB, SQL Server, Oracle adapters
- Plugin architecture for extensibility
- Pro-tier features (visual diagrams, HTML reports, advanced PII detection)
- Advanced data quality metrics and anomaly detection

### Phased Delivery Strategy

**Phase 1 (v0.5)**: Complete PostgreSQL foundation (2-3 months)
**Phase 2 (v1.0)**: Add MySQL and SQLite for core SQL coverage (5-6 months total)
**Phase 3 (v1.5)**: MongoDB for NoSQL expansion (7-9 months total)
**Phase 4 (v2.0)**: Enterprise databases and plugin architecture (10-13 months total)
