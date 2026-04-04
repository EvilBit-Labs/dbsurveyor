# Development Gotchas & Pitfalls

This document tracks non-obvious behaviors, common pitfalls, and hard-earned lessons in the DBSurveyor codebase. Referenced from AGENTS.md and CONTRIBUTING.md.

## 1. Adapter Architecture

### 1.1 `DatabaseAdapter` Trait and Placeholder Macro

`define_placeholder_adapter!` in `adapters/placeholder.rs` generates `DatabaseAdapter` trait impls. Adding a new trait method requires updating the macro (covers MSSQL placeholder).

### 1.2 `TableSample` Struct Literals

`TableSample` struct literals exist in ~15 files (quality/{completeness,anomaly,consistency,uniqueness,analyzer}.rs, adapters/{postgres,mysql,sqlite,mongodb}/sampling.rs, models.rs tests, placeholder macro). Adding a field requires updating all of them.

### 1.3 Module Visibility

All adapter sub-modules (connection, sampling, schema_collection, type_mapping) are private with explicit `pub use` re-exports. This matches the PostgreSQL adapter pattern established in PR #126. Do not make sub-modules `pub mod`.

### 1.4 `RowExt` is PostgreSQL-Only

The `RowExt` trait lives in `adapters/postgres/row_ext.rs`, not in shared code. It is hardcoded to `PgRow`/`sqlx::Postgres`. MySQL and SQLite adapters use `try_get()` directly.

### 1.5 `try_get()` Error Handling in MySQL/SQLite

MySQL and SQLite adapters must use `try_get(...).map_err(|e| DbSurveyorError::collection_failed(...))?` for critical schema fields (names, types, ordinal positions). Do NOT use `unwrap_or_default()` on critical fields -- it silently produces ghost columns with empty names. Optional fields (comments, default values, referential actions) may use `unwrap_or_default()`.

### 1.6 `DatabaseSchema` Uses Immutable Builder Pattern

`DatabaseSchema` methods use `with_*` pattern (consuming `self`, returning `Self`) instead of `&mut self`. Call sites use `schema = schema.with_quality_metrics(...)` reassignment. Do not add `&mut self` methods to `DatabaseSchema`.

## 2. Configuration & Validation

### 2.1 `SamplingConfig` Has Three Construction Paths

Builder methods (`.with_sample_size()`), direct struct literals, and deserialization all create `SamplingConfig`. The builder clamps values (e.g., `sample_size` to `[1, MAX_SAMPLE_SIZE]`), but struct literals and deserialization bypass the builder. `validate()` must reject values the builder would never produce.

### 2.2 `compiled_patterns` is an Internal Cache

`SamplingConfig.compiled_patterns` is `pub(crate)` and `#[serde(skip)]`. After deserialization it will be empty. Callers must invoke `recompile_patterns()` after deserializing. The `Default` impl pre-compiles patterns automatically.

### 2.3 `ConnectionConfig` Field Names

`host` = hostname, `database: Option<String>` = database name. Do not confuse host with database. `postgres/sampling.rs` `sample_table()` accepts `Option<&str>` for schema (not `&str`) and defaults to "public" internally.

### 2.4 Clap `conflicts_with` for Mutually Exclusive Flags

Use `conflicts_with` for flags like `--no-redact` vs `--redact-mode`. Several CLI flags (`--sample`, `--throttle`, `--redact-mode`) are parsed but not yet wired to functionality -- they emit runtime warnings.

## 3. Security

### 3.1 Connection URLs are Zeroized

All adapter structs store connection URLs as `Zeroizing<String>`. When constructing adapters, wrap with `Zeroizing::new(url.to_string())`. The `Deref<Target=String>` impl means read sites work transparently.

### 3.2 Credential Scanning

Output validation (`validate_and_parse_schema`) performs recursive credential scanning on JSON output. All input paths (plain JSON, compressed `.json.zst`, encrypted `.enc`) must route through this validation -- do not use raw `serde_json::from_str`.

### 3.3 `pub(crate)` for Sensitive Fields

`SqliteAdapter.connection_string` and `KdfParams` fields are `pub(crate)`. Integration tests in `tests/` cannot access `pub(crate)` fields (separate compilation units). Use public constructors like `SqliteAdapter::from_pool()` instead of direct struct construction.

### 3.4 Advisory Suppressions

RUSTSEC-2023-0071 (Marvin Attack, RSA timing side-channel) is suppressed in both `deny.toml` and `.cargo/audit.toml`. It is a transitive dependency through sqlx-mysql. Keep both files in sync and update the review date periodically.

## 4. Async & Concurrency

### 4.1 `spawn_blocking` for CPU-Intensive Work

Argon2id KDF (~0.5-1.0s) and zstd compression/decompression run in `tokio::task::spawn_blocking`. Prefer taking ownership in the closure over cloning large data (e.g., `decrypt_data_async` takes `EncryptedData` by value, not `&EncryptedData`).

### 4.2 `tokio::join!` Error Counting

When counting failures from `tokio::join!`, check `is_err()` on the `Result` values BEFORE match arms consume them. Checking `is_empty()` on result vectors after the match conflates "no data" (valid) with "query failed" (error).

### 4.3 Multi-Database Pool Sizing

In multi-database mode, each database gets a pool with `max_connections: 2`, `min_idle: 0`. Pools are explicitly closed after collection. This prevents connection exhaustion when scanning many databases.

## 5. SQLite-Specific

### 5.1 PRAGMA vs DML Escaping

PRAGMA arguments use single-quote escaping (`replace('\'', "''")`). DML identifiers use double-quote escaping (`escape_identifier()`). These are different quoting contexts in SQLite. Use the shared utilities in `sqlite/mod.rs`: `escape_identifier()` and `escape_pragma_arg()`.

### 5.2 `ordinal_position` is 1-Based

SQLite's `PRAGMA table_info` returns `cid` as 0-based. Convert with `cid + 1` for `ordinal_position`. The JSON Schema requires minimum 1.

### 5.3 SystemRowId Quoting

`ORDER BY` clauses for SystemRowId use quoted identifiers: `ORDER BY "rowid" DESC`. This is defense-in-depth even though `rowid` is always system-generated.

## 6. Build & CI

### 6.1 `just ci-check` Takes ~3 Minutes

The recipe runs: `fmt-check`, `lint` (pre-commit + clippy), `test-ci` (nextest), `coverage-ci`, `audit-ci`, `deny`. Do not re-run repeatedly to troubleshoot. Read the justfile, identify which step failed, and check that step individually.

### 6.2 `cargo fmt` Separator

`cargo fmt --all -- --check` requires the `--` separator. Without it, `--check` is silently ignored by cargo and formatting issues pass. The release workflow was fixed for this in PR #126.

### 6.3 Pre-Commit Hook Behavior

- Hooks stash/restore unstaged files. If commit fails (e.g., mdformat reformats docs), re-stage and commit again.
- `.pre-commit-config.yaml` must not have unstaged changes or `git commit` refuses.
- `SKIP=<hook-id> git commit` bypasses a single hook (e.g., `SKIP=cargo-audit`).
- `mdformat` may reformat docs on first run of `just ci-check`. Reset with `git checkout -- docs/` if needed.

### 6.4 rust-analyzer vs `cargo check`

rust-analyzer diagnostics are often stale after multi-file edits. Always trust `cargo check --workspace --all-features` over IDE hints. In particular, rust-analyzer does not enable all feature gates by default, so modules behind `#[cfg(feature = "postgresql")]`, `#[cfg(feature = "mysql")]`, etc. will show false "unused" and "not found" warnings. Clippy with `--all-features` is the authoritative check.

### 6.5 Advisory Ignore Sync

`cargo-audit` ignores live in `.cargo/audit.toml`. `cargo deny` ignores live in `deny.toml`. Keep both in sync when adding or removing advisory suppressions.

### 6.6 Feature-Gated Dependencies

Before removing deps flagged by `cargo-machete`, check if they are feature-gated with `#[cfg(feature)]`. Empty feature gates (e.g., `mssql = []`) are valid placeholders.

## 7. Serialization & Models

### 7.1 Serde Round-Trip Tests

When adding `Option` + `skip_serializing_if` fields to models, always add serde round-trip tests: serialize-omits-None, deserialize-without-field, deserialize-each-variant.

### 7.2 Non-ASCII Characters Prohibited

Non-ASCII characters (checkmarks, bullets, emoji) are prohibited in source code per AGENTS.md. Use `[OK]`, `-`, plain text.

### 7.3 `PartialEq` but not `Eq`

Model types derive `PartialEq` but not `Eq` because several types contain `f64` fields (quality scores, thresholds). Do not add `Eq` derives without checking for float fields.
