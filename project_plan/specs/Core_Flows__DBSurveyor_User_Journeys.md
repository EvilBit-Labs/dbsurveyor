# Core Flows: DBSurveyor User Journeys

## Overview

DBSurveyor has two distinct interaction surfaces: the **collector** (`dbsurveyor-collect`) that touches the target database, and the **postprocessor** (`dbsurveyor`) that operates entirely offline. All flows cross at a file boundary — the `.dbsurveyor.json` (or `.zst`/`.enc`) artifact produced by the collector and consumed by the postprocessor.

```mermaid
graph TD
    A[Operator] --> B[dbsurveyor-collect]
    B --> C{Output format}
    C --> D[.dbsurveyor.json]
    C --> E[.dbsurveyor.json.zst]
    C --> F[.dbsurveyor.enc]
    D --> G[dbsurveyor postprocessor]
    E --> G
    F --> G
    G --> H[Markdown report (production)]
    G --> I[HTML report (experimental build)]
    G --> J[SQL DDL (experimental build)]
    G --> K[Mermaid ERD (experimental build)]
    G --> L[JSON analysis (experimental build)]
```

## Flow 1: Standard Schema Collection

**Who**: Any operator with database access (DBA, developer, red team).

**Trigger**: Operator runs `dbsurveyor-collect` with a connection string.

**Steps**:

01. Operator provides a database connection URL and optional flags (`--sample`, `--throttle`, `--output`, `--compress`/`--encrypt`, `--enable-quality`). `--sample 0` means use the default sample size.
02. The tool auto-detects the database type from the URL scheme (`postgres://`, `mysql://`, `mongodb://`, etc.).
03. Connection is established. Credentials are consumed immediately and never echoed or logged.
04. Schema metadata is collected read-only: tables, columns, types, constraints, indexes, views, procedures, triggers, and foreign key relationships.
05. Optionally: N sample rows are collected per table using the best available ordering (primary key, timestamp, or random fallback). If schema is omitted, table resolution and ordering detection follow runtime `search_path` semantics. If reliable ordering cannot be determined, random sampling is used automatically and a warning is emitted.
06. If per-table sampling fails (timeout, permissions, type issues), the system retries once with a smaller sample size; if it still fails, that table’s sample is skipped with warning while overall collection continues.
07. Optionally: data quality analysis runs over samples if `--enable-quality` is set, producing threshold violation warnings.
08. The collected data is serialized and validated against the versioned JSON Schema. Validation failure aborts with a clear error.
09. Output is written as `.dbsurveyor.json` (default), `.json.zst` (with `--compress`), or prompted for a password and written as `.enc` (with `--encrypt`).
10. A summary is printed: table count, view count, index count, output path. Operator can now safely transfer the file.

**Exit**: A validated schema file on disk. No database connection remains open.

**Error states**:

- Connection refused / wrong credentials → sanitized error (no credential echo), exit non-zero.
- Insufficient privileges → partial collection with per-object warnings logged.
- Per-table sampling failure → one retry at smaller sample size, then table sample skipped with warning.
- Schema validation failure → error with field-level detail, no output file written.

## Flow 2: Multi-Database Enumeration (Variant of Flow 1)

**Who**: Operator with server-level credentials (DBA, red team with superuser access).

**Trigger**: Operator runs `dbsurveyor-collect` with `--all-databases` flag and a server-level connection string (no database name in URL, or connects to `postgres` default).

**Steps**:

1. Operator provides server-level connection URL and `--all-databases`. Optionally `--exclude-databases <list>` and `--include-system-databases`.
2. The tool connects to the server and enumerates all accessible databases (e.g., via `pg_database`). System databases are excluded by default.
3. For each accessible database: a separate connection is established, schema is collected, and progress is reported per-database.
4. Databases the operator lacks privilege to access are logged as skipped (not as errors).
5. On completion, a single output file is written containing server-level metadata and per-database collection status.
6. Summary reports: total databases discovered, collected, skipped, and any per-database errors.

**Exit**: A single validated schema file covering all collected databases, plus an explicit run status via return code semantics: `0` for complete success, `1` for total failure, and dedicated non-zero categories for partial outcomes: `partial-success-with-data`, `partial-success-without-samples`, and `partial-success-with-validation-warnings`. Any skipped database (including privilege-based skips) forces a partial-success category; full success is possible only when no databases are skipped and no partial/failure conditions apply. If multiple partial conditions occur, a single code is emitted using strict precedence: `partial-success-without-samples` > `partial-success-with-data` > `partial-success-with-validation-warnings`.

**Error states**:

- Partial failures (one DB fails) do not abort the run — remaining databases continue.
- A database with `collection_status: Failed` is included in the output with a sanitized error reason.
- Privilege-based skips are reported as skipped (not failed) but still force a partial-success category.
- Partial-success outcomes are mapped to explicit categories: `partial-success-with-data`, `partial-success-without-samples`, and `partial-success-with-validation-warnings`.
- When multiple partial-success conditions overlap, only the highest-priority category is emitted (`without-samples` > `with-data` > `with-validation-warnings`).

## Flow 3: Offline Report Generation

**Who**: Any operator in a potentially air-gapped environment (developer, DBA, compliance analyst).

**Trigger**: Operator runs `dbsurveyor generate` with a schema file path. In experimental builds only, operators may also use additional non-production report surfaces (including SQL/analysis flows).

**Steps**:

1. Operator provides a `.dbsurveyor.json` (or `.zst` / `.enc`) file path and output preference. In standard builds, Markdown is the only production-exposed documentation output. Experimental builds may expose additional non-production output modes, including JSON analysis.
2. File format is auto-detected from extension. For `.enc` files, operator is prompted for the decryption password (TTY, echo disabled).
3. The file is loaded, decompressed/decrypted as needed, and validated against the versioned JSON Schema. Invalid files produce a clear field-level error and exit.
4. Redaction mode is applied as broad best-effort guidance (not strict deterministic guarantees), with a minimum progressive contract: for the same sample set, stricter modes produce equal or greater masking than less strict modes. Redaction applies to sample values only; schema metadata fields remain unchanged. Source file is never modified.
5. The selected output is generated:

- **Markdown (production-complete for this milestone)**: comprehensive documentation including schema summary, relationships, indexes, constraints, and sampled data sections with redaction applied.
- **HTML / SQL DDL / Mermaid / JSON analysis (experimental)**: available only in experimental builds and explicitly treated as non-production in this milestone.

6. Output is written to the specified or auto-detected file path. A confirmation is printed.

**Exit**: A report file. Zero network connections made at any point.

**Error states**:

- Wrong decryption password → clear error, no output written.
- Schema file corrupt or invalid version → field-level validation error, exit non-zero.
- Non-production output requested in a standard build → clear unsupported-mode message with non-zero exit.
- Output write failure (disk full, permissions) → error with path context.

## Flow 4: Encrypted File Round-Trip

**Who**: Red team operator transferring data across a trust boundary (e.g., from target network to analyst environment).

**Trigger**: Operator needs to collect schema and transport it securely without exposing contents if intercepted.

**Steps** (Collection side):

1. Operator runs collector with `--encrypt --output schema.enc`.
2. After schema collection and validation, the tool prompts for a password and confirmation (TTY, echo disabled).
3. Password mismatch → operation is treated as canceled, returns a distinct non-zero exit category, and writes no output file. Operator can re-run.
4. Output is written as `.dbsurveyor.enc` — AES-GCM encrypted with embedded KDF parameters (Argon2id salt, iteration counts). Password is zeroed from memory immediately after encryption.

**Steps** (Analysis side, air-gapped):

1. Operator transfers `schema.enc` to analysis environment (USB, out-of-band).
2. Operator runs `dbsurveyor generate schema.enc --format markdown`.
3. Tool detects `.enc` extension, prompts for decryption password (TTY, echo disabled).
4. Correct password → decrypted in memory, validated, report generated, memory zeroed.
5. Wrong password → clear authentication error, no output.

**Exit**: Report produced entirely offline. The `.enc` file is never modified.

## Flow 5: Connection Test

**Who**: Any operator before committing to a full collection run.

**Trigger**: Operator runs `dbsurveyor-collect test <url>`.

**Steps**:

1. Operator provides a connection string.
2. The tool attempts a connection, confirms the database type is supported, and verifies read access.
3. Success → prints database type and confirmation. No schema is collected, no file is written.
4. Failure → sanitized error (credentials redacted) with exit non-zero.

**Exit**: Operator has confirmed reachability before running a full collection.

## Flow 6: Schema Validation

**Who**: Any operator who wants to verify a schema file before processing.

**Trigger**: Operator runs `dbsurveyor validate <file>`.

**Steps**:

1. Operator provides a schema file path.
2. File is loaded, format detected, and validated against the JSON Schema.
3. Success → prints format version, database name, object counts, and any collection-time warnings embedded in the file.
4. Failure → field-level validation errors printed, exit non-zero.

**Exit**: Operator has a clear validity signal before sending the file to another system or running report generation.
