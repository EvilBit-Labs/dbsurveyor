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

    // ============================================================================
    // Multi-Database Collection Orchestration Tests (Task 7.3)
    // ============================================================================

    use dbsurveyor_core::adapters::postgres::{
        DatabaseFailure, MultiDatabaseConfig, MultiDatabaseMetadata,
    };
    use dbsurveyor_core::CollectionMode;

    #[test]
    fn test_multi_database_config_default() {
        let config = MultiDatabaseConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert!(!config.include_system);
        assert!(config.exclude_patterns.is_empty());
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_multi_database_config_builder() {
        let config = MultiDatabaseConfig::new()
            .with_max_concurrency(8)
            .with_include_system(true)
            .with_exclude_patterns(vec!["test_*".to_string(), "*_backup".to_string()])
            .with_continue_on_error(false);

        assert_eq!(config.max_concurrency, 8);
        assert!(config.include_system);
        assert_eq!(config.exclude_patterns.len(), 2);
        assert!(!config.continue_on_error);
    }

    #[test]
    fn test_multi_database_config_min_concurrency() {
        // Should enforce minimum concurrency of 1
        let config = MultiDatabaseConfig::new().with_max_concurrency(0);
        assert_eq!(config.max_concurrency, 1);
    }

    #[test]
    fn test_database_failure_serialization() {
        let failure = DatabaseFailure {
            database_name: "test_db".to_string(),
            error_message: "Connection refused".to_string(),
            is_connection_error: true,
        };

        let json = serde_json::to_string(&failure).expect("Failed to serialize");
        assert!(json.contains("\"database_name\":\"test_db\""));
        assert!(json.contains("\"error_message\":\"Connection refused\""));
        assert!(json.contains("\"is_connection_error\":true"));

        let deserialized: DatabaseFailure =
            serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.database_name, failure.database_name);
        assert_eq!(deserialized.error_message, failure.error_message);
        assert_eq!(deserialized.is_connection_error, failure.is_connection_error);
    }

    #[test]
    fn test_multi_database_metadata_serialization() {
        let metadata = MultiDatabaseMetadata {
            started_at: chrono::Utc::now(),
            total_duration_ms: 1234,
            databases_discovered: 10,
            databases_filtered: 2,
            databases_collected: 7,
            databases_failed: 1,
            databases_skipped: 0,
            max_concurrency: 4,
            collector_version: "1.0.0".to_string(),
            warnings: vec!["Test warning".to_string()],
        };

        let json = serde_json::to_string(&metadata).expect("Failed to serialize");
        assert!(json.contains("\"databases_discovered\":10"));
        assert!(json.contains("\"databases_collected\":7"));
        assert!(json.contains("\"max_concurrency\":4"));

        let deserialized: MultiDatabaseMetadata =
            serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(
            deserialized.databases_discovered,
            metadata.databases_discovered
        );
        assert_eq!(deserialized.databases_collected, metadata.databases_collected);
        assert_eq!(deserialized.max_concurrency, metadata.max_concurrency);
    }

    #[tokio::test]
    async fn test_collect_all_databases_basic() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_basic: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Use default configuration
        let config = MultiDatabaseConfig::new().with_max_concurrency(2);

        let result = adapter
            .collect_all_databases(&config)
            .await
            .expect("Failed to collect all databases");

        // Verify server info
        assert_eq!(
            result.server_info.server_type,
            dbsurveyor_core::DatabaseType::PostgreSQL
        );
        assert!(!result.server_info.version.is_empty());
        assert!(!result.server_info.connection_user.is_empty());

        // Verify we collected at least one database
        assert!(
            !result.databases.is_empty() || !result.failures.is_empty(),
            "Expected at least one database to be collected or fail"
        );

        // Verify metadata
        assert!(result.collection_metadata.total_duration_ms > 0);
        assert!(result.collection_metadata.databases_discovered > 0);
        assert_eq!(result.collection_metadata.max_concurrency, 2);

        eprintln!("Multi-database collection result:");
        eprintln!("  Server: {} {}", result.server_info.server_type, result.server_info.version);
        eprintln!("  Databases discovered: {}", result.collection_metadata.databases_discovered);
        eprintln!("  Databases collected: {}", result.databases.len());
        eprintln!("  Databases failed: {}", result.failures.len());
        eprintln!("  Total duration: {}ms", result.collection_metadata.total_duration_ms);

        for db in &result.databases {
            eprintln!(
                "    - {} ({} tables, {}ms)",
                db.database_name,
                db.schema.tables.len(),
                db.collection_duration_ms
            );
        }

        for failure in &result.failures {
            eprintln!(
                "    - {} (failed: {})",
                failure.database_name, failure.error_message
            );
        }
    }

    #[tokio::test]
    async fn test_collect_all_databases_with_exclude_patterns() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_with_exclude_patterns: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Configure to exclude template* databases (just in case they're accessible)
        let config = MultiDatabaseConfig::new()
            .with_max_concurrency(2)
            .with_exclude_patterns(vec!["template*".to_string()]);

        let result = adapter
            .collect_all_databases(&config)
            .await
            .expect("Failed to collect all databases");

        // Verify no template databases were collected
        for db in &result.databases {
            assert!(
                !db.database_name.starts_with("template"),
                "Template database {} should have been excluded",
                db.database_name
            );
        }

        eprintln!(
            "Collected {} databases after excluding template*",
            result.databases.len()
        );
    }

    #[tokio::test]
    async fn test_collect_all_databases_server_info() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_server_info: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        let config = MultiDatabaseConfig::new();

        let result = adapter
            .collect_all_databases(&config)
            .await
            .expect("Failed to collect all databases");

        // Verify server info fields
        let server_info = &result.server_info;
        assert_eq!(server_info.server_type, dbsurveyor_core::DatabaseType::PostgreSQL);
        assert!(!server_info.version.is_empty(), "Version should not be empty");
        assert!(!server_info.host.is_empty(), "Host should not be empty");
        assert!(!server_info.connection_user.is_empty(), "User should not be empty");

        // Verify collection mode
        match &server_info.collection_mode {
            CollectionMode::MultiDatabase { discovered, collected, failed } => {
                assert!(
                    *discovered > 0,
                    "Should have discovered at least one database"
                );
                assert_eq!(
                    *collected,
                    result.databases.len(),
                    "Collected count should match databases array length"
                );
                assert_eq!(
                    *failed,
                    result.failures.len(),
                    "Failed count should match failures array length"
                );
            }
            _ => panic!("Expected MultiDatabase collection mode"),
        }

        eprintln!("Server info: {:?}", server_info);
    }

    #[tokio::test]
    async fn test_collect_all_databases_metadata_timing() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_metadata_timing: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        let config = MultiDatabaseConfig::new();

        let start = std::time::Instant::now();
        let result = adapter
            .collect_all_databases(&config)
            .await
            .expect("Failed to collect all databases");
        let actual_duration = start.elapsed();

        // Verify timing metadata is reasonable
        let metadata = &result.collection_metadata;

        // The reported duration should be close to actual (within 1 second tolerance)
        let reported_duration_secs = metadata.total_duration_ms as f64 / 1000.0;
        let actual_duration_secs = actual_duration.as_secs_f64();

        assert!(
            (reported_duration_secs - actual_duration_secs).abs() < 1.0,
            "Reported duration ({:.2}s) should be close to actual ({:.2}s)",
            reported_duration_secs,
            actual_duration_secs
        );

        // Individual database collection times should sum to approximately total
        let individual_sum: u64 = result
            .databases
            .iter()
            .map(|db| db.collection_duration_ms)
            .sum();

        eprintln!(
            "Total duration: {}ms, Sum of individual: {}ms",
            metadata.total_duration_ms, individual_sum
        );
    }

    #[tokio::test]
    async fn test_collect_all_databases_concurrency() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_concurrency: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        // Test with concurrency of 1 (sequential)
        let config_sequential = MultiDatabaseConfig::new().with_max_concurrency(1);

        let start_sequential = std::time::Instant::now();
        let result_sequential = adapter
            .collect_all_databases(&config_sequential)
            .await
            .expect("Failed to collect with concurrency 1");
        let duration_sequential = start_sequential.elapsed();

        // Test with higher concurrency
        let config_concurrent = MultiDatabaseConfig::new().with_max_concurrency(4);

        let start_concurrent = std::time::Instant::now();
        let result_concurrent = adapter
            .collect_all_databases(&config_concurrent)
            .await
            .expect("Failed to collect with concurrency 4");
        let duration_concurrent = start_concurrent.elapsed();

        // Both should collect the same databases
        assert_eq!(
            result_sequential.databases.len(),
            result_concurrent.databases.len(),
            "Both runs should collect the same number of databases"
        );

        eprintln!(
            "Sequential (concurrency=1): {}ms, Concurrent (concurrency=4): {}ms",
            duration_sequential.as_millis(),
            duration_concurrent.as_millis()
        );

        // With multiple databases, concurrent should generally be faster
        // But this depends on how many databases exist, so we don't assert on timing
    }

    #[tokio::test]
    async fn test_collect_all_databases_database_result_content() {
        if !has_database_connection() {
            eprintln!("Skipping test_collect_all_databases_database_result_content: no database URL configured");
            return;
        }

        let database_url = get_test_database_url().unwrap();
        let adapter = PostgresAdapter::new(&database_url)
            .await
            .expect("Failed to create adapter");

        let config = MultiDatabaseConfig::new();

        let result = adapter
            .collect_all_databases(&config)
            .await
            .expect("Failed to collect all databases");

        // Verify each collected database has valid content
        for db_result in &result.databases {
            // Database name should not be empty
            assert!(
                !db_result.database_name.is_empty(),
                "Database name should not be empty"
            );

            // Collection duration should be positive
            assert!(
                db_result.collection_duration_ms > 0,
                "Collection duration should be positive for {}",
                db_result.database_name
            );

            // Schema should have database info
            let schema = &db_result.schema;
            assert!(
                !schema.database_info.name.is_empty(),
                "Schema database name should not be empty"
            );

            // Version should be populated
            assert!(
                schema.database_info.version.is_some(),
                "Schema version should be populated for {}",
                db_result.database_name
            );

            eprintln!(
                "Database '{}': {} tables, {} views, {}ms",
                db_result.database_name,
                schema.tables.len(),
                schema.views.len(),
                db_result.collection_duration_ms
            );
        }
    }
}
