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
- `quality_metrics` absent means quality analysis did not run. An empty array
  means it ran and had no samples to score.
- `anomalies` absent on a table's quality metrics means anomaly detection was
  disabled. An empty `outliers` array means it ran and found nothing.
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

## Quality metrics

`quality_metrics` holds one entry per sampled table, scoring it on three
dimensions plus optional anomaly detection:

| Dimension | What it measures |
| --- | --- |
| `completeness` | Nulls and empty strings. An absent key and an explicit null count the same; an empty string is counted apart from both, because it is a value the engine stored. |
| `consistency` | Type uniformity within a column, and departures from a column's dominant string format. |
| `uniqueness` | Repeated values per column, and rows identical across every column. |
| `anomalies` | Values beyond a z-score threshold in numeric columns. |

`quality_score` is the weighted mean of the three dimension scores. A dimension
whose weight is zero drops out of the overall score but is still measured and
still reported as a threshold violation, so setting a weight to zero suppresses
an opinion rather than a measurement.

Every score is in `[0, 1]`. A table with no sampled rows scores 1.0 on each
dimension: there is no evidence of a problem, which is not the same as evidence
of quality, and `analyzed_rows` is what tells the two apart.

**Quality metrics carry no sampled values.** They hold counts, ratios, column
names, and -- for anomalies -- a mean and a standard deviation. A metrics block
can therefore be shared where the sample it describes cannot. The one caveat is
that on a sensitive numeric column the mean and standard deviation still
describe the distribution.

## Validity

A document is valid when it satisfies the JSON Schema and, in addition:

- Every column's `ordinal_position` is at least 1.
- Every discriminated union carries the payload its discriminator requires and
  no payload belonging to another variant.
- A foreign key's `columns` and `referenced_columns` have the same length, since
  they are positionally paired.
- Every row of a sample carries the same columns. Consumers read the column names
  from the first row, so a ragged sample would silently lose columns downstream.
- The root `indexes` and `constraints` agree with the per-table lists they
  duplicate.
- No string anywhere in the document matches a credential pattern.

`dbschema.Schema.Validate` checks all of this and returns every problem it finds
rather than stopping at the first, each located by a path such as
`tables[0].columns[2].ordinal_position`.

## Redaction

Redaction masks sampled values before a report renders them. It never changes a
stored document -- it produces copies. Four modes, each masking a superset of the
one before it:

| Mode | What it masks |
| --- | --- |
| `none` | Nothing. |
| `minimal` | Columns whose names denote a secret: password, token, api_key, private_key, and similar. |
| `balanced` | Adds columns whose names denote personal data: email, ssn, phone, date of birth, card number, and similar. |
| `conservative` | Every string except identifiers, `*_id` and `*_at` columns, and values whose shape is a date or a time. |

Column names are matched by token, not substring, so `password_hash` and `apiKey`
match while `monkey_count` does not, and a plain plural (`tokens`, `api_keys`)
matches its singular. Masked values become the fixed marker `[REDACTED]`, which
is left alone on a second pass, so redaction is idempotent.

Redaction is not a substitute for the credential scan. Redaction is about what an
operator chooses to show; the credential scan is about what the tool must never
write.

## What a document never contains

- A connection string, password, or any other credential, in any field,
  including `error`, `warnings`, and sampled row values. Every load path runs a
  recursive credential scan over the marshalled document and rejects it if one is
  present. The scan reports the path to the offending node and the name of the
  rule that matched, never the value, so reporting a leak does not become one.
  Only values are scanned; a column named `password` is metadata an operator
  needs to see.
- Telemetry, identifiers of the collecting host, or anything not read from the
  target database or supplied by the operator.

## Related specifications

- `encrypted-envelope.md` -- the AES-256-GCM envelope wrapping an encrypted document
- `compression.md` -- zstd framing and output extension dispatch
