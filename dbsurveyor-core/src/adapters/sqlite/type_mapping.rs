//! SQLite to unified data type conversion.
//!
//! SQLite uses a dynamic type system with type affinity. This module
//! maps SQLite type names to the unified `UnifiedDataType` enum.
//!
//! # SQLite Type Affinity Rules
//!
//! SQLite determines type affinity from declared type names:
//! 1. Contains "INT" -> INTEGER affinity
//! 2. Contains "CHAR", "CLOB", or "TEXT" -> TEXT affinity
//! 3. Contains "BLOB" or no type specified -> BLOB affinity
//! 4. Contains "REAL", "FLOA", or "DOUB" -> REAL affinity
//! 5. Otherwise -> NUMERIC affinity

use crate::models::UnifiedDataType;

/// Maps a SQLite data type to the unified data type system.
///
/// SQLite's flexible type system means we need to parse type names
/// and apply affinity rules to determine the best unified type mapping.
///
/// # Arguments
/// * `sqlite_type` - The SQLite type name (case-insensitive)
///
/// # Returns
/// The corresponding `UnifiedDataType`
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::sqlite::map_sqlite_type;
/// use dbsurveyor_core::models::UnifiedDataType;
///
/// let unified = map_sqlite_type("VARCHAR(255)");
/// assert!(matches!(unified, UnifiedDataType::String { .. }));
/// ```
pub fn map_sqlite_type(sqlite_type: &str) -> UnifiedDataType {
    let type_upper = sqlite_type.to_uppercase();
    let type_str = type_upper.trim();

    // Handle empty type (SQLite allows this, defaults to BLOB affinity)
    if type_str.is_empty() {
        return UnifiedDataType::Binary { max_length: None };
    }

    // Try to extract length from parentheses, e.g., VARCHAR(255)
    let (base_type, length) = parse_type_with_length(type_str);

    // Apply SQLite type affinity rules
    // Rule 1: Contains "INT" -> INTEGER affinity
    if base_type.contains("INT") {
        return map_integer_type(&base_type);
    }

    // Rule 2: Contains "CHAR", "CLOB", or "TEXT" -> TEXT affinity
    if base_type.contains("CHAR") || base_type.contains("CLOB") || base_type.contains("TEXT") {
        return UnifiedDataType::String {
            max_length: length.map(|l| l as u32),
        };
    }

    // Rule 3: Contains "BLOB" or unrecognized -> BLOB affinity
    if base_type.contains("BLOB") {
        return UnifiedDataType::Binary {
            max_length: length.map(|l| l as u32),
        };
    }

    // Rule 4: Contains "REAL", "FLOA", or "DOUB" -> REAL affinity
    // Handle FLOAT specifically (single precision) vs DOUBLE/REAL (double precision)
    if base_type == "FLOAT" {
        return UnifiedDataType::Float {
            precision: Some(24), // Single precision
        };
    }
    if base_type.contains("REAL")
        || base_type.contains("FLOA")
        || base_type.contains("DOUB")
        || base_type.contains("DOUBLE")
    {
        return UnifiedDataType::Float {
            precision: Some(53), // DOUBLE precision
        };
    }

    // Check specific type names before applying NUMERIC affinity
    match base_type.as_str() {
        // Boolean types (commonly used in SQLite)
        "BOOLEAN" | "BOOL" => UnifiedDataType::Boolean,

        // Date/Time types (stored as TEXT, INTEGER, or REAL in SQLite)
        "DATE" => UnifiedDataType::Date,
        "TIME" => UnifiedDataType::Time {
            with_timezone: false,
        },
        "DATETIME" | "TIMESTAMP" => UnifiedDataType::DateTime {
            with_timezone: false,
        },

        // Numeric types
        "NUMERIC" | "DECIMAL" | "NUMBER" => UnifiedDataType::Float { precision: None },

        // JSON type (SQLite 3.9+ supports JSON functions)
        "JSON" | "JSONB" => UnifiedDataType::Json,

        // UUID type (stored as TEXT or BLOB in SQLite)
        "UUID" | "GUID" => UnifiedDataType::Uuid,

        // Binary types
        "BINARY" | "VARBINARY" => UnifiedDataType::Binary {
            max_length: length.map(|l| l as u32),
        },

        // String types without affinity keywords
        "STRING" | "VARCHAR" | "NVARCHAR" | "NCHAR" => UnifiedDataType::String {
            max_length: length.map(|l| l as u32),
        },

        // Float types
        "FLOAT" => UnifiedDataType::Float {
            precision: Some(24), // Single precision
        },

        // Rule 5: Default NUMERIC affinity (can store any type)
        // For unknown types, preserve as custom
        _ => {
            // If it looks like it might have numeric affinity
            if base_type.contains("NUM") || base_type.contains("DEC") {
                UnifiedDataType::Float { precision: None }
            } else {
                UnifiedDataType::Custom {
                    type_name: sqlite_type.to_string(),
                }
            }
        }
    }
}

/// Maps integer type variants to appropriate bit widths.
fn map_integer_type(type_name: &str) -> UnifiedDataType {
    // SQLite INTEGER is 64-bit internally, but we try to map
    // to appropriate sizes based on type hints
    match type_name {
        "TINYINT" => UnifiedDataType::Integer {
            bits: 8,
            signed: true,
        },
        "SMALLINT" | "INT2" => UnifiedDataType::Integer {
            bits: 16,
            signed: true,
        },
        "MEDIUMINT" | "INT3" => UnifiedDataType::Integer {
            bits: 24,
            signed: true,
        },
        "INT" | "INTEGER" | "INT4" => UnifiedDataType::Integer {
            bits: 32,
            signed: true,
        },
        "BIGINT" | "INT8" | "UNSIGNED BIG INT" => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },
        // Anything with INT defaults to 64-bit (SQLite's actual storage)
        _ => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },
    }
}

/// Parses a type string to extract base type and optional length.
///
/// # Examples
/// - "VARCHAR(255)" -> ("VARCHAR", Some(255))
/// - "INTEGER" -> ("INTEGER", None)
/// - "DECIMAL(10,2)" -> ("DECIMAL", Some(10))
fn parse_type_with_length(type_str: &str) -> (String, Option<i64>) {
    if let Some(paren_pos) = type_str.find('(') {
        let base = type_str[..paren_pos].trim().to_string();
        let params = &type_str[paren_pos + 1..];

        // Extract the first number (length or precision)
        if let Some(end_pos) = params.find(|c: char| !c.is_ascii_digit())
            && end_pos > 0
            && let Ok(length) = params[..end_pos].parse::<i64>()
        {
            return (base, Some(length));
        } else if let Some(close_pos) = params.find(')')
            && let Ok(length) = params[..close_pos].parse::<i64>()
        {
            return (base, Some(length));
        }

        (base, None)
    } else {
        (type_str.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // Integer Type Tests
    // =============================================================================

    #[test]
    fn test_map_integer_types() {
        let result = map_sqlite_type("INTEGER");
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));

        let result = map_sqlite_type("INT");
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));

        let result = map_sqlite_type("TINYINT");
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 8,
                signed: true
            }
        ));

        let result = map_sqlite_type("SMALLINT");
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 16,
                signed: true
            }
        ));

        let result = map_sqlite_type("BIGINT");
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 64,
                signed: true
            }
        ));
    }

    // =============================================================================
    // String Type Tests
    // =============================================================================

    #[test]
    fn test_map_string_types() {
        let result = map_sqlite_type("TEXT");
        assert!(matches!(
            result,
            UnifiedDataType::String { max_length: None }
        ));

        let result = map_sqlite_type("VARCHAR(255)");
        assert!(matches!(
            result,
            UnifiedDataType::String {
                max_length: Some(255)
            }
        ));

        let result = map_sqlite_type("CHAR(10)");
        assert!(matches!(
            result,
            UnifiedDataType::String {
                max_length: Some(10)
            }
        ));

        let result = map_sqlite_type("CLOB");
        assert!(matches!(
            result,
            UnifiedDataType::String { max_length: None }
        ));

        let result = map_sqlite_type("NVARCHAR(100)");
        assert!(matches!(
            result,
            UnifiedDataType::String {
                max_length: Some(100)
            }
        ));
    }

    // =============================================================================
    // Real/Float Type Tests
    // =============================================================================

    #[test]
    fn test_map_real_types() {
        let result = map_sqlite_type("REAL");
        assert!(matches!(
            result,
            UnifiedDataType::Float {
                precision: Some(53)
            }
        ));

        let result = map_sqlite_type("DOUBLE");
        assert!(matches!(
            result,
            UnifiedDataType::Float {
                precision: Some(53)
            }
        ));

        let result = map_sqlite_type("DOUBLE PRECISION");
        assert!(matches!(
            result,
            UnifiedDataType::Float {
                precision: Some(53)
            }
        ));

        let result = map_sqlite_type("FLOAT");
        assert!(matches!(
            result,
            UnifiedDataType::Float {
                precision: Some(24)
            }
        ));
    }

    // =============================================================================
    // Binary/Blob Type Tests
    // =============================================================================

    #[test]
    fn test_map_blob_types() {
        let result = map_sqlite_type("BLOB");
        assert!(matches!(
            result,
            UnifiedDataType::Binary { max_length: None }
        ));

        let result = map_sqlite_type("BINARY(16)");
        assert!(matches!(
            result,
            UnifiedDataType::Binary {
                max_length: Some(16)
            }
        ));

        // Empty type defaults to BLOB
        let result = map_sqlite_type("");
        assert!(matches!(
            result,
            UnifiedDataType::Binary { max_length: None }
        ));
    }

    // =============================================================================
    // Boolean Type Tests
    // =============================================================================

    #[test]
    fn test_map_boolean_types() {
        let result = map_sqlite_type("BOOLEAN");
        assert!(matches!(result, UnifiedDataType::Boolean));

        let result = map_sqlite_type("BOOL");
        assert!(matches!(result, UnifiedDataType::Boolean));
    }

    // =============================================================================
    // Date/Time Type Tests
    // =============================================================================

    #[test]
    fn test_map_datetime_types() {
        let result = map_sqlite_type("DATE");
        assert!(matches!(result, UnifiedDataType::Date));

        let result = map_sqlite_type("TIME");
        assert!(matches!(
            result,
            UnifiedDataType::Time {
                with_timezone: false
            }
        ));

        let result = map_sqlite_type("DATETIME");
        assert!(matches!(
            result,
            UnifiedDataType::DateTime {
                with_timezone: false
            }
        ));

        let result = map_sqlite_type("TIMESTAMP");
        assert!(matches!(
            result,
            UnifiedDataType::DateTime {
                with_timezone: false
            }
        ));
    }

    // =============================================================================
    // Special Type Tests
    // =============================================================================

    #[test]
    fn test_map_json_type() {
        let result = map_sqlite_type("JSON");
        assert!(matches!(result, UnifiedDataType::Json));
    }

    #[test]
    fn test_map_uuid_type() {
        let result = map_sqlite_type("UUID");
        assert!(matches!(result, UnifiedDataType::Uuid));

        let result = map_sqlite_type("GUID");
        assert!(matches!(result, UnifiedDataType::Uuid));
    }

    #[test]
    fn test_map_numeric_type() {
        let result = map_sqlite_type("NUMERIC");
        assert!(matches!(result, UnifiedDataType::Float { precision: None }));

        let result = map_sqlite_type("DECIMAL(10,2)");
        assert!(matches!(result, UnifiedDataType::Float { precision: None }));
    }

    // =============================================================================
    // Unknown Type Tests
    // =============================================================================

    #[test]
    fn test_map_unknown_type() {
        let result = map_sqlite_type("MY_CUSTOM_TYPE");
        assert!(matches!(
            result,
            UnifiedDataType::Custom { ref type_name } if type_name == "MY_CUSTOM_TYPE"
        ));
    }

    // =============================================================================
    // Case Insensitivity Tests
    // =============================================================================

    #[test]
    fn test_case_insensitivity() {
        let upper = map_sqlite_type("INTEGER");
        let lower = map_sqlite_type("integer");
        let mixed = map_sqlite_type("Integer");

        assert!(matches!(
            upper,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
        assert!(matches!(
            lower,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
        assert!(matches!(
            mixed,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
    }

    // =============================================================================
    // Length Parsing Tests
    // =============================================================================

    #[test]
    fn test_parse_type_with_length() {
        let (base, len) = parse_type_with_length("VARCHAR(255)");
        assert_eq!(base, "VARCHAR");
        assert_eq!(len, Some(255));

        let (base, len) = parse_type_with_length("INTEGER");
        assert_eq!(base, "INTEGER");
        assert_eq!(len, None);

        let (base, len) = parse_type_with_length("DECIMAL(10,2)");
        assert_eq!(base, "DECIMAL");
        assert_eq!(len, Some(10));

        let (base, len) = parse_type_with_length("CHAR(1)");
        assert_eq!(base, "CHAR");
        assert_eq!(len, Some(1));
    }
}
