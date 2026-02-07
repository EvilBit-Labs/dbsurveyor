# Implement SQL DDL Reconstruction

## Overview

Replace placeholder SQL DDL generation with production-ready SQL reconstruction. Generate executable DDL scripts that recreate database schemas in target environments.

## Scope

**What's Included**:
- Implement full SQL DDL generation in `generate_sql()` in `file:dbsurveyor/src/main.rs`:
  - **CREATE TABLE statements**: For each table:
    - Column definitions with types, nullability, defaults
    - Primary key constraints
    - Unique constraints
    - Check constraints
    - Foreign key constraints (with ON DELETE/UPDATE actions)
  - **CREATE INDEX statements**: For each index (excluding primary key indexes)
  - **CREATE VIEW statements**: For each view (if definition available)
  - **CREATE PROCEDURE statements**: For each procedure (if definition available)
  - **CREATE FUNCTION statements**: For each function (if definition available)
  - **CREATE TRIGGER statements**: For each trigger (if definition available)
  - **CREATE TYPE statements**: For custom types (if applicable)
- Add SQL dialect support:
  - PostgreSQL dialect (default)
  - MySQL dialect
  - SQLite dialect
  - Generic SQL (ANSI standard, best-effort)
- Add SQL formatting utilities:
  - Identifier quoting (dialect-specific: `"` for PostgreSQL, `` ` `` for MySQL, `"` for SQLite)
  - Type mapping (unified types → dialect-specific types)
  - Constraint naming conventions
  - Statement ordering (types → tables → indexes → views → procedures → functions → triggers)
- Handle multi-database bundles:
  - Combined mode: Generate single SQL file with per-database sections
  - Split mode: Generate separate SQL files per database
- Add comments in SQL output:
  - Header with database name, generation date, source file
  - Section headers for tables, views, etc.
  - Inline comments for table/column comments from schema
- Add unit tests for SQL generation (all dialects)
- Add integration tests with real schema files

**What's Explicitly Out**:
- Data migration (INSERT statements) (deferred)
- Advanced database-specific features (partitions, tablespaces, etc.) (deferred)
- SQL Server and Oracle dialects (deferred to post-v1.0)

## SQL Dialect Differences

| Feature | PostgreSQL | MySQL | SQLite | Generic |
|---------|------------|-------|--------|---------|
| Identifier Quote | `"name"` | `` `name` `` | `"name"` | `"name"` |
| Auto Increment | `SERIAL` | `AUTO_INCREMENT` | `INTEGER PRIMARY KEY` | `GENERATED` |
| Boolean Type | `BOOLEAN` | `TINYINT(1)` | `INTEGER` | `BOOLEAN` |
| Text Type | `TEXT` | `TEXT` | `TEXT` | `TEXT` |
| Timestamp | `TIMESTAMP` | `TIMESTAMP` | `TEXT` | `TIMESTAMP` |

## Acceptance Criteria

- [ ] SQL DDL generation produces executable CREATE statements for all schema objects
- [ ] PostgreSQL dialect generates valid PostgreSQL DDL
- [ ] MySQL dialect generates valid MySQL DDL
- [ ] SQLite dialect generates valid SQLite DDL
- [ ] Generic SQL dialect generates ANSI-compliant DDL (best-effort)
- [ ] Foreign key constraints include ON DELETE/UPDATE actions
- [ ] Indexes are created with correct uniqueness and column order
- [ ] Views, procedures, functions, triggers are included (if definitions available)
- [ ] Multi-database bundles generate combined or split SQL based on CLI flags
- [ ] Generated SQL includes helpful comments and section headers
- [ ] Unit tests verify SQL generation for all dialects
- [ ] Integration tests verify generated SQL can be executed in target databases
- [ ] SQL output is formatted and readable

## References

- **Spec**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/820ca524-8c7d-4939-8097-f1158e7d67ea` (Tech Plan - Postprocessor)
- **Core Flows**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/661dbe3d-b679-4287-991e-26f4a0dd98b9` (Flow 7 - SQL DDL reconstruction)
- **Epic Brief**: `spec:de2eeeb8-bfeb-4a11-98aa-84efc70568b2/64fc1d47-e1e3-40db-a5dc-8dc9c248814c` (v1.0 Must Have - SQL reconstruction)