# Database Support

DBSurveyor supports multiple database engines with comprehensive schema collection capabilities. This guide details the support level and specific features for each database type.

## Supported Databases

| Database   | Status     | Feature Flag | Default | Version Support |
| ---------- | ---------- | ------------ | ------- | --------------- |
| PostgreSQL | ✅ Full    | `postgresql` | ✅ Yes  | 9.6+            |
| SQLite     | ✅ Full    | `sqlite`     | ✅ Yes  | 3.6+            |
| MySQL      | 🚧 Partial | `mysql`      | ❌ No   | 5.7+, 8.0+      |
| MongoDB    | 🚧 Planned | `mongodb`    | ❌ No   | 4.0+            |
| SQL Server | 🚧 Planned | `mssql`      | ❌ No   | 2017+           |

**Legend:**

- ✅ Full: Complete feature support with comprehensive testing
- ⚠️ Partial: Core features supported, some limitations
- 🚧 Basic: Minimal support, under development

## PostgreSQL Support

**Status**: ✅ Full Support (Default)\
**Feature Flag**: `postgresql`\
**Driver**: SQLx with Tokio runtime

### Connection Examples

```bash
# Basic connection
dbsurveyor-collect postgres://user:password@localhost:5432/mydb

# With SSL
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=require"

# Connection pooling
dbsurveyor-collect "postgres://user:pass@localhost/db?pool_max_conns=5"

# Multiple schemas
dbsurveyor-collect "postgres://user:pass@localhost/db?search_path=public,custom"
```

### Supported Objects

| Object Type | Support | Notes                                      |
| ----------- | ------- | ------------------------------------------ |
| Tables      | ✅ Full | Including partitioned tables               |
| Views       | ✅ Full | Regular and materialized views             |
| Indexes     | ✅ Full | All index types (B-tree, Hash, GiST, etc.) |
| Constraints | ✅ Full | PK, FK, Check, Unique, Exclusion           |
| Functions   | ✅ Full | SQL and PL/pgSQL functions                 |
| Procedures  | ✅ Full | Stored procedures (PostgreSQL 11+)         |
| Triggers    | ✅ Full | Row and statement triggers                 |
| Types       | ✅ Full | Custom types, domains, enums               |
| Extensions  | ✅ Full | Installed extensions                       |
| Schemas     | ✅ Full | Multiple schema support                    |

### PostgreSQL-Specific Features

```sql
-- Advanced data types
CREATE TABLE example (
    id SERIAL PRIMARY KEY,
    data JSONB,
    tags TEXT[],
    coordinates POINT,
    search_vector TSVECTOR
);

-- Custom types
CREATE TYPE status_enum AS ENUM ('active', 'inactive', 'pending');
CREATE DOMAIN email AS TEXT CHECK (VALUE ~ '^[^@]+@[^@]+\.[^@]+$');

-- Advanced indexes
CREATE INDEX CONCURRENTLY idx_data_gin ON example USING GIN (data);
CREATE INDEX idx_search ON example USING GIN (search_vector);
```

### Collection Output

```json
{
  "database_info": {
    "name": "mydb",
    "version": "13.7",
    "encoding": "UTF8",
    "collation": "en_US.UTF-8"
  },
  "tables": [
    {
      "name": "example",
      "schema": "public",
      "columns": [
        {
          "name": "data",
          "data_type": {
            "Json": {}
          },
          "is_nullable": true
        }
      ]
    }
  ]
}
```

## SQLite Support

**Status**: ✅ Full Support (Default)\
**Feature Flag**: `sqlite`\
**Driver**: SQLx with Tokio runtime

### Connection Examples

```bash
# File path
dbsurveyor-collect sqlite:///path/to/database.db
dbsurveyor-collect /path/to/database.sqlite

# Read-only mode
dbsurveyor-collect "sqlite:///path/to/db.sqlite?mode=ro"

# In-memory database (for testing)
dbsurveyor-collect "sqlite://:memory:"
```

### Supported Objects

| Object Type    | Support    | Notes                            |
| -------------- | ---------- | -------------------------------- |
| Tables         | ✅ Full    | Including WITHOUT ROWID tables   |
| Views          | ✅ Full    | Regular views                    |
| Indexes        | ✅ Full    | B-tree and partial indexes       |
| Constraints    | ⚠️ Partial | Limited constraint introspection |
| Triggers       | ✅ Full    | BEFORE, AFTER, INSTEAD OF        |
| Virtual Tables | ✅ Full    | FTS, R-Tree, etc.                |
| Attached DBs   | ✅ Full    | Multiple attached databases      |

### SQLite-Specific Features

```sql
-- WITHOUT ROWID tables
CREATE TABLE example (
    id TEXT PRIMARY KEY,
    data TEXT
) WITHOUT ROWID;

-- Virtual tables
CREATE VIRTUAL TABLE docs_fts USING fts5(title, content);

-- Partial indexes
CREATE INDEX idx_active_users ON users(name) WHERE active = 1;

-- JSON support (SQLite 3.38+)
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    data JSON
);
```

### Limitations

- Limited constraint introspection (SQLite stores constraints as DDL text)
- No stored procedures or functions
- No custom types (uses affinity system)
- No schemas (single namespace per database file)

## MySQL Support

**Status**: ⚠️ Partial Support\
**Feature Flag**: `mysql` (not default)\
**Driver**: SQLx with Tokio runtime

### Connection Examples

```bash
# Basic connection
dbsurveyor-collect mysql://root:password@localhost:3306/mydb

# With SSL
dbsurveyor-collect "mysql://user:pass@localhost/db?ssl-mode=REQUIRED"

# Character set
dbsurveyor-collect "mysql://user:pass@localhost/db?charset=utf8mb4"
```

### Supported Objects

| Object Type | Support    | Notes                            |
| ----------- | ---------- | -------------------------------- |
| Tables      | ✅ Full    | All storage engines              |
| Views       | ✅ Full    | Regular views                    |
| Indexes     | ✅ Full    | Primary, Unique, Index, Fulltext |
| Constraints | ⚠️ Partial | PK, FK, Check (MySQL 8.0+)       |
| Procedures  | ✅ Full    | Stored procedures                |
| Functions   | ✅ Full    | User-defined functions           |
| Triggers    | ✅ Full    | BEFORE, AFTER triggers           |
| Events      | ✅ Full    | Scheduled events                 |

### MySQL-Specific Features

```sql
-- Storage engines
CREATE TABLE innodb_table (
    id INT PRIMARY KEY AUTO_INCREMENT,
    data TEXT
) ENGINE=InnoDB;

-- Partitioning
CREATE TABLE partitioned (
    id INT,
    created_date DATE
) PARTITION BY RANGE (YEAR(created_date)) (
    PARTITION p2023 VALUES LESS THAN (2024),
    PARTITION p2024 VALUES LESS THAN (2025)
);

-- Full-text indexes
CREATE TABLE articles (
    id INT PRIMARY KEY,
    title VARCHAR(255),
    content TEXT,
    FULLTEXT(title, content)
);
```

### Known Limitations

- Check constraints only supported in MySQL 8.0+
- Limited JSON introspection compared to PostgreSQL
- Some storage engine specific features not captured

### Security Advisory

⚠️ **RUSTSEC-2023-0071**: MySQL support uses the RSA crate which has a known timing side-channel vulnerability. MySQL support is disabled by default. Use PostgreSQL or SQLite for production environments.

## MongoDB Support

**Status**: 🚧 Basic Support\
**Feature Flag**: `mongodb` (not default)\
**Driver**: Official MongoDB Rust driver

### Connection Examples

```bash
# Basic connection
dbsurveyor-collect mongodb://user:password@localhost:27017/mydb

# With authentication database
dbsurveyor-collect "mongodb://user:pass@localhost/mydb?authSource=admin"

# Replica set
dbsurveyor-collect "mongodb://user:pass@host1,host2,host3/mydb?replicaSet=rs0"
```

### Supported Objects

| Object Type      | Support    | Notes                             |
| ---------------- | ---------- | --------------------------------- |
| Collections      | ✅ Full    | Document collections              |
| Indexes          | ✅ Full    | Single field, compound, text, geo |
| Schema Inference | ✅ Basic   | Inferred from document sampling   |
| GridFS           | ⚠️ Partial | Basic GridFS collection detection |
| Views            | 🚧 Planned | Aggregation pipeline views        |

### MongoDB-Specific Features

```javascript
// Schema inference from documents
{
  "_id": ObjectId("..."),
  "name": "string",
  "age": "number",
  "tags": ["array", "of", "strings"],
  "address": {
    "street": "string",
    "city": "string"
  }
}

// Index types
db.users.createIndex({ "name": 1 })                    // Single field
db.users.createIndex({ "name": 1, "age": -1 })         // Compound
db.articles.createIndex({ "title": "text" })           // Text search
db.locations.createIndex({ "coordinates": "2dsphere" }) // Geospatial
```

### Current Limitations

- Schema inference is sampling-based (may miss rare fields)
- No aggregation pipeline analysis
- Limited sharding information
- No user-defined functions

## SQL Server Support

**Status**: 🚧 Basic Support\
**Feature Flag**: `mssql` (not default)\
**Driver**: Tiberius (native TDS protocol)

### Connection Examples

```bash
# Basic connection
dbsurveyor-collect mssql://sa:password@localhost:1433/mydb

# Windows Authentication (planned)
dbsurveyor-collect "mssql://localhost/mydb?trusted_connection=yes"

# Named instance
dbsurveyor-collect "mssql://sa:pass@localhost\\SQLEXPRESS/mydb"
```

### Supported Objects

| Object Type | Support    | Notes                   |
| ----------- | ---------- | ----------------------- |
| Tables      | ✅ Full    | User tables             |
| Views       | ✅ Full    | Regular views           |
| Indexes     | ⚠️ Partial | Basic index information |
| Constraints | ⚠️ Partial | PK, FK constraints      |
| Procedures  | 🚧 Planned | Stored procedures       |
| Functions   | 🚧 Planned | User-defined functions  |
| Triggers    | 🚧 Planned | DML triggers            |

### Current Limitations

- Limited to basic table and view introspection
- No stored procedure analysis yet
- No advanced SQL Server features (CLR, XML, spatial)
- Windows Authentication not yet supported

## Feature Comparison Matrix

| Feature      | PostgreSQL | SQLite | MySQL | MongoDB | SQL Server |
| ------------ | ---------- | ------ | ----- | ------- | ---------- |
| Tables       | ✅         | ✅     | ✅    | ✅      | ✅         |
| Views        | ✅         | ✅     | ✅    | 🚧      | ✅         |
| Indexes      | ✅         | ✅     | ✅    | ✅      | ⚠️         |
| Constraints  | ✅         | ⚠️     | ⚠️    | ❌      | ⚠️         |
| Procedures   | ✅         | ❌     | ✅    | ❌      | 🚧         |
| Functions    | ✅         | ❌     | ✅    | ❌      | 🚧         |
| Triggers     | ✅         | ✅     | ✅    | ❌      | 🚧         |
| Custom Types | ✅         | ❌     | ⚠️    | ❌      | 🚧         |
| JSON Support | ✅         | ✅     | ⚠️    | ✅      | 🚧         |
| Multi-DB     | ✅         | ⚠️     | ✅    | ❌      | 🚧         |

## Building with Database Support

### Default Build

```bash
# Includes PostgreSQL and SQLite
cargo build --release
```

### Custom Database Selection

```bash
# PostgreSQL only
cargo build --release --no-default-features --features postgresql

# All databases
cargo build --release --all-features

# Specific combination
cargo build --release --no-default-features --features postgresql,mysql,encryption
```

### Feature Dependencies

```toml
[features]
postgresql = ["sqlx", "sqlx/postgres", "sqlx/runtime-tokio-rustls"]
mysql = ["sqlx", "sqlx/mysql", "sqlx/runtime-tokio-rustls"]
sqlite = ["sqlx", "sqlx/sqlite", "sqlx/runtime-tokio-rustls"]
mongodb = ["dep:mongodb"]
mssql = ["dep:tiberius"]
```

## Database-Specific Best Practices

### PostgreSQL

```bash
# Use read-only user
CREATE USER dbsurveyor_readonly;
GRANT CONNECT ON DATABASE mydb TO dbsurveyor_readonly;
GRANT USAGE ON SCHEMA public TO dbsurveyor_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO dbsurveyor_readonly;

# For multiple schemas
GRANT USAGE ON SCHEMA schema1, schema2 TO dbsurveyor_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA schema1, schema2 TO dbsurveyor_readonly;
```

### MySQL

```bash
# Create read-only user
CREATE USER 'dbsurveyor_readonly'@'%' IDENTIFIED BY 'password';
GRANT SELECT ON mydb.* TO 'dbsurveyor_readonly'@'%';
GRANT SELECT ON information_schema.* TO 'dbsurveyor_readonly'@'%';
```

### SQLite

```bash
# Ensure read permissions
chmod 644 /path/to/database.db

# Use read-only mode for safety
dbsurveyor-collect "sqlite:///path/to/db.sqlite?mode=ro"
```

### MongoDB

```javascript
// Create read-only user
use admin
db.createUser({
  user: "dbsurveyor_readonly",
  pwd: "password",
  roles: [
    { role: "read", db: "mydb" },
    { role: "read", db: "config" }  // For sharding info
  ]
})
```

## Troubleshooting Database Connections

### Connection Issues

```bash
# Test connection first
dbsurveyor-collect test postgres://user:pass@localhost/db

# Check network connectivity
telnet localhost 5432  # PostgreSQL
telnet localhost 3306  # MySQL
telnet localhost 27017 # MongoDB
telnet localhost 1433  # SQL Server
```

### Permission Issues

```bash
# PostgreSQL: Check permissions
psql -h localhost -U user -d db -c "\dt"

# MySQL: Check permissions
mysql -h localhost -u user -p -e "SHOW TABLES;" db

# SQLite: Check file permissions
ls -la /path/to/database.db
```

### Driver Issues

```bash
# Check compiled features
dbsurveyor-collect list

# Verify feature compilation
cargo build --features postgresql --verbose
```

## Roadmap

### Planned Improvements

**PostgreSQL**:

- Advanced partitioning support
- Extension-specific object types
- Performance statistics collection

**MySQL**:

- Enhanced JSON column support
- Partition pruning analysis
- Storage engine optimization hints

**MongoDB**:

- Aggregation pipeline analysis
- Sharding topology mapping
- Index usage statistics

**SQL Server**:

- Complete stored procedure support
- CLR integration analysis
- Spatial data type support

**General**:

- Cross-database schema comparison
- Migration script generation
- Performance benchmarking integration

### Contributing Database Support

See the [Contributing Guide](./contributing.md) for information on adding support for new database engines or improving existing support.
