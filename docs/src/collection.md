# Database Collection

The `dbsurveyor-collect` tool connects to databases and extracts comprehensive schema information. This guide covers all collection features and options.

## Basic Collection

### Simple Schema Collection

```bash
# Collect from PostgreSQL
dbsurveyor-collect postgres://user:password@localhost:5432/mydb

# Collect from SQLite
dbsurveyor-collect sqlite:///path/to/database.db

# Collect from MySQL
dbsurveyor-collect mysql://root:password@localhost:3306/mydb
```

### Connection Testing

Always test your connection before collecting:

```bash
# Test connection without collecting schema
dbsurveyor-collect test postgres://user:pass@localhost/db

# Example output:
# ✓ Connection test successful
# Connection to PostgreSQL database successful
```

## Advanced Collection Options

### Output Formats

Control how the collected schema is stored:

```bash
# Standard JSON (default)
dbsurveyor-collect --output schema.json postgres://localhost/db

# Compressed with Zstandard
dbsurveyor-collect --compress --output schema.json.zst postgres://localhost/db

# Encrypted with AES-GCM (prompts for password)
dbsurveyor-collect --encrypt --output schema.enc postgres://localhost/db

# Both compressed and encrypted
dbsurveyor-collect --compress --encrypt postgres://localhost/db
```

### Multi-Database Collection

Collect schemas from all databases on a server:

```bash
# Collect all accessible databases
dbsurveyor-collect --all-databases postgres://user:pass@localhost

# Include system databases (postgres, template0, etc.)
dbsurveyor-collect --all-databases --include-system-databases postgres://localhost

# Exclude specific databases
dbsurveyor-collect --all-databases --exclude-databases postgres,template0,template1 postgres://localhost
```

### Performance and Stealth Options

```bash
# Throttle operations for stealth (1 second delay between operations)
dbsurveyor-collect --throttle 1000 postgres://localhost/db

# Limit sample data collection
dbsurveyor-collect --sample 50 postgres://localhost/db

# Verbose logging for debugging
dbsurveyor-collect -vvv postgres://localhost/db
```

## What Gets Collected

DBSurveyor extracts comprehensive database metadata:

### Core Schema Objects

- **Tables**: Names, schemas, columns, data types, constraints
- **Views**: Definitions, columns, dependencies
- **Indexes**: Names, columns, uniqueness, types
- **Constraints**: Primary keys, foreign keys, check constraints, unique constraints
- **Procedures**: Stored procedures with parameters and definitions
- **Functions**: User-defined functions with signatures
- **Triggers**: Event triggers with timing and definitions
- **Custom Types**: User-defined types, enums, domains

### Metadata Information

- **Database Info**: Name, version, size, encoding, collation
- **Collection Metadata**: Timestamp, duration, warnings, collector version
- **Statistics**: Row counts, object counts, relationship mappings

### Sample Data (Optional)

When enabled, DBSurveyor can collect sample data:

```bash
# Collect sample data (100 rows per table by default)
dbsurveyor-collect --sample 100 postgres://localhost/db

# Disable sample collection
dbsurveyor-collect --sample 0 postgres://localhost/db
```

**Security Note**: Sample data may contain sensitive information. Review outputs before sharing.

## Database-Specific Features

### PostgreSQL

```bash
# Full PostgreSQL collection
dbsurveyor-collect postgres://user:pass@localhost:5432/mydb
```

**Collected Objects**:

- Tables, views, materialized views
- Indexes (B-tree, Hash, GiST, SP-GiST, GIN, BRIN)
- Constraints (PK, FK, Check, Unique, Exclusion)
- Functions, procedures, triggers
- Custom types, domains, enums
- Extensions and schemas

**PostgreSQL-Specific**:

- JSONB columns and indexes
- Array types and operations
- Inheritance relationships
- Partitioned tables

### MySQL

```bash
# MySQL collection (requires --features mysql)
dbsurveyor-collect mysql://root:password@localhost:3306/mydb
```

**Collected Objects**:

- Tables, views
- Indexes (Primary, Unique, Index, Fulltext, Spatial)
- Constraints (PK, FK, Check, Unique)
- Stored procedures, functions, triggers
- Events and routines

**MySQL-Specific**:

- Storage engines (InnoDB, MyISAM, etc.)
- Partitioning information
- Auto-increment values
- Character sets and collations

### SQLite

```bash
# SQLite collection
dbsurveyor-collect sqlite:///path/to/database.db
dbsurveyor-collect /path/to/database.sqlite  # Alternative format
```

**Collected Objects**:

- Tables, views
- Indexes
- Triggers
- Constraints (limited support)

**SQLite-Specific**:

- ROWID and WITHOUT ROWID tables
- Virtual tables
- Attached databases
- PRAGMA settings

### MongoDB

```bash
# MongoDB collection (requires --features mongodb)
dbsurveyor-collect mongodb://user:pass@localhost:27017/mydb
```

**Collected Objects**:

- Collections and their schemas
- Indexes (single field, compound, text, geospatial)
- Document structure analysis
- Field statistics and types

**MongoDB-Specific**:

- Schema inference from documents
- Index usage statistics
- Sharding information
- GridFS collections

## Security Considerations

### Read-Only Operations

DBSurveyor only performs read operations:

- `SELECT` statements for data retrieval
- `DESCRIBE` or `SHOW` statements for metadata
- Information schema queries
- System catalog queries

**No write operations are ever performed.**

### Credential Handling

```bash
# Use environment variables to avoid command history
export DATABASE_URL="postgres://user:pass@localhost/db"
dbsurveyor-collect

# Credentials are sanitized in all logs and error messages
# "postgres://user:pass@localhost/db" becomes "postgres://user:****@localhost/db"
```

### Connection Security

```bash
# Use SSL/TLS connections when available
dbsurveyor-collect "postgres://user:pass@localhost/db?sslmode=require"

# Connection timeouts prevent hanging
# Default: 30 seconds for connection, 30 seconds for queries
```

## Troubleshooting Collection Issues

### Connection Problems

```bash
# Test connection first
dbsurveyor-collect test postgres://user:pass@localhost/db

# Check network connectivity
ping localhost
telnet localhost 5432  # PostgreSQL port

# Verify credentials
psql -h localhost -U user -d db -c "SELECT 1;"
```

### Permission Issues

```bash
# PostgreSQL: Grant read permissions
GRANT CONNECT ON DATABASE mydb TO dbsurveyor_user;
GRANT USAGE ON SCHEMA public TO dbsurveyor_user;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO dbsurveyor_user;

# MySQL: Grant read permissions
GRANT SELECT ON mydb.* TO 'dbsurveyor_user'@'%';

# SQLite: Ensure file read permissions
chmod 644 /path/to/database.db
```

### Large Database Handling

```bash
# Use compression for large schemas
dbsurveyor-collect --compress postgres://localhost/large_db

# Throttle operations to reduce load
dbsurveyor-collect --throttle 500 postgres://localhost/large_db

# Exclude large or unnecessary databases
dbsurveyor-collect --all-databases --exclude-databases logs,temp,backup postgres://localhost
```

### Memory and Performance

```bash
# Monitor collection progress
dbsurveyor-collect -v postgres://localhost/db

# For very large databases, consider collecting specific schemas
# (This feature is planned for future releases)
```

## Output File Structure

The collected schema file contains:

```json
{
  "format_version": "1.0",
  "database_info": {
    "name": "mydb",
    "version": "13.7",
    "size_bytes": 1048576,
    "encoding": "UTF8"
  },
  "tables": [...],
  "views": [...],
  "indexes": [...],
  "constraints": [...],
  "procedures": [...],
  "functions": [...],
  "triggers": [...],
  "custom_types": [...],
  "collection_metadata": {
    "collected_at": "2024-01-15T10:30:00Z",
    "collection_duration_ms": 5432,
    "collector_version": "0.1.0",
    "warnings": []
  }
}
```

## Next Steps

After collecting schema data:

1. **Generate Documentation**: Use `dbsurveyor` to create reports
2. **Analyze Schema**: Use analysis commands to understand the database structure
3. **Secure Storage**: Consider encrypting schema files for sensitive databases
4. **Version Control**: Track schema changes over time

See the [Documentation Generation](./documentation.md) guide for the next steps.
