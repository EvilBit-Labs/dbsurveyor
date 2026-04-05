# Epic Brief: DBSurveyor — Database Schema Collection & Documentation

## Summary

DBSurveyor is a dual-binary Rust toolchain for security operators, database administrators, and developers who need to document, analyze, and reconstruct database schemas — particularly in environments where internet connectivity cannot be assumed. The **collector** (`dbsurveyor-collect`) connects to a target database, extracts comprehensive schema metadata and optional data samples, and writes a structured, validated output file. The **postprocessor** (`dbsurveyor`) consumes that file entirely offline to produce human-readable reports, visual diagrams, and reconstructable DDL. The two binaries communicate only through files, enabling use in air-gapped and contested environments. The system is built by operators, for operators: no telemetry, no cloud dependencies, no dark patterns.

## Context & Problem

### Who Is Affected

| Persona                           | Pain                                                                                                |
| --------------------------------- | --------------------------------------------------------------------------------------------------- |
| **Red team operator**             | Needs to enumerate database schemas covertly, without triggering detection or touching the internet |
| **Database administrator**        | Inherits undocumented systems and needs to produce accurate schema documentation quickly            |
| **Security / compliance analyst** | Must identify PII/PCI risk fields across unfamiliar schemas without exposing data externally        |
| **Developer**                     | Needs to understand a new database or replicate a schema in a new environment                       |

### The Problem

Existing database documentation tools assume connectivity (cloud dashboards, SaaS schema browsers), require installation of heavy runtimes, or produce outputs that contain credentials and sensitive metadata. There is no purpose-built, self-contained CLI tool that:

1. Works fully offline after installation
2. Operates read-only with zero write risk
3. Keeps credentials out of every output, log, and error message
4. Separates collection (requires DB access) from analysis (requires no network) so the two steps can run in different environments
5. Supports both relational (PostgreSQL, MySQL, SQLite, SQL Server) and NoSQL (MongoDB) databases through a unified output format

### Where in the Product

This Epic covers the entire DBSurveyor system — it is the product. The work spans: the core data model and shared library (`dbsurveyor-core`), the collector binary (`dbsurveyor-collect`) with database-specific adapters, and the postprocessor binary (`dbsurveyor`) for offline report generation.

## Core Principles (Non-Negotiable)

- **Offline-first**: The postprocessor makes zero network calls. The collector contacts only the target database.
- **Credential protection**: Passwords never appear in output files, logs, or error messages. Memory is zeroized after use.
- **Read-only**: No write operations to any database, ever.
- **Data integrity in collection**: The collector stores samples exactly as retrieved — no redaction, no transformation. Redaction is exclusively a postprocessor concern.
- **Unified output format**: All adapters produce `.dbsurveyor.json` (optionally compressed to `.zst` or encrypted to `.enc`), validated against a versioned JSON Schema.

## Milestone Scope Decision

For the current milestone, **Markdown output is the only postprocessor format required to be production-complete**. Non-Markdown outputs (HTML, SQL DDL, Mermaid, and JSON analysis) are treated as staged/experimental and must be hidden from standard builds; they are exposed only in explicitly experimental builds.

## Success Criteria (Product Validation)

### Scenario-based criteria

1. An operator can complete end-to-end collection and offline reporting: collect schema from a supported database, transfer file, and generate a Markdown report without internet connectivity.
2. In multi-database collection, run outcomes are explicitly communicated via return-code categories: full success, total failure, and three partial-success categories: **partial-success-with-data**, **partial-success-without-samples**, and **partial-success-with-validation-warnings**. Per-database outcomes are captured in output metadata.
3. Any skipped database in multi-database mode (including privilege-based skips) forces a partial-success outcome; full success is allowed only when no databases are skipped and no partial/failure conditions apply.
4. When sampling cannot determine reliable ordering, collection falls back to random sampling with an explicit warning.
5. When a schema is omitted for sampling, table resolution and ordering detection follow runtime `search_path` semantics rather than assuming `public`.
6. When per-table sampling fails, the system retries once with a smaller sample size, then skips that table’s sample with warning while continuing collection.
7. `--sample 0` is a valid input and means “use default sample size.”
8. In standard builds, operators are offered only production-supported output modes. Staged outputs are available only in experimental builds and are clearly treated as non-production.
9. If encryption password confirmation mismatches during collection, the run is treated as a canceled operation with a distinct non-zero exit code and no output file written.

### Minimal capability checklist

- Collector supports plain, compressed, and encrypted schema file generation.
- Postprocessor can validate and load plain/compressed/encrypted inputs offline.
- Redaction applies to sample values only and does not alter schema metadata fields.
- Redaction modes follow a minimum progressive contract: for the same sample set, each stricter mode produces equal or greater masking than the previous mode.
- Markdown output is production-complete and includes comprehensive schema documentation: relationships, indexes, constraints, and sampled data with redaction applied.
- Requirement and flow language is testable, unambiguous, and aligned to operator value (security, offline operation, controlled failure behavior).
