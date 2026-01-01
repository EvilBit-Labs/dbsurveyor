//! JSON Schema validation for DBSurveyor output format.
//!
//! This module provides comprehensive validation of the .dbsurveyor.json output format
//! using JSON Schema. It ensures consistent, secure output across all database adapters
//! and provides detailed error reporting for validation failures.
//!
//! # Security Guarantees
//! - Validates that no credential fields are present in output
//! - Ensures connection strings are never serialized
//! - Detects potentially sensitive data patterns
//! - Validates format version compatibility
//!
//! # Example
//! ```rust,no_run
//! use dbsurveyor_core::validation::{validate_schema_output, initialize_schema_validator};
//! use dbsurveyor_core::models::{DatabaseSchema, DatabaseInfo};
//! use serde_json::Value;
//!
//! // Initialize the schema validator once at startup
//! initialize_schema_validator().expect("Failed to initialize validator");
//!
//! // Create and validate a database schema
//! let db_info = DatabaseInfo::new("test_db".to_string());
//! let schema = DatabaseSchema::new(db_info);
//! let json_value = serde_json::to_value(&schema).expect("Failed to serialize");
//!
//! validate_schema_output(&json_value).expect("Validation failed");
//! println!("Schema validation passed!");
//! ```

use jsonschema::Validator;
use serde_json::Value;
use std::sync::OnceLock;
use thiserror::Error;

/// JSON Schema validation errors with detailed field-level reporting
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Schema compilation failed during initialization
    #[error("JSON Schema compilation failed: {message}")]
    SchemaCompilation { message: String },

    /// Validation failed with specific field errors
    #[error("Schema validation failed with {error_count} errors: {errors:?}")]
    ValidationFailed {
        error_count: usize,
        errors: Vec<String>,
    },

    /// Unsupported format version detected
    #[error("Unsupported format version '{version}'. Supported versions: {supported:?}")]
    UnsupportedVersion {
        version: String,
        supported: Vec<String>,
    },

    /// Security validation failed - potential credential exposure
    #[error("Security validation failed: {reason}")]
    SecurityViolation { reason: String },

    /// JSON parsing error
    #[error("JSON parsing failed: {source}")]
    JsonParsing {
        #[from]
        source: serde_json::Error,
    },
}

/// Supported format versions for backward compatibility
const SUPPORTED_VERSIONS: &[&str] = &["1.0"];

/// Embedded JSON Schema for v1.0 format validation
const SCHEMA_V1_0: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "DBSurveyor Database Schema Collection Format v1.0",
  "type": "object",
  "required": ["format_version", "database_info", "collection_metadata"],
  "properties": {
    "format_version": {
      "type": "string",
      "pattern": "^1\\.0$"
    },
    "database_info": {
      "type": "object",
      "required": ["name", "access_level", "collection_status"],
      "properties": {
        "name": { "type": "string", "minLength": 1 },
        "version": { "type": ["string", "null"] },
        "size_bytes": { "type": ["integer", "null"], "minimum": 0 },
        "encoding": { "type": ["string", "null"] },
        "collation": { "type": ["string", "null"] },
        "owner": { "type": ["string", "null"] },
        "is_system_database": { "type": "boolean", "default": false },
        "access_level": { "enum": ["Full", "Limited", "None"] },
        "collection_status": {
          "oneOf": [
            { "const": "Success" },
            {
              "type": "object",
              "required": ["Failed"],
              "additionalProperties": false,
              "properties": {
                "Failed": {
                  "type": "object",
                  "required": ["error"],
                  "properties": { "error": { "type": "string" } }
                }
              }
            },
            {
              "type": "object",
              "required": ["Skipped"],
              "additionalProperties": false,
              "properties": {
                "Skipped": {
                  "type": "object",
                  "required": ["reason"],
                  "properties": { "reason": { "type": "string" } }
                }
              }
            }
          ]
        }
      }
    },
    "tables": { "type": "array", "items": { "$ref": "#/$defs/Table" }, "default": [] },
    "views": { "type": "array", "default": [] },
    "indexes": { "type": "array", "default": [] },
    "constraints": { "type": "array", "default": [] },
    "procedures": { "type": "array", "default": [] },
    "functions": { "type": "array", "default": [] },
    "triggers": { "type": "array", "default": [] },
    "custom_types": { "type": "array", "default": [] },
    "samples": { "type": ["array", "null"] },
    "collection_metadata": {
      "type": "object",
      "required": ["collected_at", "collection_duration_ms", "collector_version"],
      "properties": {
        "collected_at": { "type": "string", "format": "date-time" },
        "collection_duration_ms": { "type": "integer", "minimum": 0 },
        "collector_version": { "type": "string", "minLength": 1 },
        "warnings": { "type": "array", "items": { "type": "string" }, "default": [] }
      }
    }
  },
  "$defs": {
    "Table": {
      "type": "object",
      "required": ["name", "columns"],
      "properties": {
        "name": { "type": "string", "minLength": 1 },
        "schema": { "type": ["string", "null"] },
        "columns": { "type": "array", "items": { "$ref": "#/$defs/Column" }, "minItems": 1 },
        "primary_key": { "$ref": "#/$defs/PrimaryKey" },
        "foreign_keys": { "type": "array", "items": { "$ref": "#/$defs/ForeignKey" } },
        "indexes": { "type": "array" },
        "constraints": { "type": "array" },
        "comment": { "type": ["string", "null"] },
        "row_count": { "type": ["integer", "null"], "minimum": 0 }
      }
    },
    "Column": {
      "type": "object",
      "required": ["name", "data_type", "is_nullable", "ordinal_position"],
      "properties": {
        "name": { "type": "string", "minLength": 1 },
        "data_type": { "$ref": "#/$defs/UnifiedDataType" },
        "is_nullable": { "type": "boolean" },
        "is_primary_key": { "type": "boolean" },
        "is_auto_increment": { "type": "boolean" },
        "default_value": { "type": ["string", "null"] },
        "comment": { "type": ["string", "null"] },
        "ordinal_position": { "type": "integer", "minimum": 1 }
      }
    },
    "UnifiedDataType": {
      "oneOf": [
        { "const": "Boolean" },
        { "const": "Date" },
        { "const": "Json" },
        { "const": "Uuid" },
        {
          "type": "object",
          "required": ["String"],
          "additionalProperties": false,
          "properties": {
            "String": {
              "type": "object",
              "additionalProperties": false,
              "properties": {
                "max_length": { "type": ["integer", "null"], "minimum": 1 }
              }
            }
          }
        },
        {
          "type": "object",
          "required": ["Integer"],
          "additionalProperties": false,
          "properties": {
            "Integer": {
              "type": "object",
              "additionalProperties": false,
              "required": ["bits", "signed"],
              "properties": {
                "bits": { "type": "integer", "enum": [8, 16, 32, 64, 128] },
                "signed": { "type": "boolean" }
              }
            }
          }
        },
        {
          "type": "object",
          "required": ["Float"],
          "additionalProperties": false,
          "properties": {
            "Float": {
              "type": "object",
              "additionalProperties": false,
              "properties": {
                "precision": { "type": ["integer", "null"], "minimum": 1, "maximum": 255 }
              }
            }
          }
        },
        {
          "type": "object",
          "required": ["DateTime"],
          "additionalProperties": false,
          "properties": {
            "DateTime": {
              "type": "object",
              "additionalProperties": false,
              "required": ["with_timezone"],
              "properties": {
                "with_timezone": { "type": "boolean" }
              }
            }
          }
        },
        {
          "type": "object",
          "required": ["Time"],
          "additionalProperties": false,
          "properties": {
            "Time": {
              "type": "object",
              "additionalProperties": false,
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
          "additionalProperties": false,
          "properties": {
            "Binary": {
              "type": "object",
              "additionalProperties": false,
              "properties": {
                "max_length": { "type": ["integer", "null"], "minimum": 1 }
              }
            }
          }
        },
        {
          "type": "object",
          "required": ["Array"],
          "additionalProperties": false,
          "properties": {
            "Array": {
              "type": "object",
              "additionalProperties": false,
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
          "additionalProperties": false,
          "properties": {
            "Custom": {
              "type": "object",
              "additionalProperties": false,
              "required": ["type_name"],
              "properties": {
                "type_name": { "type": "string", "minLength": 1 }
              }
            }
          }
        }
      ]
    },
    "PrimaryKey": {
      "type": ["object", "null"],
      "properties": {
        "name": { "type": ["string", "null"] },
        "columns": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
      }
    },
    "ForeignKey": {
      "type": "object",
      "required": ["referenced_table", "referenced_columns", "columns"],
      "properties": {
        "name": { "type": ["string", "null"] },
        "columns": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
        "referenced_table": { "type": "string", "minLength": 1 },
        "referenced_schema": { "type": ["string", "null"] },
        "referenced_columns": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
        "on_delete": { "enum": ["Cascade", "SetNull", "SetDefault", "Restrict", "NoAction", null] },
        "on_update": { "enum": ["Cascade", "SetNull", "SetDefault", "Restrict", "NoAction", null] }
      }
    }
  }
}"##;

/// Compiled JSON Schema instance (initialized once)
static COMPILED_SCHEMA: OnceLock<Validator> = OnceLock::new();

/// Initialize and compile the JSON Schema for validation
///
/// This function compiles the embedded JSON Schema and caches it for reuse.
/// It should be called once during application startup.
///
/// # Errors
/// Returns `ValidationError::SchemaCompilation` if the embedded schema is invalid.
pub fn initialize_schema_validator() -> Result<(), ValidationError> {
    let schema_json: Value =
        serde_json::from_str(SCHEMA_V1_0).map_err(|e| ValidationError::SchemaCompilation {
            message: format!("Failed to parse embedded schema: {}", e),
        })?;

    let compiled = jsonschema::validator_for(&schema_json).map_err(|e| {
        ValidationError::SchemaCompilation {
            message: format!("Schema compilation error: {}", e),
        }
    })?;

    // Try to set the compiled schema, but don't error if it's already set
    let _ = COMPILED_SCHEMA.set(compiled);

    Ok(())
}

/// Validate a DatabaseSchema JSON output against the JSON Schema
///
/// This function performs comprehensive validation including:
/// - JSON Schema structure validation
/// - Format version compatibility checking
/// - Security validation (credential protection)
/// - Field-level validation with detailed error reporting
///
/// # Arguments
/// * `json_value` - The JSON representation of a DatabaseSchema
///
/// # Errors
/// Returns detailed validation errors if the JSON doesn't conform to the schema
/// or contains security violations.
///
/// # Example
/// ```rust
/// use dbsurveyor_core::validation::validate_schema_output;
/// use serde_json::json;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let valid_schema = json!({
///     "format_version": "1.0",
///     "database_info": {
///         "name": "test_db",
///         "access_level": "Full",
///         "collection_status": "Success"
///     },
///     "tables": [],
///     "collection_metadata": {
///         "collected_at": "2024-01-15T10:30:00Z",
///         "collection_duration_ms": 1500,
///         "collector_version": "1.0.0",
///         "warnings": []
///     }
/// });
///
/// validate_schema_output(&valid_schema)?;
/// # Ok(())
/// # }
/// ```
pub fn validate_schema_output(json_value: &Value) -> Result<(), ValidationError> {
    // Ensure schema is initialized
    let schema = COMPILED_SCHEMA
        .get()
        .ok_or_else(|| ValidationError::SchemaCompilation {
            message: "Schema validator not initialized. Call initialize_schema_validator() first."
                .to_string(),
        })?;

    // Check format version compatibility first
    validate_format_version(json_value)?;

    // Perform comprehensive JSON Schema validation
    if let Err(validation_error) = schema.validate(json_value) {
        let error_message = format!("Schema validation failed: {}", validation_error);

        return Err(ValidationError::ValidationFailed {
            error_count: 1,
            errors: vec![error_message],
        });
    }

    // Additional security validation
    validate_security_constraints(json_value)?;

    Ok(())
}

/// Validate format version compatibility
///
/// Ensures the format_version field is present and supported.
fn validate_format_version(json_value: &Value) -> Result<(), ValidationError> {
    let version = json_value
        .get("format_version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ValidationError::ValidationFailed {
            error_count: 1,
            errors: vec!["Missing required field 'format_version'".to_string()],
        })?;

    if !SUPPORTED_VERSIONS.contains(&version) {
        return Err(ValidationError::UnsupportedVersion {
            version: version.to_string(),
            supported: SUPPORTED_VERSIONS.iter().map(|s| s.to_string()).collect(),
        });
    }

    Ok(())
}

/// Perform additional security validation beyond JSON Schema
///
/// This function implements security checks that are difficult to express
/// in JSON Schema, such as pattern matching for credentials and sensitive data.
fn validate_security_constraints(json_value: &Value) -> Result<(), ValidationError> {
    // Check for credential-like patterns in all string values
    validate_no_credentials_recursive(json_value, "")?;

    // Validate that no connection strings are present
    validate_no_connection_strings_recursive(json_value, "")?;

    Ok(())
}

/// Recursively check for credential patterns in JSON values
fn validate_no_credentials_recursive(value: &Value, path: &str) -> Result<(), ValidationError> {
    match value {
        Value::String(s) => {
            // Check for credential-like patterns
            let lower_s = s.to_lowercase();

            // Check for password patterns
            if lower_s.contains("password=") || lower_s.contains("pwd=") {
                return Err(ValidationError::SecurityViolation {
                    reason: format!(
                        "Potential password found at path '{}': contains password pattern",
                        path
                    ),
                });
            }

            // Check for secret/token patterns
            if lower_s.contains("secret=") || lower_s.contains("token=") || lower_s.contains("key=")
            {
                return Err(ValidationError::SecurityViolation {
                    reason: format!(
                        "Potential secret found at path '{}': contains credential pattern",
                        path
                    ),
                });
            }

            // Check for API key patterns
            if lower_s.contains("api_key=") || lower_s.contains("apikey=") {
                return Err(ValidationError::SecurityViolation {
                    reason: format!(
                        "Potential API key found at path '{}': contains API key pattern",
                        path
                    ),
                });
            }
        }
        Value::Object(obj) => {
            // Check field names for credential-related terms
            for (key, val) in obj {
                let lower_key = key.to_lowercase();
                if lower_key.contains("password")
                    || lower_key.contains("secret")
                    || lower_key.contains("token")
                    || lower_key.contains("credential")
                    || lower_key.contains("auth")
                {
                    return Err(ValidationError::SecurityViolation {
                        reason: format!("Credential-related field name found: '{}'", key),
                    });
                }

                // Special check for "name" fields that might contain credential-related values
                if key == "name" {
                    if let Value::String(name_value) = val {
                        let lower_name = name_value.to_lowercase();
                        if lower_name.contains("password")
                            || lower_name.contains("secret")
                            || lower_name.contains("token")
                            || lower_name.contains("credential")
                            || lower_name.contains("auth")
                        {
                            return Err(ValidationError::SecurityViolation {
                                reason: format!(
                                    "Credential-related name value found at path '{}': '{}'",
                                    path, name_value
                                ),
                            });
                        }
                    }
                }

                let new_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };
                validate_no_credentials_recursive(val, &new_path)?;
            }
        }
        Value::Array(arr) => {
            for (index, item) in arr.iter().enumerate() {
                let new_path = format!("{}[{}]", path, index);
                validate_no_credentials_recursive(item, &new_path)?;
            }
        }
        _ => {} // Numbers, booleans, null are safe
    }

    Ok(())
}

/// Recursively check for connection string patterns in JSON values
fn validate_no_connection_strings_recursive(
    value: &Value,
    path: &str,
) -> Result<(), ValidationError> {
    if let Value::String(s) = value {
        // Use pre-compiled patterns for performance
        let patterns = crate::adapters::helpers::ValidationPatterns::instance();
        if patterns.contains_credentials(s) {
            return Err(ValidationError::SecurityViolation {
                reason: format!(
                    "Connection string pattern with credentials found at path '{}'",
                    path
                ),
            });
        }
    } else if let Value::Object(obj) = value {
        for (key, val) in obj {
            let new_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", path, key)
            };
            validate_no_connection_strings_recursive(val, &new_path)?;
        }
    } else if let Value::Array(arr) = value {
        for (index, item) in arr.iter().enumerate() {
            let new_path = format!("{}[{}]", path, index);
            validate_no_connection_strings_recursive(item, &new_path)?;
        }
    }

    Ok(())
}

/// Validate and load a DatabaseSchema from JSON with comprehensive error reporting
///
/// This function combines JSON parsing, schema validation, and deserialization
/// into a single operation with detailed error reporting.
///
/// # Arguments
/// * `json_str` - JSON string representation of a DatabaseSchema
///
/// # Errors
/// Returns validation errors for malformed JSON, schema violations, or security issues.
///
/// # Example
/// ```rust
/// use dbsurveyor_core::validation::validate_and_parse_schema;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let json_str = r#"{
///     "format_version": "1.0",
///     "database_info": {
///         "name": "test_db",
///         "access_level": "Full",
///         "collection_status": "Success"
///     },
///     "tables": [],
///     "collection_metadata": {
///         "collected_at": "2024-01-15T10:30:00Z",
///         "collection_duration_ms": 1500,
///         "collector_version": "1.0.0",
///         "warnings": []
///     }
/// }"#;
///
/// let schema = validate_and_parse_schema(json_str)?;
/// println!("Loaded schema for database: {}", schema.database_info.name);
/// # Ok(())
/// # }
/// ```
pub fn validate_and_parse_schema(
    json_str: &str,
) -> Result<crate::models::DatabaseSchema, ValidationError> {
    // Parse JSON
    let json_value: Value = serde_json::from_str(json_str)?;

    // Validate against schema
    validate_schema_output(&json_value)?;

    // Deserialize to strongly-typed structure
    let schema: crate::models::DatabaseSchema = serde_json::from_value(json_value)
        .map_err(|e| ValidationError::JsonParsing { source: e })?;

    Ok(schema)
}

/// Get the embedded JSON Schema as a parsed Value for external use
///
/// This function provides access to the compiled JSON Schema for tools
/// that need to work with the schema definition directly.
pub fn get_schema_definition() -> Result<Value, ValidationError> {
    serde_json::from_str(SCHEMA_V1_0).map_err(|e| ValidationError::SchemaCompilation {
        message: format!("Failed to parse embedded schema: {}", e),
    })
}

#[cfg(test)]
mod tests;
