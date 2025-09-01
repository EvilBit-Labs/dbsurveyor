# JSON Schema Specification

The DBSurveyor system uses a comprehensive JSON Schema specification based on the Frictionless Data Table Schema specification to ensure consistent, validated output across all database adapters.

## Schema Foundation

Building on the Frictionless Data Table Schema specification (<https://specs.frictionlessdata.io/schemas/table-schema.json>), the DBSurveyor schema extends the foundation with database-specific metadata while maintaining compatibility with data processing tools.

## Core Schema Structure

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "DBSurveyor Schema Collection Format",
  "description": "Comprehensive database schema collection format based on Frictionless Data Table Schema",
  "type": "object",
  "required": ["format_version", "database_info", "collection_metadata"],
  "properties": {
    "format_version": {
      "type": "string",
      "enum": ["1.0"],
      "description": "Schema format version for backward compatibility"
    },
    "database_info": {
      "$ref": "#/definitions/DatabaseInfo"
    },
    "server_info": {
      "$ref": "#/definitions/ServerInfo",
      "description": "Present only in multi-database collection mode"
    },
    "tables": {
      "type": "array",
      "items": { "$ref": "#/definitions/Table" }
    },
    "views": {
      "type": "array", 
      "items": { "$ref": "#/definitions/View" }
    },
    "indexes": {
      "type": "array",
      "items": { "$ref": "#/definitions/Index" }
    },
    "constraints": {
      "type": "array",
      "items": { "$ref": "#/definitions/Constraint" }
    },
    "collection_metadata": {
      "$ref": "#/definitions/CollectionMetadata"
    }
  }
}
```

## Database-Specific Extensions

The schema extends the Frictionless Data specification with database-specific metadata:

```json
{
  "definitions": {
    "DatabaseInfo": {
      "type": "object",
      "required": ["name", "database_type"],
      "properties": {
        "name": { "type": "string" },
        "database_type": {
          "enum": ["PostgreSQL", "MySQL", "SQLite", "MongoDB", "SqlServer"]
        },
        "version": { "type": "string" },
        "size_bytes": { "type": "integer", "minimum": 0 },
        "encoding": { "type": "string" },
        "collation": { "type": "string" },
        "access_level": {
          "enum": ["Full", "Limited", "None"]
        },
        "collection_status": {
          "$ref": "#/definitions/CollectionStatus"
        }
      }
    },
    "Table": {
      "allOf": [
        {
          "type": "object",
          "required": ["name", "columns"],
          "properties": {
            "name": { "type": "string" },
            "schema": { "type": "string" },
            "columns": {
              "type": "array",
              "items": { "$ref": "#/definitions/Column" }
            },
            "primary_key": { "$ref": "#/definitions/PrimaryKey" },
            "foreign_keys": {
              "type": "array",
              "items": { "$ref": "#/definitions/ForeignKey" }
            },
            "row_count": { "type": "integer", "minimum": 0 },
            "samples": {
              "type": "array",
              "items": { "$ref": "#/definitions/TableSample" }
            }
          }
        }
      ]
    },
    "Column": {
      "type": "object",
      "required": ["name", "data_type"],
      "properties": {
        "name": { "type": "string" },
        "data_type": { "$ref": "#/definitions/UnifiedDataType" },
        "nullable": { "type": "boolean" },
        "default_value": { "type": ["string", "null"] },
        "is_auto_increment": { "type": "boolean" },
        "ordinal_position": { "type": "integer", "minimum": 1 },
        "comment": { "type": "string" }
      }
    },
    "UnifiedDataType": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "const": "String" },
            "max_length": { "type": "integer", "minimum": 1 },
            "fixed_length": { "type": "boolean" }
          }
        },
        {
          "type": "object", 
          "required": ["type"],
          "properties": {
            "type": { "const": "Integer" },
            "size": { "type": "integer", "enum": [1, 2, 4, 8] },
            "signed": { "type": "boolean" }
          }
        },
        {
          "type": "object",
          "required": ["type"], 
          "properties": {
            "type": { "const": "Float" },
            "precision": { "type": "integer", "minimum": 1 }
          }
        },
        {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "const": "Boolean" }
          }
        },
        {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "const": "DateTime" },
            "timezone_aware": { "type": "boolean" }
          }
        },
        {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "const": "Array" },
            "element_type": { "$ref": "#/definitions/UnifiedDataType" }
          }
        },
        {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "const": "Custom" },
            "type_name": { "type": "string" },
            "database_type": { "type": "string" }
          }
        }
      ]
    }
  }
}
```

## Security and Validation Features

The JSON Schema includes comprehensive validation rules to ensure:

- **Credential Protection**: No credential fields are allowed in any structure
- **Format Versioning**: Strict version validation for backward compatibility  
- **Data Integrity**: Required fields and type constraints prevent malformed output
- **Cross-Database Consistency**: Unified type system works across all database engines

## Schema Validation Integration

Both the collector and postprocessor validate against this schema:

```rust
use jsonschema::{JSONSchema, ValidationError};
use serde_json::Value;

pub struct SchemaValidator {
    schema: JSONSchema,
}

impl SchemaValidator {
    pub fn new() -> Result<Self, ValidationError> {
        let schema_json = include_str!("../schemas/dbsurveyor-schema.json");
        let schema_value: Value = serde_json::from_str(schema_json)?;
        let schema = JSONSchema::compile(&schema_value)?;
        
        Ok(Self { schema })
    }

    pub fn validate_output(&self, data: &DatabaseSchema) -> Result<(), Vec<ValidationError>> {
        let json_value = serde_json::to_value(data)?;
        
        match self.schema.validate(&json_value) {
            Ok(_) => Ok(()),
            Err(errors) => Err(errors.collect()),
        }
    }
}

// Integration in collector output generation
pub async fn generate_output(
    schema: &DatabaseSchema,
    format: OutputFormat,
) -> Result<Vec<u8>, OutputError> {
    // Validate against JSON Schema before output
    let validator = SchemaValidator::new()?;
    validator.validate_output(schema)
        .map_err(|errors| OutputError::ValidationFailed(errors))?;

    match format {
        OutputFormat::Json => Ok(serde_json::to_vec_pretty(schema)?),
        OutputFormat::JsonCompressed => {
            let json = serde_json::to_vec(schema)?;
            Ok(zstd::encode_all(&json[..], 3)?)
        }
        OutputFormat::Encrypted => {
            let json = serde_json::to_vec(schema)?;
            let encrypted = encrypt_schema_data(&json, &get_encryption_key()?).await?;
            Ok(encrypted.to_bytes())
        }
    }
}
```

## Complete Schema Definitions

### Table Sample Structure

```json
{
  "TableSample": {
    "type": "object",
    "required": ["table_name", "rows", "sample_size", "sampling_strategy", "collected_at"],
    "properties": {
      "table_name": { "type": "string" },
      "schema_name": { "type": ["string", "null"] },
      "rows": {
        "type": "array",
        "items": { "type": "object" },
        "description": "Sample row data as JSON objects"
      },
      "sample_size": { "type": "integer", "minimum": 0 },
      "total_rows": { "type": ["integer", "null"], "minimum": 0 },
      "sampling_strategy": { "$ref": "#/definitions/SamplingStrategy" },
      "collected_at": { "type": "string", "format": "date-time" },
      "warnings": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Security warnings about potentially sensitive data"
      }
    }
  }
}
```

### Constraint Definitions

```json
{
  "Constraint": {
    "type": "object",
    "required": ["name", "constraint_type", "table_name"],
    "properties": {
      "name": { "type": "string" },
      "constraint_type": {
        "enum": ["PrimaryKey", "ForeignKey", "Unique", "Check", "NotNull"]
      },
      "table_name": { "type": "string" },
      "columns": {
        "type": "array",
        "items": { "type": "string" }
      },
      "referenced_table": { "type": ["string", "null"] },
      "referenced_columns": {
        "type": ["array", "null"],
        "items": { "type": "string" }
      },
      "on_delete": {
        "enum": ["CASCADE", "SET NULL", "SET DEFAULT", "RESTRICT", "NO ACTION"]
      },
      "on_update": {
        "enum": ["CASCADE", "SET NULL", "SET DEFAULT", "RESTRICT", "NO ACTION"]
      }
    }
  }
}
```

## Documentation and Examples

The JSON Schema includes comprehensive documentation with:

- **Field Descriptions**: Every field includes purpose and usage notes
- **Validation Examples**: Sample valid and invalid data structures  
- **Database-Specific Examples**: Type mapping examples for each supported database
- **Migration Guides**: Format evolution and backward compatibility notes

## Implementation Requirements

### Collector Integration

The collector must validate all output against this schema before generation:

1. **Pre-Serialization Validation**: Validate Rust structs before JSON conversion
2. **Post-Serialization Validation**: Validate final JSON output
3. **Error Reporting**: Provide detailed validation error messages
4. **Performance**: Schema validation should not significantly impact collection performance

### Postprocessor Integration

The postprocessor must validate all input against this schema:

1. **Input Validation**: Reject malformed input files with clear error messages
2. **Version Compatibility**: Handle different format versions gracefully
3. **Partial Validation**: Allow processing of partially valid schemas with warnings
4. **Migration Support**: Provide tools for upgrading older format versions

This schema foundation ensures consistent, validated output across all database adapters while providing a clear contract for postprocessor input validation.
