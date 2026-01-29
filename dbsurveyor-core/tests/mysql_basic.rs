//! Basic MySQL adapter tests without requiring a real database.

#![cfg(feature = "mysql")]

mod mysql_basic_tests {
    use dbsurveyor_core::{adapters::create_adapter, models::DatabaseType};

    #[tokio::test]
    async fn test_mysql_adapter_creation_from_connection_string() {
        // Test that we can create a MySQL adapter from a connection string
        let connection_string = "mysql://user:pass@localhost:3306/testdb";

        let result = create_adapter(connection_string).await;

        // This should succeed in creating the adapter (even if connection fails later)
        assert!(result.is_ok());

        let adapter = result.unwrap();
        assert_eq!(adapter.database_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    async fn test_mysql_adapter_features() {
        use dbsurveyor_core::adapters::AdapterFeature;

        let connection_string = "mysql://user:pass@localhost:3306/testdb";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Test that MySQL adapter supports expected features
        assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
        assert!(adapter.supports_feature(AdapterFeature::DataSampling));
        assert!(adapter.supports_feature(AdapterFeature::MultiDatabase));
        assert!(adapter.supports_feature(AdapterFeature::ConnectionPooling));
        assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));
        assert!(adapter.supports_feature(AdapterFeature::ReadOnlyMode));
    }

    #[tokio::test]
    async fn test_mysql_connection_config() {
        let connection_string = "mysql://testuser@localhost:3306/testdb";
        let adapter = create_adapter(connection_string).await.unwrap();

        let config = adapter.connection_config();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(3306));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
        assert!(config.read_only);
    }

    #[tokio::test]
    async fn test_mysql_connection_test_fails_gracefully() {
        // Test with an invalid connection that should fail
        let connection_string = "mysql://invalid:invalid@localhost:9999/invalid";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Connection test should fail gracefully
        let result = adapter.test_connection().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mysql_schema_collection_fails_gracefully() {
        // Test with an invalid connection that should fail
        let connection_string = "mysql://invalid:invalid@localhost:9999/invalid";
        let adapter = create_adapter(connection_string).await.unwrap();

        // Schema collection should fail gracefully
        let result = adapter.collect_schema().await;
        assert!(result.is_err());
    }
}
