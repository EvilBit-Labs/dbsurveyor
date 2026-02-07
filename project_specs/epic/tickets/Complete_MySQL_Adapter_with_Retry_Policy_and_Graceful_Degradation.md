# Complete MySQL Adapter with Retry Policy and Graceful Degradation

## Overview

Bring MySQL adapter to feature parity with PostgreSQL for v1.0 release. Integrate retry policy, graceful degradation with object failure tracking, and comprehensive schema collection.

## Scope

**What's Included**:

- Integrate `RetryPolicy` into MySQL query execution (similar to `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/da7b9aca-42df-40ef-9c17-7df3bfbaf3e8`):
  - Create query execution wrapper using `DefaultRetryPolicy`
  - Categorize MySQL-specific errors (error codes 1142 for permission, etc.)
  - Apply retry logic to all schema collection queries
- Refactor MySQL schema collection for graceful degradation (similar to `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/007550ea-8762-4db6-a886-197a8a7654c7`):
  - Update `collect_tables()` in `file:dbsurveyor-core/src/adapters/mysql/schema_collection.rs` to continue on per-table failures
  - Record `ObjectFailure` for failed tables/columns/indexes/constraints
  - Populate `CollectionMetadata.object_failures`
- Complete schema collection implementation:
  - Verify table, column, constraint, index collection works correctly
  - Add support for views (if not already implemented)
  - Add support for stored procedures and functions
  - Add support for triggers
- Data sampling integration:
  - Verify `sample_table()` works with retry policy
  - Test ordering strategy detection
- Integration tests with testcontainers:
  - Test retry behavior with simulated failures
  - Test partial collection with permission errors
  - Test graceful degradation for views/routines/triggers
- Update documentation in `file:dbsurveyor-core/src/adapters/mysql/mod.rs`

**What's Explicitly Out**:

- Multi-database collection for MySQL (deferred to post-v1.0)
- Advanced MySQL-specific features (partitions, events) (deferred)
- MySQL 5.x compatibility (focus on MySQL 8.0+)

## MySQL Error Categorization

| MySQL Error Code | Error Category | Retry? |
|------------------|----------------|--------|
| 1142 (SELECT denied) | Permission | No |
| 1044 (DB access denied) | Permission | No |
| 2013 (Lost connection) | Connection | Yes |
| 1205 (Lock wait timeout) | Timeout | Yes |
| Other | Other | Yes (up to 3 attempts) |

## Acceptance Criteria

- [ ] All MySQL schema collection queries use retry wrapper with `DefaultRetryPolicy`
- [ ] Permission errors (MySQL code 1142, 1044) are categorized as `ErrorCategory::Permission` and not retried
- [ ] Connection errors (MySQL code 2013) are retried with exponential backoff
- [ ] Table collection continues when individual table sub-collections fail
- [ ] `ObjectFailure` entries populated with MySQL-specific error details
- [ ] `CollectionMetadata.object_failures` contains all recorded failures
- [ ] Views, procedures, functions, triggers collection degrades gracefully (warnings, not failures)
- [ ] Integration tests verify retry behavior with testcontainers MySQL
- [ ] Integration tests verify partial collection with simulated permission errors
- [ ] 70% test coverage maintained for MySQL adapter

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - Adapter Integration)
- **Epic Brief**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/64fc1d47-e1e3-40db-a5dc-8dc9c248814c` (v1.0 Must Have - MySQL adapter)
- **Related Tickets**:
  - `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/a6020349-6b60-4e3e-a5b5-f7ee3a264721` (RetryPolicy)
  - `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/dbbfad98-830e-499a-9363-7dc1badbb23a` (ObjectFailure model)
