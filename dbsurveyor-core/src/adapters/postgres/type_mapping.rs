//! PostgreSQL to unified data type conversion.
//!
//! This module handles mapping between PostgreSQL-specific data types
//! and the unified type system used by DBSurveyor.

use crate::Result;
use crate::models::{ReferentialAction, UnifiedDataType};

use super::PostgresAdapter;

impl PostgresAdapter {
    /// Maps PostgreSQL data types to unified data types (internal version).
    ///
    /// # Arguments
    /// * `data_type` - PostgreSQL data_type from information_schema
    /// * `udt_name` - User-defined type name from information_schema
    /// * `character_maximum_length` - Maximum character length for string types
    /// * `numeric_precision` - Numeric precision for decimal types
    /// * `numeric_scale` - Numeric scale for decimal types
    /// * `array_element_type` - Element type for array types
    ///
    /// # Returns
    /// Returns the corresponding UnifiedDataType or an error if the type is unsupported
    pub(crate) fn map_postgres_type_to_unified(
        data_type: &str,
        udt_name: &str,
        character_maximum_length: Option<i32>,
        numeric_precision: Option<i32>,
        numeric_scale: Option<i32>,
        array_element_type: Option<&str>,
    ) -> Result<UnifiedDataType> {
        let unified_type = match data_type.to_lowercase().as_str() {
            // String/Character types
            "character varying" | "varchar" => UnifiedDataType::String {
                max_length: character_maximum_length.map(|l| l as u32),
            },
            "character" | "char" => UnifiedDataType::String {
                max_length: character_maximum_length.map(|l| l as u32),
            },
            "text" => UnifiedDataType::String { max_length: None },

            // Integer types
            "smallint" | "int2" => UnifiedDataType::Integer {
                bits: 16,
                signed: true,
            },
            "integer" | "int" | "int4" => UnifiedDataType::Integer {
                bits: 32,
                signed: true,
            },
            "bigint" | "int8" => UnifiedDataType::Integer {
                bits: 64,
                signed: true,
            },

            // Floating point types
            "real" | "float4" => UnifiedDataType::Float {
                precision: Some(24),
            },
            "double precision" | "float8" => UnifiedDataType::Float {
                precision: Some(53),
            },
            "numeric" | "decimal" => {
                if let Some(scale) = numeric_scale {
                    if scale == 0 {
                        // No decimal places - treat as integer
                        let bits = match numeric_precision {
                            Some(p) if p <= 4 => 16,
                            Some(p) if p <= 9 => 32,
                            _ => 64,
                        };
                        UnifiedDataType::Integer { bits, signed: true }
                    } else {
                        // Has decimal places - treat as float
                        UnifiedDataType::Float {
                            precision: numeric_precision.map(|p| p as u8),
                        }
                    }
                } else {
                    UnifiedDataType::Float {
                        precision: numeric_precision.map(|p| p as u8),
                    }
                }
            }

            // Boolean type
            "boolean" | "bool" => UnifiedDataType::Boolean,

            // Date and time types
            "timestamp without time zone" | "timestamp" => UnifiedDataType::DateTime {
                with_timezone: false,
            },
            "timestamp with time zone" | "timestamptz" => UnifiedDataType::DateTime {
                with_timezone: true,
            },
            "date" => UnifiedDataType::Date,
            "time without time zone" | "time" => UnifiedDataType::Time {
                with_timezone: false,
            },
            "time with time zone" | "timetz" => UnifiedDataType::Time {
                with_timezone: true,
            },

            // Binary types
            "bytea" => UnifiedDataType::Binary { max_length: None },

            // JSON types
            "json" => UnifiedDataType::Json,
            "jsonb" => UnifiedDataType::Json,

            // UUID type
            "uuid" => UnifiedDataType::Uuid,

            // Array types
            "array" => {
                if let Some(element_type) = array_element_type {
                    // Recursively map the element type
                    let element_unified_type = Self::map_postgres_type_to_unified(
                        element_type,
                        element_type,
                        character_maximum_length,
                        numeric_precision,
                        numeric_scale,
                        None, // Arrays of arrays not supported in this mapping
                    )?;
                    UnifiedDataType::Array {
                        element_type: Box::new(element_unified_type),
                    }
                } else {
                    // Fallback for unknown array element type
                    UnifiedDataType::Custom {
                        type_name: format!("{}[]", udt_name),
                    }
                }
            }

            // PostgreSQL-specific types that map to custom
            "inet" | "cidr" | "macaddr" | "macaddr8" => UnifiedDataType::Custom {
                type_name: udt_name.to_string(),
            },
            "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => {
                UnifiedDataType::Custom {
                    type_name: udt_name.to_string(),
                }
            }
            "tsvector" | "tsquery" => UnifiedDataType::Custom {
                type_name: udt_name.to_string(),
            },
            "xml" => UnifiedDataType::Custom {
                type_name: "xml".to_string(),
            },

            // Handle user-defined types and enums
            "user-defined" => {
                // Check for common PostgreSQL built-in types that appear as user-defined
                match udt_name {
                    "uuid" => UnifiedDataType::Uuid,
                    "json" => UnifiedDataType::Json,
                    "jsonb" => UnifiedDataType::Json,
                    "inet" | "cidr" | "macaddr" | "macaddr8" => UnifiedDataType::Custom {
                        type_name: udt_name.to_string(),
                    },
                    _ => {
                        // Assume it's an enum or custom type
                        UnifiedDataType::Custom {
                            type_name: udt_name.to_string(),
                        }
                    }
                }
            }

            // Fallback for unknown types
            _ => {
                tracing::warn!(
                    "Unknown PostgreSQL data type '{}' (UDT: '{}'), mapping to custom type",
                    data_type,
                    udt_name
                );
                // Use UDT name if available and different from data_type, otherwise just data_type
                let type_name = if udt_name != data_type && !udt_name.is_empty() {
                    format!("{}({})", data_type, udt_name)
                } else {
                    data_type.to_string()
                };
                UnifiedDataType::Custom { type_name }
            }
        };

        Ok(unified_type)
    }
}

/// Maps PostgreSQL data types to unified data types.
///
/// # Arguments
/// * `pg_type` - PostgreSQL type name
/// * `char_max_length` - Maximum character length for string types
/// * `numeric_precision` - Numeric precision for decimal types
/// * `numeric_scale` - Numeric scale for decimal types
///
/// # Returns
/// Returns the corresponding UnifiedDataType or an error if the type is unsupported
pub fn map_postgresql_type(
    pg_type: &str,
    char_max_length: Option<i32>,
    _numeric_precision: Option<i32>,
    _numeric_scale: Option<i32>,
) -> Result<UnifiedDataType> {
    let unified_type = match pg_type {
        // String types
        "character varying" | "varchar" => UnifiedDataType::String {
            max_length: char_max_length.map(|l| l as u32),
        },
        "text" | "character" | "char" => UnifiedDataType::String { max_length: None },

        // Integer types
        "smallint" | "int2" => UnifiedDataType::Integer {
            bits: 16,
            signed: true,
        },
        "integer" | "int" | "int4" => UnifiedDataType::Integer {
            bits: 32,
            signed: true,
        },
        "bigint" | "int8" => UnifiedDataType::Integer {
            bits: 64,
            signed: true,
        },

        // Boolean type
        "boolean" | "bool" => UnifiedDataType::Boolean,

        // Date/time types
        "timestamp without time zone" | "timestamp" => UnifiedDataType::DateTime {
            with_timezone: false,
        },
        "timestamp with time zone" | "timestamptz" => UnifiedDataType::DateTime {
            with_timezone: true,
        },
        "date" => UnifiedDataType::Date,
        "time" | "time without time zone" => UnifiedDataType::Time {
            with_timezone: false,
        },
        "time with time zone" | "timetz" => UnifiedDataType::Time {
            with_timezone: true,
        },

        // JSON types
        "json" | "jsonb" => UnifiedDataType::Json,

        // UUID type
        "uuid" => UnifiedDataType::Uuid,

        // Binary type
        "bytea" => UnifiedDataType::Binary { max_length: None },

        // Array types (simplified detection)
        t if t.ends_with("[]") => {
            let base_type = &t[..t.len() - 2];
            let element_type = Box::new(map_postgresql_type(base_type, None, None, None)?);
            UnifiedDataType::Array { element_type }
        }

        // Custom/unknown types
        _ => UnifiedDataType::Custom {
            type_name: pg_type.to_string(),
        },
    };

    Ok(unified_type)
}

/// Maps PostgreSQL referential action codes to unified referential actions.
///
/// # Arguments
/// * `action_rule` - PostgreSQL referential action rule (CASCADE, SET NULL, SET DEFAULT, RESTRICT, NO ACTION)
///
/// # Returns
/// Returns the corresponding ReferentialAction or None if unknown
///
/// # PostgreSQL Referential Actions
/// - CASCADE: Automatically delete/update dependent rows
/// - SET NULL: Set foreign key columns to NULL
/// - SET DEFAULT: Set foreign key columns to their default values
/// - RESTRICT: Prevent delete/update if dependent rows exist
/// - NO ACTION: Same as RESTRICT but check can be deferred
pub fn map_referential_action(action_rule: &str) -> Option<ReferentialAction> {
    match action_rule.to_uppercase().as_str() {
        "CASCADE" => Some(ReferentialAction::Cascade),
        "SET NULL" => Some(ReferentialAction::SetNull),
        "SET DEFAULT" => Some(ReferentialAction::SetDefault),
        "RESTRICT" => Some(ReferentialAction::Restrict),
        "NO ACTION" => Some(ReferentialAction::NoAction),
        _ => {
            tracing::warn!("Unknown referential action rule: '{}'", action_rule);
            None
        }
    }
}
