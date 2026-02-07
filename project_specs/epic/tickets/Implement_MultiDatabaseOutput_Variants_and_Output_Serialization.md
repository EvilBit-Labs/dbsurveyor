# Implement MultiDatabaseOutput Variants and Output Serialization

## Overview

Create type-safe output mode variants and serialization logic for single bundle vs one-per-database outputs. This enables flexible output formats for different DBA workflows.

## Scope

**What's Included**:
- Define `MultiDatabaseOutput` enum in `file:dbsurveyor-core/src/adapters/config/multi_database.rs`:
  ```rust
  pub enum MultiDatabaseOutput {
      Bundle(MultiDatabaseResult),
      PerDatabase {
          manifest: MultiDatabaseManifest,
          schemas: Vec<(String, DatabaseSchema)>,
      },
  }
  ```
- Define `MultiDatabaseManifest` struct with metadata:
  - `server_info: ServerInfo`
  - `databases: Vec<DatabaseManifestEntry>` (name, status, collection_duration_ms)
  - `collection_metadata: MultiDatabaseMetadata`
- Extend `MultiDatabaseConfig` with `output_mode: OutputMode` enum:
  - `OutputMode::SingleBundle` (default)
  - `OutputMode::OnePerDatabase`
- Update `collect_all_databases()` signature in `file:dbsurveyor-core/src/adapters/postgres/multi_database.rs`:
  - Return `MultiDatabaseOutput` instead of `MultiDatabaseResult`
  - Build appropriate variant based on `config.output_mode`
- Implement output serialization in `file:dbsurveyor-collect/src/main.rs`:
  - Bundle variant: serialize to single JSON file (existing logic)
  - PerDatabase variant: write manifest + per-db schema files (success) + per-db failure stub files (failed databases)
- CLI argument parsing:
  - Add `--one-per-database` flag
  - Add `--output-dir <DIR>` flag (required for one-per-database mode)
- Unit tests for variant construction and serialization
- Integration tests for both output modes

**What's Explicitly Out**:
- Encryption/compression handling (reuse existing logic from single-file output)
- Postprocessor support for multi-db bundles (separate work)
- Manifest-based cross-database analysis (future feature)

## Output Mode Comparison

| Mode | CLI Flags | Output Files | Use Case |
|------|-----------|--------------|----------|
| **Single Bundle** | `--output schemas.json` | `schemas.json` (all databases) | Default, easy transfer |
| **One Per Database** | `--one-per-database --output-dir ./schemas/` | `manifest.json`, `db1.json`, `db2.json`, ... | Individual processing |

## Acceptance Criteria

- [ ] `MultiDatabaseOutput` enum compiles and pattern-matches correctly
- [ ] `MultiDatabaseManifest` serializes to JSON with all required metadata
- [ ] Single bundle mode writes one file with all databases (existing behavior preserved)
- [ ] One-per-database mode writes manifest + N schema files + M failure stub files
- [ ] Failure stub files include `collection_status: CollectionStatus::Failed` + error summary
- [ ] CLI flags `--one-per-database` and `--output-dir` correctly control output mode
- [ ] Integration tests verify file structure for both modes with real PostgreSQL server
- [ ] Encryption/compression work correctly for both output modes

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - MultiDatabaseOutput section)
- **Core Flows**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/661dbe3d-b679-4287-991e-26f4a0dd98b9` (Flow 2 - single bundle default with opt-in)