//! MySQL to unified data type conversion.
//!
//! This module provides mapping from MySQL data types to the unified
//! `UnifiedDataType` enum used across all database adapters.

use crate::models::UnifiedDataType;

/// Maps a MySQL data type to the unified data type system.
///
/// # Arguments
/// * `mysql_type` - The MySQL type name (lowercase)
/// * `char_max_length` - Maximum character length for string types
/// * `numeric_precision` - Precision for numeric types
/// * `numeric_scale` - Scale for decimal types
///
/// # Returns
/// The corresponding `UnifiedDataType`
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::mysql::map_mysql_type;
/// use dbsurveyor_core::models::UnifiedDataType;
///
/// let unified = map_mysql_type("varchar", Some(255), None, None);
/// assert!(matches!(unified, UnifiedDataType::String { max_length: Some(255) }));
/// ```
pub fn map_mysql_type(
    mysql_type: &str,
    char_max_length: Option<u32>,
    numeric_precision: Option<u8>,
    numeric_scale: Option<u8>,
) -> UnifiedDataType {
    let type_lower = mysql_type.to_lowercase();
    let type_str = type_lower.as_str();

    // Check for unsigned modifier
    let is_unsigned = type_str.contains("unsigned");
    let base_type = type_str.replace(" unsigned", "").replace("unsigned ", "");
    let base_type = base_type.trim();

    match base_type {
        // String types
        "char" | "character" => UnifiedDataType::String {
            max_length: char_max_length,
        },
        "varchar" | "character varying" => UnifiedDataType::String {
            max_length: char_max_length,
        },
        "tinytext" => UnifiedDataType::String {
            max_length: Some(255),
        },
        "text" => UnifiedDataType::String {
            max_length: Some(65535),
        },
        "mediumtext" => UnifiedDataType::String {
            max_length: Some(16_777_215),
        },
        "longtext" => UnifiedDataType::String { max_length: None },

        // Integer types
        "tinyint" => {
            // TINYINT(1) is commonly used as boolean in MySQL
            if char_max_length == Some(1) {
                UnifiedDataType::Boolean
            } else {
                UnifiedDataType::Integer {
                    bits: 8,
                    signed: !is_unsigned,
                }
            }
        }
        "smallint" => UnifiedDataType::Integer {
            bits: 16,
            signed: !is_unsigned,
        },
        "mediumint" => UnifiedDataType::Integer {
            bits: 24,
            signed: !is_unsigned,
        },
        "int" | "integer" => UnifiedDataType::Integer {
            bits: 32,
            signed: !is_unsigned,
        },
        "bigint" => UnifiedDataType::Integer {
            bits: 64,
            signed: !is_unsigned,
        },

        // Decimal/Numeric types
        // Map to Float with precision (scale information is lost but precision is preserved)
        "decimal" | "numeric" | "dec" | "fixed" => {
            if let Some(scale) = numeric_scale {
                if scale == 0 {
                    // No decimal places - treat as integer
                    let bits = match numeric_precision {
                        Some(p) if p <= 2 => 8,
                        Some(p) if p <= 4 => 16,
                        Some(p) if p <= 9 => 32,
                        _ => 64,
                    };
                    UnifiedDataType::Integer { bits, signed: true }
                } else {
                    // Has decimal places - treat as float
                    UnifiedDataType::Float {
                        precision: numeric_precision,
                    }
                }
            } else {
                UnifiedDataType::Float {
                    precision: numeric_precision,
                }
            }
        }

        // Floating point types
        "float" => UnifiedDataType::Float {
            precision: Some(24),
        },
        "double" | "double precision" | "real" => UnifiedDataType::Float {
            precision: Some(53),
        },

        // Boolean type
        "boolean" | "bool" => UnifiedDataType::Boolean,

        // Date/Time types
        "date" => UnifiedDataType::Date,
        "time" => UnifiedDataType::Time {
            with_timezone: false,
        },
        "datetime" => UnifiedDataType::DateTime {
            with_timezone: false,
        },
        "timestamp" => UnifiedDataType::DateTime {
            with_timezone: true,
        },
        "year" => UnifiedDataType::Integer {
            bits: 16,
            signed: false,
        },

        // Binary types
        "binary" => UnifiedDataType::Binary {
            max_length: char_max_length,
        },
        "varbinary" => UnifiedDataType::Binary {
            max_length: char_max_length,
        },
        "tinyblob" => UnifiedDataType::Binary {
            max_length: Some(255),
        },
        "blob" => UnifiedDataType::Binary {
            max_length: Some(65535),
        },
        "mediumblob" => UnifiedDataType::Binary {
            max_length: Some(16_777_215),
        },
        "longblob" => UnifiedDataType::Binary { max_length: None },

        // BIT type
        "bit" => {
            if char_max_length == Some(1) {
                UnifiedDataType::Boolean
            } else {
                // BIT(n) stores n bits, calculate byte length
                let bits = char_max_length.unwrap_or(1);
                let bytes = bits.div_ceil(8);
                UnifiedDataType::Binary {
                    max_length: Some(bytes),
                }
            }
        }

        // JSON type
        "json" => UnifiedDataType::Json,

        // ENUM and SET types (MySQL-specific)
        "enum" => UnifiedDataType::Custom {
            type_name: "enum".to_string(),
        },
        "set" => UnifiedDataType::Custom {
            type_name: "set".to_string(),
        },

        // Geometry types
        "geometry" | "point" | "linestring" | "polygon" | "multipoint" | "multilinestring"
        | "multipolygon" | "geometrycollection" => UnifiedDataType::Custom {
            type_name: base_type.to_string(),
        },

        // Unknown type - preserve as custom
        _ => UnifiedDataType::Custom {
            type_name: base_type.to_string(),
        },
    }
}

/// Maps MySQL referential action to our unified representation
///
/// # Arguments
/// * `action` - The MySQL referential action string
///
/// # Returns
/// The standardized action string
pub fn map_referential_action(action: &str) -> String {
    match action.to_uppercase().as_str() {
        "CASCADE" => "CASCADE".to_string(),
        "SET NULL" => "SET NULL".to_string(),
        "SET DEFAULT" => "SET DEFAULT".to_string(),
        "RESTRICT" => "RESTRICT".to_string(),
        "NO ACTION" => "NO ACTION".to_string(),
        _ => action.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_type_with_length() {
        let result = map_mysql_type("VARCHAR", Some(100), None, None);
        assert!(matches!(
            result,
            UnifiedDataType::String {
                max_length: Some(100)
            }
        ));
    }

    #[test]
    fn test_unsigned_integer() {
        let result = map_mysql_type("INT UNSIGNED", None, None, None);
        assert!(matches!(
            result,
            UnifiedDataType::Integer {
                bits: 32,
                signed: false
            }
        ));
    }

    #[test]
    fn test_referential_action_mapping() {
        assert_eq!(map_referential_action("cascade"), "CASCADE");
        assert_eq!(map_referential_action("SET NULL"), "SET NULL");
        assert_eq!(map_referential_action("no action"), "NO ACTION");
    }
}
