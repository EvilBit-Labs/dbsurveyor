//! Basic MongoDB adapter tests without requiring a real database.

#![cfg(feature = "mongodb")]

mod mongodb_basic_tests {
    use dbsurveyor_core::adapters::create_adapter;
    use dbsurveyor_core::models::DatabaseType;

    #[tokio::test]
    async fn test_mongodb_adapter_creation_from_connection_string() {
        // Test that we can create a MongoDB adapter from a connection string
        let connection_string = "mongodb://user:pass@localhost:27017/testdb";

        let result = create_adapter(connection_string).await;

        // This should succeed in creating the adapter (even if connection fails later)
        assert!(result.is_ok());

        let adapter = result.unwrap();
        assert_eq!(adapter.database_type(), DatabaseType::MongoDB);
    }

    #[tokio::test]
    async fn test_mongodb_adapter_features() {
        use dbsurveyor_core::adapters::AdapterFeature;

        let connection_string = "mongodb://user:pass@localhost:27017/testdb";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Test that MongoDB adapter supports expected features
        assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
        assert!(adapter.supports_feature(AdapterFeature::DataSampling));
        assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));

        // MongoDB doesn't support these features
        assert!(!adapter.supports_feature(AdapterFeature::MultiDatabase));
        assert!(!adapter.supports_feature(AdapterFeature::ConnectionPooling));
        assert!(!adapter.supports_feature(AdapterFeature::ReadOnlyMode));
    }

    #[tokio::test]
    async fn test_mongodb_connection_config() {
        let connection_string = "mongodb://testuser@localhost:27017/testdb";
        let adapter = create_adapter(connection_string).await.unwrap();

        let config = adapter.connection_config();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(27017));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
        assert!(config.read_only);
    }

    #[tokio::test]
    async fn test_mongodb_connection_test_fails_gracefully() {
        // Test with an invalid connection that should fail
        let connection_string = "mongodb://invalid:invalid@localhost:59999/invalid";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Connection test should fail gracefully
        let result = adapter.test_connection().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mongodb_schema_collection_fails_gracefully() {
        // Test with an invalid connection that should fail
        let connection_string = "mongodb://invalid:invalid@localhost:59999/invalid";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Schema collection should fail gracefully
        let result = adapter.collect_schema().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mongodb_srv_connection_string_parsing() {
        // Test that SRV connection string parsing works at the adapter level.
        // Note: The mongodb crate validates SRV records during client creation,
        // so we test the connection string validation separately.
        use dbsurveyor_core::adapters::mongodb::MongoAdapter;

        // The connection string parsing should work
        let connection_string = "mongodb+srv://user@cluster.example.com/testdb";
        let config = MongoAdapter::parse_connection_config(connection_string);

        assert!(config.is_ok(), "Should parse SRV connection string");
        let config = config.unwrap();
        assert_eq!(config.host, "cluster.example.com");
        assert_eq!(config.database, Some("testdb".to_string()));

        // Actually creating a client with SRV will fail due to DNS resolution
        // but that's expected behavior - the parsing worked correctly
    }
}
