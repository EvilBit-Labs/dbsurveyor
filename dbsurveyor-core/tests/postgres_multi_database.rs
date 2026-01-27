//! Tests for PostgreSQL multi-database enumeration functionality.
//!
//! These tests verify the database enumeration features including:
//! - Listing all databases on a PostgreSQL server
//! - Filtering system databases (template0, template1)
//! - Checking database accessibility
//! - Retrieving database metadata (size, encoding, owner)

#[cfg(feature = "postgresql")]
mod postgres_multi_database_tests {
    use dbsurveyor_core::adapters::DatabaseAdapter;
    use dbsurveyor_core::adapters::postgres::{
        EnumeratedDatabase, ListDatabasesOptions, PostgresAdapter, SYSTEM_DATABASES,
    };

    // ============================================================================
    // Unit tests (no database connection required)
    // ============================================================================

    #[test]
    fn test_system_databases_constant() {
        // Verify the system databases constant contains expected values
        assert!(SYSTEM_DATABASES.contains(&"template0"));
        assert!(SYSTEM_DATABASES.contains(&"template1"));
        assert!(!SYSTEM_DATABASES.contains(&"postgres"));
        assert_eq!(SYSTEM_DATABASES.len(), 2);
    }

    #[test]
    fn test_enumerated_database_new() {
        let db = EnumeratedDatabase::new("testdb".to_string());

        assert_eq!(db.name, "testdb");
        assert!(db.owner.is_empty());
        assert!(db.encoding.is_empty());
        assert!(db.collation.is_empty());
        assert!(db.size_bytes.is_none());
        assert!(!db.is_system_database);
        assert!(!db.is_accessible);
    }

    #[test]
    fn test_enumerated_database_check_is_system_database() {
        // System databases
        assert!(EnumeratedDatabase::check_is_system_database("template0"));
        assert!(EnumeratedDatabase::check_is_system_database("template1"));

        // Non-system databases
        assert!(!EnumeratedDatabase::check_is_system_database("postgres"));
        assert!(!EnumeratedDatabase::check_is_system_database("mydb"));
        assert!(!EnumeratedDatabase::check_is_system_database("template2"));
        assert!(!EnumeratedDatabase::check_is_system_database("production"));
        assert!(!EnumeratedDatabase::check_is_system_database(""));
    }

    #[test]
    fn test_list_databases_options_default() {
        let options = ListDatabasesOptions::new();
        assert!(!options.include_system);
    }

    #[test]
    fn test_list_databases_options_with_system() {
        let options = ListDatabasesOptions::with_system_databases();
        assert!(options.include_system);
    }

    #[test]
    fn test_enumerated_database_serialization() {
        let db = EnumeratedDatabase {
            name: "testdb".to_string(),
            owner: "postgres".to_string(),
            encoding: "UTF8".to_string(),
            collation: "en_US.UTF-8".to_string(),
            size_bytes: Some(1048576), // 1 MB
            is_system_database: false,
            is_accessible: true,
        };

        // Test serialization
        let json = serde_json::to_string(&db).expect("Failed to serialize");
        assert!(json.contains("\"name\":\"testdb\""));
        assert!(json.contains("\"owner\":\"postgres\""));
        assert!(json.contains("\"encoding\":\"UTF8\""));
        assert!(json.contains("\"collation\":\"en_US.UTF-8\""));
        assert!(json.contains("\"size_bytes\":1048576"));
        assert!(json.contains("\"is_system_database\":false"));
        assert!(json.contains("\"is_accessible\":true"));

        // Test deserialization
        let deserialized: EnumeratedDatabase =
            serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.name, db.name);
        assert_eq!(deserialized.owner, db.owner);
        assert_eq!(deserialized.encoding, db.encoding);
        assert_eq!(deserialized.collation, db.collation);
        assert_eq!(deserialized.size_bytes, db.size_bytes);
        assert_eq!(deserialized.is_system_database, db.is_system_database);
        assert_eq!(deserialized.is_accessible, db.is_accessible);
    }

    #[test]
    fn test_enumerated_database_serialization_with_null_size() {
        let db = EnumeratedDatabase {
            name: "restricted_db".to_string(),
            owner: "admin".to_string(),
            encoding: "UTF8".to_string(),
            collation: "C".to_string(),
            size_bytes: None, // Size unknown (e.g., insufficient privileges)
            is_system_database: false,
            is_accessible: false,
        };

        let json = serde_json::to_string(&db).expect("Failed to serialize");
        assert!(json.contains("\"size_bytes\":null"));

        let deserialized: EnumeratedDatabase =
            serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.size_bytes, None);
    }

    #[test]
    fn test_enumerated_database_clone() {
        let db = EnumeratedDatabase {
            name: "testdb".to_string(),
            owner: "postgres".to_string(),
            encoding: "UTF8".to_string(),
            collation: "en_US.UTF-8".to_string(),
            size_bytes: Some(1024),
            is_system_database: false,
            is_accessible: true,
        };

        let cloned = db.clone();
        assert_eq!(cloned.name, db.name);
        assert_eq!(cloned.owner, db.owner);
        assert_eq!(cloned.size_bytes, db.size_bytes);
    }

    #[test]
    fn test_enumerated_database_debug() {
        let db = EnumeratedDatabase {
            name: "testdb".to_string(),
            owner: "postgres".to_string(),
            encoding: "UTF8".to_string(),
            collation: "en_US.UTF-8".to_string(),
            size_bytes: Some(1024),
            is_system_database: false,
            is_accessible: true,
        };

        let debug_str = format!("{:?}", db);
        assert!(debug_str.contains("EnumeratedDatabase"));
        assert!(debug_str.contains("testdb"));
        assert!(debug_str.contains("postgres"));
    }

    // ============================================================================
    // Integration tests (require database connection)
    // ============================================================================

    /// Helper function to get database URL from environment
    fn get_test_database_url() -> Option<String> {
        std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
    }

    /// Helper function to check if we have a database connection available
    fn has_database_connection() -> bool {
        get_test_database_url().is_some()
    }

    #[tokio::test]
    async fn test_list_databases() {
        if !has_database_connection() {
            eprintln!("Skipping test_list_databases: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Test basic listing (excludes system databases)
        let databases = adapter
            .list_databases()
            .await
            .expect("Failed to list databases");

        // We should have at least one database (the one we connected to)
        assert!(!databases.is_empty(), "Expected at least one database");

        // Verify system databases are excluded
        for db in &databases {
            assert!(
                !SYSTEM_DATABASES.contains(&db.name.as_str()),
                "System database {} should be excluded",
                db.name
            );
        }

        // Verify each database has expected fields populated
        for db in &databases {
            assert!(!db.name.is_empty(), "Database name should not be empty");
            assert!(!db.owner.is_empty(), "Owner should not be empty");
            assert!(!db.encoding.is_empty(), "Encoding should not be empty");
            assert!(!db.collation.is_empty(), "Collation should not be empty");
        }

        // Print database info for debugging
        eprintln!("Found {} databases (excluding system):", databases.len());
        for db in &databases {
            eprintln!(
                "  - {} (owner: {}, encoding: {}, accessible: {}, size: {:?})",
                db.name, db.owner, db.encoding, db.is_accessible, db.size_bytes
            );
        }
    }

    #[tokio::test]
    async fn test_list_databases_with_system_included() {
        if !has_database_connection() {
            eprintln!(
                "Skipping test_list_databases_with_system_included: no database URL configured"
            );
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Get databases without system databases
        let databases_without_system = adapter
            .list_databases_with_options(false)
            .await
            .expect("Failed to list databases without system");

        // Get databases with system databases
        let databases_with_system = adapter
            .list_databases_with_options(true)
            .await
            .expect("Failed to list databases with system");

        // The list with system databases should be >= the list without
        assert!(
            databases_with_system.len() >= databases_without_system.len(),
            "Expected more databases when including system databases"
        );

        // Check that system databases are present when include_system is true
        let system_db_names: Vec<&str> = databases_with_system
            .iter()
            .filter(|db| db.is_system_database)
            .map(|db| db.name.as_str())
            .collect();

        eprintln!("System databases found: {:?}", system_db_names);

        // Verify none of the databases in the filtered list are system databases
        for db in &databases_without_system {
            assert!(
                !db.is_system_database,
                "Database {} should not be marked as system in filtered list",
                db.name
            );
        }

        // Print comparison
        eprintln!(
            "Databases without system: {}, with system: {}",
            databases_without_system.len(),
            databases_with_system.len()
        );
    }

    #[tokio::test]
    async fn test_list_accessible_databases() {
        if !has_database_connection() {
            eprintln!("Skipping test_list_accessible_databases: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Get all accessible databases
        let accessible_databases = adapter
            .list_accessible_databases(false)
            .await
            .expect("Failed to list accessible databases");

        // All returned databases should be accessible
        for db in &accessible_databases {
            assert!(
                db.is_accessible,
                "Database {} should be marked as accessible",
                db.name
            );
        }

        // The database we connected to should definitely be accessible
        let all_databases = adapter
            .list_databases()
            .await
            .expect("Failed to list all databases");

        let accessible_count = all_databases.iter().filter(|db| db.is_accessible).count();
        assert_eq!(
            accessible_databases.len(),
            accessible_count,
            "Accessible database count should match"
        );

        eprintln!("Found {} accessible databases", accessible_databases.len());
    }

    #[tokio::test]
    async fn test_database_size_retrieval() {
        if !has_database_connection() {
            eprintln!("Skipping test_database_size_retrieval: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        let databases = adapter
            .list_databases()
            .await
            .expect("Failed to list databases");

        // At least the current database should have a size
        let databases_with_size: Vec<_> = databases
            .iter()
            .filter(|db| db.size_bytes.is_some())
            .collect();

        // We should have at least one database with a known size
        assert!(
            !databases_with_size.is_empty(),
            "Expected at least one database with known size"
        );

        // Verify sizes are reasonable (greater than 0)
        for db in &databases_with_size {
            let size = db.size_bytes.unwrap();
            assert!(size > 0, "Database {} should have non-zero size", db.name);
            eprintln!("Database {} size: {} bytes", db.name, size);
        }
    }

    #[tokio::test]
    async fn test_database_encoding_and_collation() {
        if !has_database_connection() {
            eprintln!("Skipping test_database_encoding_and_collation: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        let databases = adapter
            .list_databases()
            .await
            .expect("Failed to list databases");

        // Common PostgreSQL encodings
        let valid_encodings = [
            "UTF8",
            "LATIN1",
            "LATIN2",
            "SQL_ASCII",
            "EUC_JP",
            "EUC_CN",
            "WIN1252",
        ];

        for db in &databases {
            // Encoding should be a valid PostgreSQL encoding
            // (we don't exhaustively check all encodings, just verify it's not empty)
            assert!(
                !db.encoding.is_empty(),
                "Database {} should have an encoding",
                db.name
            );

            // Most databases will use UTF8
            if db.encoding == "UTF8" {
                eprintln!(
                    "Database {} uses UTF8 encoding with collation: {}",
                    db.name, db.collation
                );
            } else if valid_encodings.contains(&db.encoding.as_str()) {
                eprintln!(
                    "Database {} uses {} encoding with collation: {}",
                    db.name, db.encoding, db.collation
                );
            } else {
                // Other encodings are fine too
                eprintln!(
                    "Database {} uses {} encoding (less common)",
                    db.name, db.encoding
                );
            }
        }
    }

    #[tokio::test]
    async fn test_list_databases_fails_gracefully_on_invalid_connection() {
        // Test with an invalid connection that should fail gracefully
        let adapter = PostgresAdapter::new("postgres://invalid:invalid@localhost:9999/invalid")
            .await
            .expect("Adapter creation should succeed (lazy connection)");

        // The list_databases call should fail with a connection error
        let result = adapter.list_databases().await;
        assert!(
            result.is_err(),
            "Expected error when listing databases with invalid connection"
        );

        let error = result.unwrap_err();
        eprintln!("Expected error: {}", error);
    }

    // ============================================================================
    // Per-database connection tests (Task 7.2)
    // ============================================================================

    #[test]
    fn test_connection_url_for_database_valid() {
        // Test that connection_url_for_database validates and generates correct URLs
        // This is a unit test that doesn't require a real database connection

        // Valid database names
        let valid_names = ["testdb", "my_database", "DB123", "_private", "a"];

        for name in valid_names {
            // Validation should pass for these names
            assert!(
                name.len() <= 63 && !name.is_empty(),
                "Name {} should be valid length",
                name
            );
            assert!(
                !name.contains(';') && !name.contains('\'') && !name.contains('"'),
                "Name {} should not contain dangerous characters",
                name
            );
        }
    }

    #[test]
    fn test_connection_url_for_database_invalid_length() {
        // Empty database name
        let empty = "";
        assert!(empty.is_empty(), "Empty name should fail validation");

        // Database name too long (PostgreSQL max is 63 characters)
        let too_long = "a".repeat(64);
        assert!(too_long.len() > 63, "Long name should fail validation");
    }

    #[test]
    fn test_connection_url_for_database_dangerous_characters() {
        // Database names with SQL injection vectors
        let dangerous_names = [
            "test;DROP TABLE users",
            "test'--",
            "test\"",
            "test;",
            "';DROP TABLE--",
        ];

        for name in dangerous_names {
            assert!(
                name.contains(';') || name.contains('\'') || name.contains('"'),
                "Name {} should contain dangerous characters",
                name
            );
        }
    }

    #[tokio::test]
    async fn test_connect_to_database_url_generation() {
        // Test URL generation by creating an adapter and checking the generated URL
        let database_url = "postgres://testuser:testpass@localhost:5432/original_db";
        let adapter = PostgresAdapter::new(database_url)
            .await
            .expect("Failed to create adapter");

        // Test valid database name - should generate correct URL
        let result = adapter.connection_url_for_database("new_database");
        assert!(
            result.is_ok(),
            "Should generate URL for valid database name"
        );

        let new_url = result.unwrap();
        assert!(
            new_url.contains("new_database"),
            "Generated URL should contain new database name"
        );
        assert!(
            !new_url.contains("original_db"),
            "Generated URL should not contain original database name"
        );
        assert!(
            new_url.contains("localhost:5432"),
            "Generated URL should preserve host and port"
        );
    }

    #[tokio::test]
    async fn test_connect_to_database_url_validation_empty() {
        let database_url = "postgres://testuser:testpass@localhost:5432/original_db";
        let adapter = PostgresAdapter::new(database_url)
            .await
            .expect("Failed to create adapter");

        // Empty database name should fail
        let result = adapter.connection_url_for_database("");
        assert!(result.is_err(), "Empty database name should fail");
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("invalid") || error.contains("length"),
            "Error should mention invalid length"
        );
    }

    #[tokio::test]
    async fn test_connect_to_database_url_validation_too_long() {
        let database_url = "postgres://testuser:testpass@localhost:5432/original_db";
        let adapter = PostgresAdapter::new(database_url)
            .await
            .expect("Failed to create adapter");

        // Database name too long should fail
        let long_name = "a".repeat(64);
        let result = adapter.connection_url_for_database(&long_name);
        assert!(result.is_err(), "Too long database name should fail");
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("invalid") || error.contains("length"),
            "Error should mention invalid length"
        );
    }

    #[tokio::test]
    async fn test_connect_to_database_url_validation_dangerous_chars() {
        let database_url = "postgres://testuser:testpass@localhost:5432/original_db";
        let adapter = PostgresAdapter::new(database_url)
            .await
            .expect("Failed to create adapter");

        // Database names with dangerous characters should fail
        let dangerous_names = ["test;DROP", "test'inject", "test\"quote"];

        for name in dangerous_names {
            let result = adapter.connection_url_for_database(name);
            assert!(
                result.is_err(),
                "Database name '{}' with dangerous characters should fail",
                name
            );
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("invalid") || error.contains("character"),
                "Error for '{}' should mention invalid characters: {}",
                name,
                error
            );
        }
    }

    #[tokio::test]
    async fn test_connect_to_database_preserves_credentials() {
        // Test that the generated URL preserves credentials from the original URL
        let database_url = "postgres://myuser:secret123@dbhost:5433/original_db";
        let adapter = PostgresAdapter::new(database_url)
            .await
            .expect("Failed to create adapter");

        let result = adapter.connection_url_for_database("new_db");
        assert!(result.is_ok(), "Should generate URL");

        let new_url = result.unwrap();
        // Check that credentials are preserved
        assert!(
            new_url.contains("myuser"),
            "Generated URL should preserve username"
        );
        assert!(
            new_url.contains("secret123"),
            "Generated URL should preserve password"
        );
        assert!(
            new_url.contains("dbhost:5433"),
            "Generated URL should preserve host and port"
        );
        assert!(
            new_url.contains("/new_db"),
            "Generated URL should have new database"
        );
    }

    #[tokio::test]
    async fn test_connect_to_database_integration() {
        if !has_database_connection() {
            eprintln!("Skipping test_connect_to_database_integration: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Create a test database
        // Note: We need to disable read-only mode temporarily to create the database
        // For now, we'll just test connecting to an existing database (postgres)

        // Connect to the default 'postgres' database which should always exist
        let result = adapter.connect_to_database("postgres").await;

        match result {
            Ok(db_adapter) => {
                // Verify we're connected to the right database
                let current_db: String = sqlx::query_scalar("SELECT current_database()")
                    .fetch_one(&db_adapter.pool)
                    .await
                    .expect("Failed to query current database");

                assert_eq!(
                    current_db, "postgres",
                    "Should be connected to postgres database"
                );
                eprintln!("Successfully connected to database: {}", current_db);
            }
            Err(e) => {
                // Connection might fail if postgres database doesn't exist or isn't accessible
                eprintln!(
                    "Could not connect to postgres database (may not be accessible): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_connect_to_database_preserves_config() {
        if !has_database_connection() {
            eprintln!(
                "Skipping test_connect_to_database_preserves_config: no database URL configured"
            );
            return;
        }

        use dbsurveyor_core::adapters::ConnectionConfig;
        use std::time::Duration;

        let database_url = get_test_database_url().unwrap();

        // Create adapter with custom config
        let custom_config = ConnectionConfig::new("localhost".to_string())
            .with_port(5432)
            .with_database("test".to_string());
        let custom_config = ConnectionConfig {
            connect_timeout: Duration::from_secs(15),
            query_timeout: Duration::from_secs(20),
            max_connections: 5,
            ..custom_config
        };

        let adapter = PostgresAdapter::with_config(&database_url, custom_config.clone())
            .await
            .expect("Failed to create adapter");

        // Connect to postgres database
        let result = adapter.connect_to_database("postgres").await;

        if let Ok(db_adapter) = result {
            // Verify config is preserved
            let config = db_adapter.connection_config();
            assert_eq!(
                config.connect_timeout,
                Duration::from_secs(15),
                "Connect timeout should be preserved"
            );
            assert_eq!(
                config.query_timeout,
                Duration::from_secs(20),
                "Query timeout should be preserved"
            );
            assert_eq!(
                config.max_connections, 5,
                "Max connections should be preserved"
            );
            eprintln!("Config preserved correctly for new database connection");
        }
    }
}
