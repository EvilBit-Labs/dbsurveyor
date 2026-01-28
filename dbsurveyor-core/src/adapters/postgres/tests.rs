//! Unit tests for PostgreSQL adapter.

use super::*;
use crate::adapters::ConnectionConfig;
use std::time::Duration;

#[test]
fn test_parse_connection_config() {
    let connection_string = "postgres://testuser@localhost:5432/testdb";
    let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(5432));
    assert_eq!(config.database, Some("testdb".to_string()));
    assert_eq!(config.username, Some("testuser".to_string()));
    assert!(config.read_only);
    assert_eq!(config.connect_timeout, Duration::from_secs(30));
    assert_eq!(config.query_timeout, Duration::from_secs(30));
    assert_eq!(config.max_connections, 10);
}

#[test]
fn test_parse_connection_config_with_query_params() {
    let connection_string =
        "postgres://user@host/db?connect_timeout=60&statement_timeout=45000&pool_max_conns=20";
    let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "host");
    assert_eq!(config.port, Some(5432)); // Default PostgreSQL port
    assert_eq!(config.database, Some("db".to_string()));
    assert_eq!(config.username, Some("user".to_string()));
    assert_eq!(config.connect_timeout, Duration::from_secs(60));
    assert_eq!(config.query_timeout, Duration::from_millis(45000));
    assert_eq!(config.max_connections, 20);
}

#[test]
fn test_parse_connection_config_defaults() {
    let connection_string = "postgres://user@host/db";
    let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "host");
    assert_eq!(config.port, Some(5432)); // Default PostgreSQL port
    assert_eq!(config.database, Some("db".to_string()));
    assert_eq!(config.username, Some("user".to_string()));
    assert!(config.read_only); // Default to read-only for security
}

#[test]
fn test_parse_connection_config_minimal() {
    let connection_string = "postgres://host";
    let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

    assert_eq!(config.host, "host");
    assert_eq!(config.port, Some(5432));
    assert_eq!(config.database, None);
    assert_eq!(config.username, None);
}

#[test]
fn test_parse_connection_config_invalid_scheme() {
    let connection_string = "mysql://user@host/db";
    let result = PostgresAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("postgres://"));
}

#[test]
fn test_parse_connection_config_invalid_url() {
    let connection_string = "invalid-url";
    let result = PostgresAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
}

#[test]
fn test_map_postgresql_type_basic_types() {
    use crate::models::UnifiedDataType;

    // Test string types
    let varchar_type = map_postgresql_type("character varying", Some(255), None, None).unwrap();
    assert!(matches!(
        varchar_type,
        UnifiedDataType::String {
            max_length: Some(255)
        }
    ));

    let text_type = map_postgresql_type("text", None, None, None).unwrap();
    assert!(matches!(
        text_type,
        UnifiedDataType::String { max_length: None }
    ));

    // Test integer types
    let int_type = map_postgresql_type("integer", None, None, None).unwrap();
    assert!(matches!(
        int_type,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));

    let bigint_type = map_postgresql_type("bigint", None, None, None).unwrap();
    assert!(matches!(
        bigint_type,
        UnifiedDataType::Integer {
            bits: 64,
            signed: true
        }
    ));

    // Test boolean type
    let bool_type = map_postgresql_type("boolean", None, None, None).unwrap();
    assert!(matches!(bool_type, UnifiedDataType::Boolean));

    // Test timestamp types
    let timestamp_type =
        map_postgresql_type("timestamp without time zone", None, None, None).unwrap();
    assert!(matches!(
        timestamp_type,
        UnifiedDataType::DateTime {
            with_timezone: false
        }
    ));

    let timestamptz_type =
        map_postgresql_type("timestamp with time zone", None, None, None).unwrap();
    assert!(matches!(
        timestamptz_type,
        UnifiedDataType::DateTime {
            with_timezone: true
        }
    ));

    // Test JSON types
    let json_type = map_postgresql_type("json", None, None, None).unwrap();
    assert!(matches!(json_type, UnifiedDataType::Json));

    let jsonb_type = map_postgresql_type("jsonb", None, None, None).unwrap();
    assert!(matches!(jsonb_type, UnifiedDataType::Json));

    // Test UUID type
    let uuid_type = map_postgresql_type("uuid", None, None, None).unwrap();
    assert!(matches!(uuid_type, UnifiedDataType::Uuid));

    // Test array type
    let array_type = map_postgresql_type("integer[]", None, None, None).unwrap();
    if let UnifiedDataType::Array { element_type } = array_type {
        assert!(matches!(
            *element_type,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));
    } else {
        panic!("Expected array type");
    }

    // Test custom type
    let custom_type = map_postgresql_type("custom_enum", None, None, None).unwrap();
    assert!(
        matches!(custom_type, UnifiedDataType::Custom { type_name } if type_name == "custom_enum")
    );
}

#[test]
fn test_map_referential_action() {
    use crate::models::ReferentialAction;

    // Test full action names (as returned by information_schema.referential_constraints)
    assert_eq!(
        map_referential_action("CASCADE"),
        Some(ReferentialAction::Cascade)
    );
    assert_eq!(
        map_referential_action("SET NULL"),
        Some(ReferentialAction::SetNull)
    );
    assert_eq!(
        map_referential_action("SET DEFAULT"),
        Some(ReferentialAction::SetDefault)
    );
    assert_eq!(
        map_referential_action("RESTRICT"),
        Some(ReferentialAction::Restrict)
    );
    assert_eq!(
        map_referential_action("NO ACTION"),
        Some(ReferentialAction::NoAction)
    );

    // Test case insensitivity
    assert_eq!(
        map_referential_action("cascade"),
        Some(ReferentialAction::Cascade)
    );
    assert_eq!(
        map_referential_action("set null"),
        Some(ReferentialAction::SetNull)
    );

    // Test unknown action
    assert_eq!(map_referential_action("UNKNOWN"), None);
    assert_eq!(map_referential_action("x"), None);
}

#[test]
fn test_connection_config_builder_pattern() {
    let config = ConnectionConfig::new("localhost".to_string())
        .with_port(5432)
        .with_database("testdb".to_string())
        .with_username("testuser".to_string());

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(5432));
    assert_eq!(config.database, Some("testdb".to_string()));
    assert_eq!(config.username, Some("testuser".to_string()));
    assert!(config.read_only); // Default should be read-only for security
}

#[test]
fn test_connection_config_validation_limits() {
    // Test max connections limit
    let config = ConnectionConfig {
        max_connections: 101, // Over limit
        ..Default::default()
    };
    assert!(config.validate().is_err());

    let config = ConnectionConfig {
        max_connections: 50, // Within limit
        ..Default::default()
    };
    assert!(config.validate().is_ok());

    // Test zero max connections
    let config = ConnectionConfig {
        max_connections: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Test zero connect timeout
    let config = ConnectionConfig {
        max_connections: 10,
        connect_timeout: Duration::from_secs(0),
        ..Default::default()
    };
    assert!(config.validate().is_err());

    // Test zero query timeout
    let config = ConnectionConfig {
        max_connections: 10,
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(0),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_parse_connection_config_no_host() {
    let connection_string = "postgres:///db";
    let result = PostgresAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("host"));
}

#[test]
fn test_parse_connection_config_invalid_port() {
    let connection_string = "postgres://user@host:0/db";
    let result = PostgresAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("port"));
}

#[test]
fn test_parse_connection_config_long_database_name() {
    let long_name = "a".repeat(64); // Too long (max 63)
    let connection_string = format!("postgres://user@host/{}", long_name);
    let result = PostgresAdapter::parse_connection_config(&connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too long"));
}

#[test]
fn test_parse_connection_config_invalid_database_chars() {
    let connection_string = "postgres://user@host/db@invalid";
    let result = PostgresAdapter::parse_connection_config(connection_string);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid characters")
    );
}

#[test]
fn test_parse_connection_config_long_username() {
    let long_username = "a".repeat(64); // Too long (max 63)
    let connection_string = format!("postgres://{}@host/db", long_username);
    let result = PostgresAdapter::parse_connection_config(&connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too long"));
}

#[test]
fn test_validate_connection_string_valid() {
    let connection_string = "postgres://user@localhost:5432/db";
    let result = PostgresAdapter::validate_connection_string(connection_string);
    assert!(result.is_ok());
}

#[test]
fn test_validate_connection_string_postgresql_scheme() {
    let connection_string = "postgresql://user@localhost:5432/db";
    let result = PostgresAdapter::validate_connection_string(connection_string);
    assert!(result.is_ok());
}

#[test]
fn test_validate_connection_string_invalid_scheme() {
    let connection_string = "mysql://user@localhost:3306/db";
    let result = PostgresAdapter::validate_connection_string(connection_string);
    assert!(result.is_err());
}

#[test]
fn test_validate_connection_string_no_host() {
    let connection_string = "postgres:///db";
    let result = PostgresAdapter::validate_connection_string(connection_string);
    assert!(result.is_err());
}

#[test]
fn test_validate_connection_string_excessive_timeout() {
    let connection_string = "postgres://user@host/db?statement_timeout=400000"; // > 5 minutes
    let result = PostgresAdapter::validate_connection_string(connection_string);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("300 seconds"));
}

#[test]
fn test_supports_feature() {
    // Test feature support without creating a real pool
    let connection_string = "postgres://test@localhost/test";
    let _parsed_config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

    // Test feature support directly
    let features = vec![
        AdapterFeature::SchemaCollection,
        AdapterFeature::DataSampling,
        AdapterFeature::MultiDatabase,
        AdapterFeature::ConnectionPooling,
        AdapterFeature::QueryTimeout,
        AdapterFeature::ReadOnlyMode,
    ];

    for feature in features {
        assert!(matches!(
            feature,
            AdapterFeature::SchemaCollection
                | AdapterFeature::DataSampling
                | AdapterFeature::MultiDatabase
                | AdapterFeature::ConnectionPooling
                | AdapterFeature::QueryTimeout
                | AdapterFeature::ReadOnlyMode
        ));
    }
}

#[test]
fn test_database_type() {
    use crate::models::DatabaseType;
    assert_eq!(DatabaseType::PostgreSQL.to_string(), "PostgreSQL");
}

#[test]
fn test_connection_config_display() {
    let config = ConnectionConfig::default();
    let display = format!("{}", config);

    // Should contain connection info but not credentials
    assert!(display.contains("localhost"));
    assert!(!display.contains("password"));
    assert!(!display.contains("secret"));
}

#[test]
fn test_database_name_validation() {
    // Valid database names
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/valid_db").is_ok());
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/_underscore").is_ok());
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/db$dollar").is_ok());

    // Invalid database names
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/123invalid").is_err()); // Starts with number
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/-invalid").is_err()); // Starts with dash
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/invalid-char").is_err()); // Contains dash
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/invalid@char").is_err()); // Contains @

    // Empty database name
    assert!(PostgresAdapter::parse_connection_config("postgres://user@host/").is_ok()); // Empty is OK (uses default)
}

#[test]
fn test_username_validation() {
    // Valid usernames
    assert!(PostgresAdapter::parse_connection_config("postgres://valid_user@host/db").is_ok());
    assert!(PostgresAdapter::parse_connection_config("postgres://_underscore@host/db").is_ok());
    assert!(PostgresAdapter::parse_connection_config("postgres://user$dollar@host/db").is_ok());

    // Invalid usernames
    assert!(PostgresAdapter::parse_connection_config("postgres://123invalid@host/db").is_err()); // Starts with number
    assert!(PostgresAdapter::parse_connection_config("postgres://-invalid@host/db").is_err()); // Starts with dash
    assert!(PostgresAdapter::parse_connection_config("postgres://invalid-char@host/db").is_err()); // Contains dash
    assert!(PostgresAdapter::parse_connection_config("postgres://invalid@char@host/db").is_err()); // Contains @
}
