//! Unit tests for MySQL adapter.
//!
//! These tests verify the MySQL adapter functionality including:
//! - Type mapping from MySQL types to UnifiedDataType
//! - Connection configuration parsing
//! - Schema collection queries

use crate::adapters::{AdapterFeature, DatabaseAdapter};
use crate::models::{DatabaseType, UnifiedDataType};

use super::MySqlAdapter;
use super::type_mapping::map_mysql_type;

// =============================================================================
// Type Mapping Tests
// =============================================================================

#[test]
fn test_map_mysql_varchar_type() {
    let result = map_mysql_type("varchar", Some(255), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(255)
        }
    ));
}

#[test]
fn test_map_mysql_varchar_without_length() {
    let result = map_mysql_type("varchar", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String { max_length: None }
    ));
}

#[test]
fn test_map_mysql_char_type() {
    let result = map_mysql_type("char", Some(10), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(10)
        }
    ));
}

#[test]
fn test_map_mysql_text_types() {
    // TINYTEXT
    let result = map_mysql_type("tinytext", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(255)
        }
    ));

    // TEXT
    let result = map_mysql_type("text", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(65535)
        }
    ));

    // MEDIUMTEXT
    let result = map_mysql_type("mediumtext", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(16_777_215)
        }
    ));

    // LONGTEXT
    let result = map_mysql_type("longtext", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::String { max_length: None }
    ));
}

#[test]
fn test_map_mysql_integer_types() {
    // TINYINT
    let result = map_mysql_type("tinyint", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 8,
            signed: true
        }
    ));

    // TINYINT UNSIGNED
    let result = map_mysql_type("tinyint unsigned", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 8,
            signed: false
        }
    ));

    // SMALLINT
    let result = map_mysql_type("smallint", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 16,
            signed: true
        }
    ));

    // MEDIUMINT
    let result = map_mysql_type("mediumint", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 24,
            signed: true
        }
    ));

    // INT / INTEGER
    let result = map_mysql_type("int", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));

    let result = map_mysql_type("integer", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));

    // BIGINT
    let result = map_mysql_type("bigint", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 64,
            signed: true
        }
    ));

    // BIGINT UNSIGNED
    let result = map_mysql_type("bigint unsigned", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 64,
            signed: false
        }
    ));
}

#[test]
fn test_map_mysql_decimal_types() {
    // DECIMAL with precision and scale - maps to Float
    let result = map_mysql_type("decimal", None, Some(10), Some(2));
    assert!(matches!(
        result,
        UnifiedDataType::Float {
            precision: Some(10)
        }
    ));

    // DECIMAL with scale 0 - maps to Integer
    let result = map_mysql_type("decimal", None, Some(10), Some(0));
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 64,
            signed: true
        }
    ));

    // Small DECIMAL with scale 0 - maps to smaller Integer
    let result = map_mysql_type("decimal", None, Some(4), Some(0));
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 16,
            signed: true
        }
    ));
}

#[test]
fn test_map_mysql_float_types() {
    // FLOAT
    let result = map_mysql_type("float", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Float {
            precision: Some(24)
        }
    ));

    // DOUBLE
    let result = map_mysql_type("double", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Float {
            precision: Some(53)
        }
    ));

    // REAL (alias for DOUBLE in MySQL)
    let result = map_mysql_type("real", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Float {
            precision: Some(53)
        }
    ));
}

#[test]
fn test_map_mysql_boolean_type() {
    // BOOLEAN / BOOL (aliases for TINYINT(1) in MySQL)
    let result = map_mysql_type("boolean", None, None, None);
    assert!(matches!(result, UnifiedDataType::Boolean));

    let result = map_mysql_type("bool", None, None, None);
    assert!(matches!(result, UnifiedDataType::Boolean));

    // TINYINT(1) is also commonly used as boolean
    let result = map_mysql_type("tinyint", Some(1), None, None);
    assert!(matches!(result, UnifiedDataType::Boolean));
}

#[test]
fn test_map_mysql_date_time_types() {
    // DATE
    let result = map_mysql_type("date", None, None, None);
    assert!(matches!(result, UnifiedDataType::Date));

    // TIME
    let result = map_mysql_type("time", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Time {
            with_timezone: false
        }
    ));

    // DATETIME
    let result = map_mysql_type("datetime", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::DateTime {
            with_timezone: false
        }
    ));

    // TIMESTAMP (has timezone in MySQL)
    let result = map_mysql_type("timestamp", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::DateTime {
            with_timezone: true
        }
    ));

    // YEAR
    let result = map_mysql_type("year", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 16,
            signed: false
        }
    ));
}

#[test]
fn test_map_mysql_binary_types() {
    // BINARY
    let result = map_mysql_type("binary", Some(16), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(16)
        }
    ));

    // VARBINARY
    let result = map_mysql_type("varbinary", Some(255), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(255)
        }
    ));

    // BLOB types
    let result = map_mysql_type("tinyblob", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(255)
        }
    ));

    let result = map_mysql_type("blob", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(65535)
        }
    ));

    let result = map_mysql_type("mediumblob", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(16_777_215)
        }
    ));

    let result = map_mysql_type("longblob", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary { max_length: None }
    ));
}

#[test]
fn test_map_mysql_json_type() {
    let result = map_mysql_type("json", None, None, None);
    assert!(matches!(result, UnifiedDataType::Json));
}

#[test]
fn test_map_mysql_enum_type() {
    let result = map_mysql_type("enum", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Custom { ref type_name } if type_name == "enum"
    ));
}

#[test]
fn test_map_mysql_set_type() {
    let result = map_mysql_type("set", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Custom { ref type_name } if type_name == "set"
    ));
}

#[test]
fn test_map_mysql_geometry_types() {
    for geo_type in [
        "geometry",
        "point",
        "linestring",
        "polygon",
        "multipoint",
        "multilinestring",
        "multipolygon",
        "geometrycollection",
    ] {
        let result = map_mysql_type(geo_type, None, None, None);
        assert!(matches!(
            result,
            UnifiedDataType::Custom { ref type_name } if type_name == geo_type
        ));
    }
}

#[test]
fn test_map_mysql_bit_type() {
    let result = map_mysql_type("bit", Some(1), None, None);
    assert!(matches!(result, UnifiedDataType::Boolean));

    let result = map_mysql_type("bit", Some(8), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(1)
        }
    ));

    let result = map_mysql_type("bit", Some(64), None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Binary {
            max_length: Some(8)
        }
    ));
}

#[test]
fn test_map_mysql_unknown_type() {
    let result = map_mysql_type("unknown_custom_type", None, None, None);
    assert!(matches!(
        result,
        UnifiedDataType::Custom { ref type_name } if type_name == "unknown_custom_type"
    ));
}

// =============================================================================
// Adapter Feature Tests
// =============================================================================

#[tokio::test]
async fn test_mysql_adapter_database_type() {
    // This test verifies the adapter returns the correct database type
    // For now, we test with a placeholder - integration tests will test real connections
    let adapter = MySqlAdapter::new("mysql://localhost/test").await.unwrap();
    assert_eq!(adapter.database_type(), DatabaseType::MySQL);
}

#[tokio::test]
async fn test_mysql_adapter_supports_features() {
    let adapter = MySqlAdapter::new("mysql://localhost/test").await.unwrap();

    // MySQL should support these features
    assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
    assert!(adapter.supports_feature(AdapterFeature::DataSampling));
    assert!(adapter.supports_feature(AdapterFeature::MultiDatabase));
    assert!(adapter.supports_feature(AdapterFeature::ConnectionPooling));
    assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));
    assert!(adapter.supports_feature(AdapterFeature::ReadOnlyMode));
}

// =============================================================================
// Connection Configuration Tests
// =============================================================================

#[test]
fn test_parse_mysql_connection_config() {
    use super::connection::parse_mysql_connection_config;

    let config = parse_mysql_connection_config("mysql://user:pass@localhost:3306/testdb").unwrap();

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(3306));
    assert_eq!(config.database, Some("testdb".to_string()));
    assert_eq!(config.username, Some("user".to_string()));
}

#[test]
fn test_parse_mysql_connection_config_defaults() {
    use super::connection::parse_mysql_connection_config;

    let config = parse_mysql_connection_config("mysql://localhost/mydb").unwrap();

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(3306)); // MySQL default port
    assert_eq!(config.database, Some("mydb".to_string()));
}

#[test]
fn test_validate_mysql_connection_string_valid() {
    use super::connection::validate_mysql_connection_string;

    assert!(validate_mysql_connection_string("mysql://localhost/test").is_ok());
    assert!(validate_mysql_connection_string("mysql://user:pass@localhost:3306/db").is_ok());
}

#[test]
fn test_validate_mysql_connection_string_invalid_scheme() {
    use super::connection::validate_mysql_connection_string;

    let result = validate_mysql_connection_string("postgres://localhost/test");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("mysql://"));
}

#[test]
fn test_validate_mysql_connection_string_missing_host() {
    use super::connection::validate_mysql_connection_string;

    let result = validate_mysql_connection_string("mysql:///test");
    assert!(result.is_err());
}
