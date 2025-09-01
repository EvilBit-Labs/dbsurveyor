//! PostgreSQL schema enumeration tests for task 2.2.
//!
//! These tests verify the basic schema enumeration queries implementation
//! including schema discovery, table enumeration, and error handling.

#[cfg(feature = "postgresql")]
mod postgres_schema_enumeration_tests {
    use dbsurveyor_core::{
        adapters::{create_adapter, postgres::PostgresAdapter},
        models::{DatabaseType, UnifiedDataType},
    };

    #[tokio::test]
    async fn test_postgres_adapter_database_type() {
        let connection_string = "postgres://user:pass@localhost:5432/testdb";
        let adapter = create_adapter(connection_string).await.unwrap();

        assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    }

    #[tokio::test]
    async fn test_postgres_connection_string_validation() {
        // Test valid PostgreSQL connection strings
        let valid_strings = vec![
            "postgres://user@localhost/db",
            "postgresql://user:pass@localhost:5432/db",
            "postgres://localhost/db",
            "postgresql://user@host:5432/db?sslmode=require",
        ];

        for conn_str in valid_strings {
            let result = PostgresAdapter::validate_connection_string(conn_str);
            assert!(result.is_ok(), "Connection string should be valid: {}", conn_str);
        }

        // Test invalid connection strings
        let invalid_strings = vec![
            "mysql://user@localhost/db",  // Wrong scheme
            "postgres://",                // No host
            "http://localhost/db",        // Wrong scheme
            "postgres://user@host/db?statement_timeout=400000", // Excessive timeout
        ];

        for conn_str in invalid_strings {
            let result = PostgresAdapter::validate_connection_string(conn_str);
            assert!(result.is_err(), "Connection string should be invalid: {}", conn_str);
        }
    }

    #[tokio::test]
    async fn test_postgresql_type_mapping_comprehensive() {
        // Test string types
        let varchar_type = PostgresAdapter::map_postgresql_type("character varying", Some(255), None, None).unwrap();
        assert!(matches!(varchar_type, UnifiedDataType::String { max_length: Some(255) }));

        let text_type = PostgresAdapter::map_postgresql_type("text", None, None, None).unwrap();
        assert!(matches!(text_type, UnifiedDataType::String { max_length: None }));

        // Test integer types
        let smallint_type = PostgresAdapter::map_postgresql_type("smallint", None, None, None).unwrap();
        assert!(matches!(smallint_type, UnifiedDataType::Integer { bits: 16, signed: true }));

        let int_type = PostgresAdapter::map_postgresql_type("integer", None, None, None).unwrap();
        assert!(matches!(int_type, UnifiedDataType::Integer { bits: 32, signed: true }));

        let bigint_type = PostgresAdapter::map_postgresql_type("bigint", None, None, None).unwrap();
        assert!(matches!(bigint_type, UnifiedDataType::Integer { bits: 64, signed: true }));

        // Test boolean type
        let bool_type = PostgresAdapter::map_postgresql_type("boolean", None, None, None).unwrap();
        assert!(matches!(bool_type, UnifiedDataType::Boolean));

        // Test datetime types
        let timestamp_type = PostgresAdapter::map_postgresql_type("timestamp without time zone", None, None, None).unwrap();
        assert!(matches!(timestamp_type, UnifiedDataType::DateTime { with_timezone: false }));

        let timestamptz_type = PostgresAdapter::map_postgresql_type("timestamp with time zone", None, None, None).unwrap();
        assert!(matches!(timestamptz_type, UnifiedDataType::DateTime { with_timezone: true }));

        let date_type = PostgresAdapter::map_postgresql_type("date", None, None, None).unwrap();
        assert!(matches!(date_type, UnifiedDataType::Date));

        // Test JSON types
        let json_type = PostgresAdapter::map_postgresql_type("json", None, None, None).unwrap();
        assert!(matches!(json_type, UnifiedDataType::Json));

        let jsonb_type = PostgresAdapter::map_postgresql_type("jsonb", None, None, None).unwrap();
        assert!(matches!(jsonb_type, UnifiedDataType::Json));

        // Test UUID type
        let uuid_type = PostgresAdapter::map_postgresql_type("uuid", None, None, None).unwrap();
        assert!(matches!(uuid_type, UnifiedDataType::Uuid));

        // Test binary type
        let bytea_type = PostgresAdapter::map_postgresql_type("bytea", None, None, None).unwrap();
        assert!(matches!(bytea_type, UnifiedDataType::Binary { max_length: None }));

        // Test array type
        let array_type = PostgresAdapter::map_postgresql_type("integer[]", None, None, None).unwrap();
        if let UnifiedDataType::Array { element_type } = array_type {
            assert!(matches!(*element_type, UnifiedDataType::Integer { bits: 32, signed: true }));
        } else {
            panic!("Expected array type");
        }

        // Test custom type
        let custom_type = PostgresAdapter::map_postgresql_type("custom_enum", None, None, None).unwrap();
        assert!(matches!(custom_type, UnifiedDataType::Custom { type_name } if type_name == "custom_enum"));
    }

    #[tokio::test]
    async fn test_referential_action_mapping() {
        use dbsurveyor_core::models::ReferentialAction;

        // Test all PostgreSQL referential action codes
        assert_eq!(PostgresAdapter::map_referential_action("c"), Some(ReferentialAction::Cascade));
        assert_eq!(PostgresAdapter::map_referential_action("n"), Some(ReferentialAction::SetNull));
        assert_eq!(PostgresAdapter::map_referential_action("d"), Some(ReferentialAction::SetDefault));
        assert_eq!(PostgresAdapter::map_referential_action("r"), Some(ReferentialAction::Restrict));
        assert_eq!(PostgresAdapter::map_referential_action("a"), Some(ReferentialAction::NoAction));

        // Test unknown action code
        assert_eq!(PostgresAdapter::map_referential_action("x"), None);
        assert_eq!(PostgresAdapter::map_referential_action(""), None);
    }

    #[tokio::test]
    async fn test_connection_config_parsing_comprehensive() {
        // Test full connection string with all parameters
        let connection_string = "postgres://testuser@localhost:5432/testdb?connect_timeout=60&statement_timeout=45000&pool_max_conns=20";
        let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
        assert_eq!(config.connect_timeout.as_secs(), 60);
        assert_eq!(config.query_timeout.as_millis(), 45000);
        assert_eq!(config.max_connections, 20);
        assert!(config.read_only); // Should default to read-only for security

        // Test minimal connection string
        let minimal_string = "postgres://localhost";
        let minimal_config = PostgresAdapter::parse_connection_config(minimal_string).unwrap();

        assert_eq!(minimal_config.host, "localhost");
        assert_eq!(minimal_config.port, Some(5432)); // Default PostgreSQL port
        assert_eq!(minimal_config.database, None);
        assert_eq!(minimal_config.username, None);
        assert!(minimal_config.read_only);

        // Test connection string with database but no username
        let db_only_string = "postgres://localhost/mydb";
        let db_only_config = PostgresAdapter::parse_connection_config(db_only_string).unwrap();

        assert_eq!(db_only_config.host, "localhost");
        assert_eq!(db_only_config.database, Some("mydb".to_string()));
        assert_eq!(db_only_config.username, None);
    }

    #[tokio::test]
    async fn test_connection_config_validation_limits() {
        use dbsurveyor_core::adapters::ConnectionConfig;
        use std::time::Duration;

        // Test valid configuration
        let valid_config = ConnectionConfig::new("localhost".to_string());
        assert!(valid_config.validate().is_ok());

        // Test invalid configurations
        let mut invalid_config = ConnectionConfig::default();

        // Empty host
        invalid_config.host = String::new();
        assert!(invalid_config.validate().is_err());

        // Reset host
        invalid_config.host = "localhost".to_string();

        // Invalid port
        invalid_config.port = Some(0);
        assert!(invalid_config.validate().is_err());

        // Reset port
        invalid_config.port = Some(5432);

        // Invalid max connections (too high)
        invalid_config.max_connections = 101;
        assert!(invalid_config.validate().is_err());

        // Invalid max connections (zero)
        invalid_config.max_connections = 0;
        assert!(invalid_config.validate().is_err());

        // Reset max connections
        invalid_config.max_connections = 10;

        // Invalid timeouts
        invalid_config.connect_timeout = Duration::from_secs(0);
        assert!(invalid_config.validate().is_err());

        invalid_config.connect_timeout = Duration::from_secs(30);
        invalid_config.query_timeout = Duration::from_secs(0);
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_schema_collection_error_handling() {
        // Test that schema collection fails gracefully with invalid connection
        let connection_string = "postgres://invalid_user:invalid_pass@nonexistent_host:9999/invalid_db";

        // Creating the adapter should succeed (lazy connection)
        let adapter = create_adapter(connection_string).await.unwrap();

        // But schema collection should fail gracefully
        let result = adapter.collect_schema().await;
        assert!(result.is_err());

        // The error should be properly formatted and not expose credentials
        let error_message = result.unwrap_err().to_string();
        assert!(!error_message.contains("invalid_pass"));
        assert!(!error_message.contains("invalid_user:invalid_pass"));
    }

    #[tokio::test]
    async fn test_connection_test_error_handling() {
        // Test that connection test fails gracefully with invalid connection
        let connection_string = "postgres://test_user:test_pass@localhost:9999/nonexistent";

        // Creating the adapter should succeed (lazy connection)
        let adapter = create_adapter(connection_string).await.unwrap();

        // But connection test should fail gracefully
        let result = adapter.test_connection().await;
        assert!(result.is_err());

        // The error should be properly formatted and not expose credentials
        let error_message = result.unwrap_err().to_string();
        assert!(!error_message.contains("test_pass"));
        assert!(!error_message.contains("test_user:test_pass"));
    }

    #[tokio::test]
    async fn test_adapter_feature_support() {
        use dbsurveyor_core::adapters::AdapterFeature;

        let connection_string = "postgres://user@localhost/db";
        let adapter = create_adapter(connection_string).await.unwrap();

        // PostgreSQL adapter should support all these features
        let supported_features = vec![
            AdapterFeature::SchemaCollection,
            AdapterFeature::DataSampling,
            AdapterFeature::MultiDatabase,
            AdapterFeature::ConnectionPooling,
            AdapterFeature::QueryTimeout,
            AdapterFeature::ReadOnlyMode,
        ];

        for feature in supported_features {
            assert!(
                adapter.supports_feature(feature),
                "PostgreSQL adapter should support feature: {:?}",
                feature
            );
        }
    }

    #[tokio::test]
    async fn test_connection_config_display_security() {
        use dbsurveyor_core::adapters::ConnectionConfig;

        let config = ConnectionConfig::new("example.com".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("testuser".to_string());

        let display_string = format!("{}", config);

        // Should contain connection info
        assert!(display_string.contains("example.com"));
        assert!(display_string.contains("5432"));
        assert!(display_string.contains("testdb"));

        // Should NOT contain username for security
        assert!(!display_string.contains("testuser"));

        // Should definitely not contain any password-like strings
        assert!(!display_string.contains("password"));
        assert!(!display_string.contains("secret"));
        assert!(!display_string.contains("pass"));
    }
}
