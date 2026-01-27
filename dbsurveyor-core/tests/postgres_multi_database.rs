//! Tests for PostgreSQL multi-database enumeration functionality.
//!
//! These tests verify the database enumeration features including:
//! - Listing all databases on a PostgreSQL server
//! - Filtering system databases (template0, template1)
//! - Checking database accessibility
//! - Retrieving database metadata (size, encoding, owner)

#[cfg(feature = "postgresql")]
mod postgres_multi_database_tests {
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
}
