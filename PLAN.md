# Implementation Plan: Database Schema Collection - Next Phase

## Context

This plan covers the next implementation steps for the DBSurveyor database schema collection feature, based on the spec at `.kiro/specs/database-schema-collection/`.

### Current State Summary

**Completed (Tasks 1-5.2, 11-13, 21):**
- AES-GCM encryption + Argon2id KDF (Task 1)
- PostgreSQL adapter: connection pooling, schema collection, FK mapping, views/routines/triggers, type mapping (Tasks 2.1-2.5, 2.8)
- JSON Schema specification design (Task 2.6)
- Connection pool configuration with builder pattern, env vars, validation (Tasks 5.1-5.2)
- Core workspace structure, traits, data models, CLI frameworks (Tasks 3-4)
- Output encryption/compression structure (Task 11)
- Collector & postprocessor CLI scaffolding (Tasks 12-13)
- Distribution/release automation config (Task 21)

**In Progress:**
- Task 5.3: Test coverage to 70% threshold (partially started)

**Not Started (remaining tasks):**
- Task 6: Intelligent data sampling for PostgreSQL (ordering strategy, rate limiting, sensitive data detection)
- Task 7: Multi-database collection orchestration for PostgreSQL
- Tasks 8-10: MySQL, SQLite, MongoDB adapters (dbsurveyor-collect wrappers)
- Task 14: Data redaction in postprocessor
- Task 15: SQL reconstruction and Markdown report generation
- Tasks 16-18: Pro features, plugin architecture, specialized binaries
- Tasks 19-20: Testing framework and CI expansion
- Tasks 22-25: Security testing, CLI snapshots, benchmarks, documentation

### Branch State

Branch `task_cleanup` has uncommitted changes:
- `dbsurveyor-collect/src/adapters/mod.rs` (+372 lines - ConnectionConfig builder/validation)
- `dbsurveyor-collect/src/adapters/postgresql.rs` (+9 lines - validation enforcement)
- `dbsurveyor-collect/tests/connection_pooling_tests.rs` (new - 1,099 lines of tests)
- `TASK_5.1_IMPLEMENTATION_SUMMARY.md` and `TASK_5.2_IMPLEMENTATION_SUMMARY.md` (new)

### Architecture Note

There are two adapter layers:
1. **dbsurveyor-core** adapters (postgres/, mysql/, sqlite/, mongodb/) - Full implementations with sampling, multi-DB, type mapping
2. **dbsurveyor-collect** adapters (postgresql.rs, mysql.rs, etc.) - Simpler wrappers using SchemaCollector trait + ConnectionConfig

The core adapters are mature and production-ready. The collect-level adapters are thinner wrappers that delegate to core.

---

## Phase 1: Commit Pending Work and Clean Up (Prerequisite)

### Step 1.1: Commit Task 5.1 + 5.2 changes

**Files:** `dbsurveyor-collect/src/adapters/mod.rs`, `dbsurveyor-collect/src/adapters/postgresql.rs`, `dbsurveyor-collect/tests/connection_pooling_tests.rs`
**Action:** Stage and commit the connection pooling configuration and tests. Remove `TASK_5.1_IMPLEMENTATION_SUMMARY.md` and `TASK_5.2_IMPLEMENTATION_SUMMARY.md` (implementation summaries are not needed in the repo).
**Verification:** `cargo clippy -- -D warnings` and `cargo test --lib` pass (unit tests only, no testcontainers needed).

### Step 1.2: Update tasks.md to reflect accurate status

**File:** `.kiro/specs/database-schema-collection/tasks.md`
**Action:** Mark Tasks 5.1 and 5.2 as `[x]` completed. Ensure all statuses are accurate.

---

## Phase 2: Test Coverage Improvement (Task 5.3)

### Step 2.1: Audit current test coverage

**Action:** Run `cargo llvm-cov --workspace --lib` (or `cargo tarpaulin` if llvm-cov is unavailable) to determine current coverage percentages per crate. Identify the largest coverage gaps.
**Output:** List of untested modules/functions ranked by impact.

### Step 2.2: Add unit tests for dbsurveyor-core models and validation

**Files:** `dbsurveyor-core/src/models.rs`, `dbsurveyor-core/src/validation.rs`
**Action:** Add unit tests for:
- `DatabaseSchema` serialization/deserialization roundtrips
- `UnifiedDataType` mapping edge cases (all variants)
- `CollectionMetadata` construction and validation
- Format version validation
- `DatabaseInfo` with various `AccessLevel` and `CollectionStatus` combinations
**Requirements:** Req 1.3, 9.1

### Step 2.3: Add unit tests for dbsurveyor-core error handling and security

**Files:** `dbsurveyor-core/src/error.rs`, security modules
**Action:** Test credential sanitization in error messages, URL redaction patterns, error chaining. Verify no connection strings leak through any error path.
**Requirements:** Req 2.4

### Step 2.4: Add unit tests for dbsurveyor-core adapter config modules

**Files:** `dbsurveyor-core/src/adapters/config/`
**Action:** Test `ConnectionConfig`, `SamplingConfig`, `CollectionConfig` construction, defaults, and validation. Cover edge cases for each config struct.
**Requirements:** Req 1.1, 8.1

---

## Phase 3: Data Sampling Integration (Task 6)

> Note: The core sampling implementation already exists in `dbsurveyor-core/src/adapters/postgres/sampling.rs` (563 lines). This phase wires it into the dbsurveyor-collect adapter layer and ensures end-to-end functionality.

### Step 3.1: Implement ordering strategy detection in collect adapter (Task 6.1)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs`
**Action:** Add a `detect_ordering` method that delegates to `dbsurveyor_core::adapters::postgres::sampling::detect_ordering_strategy()`. Expose it through a new `DataSampler` trait or extend `SchemaCollector` with an optional `sample_tables` method.
**Tests:** Unit test strategy detection for tables with PKs, timestamps, auto-increment, and no ordering.
**Requirements:** Req 11.1-11.3

### Step 3.2: Create configurable sampling infrastructure (Task 6.2)

**Files:** `dbsurveyor-collect/src/adapters/mod.rs` (add `SamplingConfig`), `dbsurveyor-collect/src/adapters/postgresql.rs`
**Action:** Add `SamplingConfig` to the collect-level adapter (or reuse from core). Add CLI flags for `--sample-size`, `--sample-rate-limit`, `--no-sample`. Wire sampling into `collect_metadata` flow.
**Tests:** Config validation, rate limit parameter bounds.
**Requirements:** Req 11.1-11.4, 8.1-8.3

### Step 3.3: Implement safe query execution with timeouts (Task 6.3)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs`
**Action:** Add `statement_timeout` configuration to the PostgreSQL pool's after-connect hook. Implement per-query timeout wrapping. Use indexed ordering for efficient sampling.
**Tests:** Integration test with testcontainers verifying statement_timeout enforcement.
**Requirements:** Req 11.1, 11.3-11.4

### Step 3.4: Add sensitive data detection and logging (Task 6.4)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs` or new `dbsurveyor-collect/src/sensitivity.rs`
**Action:** Implement column name pattern matching for PII/credential detection. Log warnings (never redact at collector level). Patterns: SSN, credit card, email, password, token, secret, etc.
**Tests:** Pattern matching unit tests against known-sensitive and false-positive column names.
**Requirements:** Req 11.5-11.6, 4.1-4.2

### Step 3.5: Integration test data sampling (Task 6.5)

**Files:** `dbsurveyor-collect/tests/` or `dbsurveyor-core/tests/`
**Action:** Test sampling end-to-end with testcontainers PostgreSQL:
- Table with PK ordering
- Table with timestamp ordering
- Table with no clear ordering (random fallback)
- Rate limiting behavior
- Various PostgreSQL data types in samples
**Requirements:** Req 11.1-11.6

---

## Phase 4: Multi-Database Collection (Task 7)

> Note: The core multi-database implementation exists in `dbsurveyor-core/src/adapters/postgres/enumeration.rs` and `multi_database.rs`. This phase wires it into the collect adapter and CLI.

### Step 4.1: Wire database enumeration into collect adapter (Task 7.1)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs`, `dbsurveyor-collect/src/adapters/mod.rs`
**Action:** Add `list_databases` method to the collect-level PostgresAdapter that delegates to core enumeration. Add `--multi-db` / `--include-system-databases` / `--exclude-db` CLI flags.
**Tests:** Unit test database filtering logic. Integration test with testcontainers.
**Requirements:** Req 12.1-12.2, 12.4-12.5

### Step 4.2: Implement per-database connection management (Task 7.2)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs`
**Action:** Add `connect_to_database` that modifies the connection URL path to target a specific database. Manage per-database connection pools with resource limits.
**Tests:** Test URL rewriting, connection to multiple databases.
**Requirements:** Req 12.1-12.3

### Step 4.3: Create server-level collection orchestration (Task 7.3)

**Files:** `dbsurveyor-collect/src/adapters/postgresql.rs`, `dbsurveyor-collect/src/main.rs`
**Action:** Implement `collect_all_databases` orchestrator at the collect level. Wire into CLI with progress reporting. Handle partial failures with continue-on-error. Output `DatabaseServerSchema` with per-database status.
**Tests:** Integration test collecting from multiple databases on same testcontainer.
**Requirements:** Req 12.1, 12.3, 12.6

---

## Phase 5: MySQL and SQLite Collect Adapters (Tasks 8-9)

### Step 5.1: Create MySQL collect adapter (Task 8)

**Files:** `dbsurveyor-collect/src/adapters/mysql.rs`
**Action:** Implement `SchemaCollector` for MySQL, delegating to `dbsurveyor_core::adapters::mysql`. Follow same patterns as PostgreSQL collect adapter. Add sampling and multi-database support.
**Tests:** Integration tests with testcontainers MySQL.
**Requirements:** Req 1.1-1.2

### Step 5.2: Create SQLite collect adapter (Task 9)

**Files:** `dbsurveyor-collect/src/adapters/sqlite.rs`
**Action:** Implement `SchemaCollector` for SQLite, delegating to `dbsurveyor_core::adapters::sqlite`. Handle file-based connection (no pooling needed). Add ROWID-based sampling.
**Tests:** Unit tests with temp file databases.
**Requirements:** Req 1.1-1.2

---

## Phase 6: MongoDB Collect Adapter (Task 10)

### Step 6.1: Create MongoDB collect adapter

**Files:** `dbsurveyor-collect/src/adapters/mongodb.rs`
**Action:** Implement `SchemaCollector` for MongoDB, delegating to `dbsurveyor_core::adapters::mongodb`. Add document schema inference, field statistics, and collection sampling.
**Tests:** Integration tests with testcontainers MongoDB.
**Requirements:** Req 1.1-1.2, 1.6

---

## Phase 7: Postprocessor - Redaction (Task 14)

### Step 7.1: Create redaction configuration infrastructure (Task 14.1)

**Files:** `dbsurveyor/src/redaction/mod.rs` (new), `dbsurveyor/src/redaction/config.rs` (new)
**Action:** Create `RedactionConfig` with modes (Conservative, Balanced, Minimal, None). Define `RedactionPattern` with regex patterns. Add CLI integration.
**Requirements:** Req 4.1-4.2, 8.6

### Step 7.2: Implement redaction logic (Task 14.2)

**Files:** `dbsurveyor/src/redaction/engine.rs` (new)
**Action:** Implement each redaction mode. Conservative: redact all PII-like patterns. Balanced: obvious PII/credentials. Minimal: clear credentials/secrets only. None: passthrough.
**Tests:** Test each mode against sample data with known PII patterns.
**Requirements:** Req 4.1-4.2, 11.5

### Step 7.3: Add pattern-based sensitive data detection (Task 14.3)

**Files:** `dbsurveyor/src/redaction/patterns.rs` (new)
**Action:** Regex patterns for SSN, credit cards, emails, phone numbers, database credentials, API keys. Field name heuristics. Custom pattern support.
**Tests:** Comprehensive pattern matching tests.
**Requirements:** Req 4.1-4.2, 8.6, 11.5

---

## Phase 8: SQL Reconstruction and Reports (Task 15)

### Step 8.1: Implement SQL DDL generation (Task 15.1-15.2)

**Files:** `dbsurveyor/src/generators/ddl.rs` (new)
**Action:** Create `SqlDialect` enum. Generate CREATE TABLE, PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK, and CREATE INDEX statements from collected metadata. Handle database-specific syntax.
**Tests:** Roundtrip tests comparing generated DDL against known schemas.
**Requirements:** Req 3.1, 3.3, 6.1

### Step 8.2: Create Markdown report generation (Task 15.3)

**Files:** `dbsurveyor/src/generators/markdown.rs` (new), `dbsurveyor/templates/` (askama templates)
**Action:** Replace placeholder report generators with askama-powered Markdown. Include table of contents, table documentation, column details, relationship sections, statistics.
**Tests:** Snapshot tests with insta for generated Markdown.
**Requirements:** Req 3.1, 3.2

### Step 8.3: Add relationship diagrams (Task 15.4)

**Files:** `dbsurveyor/src/generators/diagrams.rs` (new)
**Action:** Generate Mermaid.js ER diagrams from foreign key relationships. Configurable complexity. Per-schema diagrams for large schemas.
**Tests:** Validate generated Mermaid syntax.
**Requirements:** Req 3.4, 10.1

---

## Phase 9: Security Testing Suite (Task 22)

### Step 9.1: Credential protection tests (Task 22.1)

**Files:** `dbsurveyor-core/tests/security_credential_tests.rs` (new)
**Action:** Test that credentials never appear in logs, errors, or output. Verify password zeroization. Test all error paths for credential sanitization.
**Requirements:** Req 2.1-2.4

### Step 9.2: SQL injection resistance tests (Task 22.2)

**Files:** `dbsurveyor-core/tests/security_injection_tests.rs` (new)
**Action:** Test with malicious table/column names. Verify parameterized queries. Test special characters and Unicode in identifiers.
**Requirements:** Req 2.1-2.4

### Step 9.3: Offline operation tests (Task 22.3)

**Files:** `dbsurveyor/tests/offline_tests.rs` (new)
**Action:** Test postprocessor works without network. Verify no external API calls. Test all output generation offline.
**Requirements:** Req 2.3-2.4

### Step 9.4: Cryptographic security tests (Task 22.4)

**Files:** `dbsurveyor-core/tests/security_crypto_tests.rs` (new)
**Action:** Test nonce uniqueness, KDF parameter validation, roundtrip with various sizes, key derivation security.
**Requirements:** Req 2.7, 9.3-9.5

---

## Phase 10: CI, Documentation, and Polish (Tasks 19-20, 23-25)

### Step 10.1: Configure nextest and expand CI (Tasks 19.1, 20.1-20.5)

**Action:** Add nextest config. Set up testcontainers in CI. Configure coverage reporting. Add matrix testing.

### Step 10.2: CLI snapshot testing (Task 23)

**Action:** Add insta snapshot tests for all CLI help output and error messages.

### Step 10.3: Performance benchmarks (Task 24)

**Action:** Criterion benchmarks for schema collection, encryption, compression.

### Step 10.4: Documentation with rustdoc and mdbook (Task 25)

**Action:** Comprehensive rustdoc, mdbook user guide, practical examples, architecture docs.

---

## Execution Priority

The phases are ordered by dependency and value:
1. **Phase 1** (commit cleanup) - prerequisite, unblocks everything
2. **Phase 2** (test coverage) - establishes quality baseline
3. **Phase 3** (sampling) - high-value feature, builds on core
4. **Phase 4** (multi-DB) - high-value feature, builds on core
5. **Phase 5-6** (MySQL/SQLite/MongoDB adapters) - expands database support
6. **Phase 7-8** (postprocessor) - enables documentation generation
7. **Phase 9** (security testing) - validates security guarantees
8. **Phase 10** (CI/docs/polish) - production readiness

---

## Constraints and Reminders

- **Zero warnings**: All code must pass `cargo clippy -- -D warnings`
- **No unsafe code**: `unsafe_code = "deny"` at workspace level
- **Conventional commits**: `feat(collector)`, `test(core)`, `refactor(adapter)`, etc.
- **Security-first**: No credentials in logs/errors/output. Read-only DB operations only.
- **Testcontainers required**: Integration tests must use real databases, not mocks
- **File size**: Max 600 lines per file preferred, split into modules when exceeded
- **No auto-commits**: Changes must be explicitly committed by maintainer
