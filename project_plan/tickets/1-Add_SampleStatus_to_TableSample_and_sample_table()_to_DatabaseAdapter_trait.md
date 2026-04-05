# Add SampleStatus to TableSample and sample_table() to DatabaseAdapter trait

## Context

This is the foundational data model and trait change that all sampling work depends on. It must land before the `SamplingOrchestrator`, the MySQL/SQLite/MongoDB adapter work, and the Markdown pipeline.

**Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/76c2adac-5a39-4686-b219-3f030de658fc — Data Model section and `DatabaseAdapter` trait section.

## Scope

### In scope

**`dbsurveyor-core/src/models.rs`** — Add `SampleStatus` enum and wire it into `TableSample`:

```rust
pub enum SampleStatus {
    Complete,
    PartialRetry { original_limit: u32 },
    Skipped { reason: String },
}
```

- `sample_status` field on `TableSample` is `Option<SampleStatus>` with `#[serde(skip_serializing_if = "Option::is_none")]` for v1.0 backward compatibility
- Consumers treating a missing `sample_status` as unspecified/legacy must not fail

**`dbsurveyor-core/src/adapters/mod.rs`** — Extend the `DatabaseAdapter` trait with a new method:

```rust
async fn sample_table(
    &self,
    table_ref: TableRef<'_>,
    config: &SamplingConfig,
) -> Result<TableSample>
```

- Introduce a `TableRef<'_>` type (or equivalent) carrying schema-qualified table identity: `schema_name: Option<&str>`, `table_name: &str`
- The method must be `async` via `async_trait` (already applied to the trait) and remain object-safe
- Each existing adapter stub must provide a placeholder implementation (returning a `TableSample` with `SampleStatus::Skipped` and appropriate reason) so the trait addition does not break the build
- The PostgreSQL adapter's existing sampling code in `dbsurveyor-core/src/adapters/postgres/sampling.rs` is upgraded to implement `sample_table()` properly (ordering strategy detection, PK/timestamp/AutoIncrement/SystemRowId/Unordered)

### Out of scope

- Retry/fallback policy (belongs in `SamplingOrchestrator`, next ticket)
- MySQL, SQLite, MongoDB real implementations of `sample_table()` (those adapters are a separate ticket)
- Any postprocessor changes

## Acceptance Criteria

- `cargo build --all-features` passes with zero warnings after the change
- `TableSample` round-trips through serde with and without `sample_status` present — old v1.0 files without the field still deserialize correctly
- The `DatabaseAdapter` trait remains object-safe (verified by confirming `Box<dyn DatabaseAdapter>` still compiles)
- PostgreSQL's `sample_table()` returns the correct `OrderingStrategy` for a table with a PK, a table with only a timestamp column, and a table with neither
