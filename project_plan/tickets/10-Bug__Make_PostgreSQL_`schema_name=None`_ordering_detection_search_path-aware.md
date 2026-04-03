# Bug: Make PostgreSQL `schema_name=None` ordering detection search_path-aware

## Why this exists

Current PostgreSQL sampling defaults ordering metadata detection to `public` when `schema_name` is omitted, while row sampling itself can use unqualified table reference. This can produce degraded ordering behavior in non-`public` search-path contexts.

## Scope

- Adjust PostgreSQL sampling flow in:
  - file:dbsurveyor-core/src/adapters/postgres/mod.rs
  - file:dbsurveyor-core/src/adapters/postgres/sampling.rs
- When `TableRef.schema_name` is `None`, query the session's active `search_path` via `SHOW search_path` or `current_schema()` and use the first non-`"$user"` schema that contains the target table for ordering detection
- If no schema can be resolved from `search_path`, emit a warning and fall back to `OrderingStrategy::Unordered` (do not silently default to `"public"`)
- The `FROM` clause in the data query must remain consistent with the resolved schema (schema-qualified when resolved, unqualified only when truly unresolvable)
- Ensure warnings are still clear and operator-facing when resolution is ambiguous

## Acceptance criteria

- For `TableRef { schema_name: None, ... }`, ordering detection matches actual table resolution behavior
- No false fallback to unordered due only to hardcoded `public` detection path
- Add/extend tests in file:dbsurveyor-core/tests/postgres_sampling.rs for `schema_name=None` search_path scenarios
- `cargo clippy --all-features -- -D warnings` passes
