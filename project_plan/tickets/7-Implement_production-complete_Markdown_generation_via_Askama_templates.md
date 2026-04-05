# Implement production-complete Markdown generation via Askama templates

## Context

`generate_markdown()` in `dbsurveyor/src/main.rs` is currently a stub that produces only a summary header. This ticket replaces it with the full Askama-template-based pipeline that meets the production-complete definition: schema summary, per-table sections (columns, PK, FKs, indexes, constraints), relationships, redacted samples, and warnings.

**Depends on**: T2 (SamplingOrchestrator — so sample data is populated), T5 (experimental gating — so the build boundary is clean), T6 (redaction pipeline — so `RedactedTableSample` is available) **Specs**: spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/76c2adac-5a39-4686-b219-3f030de658fc — Markdown generation pipeline section; spec:a851bd63-14cc-4ca5-a046-39862bd0e0a7/f257e12b-711f-4d82-9249-e74749688e3c — Markdown production-complete definition.

## Scope

### In scope

\*\*New: \*\***`dbsurveyor/src/markdown/`** — Askama template module:

- `MarkdownContext` struct bound to `DatabaseSchema` and `Vec<RedactedTableSample>` (by reference)
- Askama template file `dbsurveyor/src/markdown/schema.md.jinja` (or `.txt` — per Askama conventions) compiled at build time
- Template produces (in order):
  1. **Database summary header**: database name, version, collection date, collector version, table/view/index/constraint counts
  2. **Table of contents**: linked anchors for each table section
  3. **Per-table sections**: column table (name, type, nullable, default, PK flag, FK flag), primary key detail, foreign key list with referenced table/columns, index list, constraint list
  4. **Sampled data section** (when samples present): rendered as a Markdown table with redacted values; `SampleStatus` shown (Complete / PartialRetry / Skipped with reason)
  5. **Warnings section**: all `collection_metadata.warnings` listed if non-empty

**`dbsurveyor/src/main.rs`** — replace the stub `generate_markdown()` call with the new template renderer; thread `RedactedTableSample`s (from T6 `Redactor`) into `MarkdownContext`.

\*\*Remove placeholder ****`generate_html()`****, ****`generate_json_analysis()`****, \*\***`generate_mermaid()`** — these are now gated by `experimental` (T5) and the stub bodies are no longer needed in standard builds.

### Out of scope

- HTML, SQL DDL, Mermaid rendering (future experimental milestone)
- Any adapter or collector changes

## Acceptance Criteria

- End-to-end: given a `.dbsurveyor.json` file produced by the PostgreSQL adapter (collected in the test environment), running `dbsurveyor generate <file>` produces a `.md` file that:
  - Contains all table names as section headers
  - Contains a column table for each table with correct names and types
  - Contains a foreign key section for any table with FKs
  - Contains an index section for any table with indexes
  - Contains a sampled data Markdown table (when samples present)
  - Contains a warnings section if any collection warnings exist
- The `MarkdownContext` → template binding produces a compile-time error (not runtime) if a required field is missing from the schema data model
- `cargo build -p dbsurveyor` (standard build, no experimental) compiles cleanly without html/mermaid/sql handler stubs
- `cargo clippy -p dbsurveyor -- -D warnings` passes
- A database with 0 tables produces a valid (non-empty) Markdown file with a warnings section noting 0 objects
