# JSON Schema Specification

The `.dbsurveyor.json` format is the standard output format for DBSurveyor schema collection. This specification documents the complete structure, validation rules, and usage examples for the schema format.

## Overview

The `.dbsurveyor.json` format provides a comprehensive, validated representation of database schemas with the following characteristics:

- **Security-First**: No credential fields or sensitive data allowed
- **Validation-Complete**: Full JSON Schema validation ensures data integrity
- **Database-Agnostic**: Unified representation across PostgreSQL, MySQL, SQLite, and MongoDB
- **Version-Aware**: Format versioning for backward compatibility
- **Frictionless-Compatible**: Based on Frictionless Data Table Schema specification

## Schema Structure

### Root Object

Every `.dbsurveyor.json` file contains these required fields:

```json
{
  "format_version": "1.0",
  "database_info": { /* Database metadata */ },
  "tables": [ /* Table definitions */ ],
  "collection_metadata": { /* Collection process info */ }
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `format_version` | String | Schema format version (currently "1.0") |
| `database_info` | Object | Database-level information and status |
| `collection_metadata` | Object | Collection process metadata |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `tables` | Array | Table definitions (default: empty array) |
| `views` | Array | View definitions |
| `indexes` | Array | Database indexes |
| `constraints` | Array | Database constraints |
| `procedures` | Array | Stored procedures |
| `functions` | Array | Database functions |
| `triggers` | Array | Database triggers |
| `custom_types` | Array | Custom data types |
| `samples` | Array | Data samples from tables |

## Database Information

The `database_info` object contains essential database metadata:

```json
{
  "name": "production_db",
  "version": "13.7",
  "size_bytes": 1073741824,
  "encoding": "UTF8",
  "collation": "en_US.UTF-8",
  "owner": "dbadmin",
  "is_system_database": false,
  "access_level": "Full",
  "collection_status": "Success"
}
```

### Access Levels

- **`Full`**: Complete schema access with all metadata
- **`Limited`**: Partial access due to permission constraints
- **`None`**: No access to schema information

### Collection Status

- **`"Success"`**: Schema collected successfully
- **`{"Failed": {"error": "Permission denied"}}`**: Collection failed with reason
- **`{"Skipped": {"reason": "System database"}}`**: Database skipped with explanation

## Table Structure

Tables are defined with comprehensive metadata:

```json
{
  "name": "users",
  "schema": "public",
  "columns": [
    {
      "name": "id",
      "data_type": {"Integer": {"bits": 32, "signed": true}},
      "is_nullable": false,
      "is_primary_key": true,
      "is_auto_increment": true,
      "ordinal_position": 1,
      "comment": "Unique user identifier"
    },
    {
      "name": "email",
      "data_type": {"String": {"max_length": 255}},
      "is_nullable": false,
      "ordinal_position": 2,
      "comment": "User email address"
    }
  ],
  "primary_key": {
    "name": "users_pkey",
    "columns": ["id"]
  },
  "foreign_keys": [
    {
      "name": "users_profile_fk",
      "columns": ["profile_id"],
      "referenced_table": "profiles",
      "referenced_columns": ["id"],
      "on_delete": "Cascade",
      "on_update": "Cascade"
    }
  ],
  "indexes": [
    {
      "name": "users_email_idx",
      "columns": [{"name": "email", "order": "asc"}],
      "is_unique": true,
      "index_type": "btree"
    }
  ],
  "comment": "User account information",
  "row_count": 50000
}
```

## Data Type System

DBSurveyor uses a unified data type system that maps database-specific types to a common representation:

### Basic Types

```json
"Boolean"                    // Boolean values
"Date"                       // Date without time
"Json"                       // JSON data
"Uuid"                       // UUID/GUID values
```

### String Types

```json
{
  "String": {
    "max_length": 255        // Maximum character length
  }
}
```

### Numeric Types

```json
{
  "Integer": {
    "bits": 32,              // Bit size: 8, 16, 32, 64, 128
    "signed": true           // Signed or unsigned
  }
}

{
  "Float": {
    "precision": 53          // Floating point precision (1-53)
  }
}
```

### Date/Time Types

```json
{
  "DateTime": {
    "with_timezone": true    // Includes timezone information
  }
}

{
  "Time": {
    "with_timezone": false   // Time without timezone
  }
}
```

### Complex Types

```json
{
  "Array": {
    "element_type": "String" // Array element type
  }
}

{
  "Binary": {
    "max_length": 1024       // Maximum binary length
  }
}

{
  "Custom": {
    "type_name": "geometry"  // Database-specific custom type
  }
}
```

## Constraints and Relationships

### Primary Keys

```json
{
  "name": "users_pkey",
  "columns": ["id"]
}
```

### Foreign Keys

```json
{
  "name": "orders_user_fk",
  "columns": ["user_id"],
  "referenced_table": "users",
  "referenced_schema": "public",
  "referenced_columns": ["id"],
  "on_delete": "Cascade",
  "on_update": "Restrict"
}
```

**Referential Actions**:

- **`Cascade`**: Delete/update cascades to related records
- **`SetNull`**: Set foreign key to NULL
- **`SetDefault`**: Set foreign key to default value
- **`Restrict`**: Prevent deletion/update if references exist
- **`NoAction`**: No automatic action

### Check Constraints

```json
{
  "name": "users_age_check",
  "constraint_type": "Check",
  "definition": "age >= 0 AND age <= 150",
  "enforced": true
}
```

## Indexes

Index definitions include performance characteristics:

```json
{
  "name": "users_email_idx",
  "table_name": "users",
  "schema": "public",
  "columns": [
    {
      "name": "email",
      "order": "asc",
      "nulls_order": "last"
    }
  ],
  "is_unique": true,
  "is_primary": false,
  "index_type": "btree",
  "comment": "Unique index on email for fast lookups"
}
```

**Index Types**:

- **`btree`**: Balanced tree (default)
- **`hash`**: Hash-based index
- **`gin`**: Generalized inverted index
- **`gist`**: Generalized search tree
- **`spgist`**: Space-partitioned GiST

## Data Sampling

Optional data samples provide insight into actual data:

```json
{
  "samples": [
    {
      "table_name": "users",
      "schema_name": "public",
      "rows": [
        {
          "id": 1001,
          "email": "user1001@example.com",
          "created_at": "2024-01-15T09:00:00Z"
        },
        {
          "id": 1002,
          "email": "user1002@example.com",
          "created_at": "2024-01-15T09:15:00Z"
        }
      ],
      "sample_size": 2,
      "total_rows": 50000,
      "sampling_strategy": {"MostRecent": {"limit": 10}},
      "collected_at": "2024-01-15T10:30:00Z",
      "warnings": ["Large table - limited sample collected"]
    }
  ]
}
```

**Sampling Strategies**:

- **`{"MostRecent": {"limit": 10}}`**: Latest N rows
- **`{"Random": {"limit": 100}}`**: Random sample of N rows
- **`"None"`**: No sampling performed

## Multi-Database Collections

For server-level collections, the format supports multiple databases:

```json
{
  "format_version": "1.0",
  "server_info": {
    "server_type": "PostgreSQL",
    "version": "13.7",
    "host": "localhost",
    "port": 5432,
    "total_databases": 5,
    "collected_databases": 3,
    "system_databases_excluded": 2,
    "connection_user": "dbadmin",
    "has_superuser_privileges": true,
    "collection_mode": {
      "MultiDatabase": {
        "discovered": 5,
        "collected": 3,
        "failed": 0
      }
    }
  },
  "databases": [
    /* Individual database schemas */
  ],
  "collection_metadata": {
    "collected_at": "2024-01-15T10:30:00Z",
    "collection_duration_ms": 2500,
    "collector_version": "1.0.0",
    "warnings": []
  }
}
```

## Collection Metadata

Every schema file includes metadata about the collection process:

```json
{
  "collection_metadata": {
    "collected_at": "2024-01-15T10:30:00Z",
    "collection_duration_ms": 1500,
    "collector_version": "1.0.0",
    "warnings": [
      "Large table 'audit_logs' - collection took 45 seconds",
      "Custom type 'geometry' not fully supported"
    ],
    "collector_options": {
      "include_system_tables": false,
      "sample_data": true,
      "max_sample_size": 1000
    }
  }
}
```

## Validation Rules

### Security Validation

The schema enforces strict security rules:

- **No credential fields**: Field names cannot contain password, secret, token, etc.
- **No connection strings**: Database URLs are automatically filtered
- **No sensitive patterns**: Common credential patterns are rejected
- **Sanitized output**: All error messages are credential-free

### Data Validation

- **String lengths**: Maximum 255 characters for names, 1000 for comments
- **Array limits**: Maximum 1000 items in arrays
- **Nesting depth**: Maximum 10 levels of object nesting
- **File size**: Maximum 100MB per schema file

### Type Validation

- **Required fields**: All mandatory fields must be present
- **Data type consistency**: Values must match declared types
- **Constraint validation**: Foreign keys must reference valid tables
- **Index validation**: Index columns must exist in referenced table

## Usage Examples

### Basic Schema Collection

```bash
# Collect PostgreSQL schema
dbsurveyor-collect postgres://user:pass@localhost/mydb --output schema.json

# Validate the output
dbsurveyor schema schema.json --validate

# Generate documentation
dbsurveyor schema schema.json --format markdown --output schema.md
```

### Multi-Database Collection

```bash
# Collect all databases on server
dbsurveyor-collect postgres://admin:pass@localhost --all-databases --output server_schema.json

# Process specific database
dbsurveyor schema server_schema.json --database app_db --format json --output app_schema.json
```

### Schema Validation

```bash
# Validate against JSON Schema
dbsurveyor validate schema.json

# Check for specific issues
dbsurveyor validate schema.json --check-security --check-constraints
```

## Error Handling

### Validation Errors

When validation fails, DBSurveyor provides detailed error information:

```json
{
  "validation_errors": [
    {
      "path": "/tables/0/columns/1/data_type",
      "message": "Invalid data type: expected String, Integer, Boolean, Date, Json, Uuid, Array, Binary, Custom, or DateTime",
      "value": "VARCHAR",
      "suggestion": "Use {\"String\": {\"max_length\": 255}} instead"
    }
  ]
}
```

### Collection Warnings

Warnings are included in the metadata for non-critical issues:

```json
{
  "warnings": [
    "Large table 'audit_logs' (1.2M rows) - collection took 45 seconds",
    "Custom type 'geometry' not fully supported - using Custom type",
    "Table 'temp_users' appears to be temporary - may not persist"
  ]
}
```

## Version Compatibility

### Current Version: 1.0

- **Format**: Stable and fully supported
- **Validation**: Complete JSON Schema validation
- **Features**: All documented features available
- **Backward Compatibility**: N/A (first version)

### Future Versions

The schema is designed for evolution:

1. **Additive Changes**: New optional fields can be added
2. **Version Detection**: Format version enables version-specific handling
3. **Migration Support**: Tools will support upgrading between versions
4. **Deprecation Path**: Old fields will be marked before removal

## Integration

### Documentation Tools

The schema format integrates with all DBSurveyor tools:

- **`dbsurveyor-collect`**: Generates schema files
- **`dbsurveyor`**: Processes and validates schemas
- **`dbsurveyor-docs`**: Generates documentation from schemas
- **`dbsurveyor-validate`**: Standalone validation tool

### External Tools

The format is compatible with:

- **JSON Schema validators**: jsonschema, ajv, etc.
- **Data analysis tools**: pandas, jq, etc.
- **Documentation generators**: Docusaurus, MkDocs, etc.
- **CI/CD pipelines**: GitHub Actions, GitLab CI, etc.

## Best Practices

### Schema Collection

1. **Use descriptive names**: Avoid generic names like "db1", "test"
2. **Include comments**: Add meaningful descriptions for tables and columns
3. **Sample strategically**: Use sampling for large tables to avoid huge files
4. **Validate early**: Check schemas immediately after collection

### Schema Storage

1. **Version control**: Track schema changes in Git
2. **Backup regularly**: Keep historical schema versions
3. **Compress large files**: Use `.zst` compression for schemas >1MB
4. **Secure access**: Limit access to production schemas

### Schema Processing

1. **Validate inputs**: Always validate before processing
2. **Handle errors gracefully**: Check collection status before proceeding
3. **Monitor performance**: Track collection times for optimization
4. **Document changes**: Keep records of schema evolution

## Troubleshooting

### Common Issues

**Collection Fails with Permission Error**

```bash
# Check database user privileges
dbsurveyor-collect postgres://user:pass@localhost/db --test-connection

# Verify user has SELECT on information_schema
GRANT SELECT ON ALL TABLES IN SCHEMA information_schema TO username;
```

**Large Schema File Size**

```bash
# Use compression
dbsurveyor-collect postgres://localhost/db --compress

# Limit data sampling
dbsurveyor-collect postgres://localhost/db --max-sample-size 100
```

**Validation Errors**

```bash
# Check schema format
dbsurveyor validate schema.json --verbose

# Fix common issues
dbsurveyor fix schema.json --output fixed_schema.json
```

### Performance Optimization

1. **Connection pooling**: Use connection pooling for large databases
2. **Parallel collection**: Collect multiple databases simultaneously
3. **Selective sampling**: Only sample essential tables
4. **Incremental updates**: Collect only changed schemas

This specification provides a complete reference for the `.dbsurveyor.json` format, ensuring consistent, validated, and secure schema collection across all supported database types.
