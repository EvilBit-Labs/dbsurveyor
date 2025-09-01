//! Unit tests for PostgreSQL adapter functionality.

#[cfg(feature = "postgresql")]
mod postgres_tests {
    use dbsurveyor_core::models::UnifiedDataType;

    // Note: Adapter feature tests require a real adapter instance,
    // which needs a database connection. These are tested in integration tests.

    #[test]
    fn test_postgresql_type_mapping() {
        use dbsurveyor_core::adapters::postgres::PostgresAdapter;

        // Test string types
        let varchar_type =
            PostgresAdapter::map_postgresql_type("character varying", Some(255), None, None)
                .unwrap();
        assert!(matches!(
            varchar_type,
            UnifiedDataType::String {
                max_length: Some(255)
            }
        ));

        let text_type = PostgresAdapter::map_postgresql_type("text", None, None, None).unwrap();
        assert!(matches!(
            text_type,
            UnifiedDataType::String { max_length: None }
        ));

        // Test integer types
        let int_type = PostgresAdapter::map_postgresql_type("integer", None, None, None).unwrap();
        assert!(matches!(
            int_type,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));

        let bigint_type = PostgresAdapter::map_postgresql_type("bigint", None, None, None).unwrap();
        assert!(matches!(
            bigint_type,
            UnifiedDataType::Integer {
                bits: 64,
                signed: true
            }
        ));

        let smallint_type =
            PostgresAdapter::map_postgresql_type("smallint", None, None, None).unwrap();
        assert!(matches!(
            smallint_type,
            UnifiedDataType::Integer {
                bits: 16,
                signed: true
            }
        ));

        // Test boolean type
        let bool_type = PostgresAdapter::map_postgresql_type("boolean", None, None, None).unwrap();
        assert!(matches!(bool_type, UnifiedDataType::Boolean));

        // Test datetime types
        let timestamp_type =
            PostgresAdapter::map_postgresql_type("timestamp without time zone", None, None, None)
                .unwrap();
        assert!(matches!(
            timestamp_type,
            UnifiedDataType::DateTime {
                with_timezone: false
            }
        ));

        let timestamptz_type =
            PostgresAdapter::map_postgresql_type("timestamp with time zone", None, None, None)
                .unwrap();
        assert!(matches!(
            timestamptz_type,
            UnifiedDataType::DateTime {
                with_timezone: true
            }
        ));

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
        assert!(matches!(
            bytea_type,
            UnifiedDataType::Binary { max_length: None }
        ));

        // Test array type
        let text_array_type =
            PostgresAdapter::map_postgresql_type("text[]", None, None, None).unwrap();
        assert!(matches!(text_array_type, UnifiedDataType::Array { .. }));

        // Test custom type
        let custom_type =
            PostgresAdapter::map_postgresql_type("custom_enum", None, None, None).unwrap();
        assert!(matches!(custom_type, UnifiedDataType::Custom { .. }));
    }

    #[test]
    fn test_referential_action_mapping() {
        use dbsurveyor_core::adapters::postgres::PostgresAdapter;
        use dbsurveyor_core::models::ReferentialAction;

        assert_eq!(
            PostgresAdapter::map_referential_action("c"),
            Some(ReferentialAction::Cascade)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("n"),
            Some(ReferentialAction::SetNull)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("d"),
            Some(ReferentialAction::SetDefault)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("r"),
            Some(ReferentialAction::Restrict)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("a"),
            Some(ReferentialAction::NoAction)
        );
        assert_eq!(PostgresAdapter::map_referential_action("x"), None);
    }

    #[test]
    fn test_connection_config_parsing() {
        use dbsurveyor_core::adapters::postgres::PostgresAdapter;

        // Test basic connection string parsing
        let config =
            PostgresAdapter::parse_connection_config("postgres://user@localhost:5432/testdb")
                .unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("user".to_string()));

        // Test connection string with default port
        let config =
            PostgresAdapter::parse_connection_config("postgres://user@localhost/testdb").unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432)); // Should default to PostgreSQL port
        assert_eq!(config.database, Some("testdb".to_string()));

        // Test connection string without database
        let config =
            PostgresAdapter::parse_connection_config("postgres://user@localhost:5432").unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, None);

        // Test invalid connection string
        let result = PostgresAdapter::parse_connection_config("invalid-url");
        assert!(result.is_err());
    }
}
