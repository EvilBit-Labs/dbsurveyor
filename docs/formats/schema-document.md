# Schema document format

The schema document is the JSON artifact `dbsurveyor-collect` writes and
`dbsurveyor` reads. This specification describes version `1.0`.

The Go implementation in `internal/dbschema` is the reference for this format.
It carries no obligation to read documents produced by the retired Rust
implementation, and no Rust-era document is a valid document under this
specification.

## Published artifacts

| File | What it is |
| --- | --- |
| `schema-document.schema.json` | JSON Schema for a single-database document |
| `server-document.schema.json` | JSON Schema for a multi-database document |
| `examples/minimal-schema.json` | Smallest valid single-database document |
| `examples/full-schema.json` | Every optional field and every union variant |
| `examples/server-document.json` | Multi-database document, one collected and one failed |

All five are **generated**, not hand-maintained. `tools/genschema` reflects them
from the Go types; `just gen-schema` regenerates them. `TestPublishedArtifactsAreCurrent`
fails when a committed file and its generated form disagree, so the schema
cannot drift from the implementation. Adding a field to a Go type and forgetting
to regenerate is a test failure, not a review catch.

## Document identity

Every document carries two identifying fields:

- `format` -- `"dbsurveyor/schema"` for a single database,
  `"dbsurveyor/server-schema"` for a multi-database collection. A reader
  distinguishes the two without inspecting the rest of the document.
- `format_version` -- `"1.0"`.

Both schemas set `additionalProperties: false`. A document with an unrecognized
key fails validation rather than silently losing the value, so a hand-edited
document with a typo is reported instead of half-applied.

## Absent versus null

Optional fields are **omitted** when unset, never emitted as `null`. This
distinction is load-bearing:

- `row_count` absent means the engine reports no row statistics for this table.
  `row_count: 0` means the engine reports an empty table.
- `samples` absent means sampling was not requested. `samples: []` means
  sampling ran and matched no tables.
- `status` absent on a sample means the adapter reported no outcome. It is not
  equivalent to `{"state": "complete"}`.
- `on_delete` absent on a foreign key means the engine reported no explicit
  action, so the engine default applies. It is not equivalent to
  `{"on_delete": "no_action"}`.

Collapsing any of these to `null` would erase the difference between "not
measured" and "measured as zero", which the quality analyzers and the report
renderer both depend on.

## Discriminated unions

Go has no sum type, so the four unions in this format are objects with a
discriminator field plus the payload fields belonging to that variant. Payload
fields for other variants are absent.

### `SampleStatus` (`status` on a sample)

| Discriminator | Payload |
| --- | --- |
| `{"state": "complete"}` | none |
| `{"state": "partial_retry", "original_limit": 500}` | `original_limit` -- the limit originally requested before the adapter retried lower |
| `{"state": "skipped", "reason": "..."}` | `reason` |

### `CollectionStatus` (`collection_status` on `database_info`)

| Discriminator | Payload |
| --- | --- |
| `{"state": "success"}` | none |
| `{"state": "failed", "error": "..."}` | `error` |
| `{"state": "skipped", "reason": "..."}` | `reason` |

`error` carries an operator-facing failure description. It must never carry a
connection string or any other credential-bearing value; the credential scan
treats it as untrusted like every other string in the document.

### `SamplingStrategy` (`sampling_strategy` on a sample)

| Discriminator | Payload |
| --- | --- |
| `{"kind": "most_recent", "limit": 100}` | `limit` |
| `{"kind": "random", "limit": 100}` | `limit` |
| `{"kind": "none"}` | none |

### `UnifiedDataType` (`data_type` on a column or parameter)

`kind` is the discriminator. Payload fields by kind:

| `kind` | Payload |
| --- | --- |
| `string`, `binary` | `max_length` (absent when the engine declares no limit) |
| `integer` | `bits`, `signed` |
| `float` | `precision` (absent when the engine reports none) |
| `datetime`, `time` | `with_timezone` |
| `array` | `element_type`, itself a `UnifiedDataType`, so arrays nest |
| `custom` | `type_name` |
| `boolean`, `date`, `json`, `uuid` | none |

A fifth object, `OrderingStrategy` (`ordering` on a sample), records how the
adapter defined "most recent" for the table: `primary_key` carries `columns`,
`timestamp` carries `column` and `direction`, `auto_increment` and
`system_row_id` carry `column`, and `unordered` carries nothing. It is
informational -- a reader that ignores it loses no correctness.

## Enumerations

Every enumerated value in this format is a lowercase string with underscores.
Unrecognized values are **rejected at the parse boundary**, both by the JSON
Schema and by the Go decoder, rather than accepted as an opaque string that
fails later at the point of use.

The permitted values are generated into the published JSON Schema from the same
constants the decoder enforces, so the two cannot disagree. Read them from
`schema-document.schema.json`; they are deliberately not duplicated here, where
they would drift.

## Field-level notes

- `ordinal_position` on a column is 1-based. SQLite reports a 0-based `cid`
  through `PRAGMA table_info`, and its adapter normalizes it.
- `indexes` and `constraints` at the document root are the aggregate of the
  per-table lists. The duplication is deliberate: a report iterates the roll-up
  once instead of walking every table. `AggregateIndexesAndConstraints` rebuilds
  them, and a document whose roll-up disagrees with its tables is invalid.
- `rows` on a sample is the only place a document carries user data. It is the
  primary target of redaction and of the credential scan.
- `procedures` and `functions` hold the same type. Engines disagree on whether a
  value-returning routine is a function or a procedure, so the distinction is
  carried by which list the routine appears in rather than by a field.
- `collected_at` and `duration_ms` describe the collection run, not the
  database.

## What a document never contains

- A connection string, password, or any other credential, in any field,
  including `error`, `warnings`, and sampled row values. Every load path runs a
  recursive credential scan and rejects the document if one is present.
- Telemetry, identifiers of the collecting host, or anything not read from the
  target database or supplied by the operator.

## Related specifications

- `encrypted-envelope.md` -- the AES-256-GCM envelope wrapping an encrypted document
- `compression.md` -- zstd framing and output extension dispatch
