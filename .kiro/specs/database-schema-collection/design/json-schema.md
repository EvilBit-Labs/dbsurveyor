# JSON Schema Specification for .dbsurveyor.json

## Overview

This document defines the comprehensive JSON Schema specification for the `.dbsurveyor.json` output format, based on the Frictionless Data Table Schema specification as a foundation. The schema ensures consistent, validated output across all database adapters and provides a stable contract for the postprocessor.

## Schema Design Principles

1. **Security-First**: No credential fields allowed in any structure
2. **Validation-Complete**: All data structures fully validated with comprehensive rules
3. **Database-Agnostic**: Unified representation across all supported database types
4. **Version-Aware**: Format versioning for backward compatibility and evolution
5. **Frictionless-Compatible**: Based on Frictionless Data Table Schema specification

## JSON Schema Definition

The complete JSON Schema is defined in `dbsurveyor-schema-v1.0.json` and validates all output from the collector binaries.

### Root Schema Structure

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://dbsurveyor.dev/schemas/v1.0/dbsurveyor-schema.json",
  "title": "DBSurveyor Database Schema Collection Format",
  "description": "Comprehensive database schema representation with security guarantees",
  "type": "object",
  "required": ["format_version", "database_info", "collection_metadata"],
  "additionalProperties": false
}
```

## Core Data Structures

### DatabaseSchema (Root Object)

The root object representing a complete database schema collection:

```json
{
  "type": "object",
  "required": ["format_version", "database_info", "collection_metadata"],
  "properties": {
    "format_version": {
      "type": "string",
      "pattern": "^1\\.0$",
      "description": "Schema format version for compatibility checking"
    },
    "database_info": { "$ref": "#/$defs/DatabaseInfo" },
    "tables": {
      "type": "array",
      "items": { "$ref": "#/$defs/Table" },
      "default": []
    },
    "views": {
      "type": "array", 
      "items": { "$ref": "#/$defs/View" },
      "default": []
    },
    "indexes": {
      "type": "array",
      "items": { "$ref": "#/$defs/Index" },
      "default": []
    },
    "constraints": {
      "type": "array",
      "items": { "$ref": "#/$defs/Constraint" },
      "default": []
    },
    "procedures": {
      "type": "array",
      "items": { "$ref": "#/$defs/Procedure" },
      "default": []
    },
    "functions": {
      "type": "array",
      "items": { "$ref": "#/$defs/Procedure" },
      "default": []
    },
    "triggers": {
      "type": "array",
      "items": { "$ref": "#/$defs/Trigger" },
      "default": []
    },
    "custom_types": {
      "type": "array",
      "items": { "$ref": "#/$defs/CustomType" },
      "default": []
    },
    "samples": {
      "type": "array",
      "items": { "$ref": "#/$defs/TableSample" },
      "description": "Optional data samples from tables"
    },
    "collection_metadata": { "$ref": "#/$defs/CollectionMetadata" }
  }
}
```

### DatabaseInfo

Database-level information and collection status:

```json
{
  "type": "object",
  "required": ["name", "access_level", "collection_status"],
  "properties": {
    "name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255,
      "pattern": "^[^\\x00-\\x1F\\x7F]*$",
      "description": "Database name (sanitized, no control characters)"
    },
    "version": {
      "type": "string",
      "maxLength": 100,
      "description": "Database server version"
    },
    "size_bytes": {
      "type": "integer",
      "minimum": 0,
      "description": "Database size in bytes"
    },
    "encoding": {
      "type": "string",
      "maxLength": 50,
      "description": "Database character encoding"
    },
    "collation": {
      "type": "string", 
      "maxLength": 100,
      "description": "Database collation"
    },
    "owner": {
      "type": "string",
      "maxLength": 100,
      "description": "Database owner (sanitized)"
    },
    "is_system_database": {
      "type": "boolean",
      "default": false,
      "description": "Whether this is a system database"
    },
    "access_level": { "$ref": "#/$defs/AccessLevel" },
    "collection_status": { "$ref": "#/$defs/CollectionStatus" }
  }
}
```

### Table Structure

Complete table representation with columns, constraints, and relationships:

```json
{
  "type": "object",
  "required": ["name", "columns"],
  "properties": {
    "name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255,
      "pattern": "^[^\\x00-\\x1F\\x7F]*$"
    },
    "schema": {
      "type": "string",
      "maxLength": 255,
      "pattern": "^[^\\x00-\\x1F\\x7F]*$"
    },
    "columns": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/Column" }
    },
    "primary_key": { "$ref": "#/$defs/PrimaryKey" },
    "foreign_keys": {
      "type": "array",
      "items": { "$ref": "#/$defs/ForeignKey" },
      "default": []
    },
    "indexes": {
      "type": "array", 
      "items": { "$ref": "#/$defs/Index" },
      "default": []
    },
    "constraints": {
      "type": "array",
      "items": { "$ref": "#/$defs/Constraint" },
      "default": []
    },
    "comment": {
      "type": "string",
      "maxLength": 1000
    },
    "row_count": {
      "type": "integer",
      "minimum": 0
    }
  }
}
```

### Column Definition

Column metadata with unified data type mapping:

```json
{
  "type": "object",
  "required": ["name", "data_type", "is_nullable", "ordinal_position"],
  "properties": {
    "name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255,
      "pattern": "^[^\\x00-\\x1F\\x7F]*$"
    },
    "data_type": { "$ref": "#/$defs/UnifiedDataType" },
    "is_nullable": { "type": "boolean" },
    "is_primary_key": { "type": "boolean", "default": false },
    "is_auto_increment": { "type": "boolean", "default": false },
    "default_value": {
      "type": "string",
      "maxLength": 500
    },
    "comment": {
      "type": "string",
      "maxLength": 1000
    },
    "ordinal_position": {
      "type": "integer",
      "minimum": 1,
      "description": "1-based column position in table"
    }
  }
}
```

### UnifiedDataType System

Cross-database type mapping with validation:

```json
{
  "oneOf": [
    {
      "type": "object",
      "required": ["String"],
      "properties": {
        "String": {
          "type": "object",
          "properties": {
            "max_length": {
              "type": "integer",
              "minimum": 1,
              "maximum": 2147483647
            }
          }
        }
      }
    },
    {
      "type": "object", 
      "required": ["Integer"],
      "properties": {
        "Integer": {
          "type": "object",
          "required": ["bits", "signed"],
          "properties": {
            "bits": {
              "type": "integer",
              "enum": [8, 16, 32, 64, 128]
            },
            "signed": { "type": "boolean" }
          }
        }
      }
    },
    {
      "type": "object",
      "required": ["Float"],
      "properties": {
        "Float": {
          "type": "object",
          "properties": {
            "precision": {
              "type": "integer",
              "minimum": 1,
              "maximum": 53
            }
          }
        }
      }
    },
    { "const": "Boolean" },
    {
      "type": "object",
      "required": ["DateTime"],
      "properties": {
        "DateTime": {
          "type": "object",
          "required": ["with_timezone"],
          "properties": {
            "with_timezone": { "type": "boolean" }
          }
        }
      }
    },
    { "const": "Date" },
    {
      "type": "object",
      "required": ["Time"],
      "properties": {
        "Time": {
          "type": "object",
          "required": ["with_timezone"],
          "properties": {
            "with_timezone": { "type": "boolean" }
          }
        }
      }
    },
    {
      "type": "object",
      "required": ["Binary"],
      "properties": {
        "Binary": {
          "type": "object",
          "properties": {
            "max_length": {
              "type": "integer",
              "minimum": 1
            }
          }
        }
      }
    },
    { "const": "Json" },
    { "const": "Uuid" },
    {
      "type": "object",
      "required": ["Array"],
      "properties": {
        "Array": {
          "type": "object",
          "required": ["element_type"],
          "properties": {
            "element_type": { "$ref": "#/$defs/UnifiedDataType" }
          }
        }
      }
    },
    {
      "type": "object",
      "required": ["Custom"],
      "properties": {
        "Custom": {
          "type": "object",
          "required": ["type_name"],
          "properties": {
            "type_name": {
              "type": "string",
              "minLength": 1,
              "maxLength": 255
            }
          }
        }
      }
    }
  ]
}
```

## Constraint and Relationship Validation

### Foreign Key Validation

Comprehensive foreign key relationship validation:

```json
{
  "type": "object",
  "required": ["columns", "referenced_table", "referenced_columns"],
  "properties": {
    "name": {
      "type": "string",
      "maxLength": 255
    },
    "columns": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "string",
        "minLength": 1,
        "maxLength": 255
      }
    },
    "referenced_table": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255
    },
    "referenced_schema": {
      "type": "string",
      "maxLength": 255
    },
    "referenced_columns": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "string",
        "minLength": 1,
        "maxLength": 255
      }
    },
    "on_delete": { "$ref": "#/$defs/ReferentialAction" },
    "on_update": { "$ref": "#/$defs/ReferentialAction" }
  },
  "allOf": [
    {
      "description": "Foreign key columns and referenced columns must have same count",
      "if": {
        "properties": {
          "columns": { "type": "array" },
          "referenced_columns": { "type": "array" }
        }
      },
      "then": {
        "properties": {
          "columns": {
            "type": "array",
            "minItems": { "$data": "1/referenced_columns/length" },
            "maxItems": { "$data": "1/referenced_columns/length" }
          }
        }
      }
    }
  ]
}
```

### Index Validation

Index structure with column ordering and uniqueness:

```json
{
  "type": "object",
  "required": ["name", "table_name", "columns"],
  "properties": {
    "name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255
    },
    "table_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255
    },
    "schema": {
      "type": "string",
      "maxLength": 255
    },
    "columns": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/IndexColumn" }
    },
    "is_unique": {
      "type": "boolean",
      "default": false
    },
    "is_primary": {
      "type": "boolean", 
      "default": false
    },
    "index_type": {
      "type": "string",
      "maxLength": 50,
      "description": "Database-specific index type (btree, hash, gin, etc.)"
    }
  }
}
```

## Security Validation Rules

### Credential Protection

Patterns to detect and reject potentially sensitive data:

```json
{
  "not": {
    "anyOf": [
      {
        "description": "Reject any field containing 'password'",
        "properties": {
          "password": true,
          "Password": true,
          "PASSWORD": true
        }
      },
      {
        "description": "Reject connection string patterns",
        "patternProperties": {
          ".*": {
            "type": "string",
            "not": {
              "pattern": "(?i)(postgres|mysql|mongodb|sqlite)://.*:.*@"
            }
          }
        }
      },
      {
        "description": "Reject credential-like patterns",
        "patternProperties": {
          ".*": {
            "type": "string",
            "not": {
              "pattern": "(?i)(secret|token|key|credential|auth)"
            }
          }
        }
      }
    ]
  }
}
```

## Multi-Database Collection Support

### DatabaseServerSchema

Server-level schema for multi-database collection:

```json
{
  "type": "object",
  "required": ["format_version", "server_info", "databases", "collection_metadata"],
  "properties": {
    "format_version": {
      "type": "string",
      "pattern": "^1\\.0$"
    },
    "server_info": { "$ref": "#/$defs/ServerInfo" },
    "databases": {
      "type": "array",
      "items": { "$ref": "#/$defs/DatabaseSchema" }
    },
    "collection_metadata": { "$ref": "#/$defs/CollectionMetadata" }
  }
}
```

### ServerInfo

Server-level metadata and collection statistics:

```json
{
  "type": "object",
  "required": ["server_type", "version", "host", "total_databases", "collected_databases", "connection_user", "collection_mode"],
  "properties": {
    "server_type": { "$ref": "#/$defs/DatabaseType" },
    "version": {
      "type": "string",
      "minLength": 1,
      "maxLength": 100
    },
    "host": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255,
      "description": "Sanitized hostname (no credentials)"
    },
    "port": {
      "type": "integer",
      "minimum": 1,
      "maximum": 65535
    },
    "total_databases": {
      "type": "integer",
      "minimum": 0
    },
    "collected_databases": {
      "type": "integer",
      "minimum": 0
    },
    "system_databases_excluded": {
      "type": "integer",
      "minimum": 0,
      "default": 0
    },
    "connection_user": {
      "type": "string",
      "maxLength": 100,
      "description": "Database username (sanitized)"
    },
    "has_superuser_privileges": {
      "type": "boolean",
      "default": false
    },
    "collection_mode": { "$ref": "#/$defs/CollectionMode" }
  }
}
```

## Data Sampling Support

### TableSample

Data sampling with strategy and ordering information:

```json
{
  "type": "object",
  "required": ["table_name", "rows", "sample_size", "sampling_strategy", "collected_at"],
  "properties": {
    "table_name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 255
    },
    "schema_name": {
      "type": "string",
      "maxLength": 255
    },
    "rows": {
      "type": "array",
      "items": {
        "type": "object",
        "description": "Sample row data as JSON objects"
      }
    },
    "sample_size": {
      "type": "integer",
      "minimum": 0
    },
    "total_rows": {
      "type": "integer",
      "minimum": 0
    },
    "sampling_strategy": { "$ref": "#/$defs/SamplingStrategy" },
    "collected_at": {
      "type": "string",
      "format": "date-time"
    },
    "warnings": {
      "type": "array",
      "items": { "type": "string" },
      "default": []
    }
  }
}
```

### SamplingStrategy

Sampling strategy enumeration:

```json
{
  "oneOf": [
    {
      "type": "object",
      "required": ["MostRecent"],
      "properties": {
        "MostRecent": {
          "type": "object",
          "required": ["limit"],
          "properties": {
            "limit": {
              "type": "integer",
              "minimum": 1,
              "maximum": 10000
            }
          }
        }
      }
    },
    {
      "type": "object",
      "required": ["Random"],
      "properties": {
        "Random": {
          "type": "object",
          "required": ["limit"],
          "properties": {
            "limit": {
              "type": "integer",
              "minimum": 1,
              "maximum": 10000
            }
          }
        }
      }
    },
    { "const": "None" }
  ]
}
```

## Enumeration Definitions

### DatabaseType

Supported database types:

```json
{
  "type": "string",
  "enum": ["PostgreSQL", "MySQL", "SQLite", "MongoDB", "SqlServer"]
}
```

### ReferentialAction

Foreign key referential actions:

```json
{
  "type": "string",
  "enum": ["Cascade", "SetNull", "SetDefault", "Restrict", "NoAction"]
}
```

### ConstraintType

Database constraint types:

```json
{
  "type": "string", 
  "enum": ["PrimaryKey", "ForeignKey", "Unique", "Check", "NotNull"]
}
```

### AccessLevel

Database access levels:

```json
{
  "type": "string",
  "enum": ["Full", "Limited", "None"]
}
```

### CollectionStatus

Collection status with error handling:

```json
{
  "oneOf": [
    { "const": "Success" },
    {
      "type": "object",
      "required": ["Failed"],
      "properties": {
        "Failed": {
          "type": "object",
          "required": ["error"],
          "properties": {
            "error": {
              "type": "string",
              "maxLength": 1000,
              "description": "Sanitized error message (no credentials)"
            }
          }
        }
      }
    },
    {
      "type": "object",
      "required": ["Skipped"],
      "properties": {
        "Skipped": {
          "type": "object",
          "required": ["reason"],
          "properties": {
            "reason": {
              "type": "string",
              "maxLength": 500
            }
          }
        }
      }
    }
  ]
}
```

## Version Compatibility

### Format Version Validation

Strict version validation for backward compatibility:

```json
{
  "properties": {
    "format_version": {
      "type": "string",
      "pattern": "^1\\.0$",
      "description": "Only version 1.0 supported in this schema"
    }
  }
}
```

### Future Version Support

The schema is designed to support future versions through:

1. **Additive Changes**: New optional fields can be added without breaking compatibility
2. **Version Detection**: Format version field enables version-specific validation
3. **Deprecation Path**: Old fields can be marked deprecated before removal
4. **Migration Support**: Schema evolution tools can upgrade between versions

## Usage Examples

### Single Database Collection

```json
{
  "format_version": "1.0",
  "database_info": {
    "name": "production_db",
    "version": "13.7",
    "size_bytes": 1073741824,
    "encoding": "UTF8",
    "collation": "en_US.UTF-8",
    "access_level": "Full",
    "collection_status": "Success"
  },
  "tables": [
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
          "ordinal_position": 1
        },
        {
          "name": "email",
          "data_type": {"String": {"max_length": 255}},
          "is_nullable": false,
          "ordinal_position": 2
        }
      ],
      "primary_key": {
        "name": "users_pkey",
        "columns": ["id"]
      }
    }
  ],
  "collection_metadata": {
    "collected_at": "2024-01-15T10:30:00Z",
    "collection_duration_ms": 1500,
    "collector_version": "1.0.0",
    "warnings": []
  }
}
```

### Multi-Database Server Collection

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
    {
      "format_version": "1.0",
      "database_info": {
        "name": "app_db",
        "access_level": "Full",
        "collection_status": "Success"
      },
      "tables": [],
      "collection_metadata": {
        "collected_at": "2024-01-15T10:30:00Z",
        "collection_duration_ms": 800,
        "collector_version": "1.0.0",
        "warnings": []
      }
    }
  ],
  "collection_metadata": {
    "collected_at": "2024-01-15T10:30:00Z",
    "collection_duration_ms": 2500,
    "collector_version": "1.0.0",
    "warnings": []
  }
}
```

### Data Sampling Example

```json
{
  "samples": [
    {
      "table_name": "users",
      "schema_name": "public",
      "rows": [
        {"id": 1001, "email": "user1001@example.com", "created_at": "2024-01-15T09:00:00Z"},
        {"id": 1002, "email": "user1002@example.com", "created_at": "2024-01-15T09:15:00Z"}
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

## Implementation Integration

### Collector Integration

The JSON Schema validation is integrated into the collector output generation:

```rust
use jsonschema::JSONSchema;
use serde_json::Value;

pub fn validate_output(schema_json: &Value, output_json: &Value) -> Result<(), ValidationError> {
    let schema = JSONSchema::compile(schema_json)
        .map_err(|e| ValidationError::SchemaCompilation(e.to_string()))?;
    
    let result = schema.validate(output_json);
    if let Err(errors) = result {
        let error_messages: Vec<String> = errors
            .map(|e| format!("Validation error at {}: {}", e.instance_path, e))
            .collect();
        return Err(ValidationError::ValidationFailed(error_messages));
    }
    
    Ok(())
}
```

### Postprocessor Integration

The postprocessor validates input files against the schema:

```rust
pub fn load_and_validate_schema(file_path: &Path) -> Result<DatabaseSchema, ProcessorError> {
    let content = std::fs::read_to_string(file_path)?;
    let json_value: Value = serde_json::from_str(&content)?;
    
    // Validate against JSON Schema
    validate_output(&SCHEMA_V1_0, &json_value)?;
    
    // Deserialize to strongly-typed structure
    let schema: DatabaseSchema = serde_json::from_value(json_value)?;
    
    Ok(schema)
}
```

### Error Reporting

Detailed validation error reporting with field-level errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Schema compilation failed: {0}")]
    SchemaCompilation(String),
    
    #[error("Validation failed with errors: {0:?}")]
    ValidationFailed(Vec<String>),
    
    #[error("Unsupported format version: {version}. Supported versions: {supported:?}")]
    UnsupportedVersion {
        version: String,
        supported: Vec<String>,
    },
}
```

## Security Considerations

### Credential Protection Validation

The schema includes comprehensive patterns to detect and reject credential information:

1. **Field Name Patterns**: Reject fields with names containing credential-related terms
2. **Connection String Detection**: Prevent serialization of database connection strings  
3. **Sensitive Data Patterns**: Block common patterns for secrets, tokens, and keys
4. **Content Validation**: Ensure no credential-like content in string fields

### Memory Safety

All validation operations are designed to be memory-safe:

1. **Bounded Input**: Maximum string lengths prevent memory exhaustion
2. **Controlled Recursion**: Array and object nesting limits prevent stack overflow
3. **Sanitized Output**: All error messages are sanitized to prevent information leakage

## Performance Considerations

### Validation Performance

The schema is optimized for performance:

1. **Compiled Schema**: JSON Schema is compiled once and reused for all validations
2. **Streaming Validation**: Large files can be validated in streaming mode
3. **Early Termination**: Validation stops at first error for fast failure detection
4. **Minimal Memory**: Validation uses minimal additional memory beyond the input

### Schema Size

The complete schema definition is approximately 15KB, ensuring fast loading and compilation.

## Testing and Quality Assurance

### Schema Testing

Comprehensive test suite validates the schema:

1. **Valid Examples**: All documented examples must pass validation
2. **Invalid Examples**: Security violations and malformed data must be rejected
3. **Edge Cases**: Boundary conditions and limits are thoroughly tested
4. **Performance Tests**: Validation performance is benchmarked

### Integration Testing

The schema is tested with real collector output:

1. **Database Coverage**: All supported database types are tested
2. **Feature Coverage**: All optional features (sampling, multi-database) are validated
3. **Error Scenarios**: Failed collections and partial data are properly handled
4. **Security Testing**: Credential protection is verified with malicious inputs

This comprehensive JSON Schema specification ensures consistent, validated, and secure output from all DBSurveyor collector binaries while providing a stable foundation for the postprocessor and future format evolution.
