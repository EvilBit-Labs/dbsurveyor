# Integrate RetryPolicy into PostgreSQL Query Execution

## Overview

Add per-query retry logic to PostgreSQL adapter using RetryPolicy trait. This enables automatic recovery from transient database errors while avoiding retry storms on permission errors.

## Scope

**What's Included**:
- Create query execution wrapper in `file:dbsurveyor-core/src/adapters/postgres/mod.rs`:
  - `execute_with_retry<T, F>(&self, operation: F, retry_policy: &dyn RetryPolicy) -> Result<T>`
  - Generic wrapper that applies retry logic to any async database operation
- Categorize SQLx errors into `ErrorCategory`:
  - `sqlx::Error::Database` with code "42501" → `ErrorCategory::Permission`
  - `sqlx::Error::PoolTimedOut`, `sqlx::Error::PoolClosed` → `ErrorCategory::Timeout`
  - `sqlx::Error::Io` → `ErrorCategory::Connection`
  - Other errors → `ErrorCategory::Other`
- Apply retry logic to all schema collection queries in `file:dbsurveyor-core/src/adapters/postgres/schema_collection.rs`:
  - `collect_database_info()` queries
  - `collect_schemas()` query
  - `collect_tables()` query
  - `collect_table_columns()`, `collect_table_indexes()`, `collect_table_constraints()`, `collect_table_primary_key()`, `collect_table_foreign_keys()`
  - Views, routines, triggers queries in respective modules
- Record retry attempts and final backoff in error context (for future `ObjectFailure` population)
- Log retry attempts at debug level: "Retrying query after {backoff}ms (attempt {attempt}/{max})"
- Unit tests for error categorization (verify all SQLx error types map correctly)
- Integration tests verifying retry behavior with testcontainers:
  - Simulate transient timeout (verify retry occurs)
  - Simulate permission error (verify no retry)
  - Verify exponential backoff timing

**What's Explicitly Out**:
- Retry logic for MySQL/SQLite adapters (separate work)
- Configurable retry parameters (using hardcoded defaults from `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/2`)
- Connection pool retry (focus on query-level retry only)
- Population of `ObjectFailure` metadata (handled in `ticket:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/7`)

## Retry Wrapper Pattern

```rust
async fn execute_with_retry<T, F, Fut>(
    operation: F,
    retry_policy: &dyn RetryPolicy,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let category = categorize_error(&e);
                if !retry_policy.should_retry(category, attempt) {
                    return Err(e);
                }
                let backoff = retry_policy.backoff_duration(attempt);
                tokio::time::sleep(backoff).await;
                attempt += 1;
            }
        }
    }
}
```

## Acceptance Criteria

- [ ] All PostgreSQL schema collection queries use retry wrapper
- [ ] Permission errors (SQLx code "42501") are categorized as `ErrorCategory::Permission` and not retried
- [ ] Timeout errors are categorized as `ErrorCategory::Timeout` and retried up to 3 times
- [ ] Connection errors are retried with exponential backoff (500ms, 1000ms, 2000ms + jitter)
- [ ] Retry attempts and backoff durations are logged at debug level
- [ ] Unit tests verify error categorization for all SQLx error variants
- [ ] Integration test verifies retry behavior with simulated transient failures (using testcontainers)
- [ ] Integration test verifies no retry on permission errors

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - RetryPolicy Trait, Adapter Integration)
- **Core Flows**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/661dbe3d-b679-4287-991e-26f4a0dd98b9` (Flow 6 - retry transient errors)