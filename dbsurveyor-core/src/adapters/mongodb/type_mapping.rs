//! BSON type to `UnifiedDataType` mapping for MongoDB schema inference.
//!
//! MongoDB stores documents in BSON format, which has its own type system.
//! This module maps BSON types to the unified data types used by DBSurveyor.

use crate::models::UnifiedDataType;
use mongodb::bson::Bson;

/// Maps a BSON value to a `UnifiedDataType`.
///
/// # Arguments
/// * `value` - The BSON value to analyze
///
/// # Returns
/// The corresponding `UnifiedDataType` for the BSON value
///
/// # Example
/// ```rust,ignore
/// use mongodb::bson::Bson;
/// use dbsurveyor_core::adapters::mongodb::type_mapping::map_bson_to_unified;
///
/// let bson_value = Bson::String("hello".to_string());
/// let unified_type = map_bson_to_unified(&bson_value);
/// ```
pub fn map_bson_to_unified(value: &Bson) -> UnifiedDataType {
    match value {
        // String types
        Bson::String(_) => UnifiedDataType::String { max_length: None },

        // Integer types
        Bson::Int32(_) => UnifiedDataType::Integer {
            bits: 32,
            signed: true,
        },
        Bson::Int64(_) => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },

        // Floating point types
        Bson::Double(_) => UnifiedDataType::Float {
            precision: Some(53), // IEEE 754 double precision
        },

        // Boolean type
        Bson::Boolean(_) => UnifiedDataType::Boolean,

        // Date/time types
        Bson::DateTime(_) => UnifiedDataType::DateTime {
            with_timezone: true, // MongoDB DateTime is always UTC
        },
        Bson::Timestamp(_) => UnifiedDataType::DateTime {
            with_timezone: true,
        },

        // Binary data
        Bson::Binary(_) => UnifiedDataType::Binary { max_length: None },

        // ObjectId - treated as a 24-character hex string identifier
        Bson::ObjectId(_) => UnifiedDataType::String {
            max_length: Some(24),
        },

        // Embedded documents - represented as JSON
        Bson::Document(_) => UnifiedDataType::Json,

        // Arrays - we try to infer element type from first non-null element
        Bson::Array(arr) => {
            if arr.is_empty() {
                // Empty array - default to string element type
                UnifiedDataType::Array {
                    element_type: Box::new(UnifiedDataType::Custom {
                        type_name: "unknown".to_string(),
                    }),
                }
            } else {
                // Find the first non-null element to determine array type
                let element_type = arr
                    .iter()
                    .find(|v| !matches!(v, Bson::Null))
                    .map(map_bson_to_unified)
                    .unwrap_or(UnifiedDataType::Custom {
                        type_name: "unknown".to_string(),
                    });
                UnifiedDataType::Array {
                    element_type: Box::new(element_type),
                }
            }
        }

        // Null - represents optional/nullable fields
        // We map this to a custom type since null itself isn't a data type
        Bson::Null => UnifiedDataType::Custom {
            type_name: "null".to_string(),
        },

        // Regular expressions
        Bson::RegularExpression(_) => UnifiedDataType::Custom {
            type_name: "regex".to_string(),
        },

        // JavaScript code
        Bson::JavaScriptCode(_) | Bson::JavaScriptCodeWithScope(_) => UnifiedDataType::Custom {
            type_name: "javascript".to_string(),
        },

        // Symbols (deprecated in MongoDB)
        Bson::Symbol(_) => UnifiedDataType::String { max_length: None },

        // Decimal128 for precise decimal arithmetic
        Bson::Decimal128(_) => UnifiedDataType::Float {
            precision: Some(128),
        },

        // Min/Max keys (internal MongoDB types)
        Bson::MinKey | Bson::MaxKey => UnifiedDataType::Custom {
            type_name: "key".to_string(),
        },

        // Undefined (deprecated in MongoDB)
        Bson::Undefined => UnifiedDataType::Custom {
            type_name: "undefined".to_string(),
        },

        // DBPointer (deprecated in MongoDB)
        Bson::DbPointer(_) => UnifiedDataType::Custom {
            type_name: "dbpointer".to_string(),
        },
    }
}

/// Gets a human-readable type name for a BSON value.
///
/// # Arguments
/// * `value` - The BSON value to analyze
///
/// # Returns
/// A string describing the BSON type
pub fn bson_type_name(value: &Bson) -> &'static str {
    match value {
        Bson::String(_) => "string",
        Bson::Int32(_) => "int32",
        Bson::Int64(_) => "int64",
        Bson::Double(_) => "double",
        Bson::Boolean(_) => "bool",
        Bson::DateTime(_) => "date",
        Bson::Timestamp(_) => "timestamp",
        Bson::Binary(_) => "binData",
        Bson::ObjectId(_) => "objectId",
        Bson::Document(_) => "object",
        Bson::Array(_) => "array",
        Bson::Null => "null",
        Bson::RegularExpression(_) => "regex",
        Bson::JavaScriptCode(_) | Bson::JavaScriptCodeWithScope(_) => "javascript",
        Bson::Symbol(_) => "symbol",
        Bson::Decimal128(_) => "decimal",
        Bson::MinKey => "minKey",
        Bson::MaxKey => "maxKey",
        Bson::Undefined => "undefined",
        Bson::DbPointer(_) => "dbPointer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{Binary, DateTime, Decimal128, oid::ObjectId, spec::BinarySubtype};

    #[test]
    fn test_map_string() {
        let bson = Bson::String("hello".to_string());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::String { max_length: None }
        ));
    }

    #[test]
    fn test_map_int32() {
        let bson = Bson::Int32(42);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
    }

    #[test]
    fn test_map_int64() {
        let bson = Bson::Int64(9_999_999_999);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Integer {
                bits: 64,
                signed: true
            }
        ));
    }

    #[test]
    fn test_map_double() {
        let bson = Bson::Double(1.234);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Float {
                precision: Some(53)
            }
        ));
    }

    #[test]
    fn test_map_boolean() {
        let bson = Bson::Boolean(true);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(unified, UnifiedDataType::Boolean));
    }

    #[test]
    fn test_map_datetime() {
        let bson = Bson::DateTime(DateTime::now());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::DateTime {
                with_timezone: true
            }
        ));
    }

    #[test]
    fn test_map_binary() {
        let bson = Bson::Binary(Binary {
            subtype: BinarySubtype::Generic,
            bytes: vec![1, 2, 3],
        });
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Binary { max_length: None }
        ));
    }

    #[test]
    fn test_map_object_id() {
        let bson = Bson::ObjectId(ObjectId::new());
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::String {
                max_length: Some(24)
            }
        ));
    }

    #[test]
    fn test_map_document() {
        let bson = Bson::Document(mongodb::bson::doc! { "key": "value" });
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(unified, UnifiedDataType::Json));
    }

    #[test]
    fn test_map_array_with_elements() {
        let bson = Bson::Array(vec![Bson::Int32(1), Bson::Int32(2), Bson::Int32(3)]);
        let unified = map_bson_to_unified(&bson);
        if let UnifiedDataType::Array { element_type } = unified {
            assert!(matches!(
                *element_type,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true
                }
            ));
        } else {
            panic!("Expected Array type");
        }
    }

    #[test]
    fn test_map_empty_array() {
        let bson = Bson::Array(vec![]);
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(unified, UnifiedDataType::Array { .. }));
    }

    #[test]
    fn test_map_null() {
        let bson = Bson::Null;
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Custom { type_name } if type_name == "null"
        ));
    }

    #[test]
    fn test_map_decimal128() {
        let bson = Bson::Decimal128(Decimal128::from_bytes([0; 16]));
        let unified = map_bson_to_unified(&bson);
        assert!(matches!(
            unified,
            UnifiedDataType::Float {
                precision: Some(128)
            }
        ));
    }

    #[test]
    fn test_bson_type_names() {
        assert_eq!(bson_type_name(&Bson::String("".to_string())), "string");
        assert_eq!(bson_type_name(&Bson::Int32(0)), "int32");
        assert_eq!(bson_type_name(&Bson::Int64(0)), "int64");
        assert_eq!(bson_type_name(&Bson::Double(0.0)), "double");
        assert_eq!(bson_type_name(&Bson::Boolean(true)), "bool");
        assert_eq!(bson_type_name(&Bson::Null), "null");
        assert_eq!(bson_type_name(&Bson::ObjectId(ObjectId::new())), "objectId");
    }
}
