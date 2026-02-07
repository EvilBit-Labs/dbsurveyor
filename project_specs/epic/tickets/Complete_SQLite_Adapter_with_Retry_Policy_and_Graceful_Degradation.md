# Complete SQLite Adapter with Retry Policy and Graceful Degradation

## Overview

Bring SQLite adapter to feature parity with PostgreSQL for v1.0 release. Integrate retry policy, graceful degradation with object failure tracking, and comprehensive schema collection.

## Scope

**What's Included**:

- Integrate `RetryPolicy` into SQLite query execution (similar to `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/da7b9aca-42df-40ef-9c17-7df3bfbaf3e8`):
  - Create query execution wrapper using `DefaultRetryPolicy`
  - Categorize SQLite-specific errors (SQLITE_AUTH for permission, etc.)
  - Apply retry logic to all schema collection queries
- Refactor SQLite schema collection for graceful degradation (similar to `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/007550ea-8762-4db6-a886-197a8a7654c7`):
  - Update `collect_tables()` in `file:dbsurveyor-core/src/adapters/sqlite/schema_collection.rs` to continue on per-table failures
  - Record `ObjectFailure` for failed tables/columns/indexes/constraints
  - Populate `CollectionMetadata.object_failures`
- Complete schema collection implementation:
  - Verify table, column, constraint, index collection via `sqlite_master` and PRAGMA commands
  - Add support for views (if not already implemented)
  - Add support for triggers
  - Handle SQLite-specific features (ROWID, WITHOUT ROWID tables)
- Data sampling integration:
  - Verify `sample_table()` works with retry policy
  - Test ordering strategy detection (including ROWID fallback)
- Integration tests with testcontainers:
  - Test retry behavior with simulated failures
  - Test partial collection with locked tables
  - Test graceful degradation for views/triggers
- Update documentation in `file:dbsurveyor-core/src/adapters/sqlite/mod.rs`

**What's Explicitly Out**:

- Multi-database collection for SQLite (SQLite is single-database per file)
- SQLite-specific advanced features (FTS, R*Tree, JSON1) (deferred)
- SQLite 2.x compatibility (focus on SQLite 3.x)

## SQLite Error Categorization

| SQLite Error | Error Category | Retry? |
|--------------|----------------|--------|
| SQLITE_AUTH | Permission | No |
| SQLITE_PERM | Permission | No |
| SQLITE_BUSY | Timeout | Yes |
| SQLITE_LOCKED | Timeout | Yes |
| SQLITE_IOERR | Connection | Yes |
| Other | Other | Yes (up to 3 attempts) |

## Acceptance Criteria

- [ ] All SQLite schema collection queries use retry wrapper with `DefaultRetryPolicy`
- [ ] Permission errors (SQLITE_AUTH, SQLITE_PERM) are categorized as `ErrorCategory::Permission` and not retried
- [ ] Lock errors (SQLITE_BUSY, SQLITE_LOCKED) are retried with exponential backoff
- [ ] Table collection continues when individual table sub-collections fail
- [ ] `ObjectFailure` entries populated with SQLite-specific error details
- [ ] `CollectionMetadata.object_failures` contains all recorded failures
- [ ] Views and triggers collection degrades gracefully (warnings, not failures)
- [ ] ROWID-based ordering strategy works correctly for tables without explicit primary keys
- [ ] Integration tests verify retry behavior with testcontainers SQLite
- [ ] Integration tests verify partial collection with locked tables
- [ ] 70% test coverage maintained for SQLite adapter

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - Adapter Integration)
- **Epic Brief**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/64fc1d47-e1e3-40db-a5dc-8dc9c248814c` (v1.0 Must Have - SQLite adapter)
- **Related Tickets**:
  - `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/a6020349-6b60-4e3e-a5b5-f7ee3a264721` (RetryPolicy)
  - `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/dbbfad98-830e-499a-9363-7dc1badbb23a` (ObjectFailure model)
