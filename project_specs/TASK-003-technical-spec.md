# Technical Specification: TASK-003

## Implement Core Schema Discovery and Data Sampling Engine

**Issue**: [#3](https://github.com/EvilBit-Labs/dbsurveyor/issues/3)
**Branch**: `3-task-003-implement-core-schema-discovery-and-data-sampling-engine`
**Generated**: 2026-01-28

---

## Issue Summary

Implement comprehensive schema discovery, metadata extraction, and privacy-controlled data sampling across SQL and NoSQL databases (PostgreSQL, MySQL, SQLite, MongoDB).

---

## Problem Statement

DBSurveyor needs a robust database schema discovery that:
- Enumerates database structures safely across multiple engine types
- Extracts comprehensive metadata (tables, views, procedures, functions, triggers, indexes)
- Collects representative data samples with configurable privacy controls
- Maintains operational security with read-only operations and throttling
- Works consistently across PostgreSQL, MySQL, SQLite, and MongoDB

---

## Current State Analysis

### What's Already Implemented

| Component | Status | Location |
|-----------|--------|----------|
| PostgreSQL Adapter | COMPLETE | `dbsurveyor-core/src/adapters/postgres/` |
| MySQL Adapter | PLACEHOLDER | `dbsurveyor-core/src/adapters/mysql.rs` |
| SQLite Adapter | PLACEHOLDER | `dbsurveyor-core/src/adapters/sqlite.rs` |
| MongoDB Adapter | PLACEHOLDER | `dbsurveyor-core/src/adapters/mongodb.rs` |
| DatabaseAdapter Trait | COMPLETE | `dbsurveyor-core/src/adapters/mod.rs` |
| Data Models | COMPLETE | `dbsurveyor-core/src/models.rs` |
| Configuration | COMPLETE | `dbsurveyor-core/src/adapters/config/` |
| Error Handling | COMPLETE | `dbsurveyor-core/src/error.rs` |
| Security/Encryption | COMPLETE | `dbsurveyor-core/src/security/` |

### PostgreSQL Reference Implementation (Complete)

The PostgreSQL adapter provides a complete reference with:
- Schema collection (`schema_collection.rs` - 31KB)
- Data sampling with ordering strategies (`sampling.rs` - 17KB)
- Type mapping (`type_mapping.rs` - 11KB)
- Multi-database enumeration (`enumeration.rs` - 10KB)
- Connection pooling (`connection.rs` - 22KB)

### Gaps to Address

1. **MySQL Adapter**: Full implementation needed
2. **SQLite Adapter**: Full implementation needed
3. **MongoDB Adapter**: Full implementation with schema inference
4. **Views Collection**: Empty in PostgreSQL, needs implementation
5. **Procedures/Functions/Triggers**: Stub implementations exist

---

## Technical Approach

### Architecture Pattern

Follow the existing trait-based adapter system:

```rust
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn test_connection(&self) -> Result<()>;
    async fn collect_schema(&self) -> Result<DatabaseSchema>;
    fn database_type(&self) -> DatabaseType;
    fn supports_feature(&self, feature: AdapterFeature) -> bool;
    fn connection_config(&self) -> ConnectionConfig;
}
```

### Feature Support Matrix

| Feature | PostgreSQL | MySQL | SQLite | MongoDB |
|---------|------------|-------|--------|---------|
| SchemaCollection | Yes | Yes | Yes | Yes |
| DataSampling | Yes | Yes | Yes | Yes |
| MultiDatabase | Yes | Yes | No | Yes |
| ConnectionPooling | Yes | Yes | No | No |
| QueryTimeout | Yes | Yes | Yes | Yes |
| ReadOnlyMode | Yes | Yes | Yes | No |

---

## Implementation Plan

### Phase 1: MySQL Adapter (Core)

**Files to Create/Modify**:

```text
dbsurveyor-core/src/adapters/mysql/
  mod.rs              # Main adapter struct + trait impl
  connection.rs       # Connection pool management
  schema_collection.rs # INFORMATION_SCHEMA queries
  type_mapping.rs     # MySQL -> UnifiedDataType
  sampling.rs         # Data sampling with ordering
  enumeration.rs      # Multi-database enumeration
  tests.rs            # Unit tests
```

**Key Queries**:
```sql
-- Tables
SELECT * FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = ?

-- Columns
SELECT * FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?

-- Indexes
SELECT * FROM INFORMATION_SCHEMA.STATISTICS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?

-- Foreign Keys
SELECT * FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
WHERE TABLE_SCHEMA = ? AND REFERENCED_TABLE_NAME IS NOT NULL

-- Views
SELECT * FROM INFORMATION_SCHEMA.VIEWS WHERE TABLE_SCHEMA = ?

-- Procedures/Functions
SELECT * FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = ?
```

### Phase 2: SQLite Adapter (Core)

**Files to Create/Modify**:

```text
dbsurveyor-core/src/adapters/sqlite/
  mod.rs              # Main adapter struct
  connection.rs       # Connection handling (no pooling)
  schema_collection.rs # sqlite_master parsing
  type_mapping.rs     # SQLite -> UnifiedDataType
  sampling.rs         # Data sampling
  tests.rs            # Unit tests
```

**Key Queries**:
```sql
-- Tables
SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'

-- Indexes
SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name = ?

-- Column info
PRAGMA table_info(?)

-- Foreign keys
PRAGMA foreign_key_list(?)

-- Views
SELECT name, sql FROM sqlite_master WHERE type='view'

-- Triggers
SELECT name, sql FROM sqlite_master WHERE type='trigger'
```

### Phase 3: MongoDB Adapter (Core)

**Files to Create/Modify**:

```text
dbsurveyor-core/src/adapters/mongodb/
  mod.rs              # Main adapter struct
  connection.rs       # MongoDB client management
  schema_inference.rs # Schema inference from documents
  type_mapping.rs     # BSON -> UnifiedDataType
  sampling.rs         # Document sampling
  enumeration.rs      # Database/collection enumeration
  tests.rs            # Unit tests
```

**Key Operations**:
```rust
// List databases
client.list_database_names().await

// List collections
db.list_collection_names().await

// Get indexes
collection.list_indexes().await

// Sample documents for schema inference
collection.find().limit(100).await

// Collection stats
db.run_command(doc! { "collStats": collection_name }).await
```

### Phase 4: PostgreSQL Enhancements

**Files to Modify**:

```text
dbsurveyor-core/src/adapters/postgres/
  views.rs            # NEW: View collection
  routines.rs         # NEW: Procedures/functions
  triggers.rs         # NEW: Trigger collection
  schema_collection.rs # Integrate new modules
```

**Additional Queries**:
```sql
-- Views
SELECT schemaname, viewname, definition
FROM pg_views WHERE schemaname NOT IN ('pg_catalog', 'information_schema')

-- Procedures
SELECT proname, pronargs, prorettype, prosrc, proargnames, proargtypes
FROM pg_proc p JOIN pg_namespace n ON p.pronamespace = n.oid
WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')

-- Triggers
SELECT tgname, relname, tgtype, prosrc
FROM pg_trigger t JOIN pg_class c ON t.tgrelid = c.oid
JOIN pg_proc p ON t.tgfoid = p.oid
WHERE NOT tgisinternal
```

---

## Test Plan

### Unit Tests (Per Adapter)

1. **Type Mapping Tests**
   - All source types map to valid UnifiedDataType
   - Edge cases (null types, unknown types)
   - Precision/scale preservation

2. **Query Generation Tests**
   - Parameterized query safety
   - SQL injection prevention
   - Proper escaping

3. **Configuration Tests**
   - Default values applied correctly
   - Environment variable overrides
   - Validation errors for invalid config

### Integration Tests (Testcontainers)

**File**: `dbsurveyor-core/tests/integration_*.rs`

1. **MySQL Integration**
   ```rust
   #[tokio::test]
   async fn test_mysql_schema_collection() {
       let container = GenericImage::new("mysql", "8.0")
           .with_env_var("MYSQL_ROOT_PASSWORD", "test")
           .start().await;
       // Create test schema, verify collection
   }
   ```

2. **SQLite Integration**
   ```rust
   #[tokio::test]
   async fn test_sqlite_schema_collection() {
       let temp_file = tempfile::NamedTempFile::new().unwrap();
       // Create test schema, verify collection
   }
   ```

3. **MongoDB Integration**
   ```rust
   #[tokio::test]
   async fn test_mongodb_schema_inference() {
       let container = GenericImage::new("mongo", "6.0").start().await;
       // Insert documents, verify schema inference
   }
   ```

### Performance Tests

- Schema discovery: <10s for 1000 tables
- Table analysis: <100ms per table
- Sample collection: <500ms for 100 rows
- Memory usage: <1GB for 10,000 tables

---

## Files to Modify/Create

### New Files

| Path | Purpose | Max Lines |
|------|---------|-----------|
| `adapters/mysql/mod.rs` | MySQL adapter main | 150 |
| `adapters/mysql/connection.rs` | Connection pooling | 200 |
| `adapters/mysql/schema_collection.rs` | Schema queries | 300 |
| `adapters/mysql/type_mapping.rs` | Type conversion | 200 |
| `adapters/mysql/sampling.rs` | Data sampling | 200 |
| `adapters/mysql/enumeration.rs` | Multi-DB enum | 150 |
| `adapters/mysql/tests.rs` | Unit tests | 200 |
| `adapters/sqlite/mod.rs` | SQLite adapter main | 150 |
| `adapters/sqlite/connection.rs` | Connection handling | 100 |
| `adapters/sqlite/schema_collection.rs` | sqlite_master parsing | 250 |
| `adapters/sqlite/type_mapping.rs` | Type conversion | 150 |
| `adapters/sqlite/sampling.rs` | Data sampling | 150 |
| `adapters/sqlite/tests.rs` | Unit tests | 150 |
| `adapters/mongodb/mod.rs` | MongoDB adapter main | 150 |
| `adapters/mongodb/connection.rs` | Client management | 100 |
| `adapters/mongodb/schema_inference.rs` | Schema inference | 300 |
| `adapters/mongodb/type_mapping.rs` | BSON conversion | 150 |
| `adapters/mongodb/sampling.rs` | Document sampling | 150 |
| `adapters/mongodb/enumeration.rs` | DB/collection enum | 150 |
| `adapters/mongodb/tests.rs` | Unit tests | 200 |
| `adapters/postgres/views.rs` | View collection | 150 |
| `adapters/postgres/routines.rs` | Proc/func collection | 200 |
| `adapters/postgres/triggers.rs` | Trigger collection | 150 |

### Files to Modify

| Path | Changes |
|------|---------|
| `adapters/mod.rs` | Add module declarations, update create_adapter() |
| `adapters/mysql.rs` | DELETE (replaced by mysql/ directory) |
| `adapters/sqlite.rs` | DELETE (replaced by sqlite/ directory) |
| `adapters/mongodb.rs` | DELETE (replaced by mongodb/ directory) |
| `adapters/postgres/schema_collection.rs` | Integrate views/routines/triggers |
| `Cargo.toml` | Add testcontainers-modules features |

---

## Success Criteria

### Database Engine Coverage
- [ ] PostgreSQL adapter with complete `information_schema` extraction
- [ ] MySQL adapter with `INFORMATION_SCHEMA` support
- [ ] SQLite adapter with `sqlite_master` parsing
- [ ] MongoDB adapter with schema inference via sampling
- [ ] Unified `DatabaseAdapter` trait implementation for all engines

### Schema Discovery Completeness
- [ ] Tables: name, schema, column definitions, constraints, row counts
- [ ] Views: definition extraction and dependency analysis
- [ ] Indexes: type, columns, uniqueness, partial conditions
- [ ] Foreign Keys: source/target relationships with cascade rules
- [ ] Stored Procedures/Functions: signature and parameter extraction
- [ ] Triggers: event types and associated logic
- [ ] Statistical Analysis: row counts, table sizes

### Privacy and Security Controls
- [ ] Configurable sampling with row limits per table
- [ ] Built-in redaction patterns for sensitive data (PII/PCI)
- [ ] Privacy level controls (None/Minimal/Standard/Maximum)
- [ ] Read-only connection validation
- [ ] No credential storage in output files

### Performance and Reliability
- [ ] Throttling controls with configurable delays
- [ ] Progress tracking for operations on large databases
- [ ] Error recovery for partial failures
- [ ] Performance: <10s for databases with <1000 tables

### Quality Gates
- [ ] All code passes `cargo clippy -- -D warnings`
- [ ] Test coverage >80%
- [ ] Documentation for all public APIs
- [ ] Integration tests with testcontainers

---

## Out of Scope

1. **SQL Server adapter** - Not required for initial release
2. **CLI integration** - Separate task (covered by existing collector binary)
3. **Output encryption** - Already implemented in security module
4. **Custom redaction patterns** - Already implemented in SamplingConfig
5. **Progress UI** - Separate task (indicatif already in dependencies)

---

## Dependencies

### Already in Cargo.toml
- `sqlx` with postgres/mysql/sqlite features
- `mongodb` (feature-gated)
- `async-trait`
- `serde` / `serde_json`
- `tokio`

### To Add
- `testcontainers-modules = { version = "0.11", features = ["mysql", "mongo"] }`

---

## Security Considerations

All implementations MUST maintain:
1. **Offline Operation**: No network calls except to target databases
2. **No Telemetry**: Zero data collection or external reporting
3. **Credential Protection**: Credentials are never in output files or logs
4. **Read-Only Operations**: SELECT/SHOW only
5. **Query Parameterization**: No string concatenation for SQL
6. **Error Sanitization**: Redact credentials in all error messages

---

## Reference Documents

- PostgreSQL implementation: `dbsurveyor-core/src/adapters/postgres/`
- Data models: `dbsurveyor-core/src/models.rs`
- Configuration: `dbsurveyor-core/src/adapters/config/`
- Placeholder macro: `dbsurveyor-core/src/adapters/placeholder.rs`
