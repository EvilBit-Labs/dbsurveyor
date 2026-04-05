# Implement redaction pipeline in the postprocessor binary

## Context

`RedactionMode` is already defined as a CLI enum in `dbsurveyor/src/main.rs` but has no implementation. This ticket builds the full redaction module — private to the postprocessor binary — with progressive mode guarantees and sample-only scope.

**Independent of adapter work.** Can land in parallel with T1–T4. **Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/76c2adac-5a39-4686-b219-3f030de658fc — redaction module section; spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/0a7ebf13-73f6-444b-8b1c-adaefd118e0d — Flow 3 step 4.

## Scope

### In scope

\*\*New: \*\***`dbsurveyor/src/redaction/mod.rs`** (new module, private to the `dbsurveyor` crate):

- Move `RedactionMode` from `main.rs` into this module; re-export it from `main.rs` for CLI use
- `RedactedTableSample` struct: `table_name`, `schema_name`, `rows: Vec<serde_json::Value>` (masked), `mode_applied: RedactionMode`, `warnings: Vec<String>`
- `Redactor` struct: `fn new(mode: RedactionMode) -> Redactor` and `fn redact(&self, samples: &[TableSample]) -> Vec<RedactedTableSample>`
- Redaction applies to `rows` values only — `table_name`, `schema_name`, and all schema metadata fields are never altered
- Source `TableSample` is never mutated; a new `RedactedTableSample` is returned

**Pattern definitions per mode** (best-effort guidance with progressive guarantee):

| Mode           | Guaranteed behavior                                                                                                              |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `None`         | Pass-through; rows unchanged                                                                                                     |
| `Minimal`      | Mask field values whose keys match credential patterns: `password`, `secret`, `token`, `api_key`, `key`, `private_key`, `passwd` |
| `Balanced`     | All of Minimal + common PII patterns: `email`, `ssn`, `phone`, `dob`, `birth`, `credit_card`, `card_number`, `cvv`, `sin`        |
| `Conservative` | All string values in any field not on an explicit safe-fields allow-list; numeric IDs and timestamps preserved                   |

- **Progressive contract enforcement**: integration test verifies that for the same input sample set, `Conservative` masks ≥ `Balanced` masks ≥ `Minimal` masks ≥ `None` masks (measured by count of `"[REDACTED]"` values)

**Wire into \*\*\*\*`generate_documentation()`**: before rendering, call `Redactor::new(cli.redact_mode).redact(&schema.samples.unwrap_or_default())` and pass `RedactedTableSample`s to the rendering layer (even if Markdown rendering is still a stub — the redacted samples must flow through)

### Out of scope

- Markdown template rendering (T7)
- Metadata field redaction (explicitly out of spec)
- Custom user-defined patterns (future work)

## Acceptance Criteria

- `None` mode: all rows are identical to input
- `Minimal` mode: a field named `password` has its value replaced; a field named `username` does not
- `Balanced` mode: `email` and `ssn` fields are masked; `user_id` (integer) is not
- `Conservative` mode: arbitrary string field `description` is masked; `id` (integer) is not
- Progressive contract integration test passes: `Conservative` mask count ≥ `Balanced` ≥ `Minimal` ≥ `None` on a fixture sample with credential, PII, and free-text fields
- Source `TableSample.rows` are not mutated (verified by comparing before/after)
- `cargo clippy -p dbsurveyor -- -D warnings` passes
