# Bug: Return `SampleStatus::Skipped` from placeholder adapter `sample_table()`

## Why this exists

During implementation validation, placeholder adapters were returning a not-implemented error from trait-level sampling instead of a structured skipped sample result. You chose to track this as a bug ticket instead of treating it as accepted drift.

## Scope

- Update placeholder adapter macro behavior in file:dbsurveyor-core/src/adapters/placeholder.rs
- Ensure macro-generated `sample_table()` returns a `TableSample` with:
  - `sample_status = Some(SampleStatus::Skipped { reason: ... })`
  - empty rows and safe defaults
- Keep behavior aligned with T1 expectations from ticket:a851bd63-14cc-4ca5-a046-39862bd0e0a7/f18a700b-3999-49b8-b5a2-e8fdda51133b

## Acceptance criteria

- Placeholder adapters no longer return hard errors for trait-level `sample_table()`
- Returned sample has explicit skipped status and non-empty reason
- `cargo clippy --all-features -- -D warnings` passes
- Existing placeholder adapter tests are updated/expanded to validate skipped-status output
