//! Security tests for credential protection and data sanitization
//!
//! These tests verify that database credentials are never exposed in outputs,
//! logs, or error messages.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::uninlined_format_args)]

#[cfg(test)]
mod credential_security {
    use dbsurveyor_collect::adapters::{ConnectionConfig, SchemaCollector};

    const SENSITIVE_PASSWORD: &str = "super_secret_password_123";
    const SENSITIVE_USERNAME: &str = "admin_user";

    #[cfg(feature = "postgresql")]
    #[tokio::test]
    async fn test_postgres_no_credentials_in_error() {
        use dbsurveyor_collect::adapters::postgresql::PostgresAdapter;

        let connection_string = format!(
            "postgresql://{}:{}@invalid-host.example.com:5432/testdb",
            SENSITIVE_USERNAME, SENSITIVE_PASSWORD
        );

        let config = ConnectionConfig::default();
        let result = PostgresAdapter::new(&connection_string, config).await;

        // Connection should fail
        assert!(result.is_err());

        // Error message should NOT contain credentials
        if let Err(error) = result {
            let error_msg = format!("{:?}", error);
            assert!(
                !error_msg.contains(SENSITIVE_PASSWORD),
                "Password leaked in error message: {}",
                error_msg
            );
            assert!(
                !error_msg.contains(SENSITIVE_USERNAME),
                "Username leaked in error message: {}",
                error_msg
            );
            assert!(
                !error_msg.contains("invalid-host.example.com"),
                "Hostname leaked in error message: {}",
                error_msg
            );
        }
    }

    #[cfg(feature = "postgresql")]
    #[tokio::test]
    async fn test_postgres_safe_description_no_credentials() {
        use dbsurveyor_collect::adapters::postgresql::PostgresAdapter;
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        // Use the correct default credentials for the container
        let connection_string = format!(
            "postgresql://postgres:postgres@localhost:{}/postgres",
            port
        );

        let config = ConnectionConfig::default();
        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        let description = adapter.safe_description();

        // Description should NOT contain credentials or connection details
        assert!(
            !description.contains("postgres:postgres"),
            "Credentials leaked in description: {}",
            description
        );
        assert!(
            !description.contains(&port.to_string()),
            "Port leaked in description: {}",
            description
        );
        // Should NOT contain connection details
        assert!(
            !description.contains("localhost"),
            "Hostname leaked in description: {}",
            description
        );
        // Should contain generic information only
        assert!(description.contains("PostgreSQL"));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_sqlite_safe_description() {
        use dbsurveyor_collect::adapters::sqlite::SqliteAdapter;

        let config = ConnectionConfig::default();
        let adapter = SqliteAdapter::new("sqlite::memory:", config)
            .await
            .expect("Failed to create adapter");

        let description = adapter.safe_description();

        // Description should be generic and not leak paths
        assert!(description.contains("SQLite"));
        assert!(!description.contains("memory"));
    }

    #[cfg(feature = "mongodb")]
    #[tokio::test]
    async fn test_mongodb_no_credentials_in_error() {
        use dbsurveyor_collect::adapters::mongodb::MongoAdapter;

        // MongoDB client accepts the URL but we should test connection failure
        let connection_string = format!(
            "mongodb://{}:{}@invalid-host-that-does-not-exist-12345.example.com:27017/testdb",
            SENSITIVE_USERNAME, SENSITIVE_PASSWORD
        );

        let config = ConnectionConfig::default();
        // MongoDB client creation doesn't fail, but connection test should
        if let Ok(adapter) = MongoAdapter::new(&connection_string, config).await {
            let result = adapter.test_connection().await;
            
            // Connection should fail
            assert!(result.is_err());
            
            // Error message should NOT contain credentials
            if let Err(error) = result {
                let error_msg = format!("{:?}", error);
                assert!(
                    !error_msg.contains(SENSITIVE_PASSWORD),
                    "Password leaked in error message: {}",
                    error_msg
                );
                assert!(
                    !error_msg.contains(SENSITIVE_USERNAME),
                    "Username leaked in error message: {}",
                    error_msg
                );
            }
        }
    }

    #[cfg(feature = "mongodb")]
    #[tokio::test]
    async fn test_mongodb_safe_description_no_credentials() {
        use dbsurveyor_collect::adapters::mongodb::MongoAdapter;

        let connection_string = format!(
            "mongodb://{}:{}@localhost:27017/testdb",
            SENSITIVE_USERNAME, SENSITIVE_PASSWORD
        );

        let config = ConnectionConfig::default();
        let adapter = MongoAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        let description = adapter.safe_description();

        // Description should NOT contain credentials
        assert!(
            !description.contains(SENSITIVE_PASSWORD),
            "Password leaked in description: {}",
            description
        );
        assert!(
            !description.contains(SENSITIVE_USERNAME),
            "Username leaked in description: {}",
            description
        );
        // Should contain database name but not host
        assert!(description.contains("testdb"));
        assert!(!description.contains("localhost"));
    }

    #[test]
    fn test_adapter_error_messages_sanitized() {
        use dbsurveyor_collect::adapters::AdapterError;
        use std::time::Duration;

        // Test all error variants for credential leakage
        let errors = vec![
            AdapterError::ConnectionFailed,
            AdapterError::ConnectionTimeout(Duration::from_secs(30)),
            AdapterError::QueryFailed,
            AdapterError::InvalidParameters,
            AdapterError::UnsupportedFeature("test".to_string()),
            AdapterError::DatabaseError,
            AdapterError::PoolExhausted,
            AdapterError::Generic("test".to_string()),
        ];

        for error in errors {
            let error_msg = format!("{}", error);
            let error_debug = format!("{:?}", error);

            // None of the error messages should contain common credential patterns
            assert!(!error_msg.contains("password"));
            assert!(!error_msg.contains("secret"));
            assert!(!error_debug.contains("password"));
            assert!(!error_debug.contains("secret"));
        }
    }
}
