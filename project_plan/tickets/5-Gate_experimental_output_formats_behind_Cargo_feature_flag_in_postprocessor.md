# Gate experimental output formats behind Cargo feature flag in postprocessor

## Context

Currently `dbsurveyor/src/main.rs` exposes `Html`, `Json`, `Mermaid` variants in `OutputFormat` and an `Sql` subcommand in all builds. Per the Epic Brief and Tech Plan, non-Markdown outputs must be absent from standard builds — compile-out, not runtime-rejected.

**Independent of adapter work.** Can land in parallel with T1–T4. **Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/f257e12b-711f-4d82-9249-e74749688e3c — Milestone Scope Decision; spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/76c2adac-5a39-4686-b219-3f030de658fc — Experimental feature flag section.

## Scope

### In scope

**`dbsurveyor/Cargo.toml`** — add `experimental` to the `[features]` table; it must **not** be in `default`.

**`dbsurveyor/src/main.rs`** — gate the following behind `#[cfg(feature = "experimental")]`:

- `OutputFormat::Html`, `OutputFormat::Json`, `OutputFormat::Mermaid` enum variants and their `clap` derive attributes
- `Command::Sql(SqlArgs)` and `Command::Analyze(AnalyzeArgs)` subcommands and their `clap` derive attributes
- The handler arms for all of the above in `match &cli.command` and `match format`
- All auto-generated output file path logic for gated formats

**`.goreleaser.yaml`** — verify (and correct if needed) that the release build for `dbsurveyor` does **not** pass `--all-features`; it must use an explicit feature list (e.g., `compression,encryption`) that excludes `experimental`. Add a comment documenting this constraint.

### Out of scope

- Actual HTML/SQL/Mermaid implementation (those are future-milestone work)
- Any collector binary changes

## Acceptance Criteria

- `cargo build -p dbsurveyor` (no features) produces a binary where `--format html`, `--format json`, `--format mermaid`, and the `sql`/`analyze` subcommands are entirely absent from `--help` output and cannot be passed
- `cargo build -p dbsurveyor --features experimental` produces a binary where all formats and subcommands are present in `--help`
- `cargo clippy -p dbsurveyor -- -D warnings` passes in both configurations
- The GoReleaser config comment explicitly notes that `experimental` must not be included in release builds
