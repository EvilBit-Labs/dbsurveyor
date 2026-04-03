# Implement CollectionOutcome exit-code taxonomy for multi-database runs

## Context

Multi-database collection currently always returns `Ok(())` or a generic error. We need the three partial-success categories — with strict single-code precedence — mapped to process exit codes for automation pipelines.

**Depends on**: T2 (SamplingOrchestrator, which produces `SampleStatus` per table that feeds into outcome aggregation) **Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/76c2adac-5a39-4686-b219-3f030de658fc — CollectionOutcome section; spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/0a7ebf13-73f6-444b-8b1c-adaefd118e0d — Flow 2 (Multi-Database Enumeration).

## Scope

### In scope

**New: `dbsurveyor-collect/src/outcome.rs`** — `CollectionOutcome` enum:

| Variant                         | Exit code |
| ------------------------------- | --------- |
| `Success`                       | `0`       |
| `TotalFailure { error }`        | `1`       |
| `PartialWithoutSamples`         | `2`       |
| `PartialWithData`               | `3`       |
| `PartialWithValidationWarnings` | `4`       |
| `Canceled { reason }`           | `5`       |

- `impl CollectionOutcome` with `fn exit_code(&self) -> i32`
- `fn from_results(databases: &[DatabaseSchema]) -> CollectionOutcome` — applies strict single-code precedence: `PartialWithoutSamples` > `PartialWithData` > `PartialWithValidationWarnings`
- `Canceled` bypasses the aggregation path entirely — it is emitted directly by `main()` when a user-initiated cancellation is detected (e.g., password mismatch), before any outcome aggregation runs

**Skipped database handling**: `CollectionStatus::Skipped` must be treated as a partial-success condition during aggregation. A run where all accessible databases are collected but one or more are skipped (for any reason, including privilege-based skips) must not produce `CollectionOutcome::Success`. The `from_results()` function must inspect all three `CollectionStatus` variants — `Success`, `Failed`, and `Skipped` — when computing the outcome category.

**`dbsurveyor-collect/src/main.rs`** changes:

- `main()` signature changes from `-> Result<()>` to `-> ()` (async Tokio entry) — calls `std::process::exit(outcome.exit_code())` after the runtime finishes
- Multi-database orchestration (`--all-databases` flag path) is wired to collect per-database outcomes and feed them into `CollectionOutcome::from_results()`
- Single-database path also produces a `CollectionOutcome` (either `Success` or `TotalFailure`)
- **Canceled path**: when `save_schema()` returns a password-mismatch error, `main()` maps it directly to `CollectionOutcome::Canceled { reason }` and calls `std::process::exit(5)` — this path must not enter the partial-success aggregation logic

**Multi-database orchestration itself** (the `--all-databases` flow, currently a stub) — implement the actual per-database loop: enumerate databases via the adapter, connect to each, collect schema, handle per-database failures as `CollectionStatus::Failed` without aborting the run, write a single output file with all per-database schemas and server metadata.

### Out of scope

- Any postprocessor changes
- Adapter implementations

## Acceptance Criteria

- Exit code `0` when all databases collected successfully with no skips, no sample failures, and no validation warnings
- Exit code `1` when total failure (no schemas produced)
- Exit code `2` when at least one DB has no samples at all (`SampleStatus::Skipped` for all tables)
- Exit code `3` when at least one DB collected fully but another failed collection
- Exit code `4` when all collected but validation warnings present
- Exit code `5` when operation is canceled by the user (e.g., password mismatch during encryption) — no output file written
- **Skipped DB forces partial-success**: a run where all DBs are collected but one is `CollectionStatus::Skipped` must not return exit code `0` — verified by a test with one skipped DB and all others successful
- When multiple partial conditions co-occur, the highest-priority code wins (tested with a multi-DB scenario combining code-2 and code-4 conditions)
- A database with `CollectionStatus::Failed` is present in the output file with a sanitized error reason (no credentials)
