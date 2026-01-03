//! Integration tests for database adapters using testcontainers
//!
//! These tests verify adapter functionality against real database instances
//! to ensure comprehensive schema collection capabilities.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::uninlined_format_args)]

#[cfg(all(test, feature = "postgresql"))]
mod postgresql_integration {
    use dbsurveyor_collect::adapters::{
        ConnectionConfig, SchemaCollector, postgresql::PostgresAdapter,
    };

    #[tokio::test]
    async fn test_postgres_connection_and_metadata() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        // Start PostgreSQL container
        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        // Build connection string
        let connection_string = format!(
            "postgresql://postgres:postgres@localhost:{}/postgres",
            port
        );

        // Create adapter
        let config = ConnectionConfig::default();
        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        // Test connection
        assert_eq!(adapter.database_type(), "postgresql");
        adapter
            .test_connection()
            .await
            .expect("Connection test failed");

        // Collect metadata
        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "postgresql");
        assert!(metadata.version.is_some());
        
        // PostgreSQL should have at least one schema (public)
        assert!(!metadata.schemas.is_empty());
    }

    #[tokio::test]
    async fn test_postgres_schema_collection_with_data() {
        use sqlx::PgPool;
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        // Start PostgreSQL container
        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string = format!(
            "postgresql://postgres:postgres@localhost:{}/postgres",
            port
        );

        // Create a test table using sqlx
        let pool = PgPool::connect(&connection_string)
            .await
            .expect("Failed to connect to database");

        sqlx::query(
            "CREATE TABLE test_users (
                id SERIAL PRIMARY KEY,
                username VARCHAR(255) NOT NULL,
                email VARCHAR(255),
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test table");

        // Insert some test data
        sqlx::query("INSERT INTO test_users (username, email) VALUES ($1, $2)")
            .bind("testuser")
            .bind("test@example.com")
            .execute(&pool)
            .await
            .expect("Failed to insert test data");

        pool.close().await;

        // Now use our adapter to collect metadata
        let config = ConnectionConfig::default();
        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        // Find the public schema
        let public_schema = metadata
            .schemas
            .iter()
            .find(|s| s.name == "public")
            .expect("Public schema not found");

        // Find the test_users table
        let test_table = public_schema
            .tables
            .iter()
            .find(|t| t.name == "test_users")
            .expect("test_users table not found");

        // Verify table structure
        assert_eq!(test_table.columns.len(), 4);

        // Check for id column
        let id_column = test_table
            .columns
            .iter()
            .find(|c| c.name == "id")
            .expect("id column not found");
        assert_eq!(id_column.data_type, "integer");
        assert!(!id_column.is_nullable);

        // Check for username column
        let username_column = test_table
            .columns
            .iter()
            .find(|c| c.name == "username")
            .expect("username column not found");
        assert_eq!(username_column.data_type, "character varying");
        assert!(!username_column.is_nullable);
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod sqlite_integration {
    use dbsurveyor_collect::adapters::{ConnectionConfig, SchemaCollector, sqlite::SqliteAdapter};

    #[tokio::test]
    async fn test_sqlite_memory_database() {
        let config = ConnectionConfig::default();
        let adapter = SqliteAdapter::new("sqlite::memory:", config)
            .await
            .expect("Failed to create adapter");

        assert_eq!(adapter.database_type(), "sqlite");

        adapter
            .test_connection()
            .await
            .expect("Connection test failed");

        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "sqlite");
        assert!(metadata.version.is_some());
    }

    #[tokio::test]
    async fn test_sqlite_with_schema() {
        use sqlx::SqlitePool;
        use tempfile::NamedTempFile;

        // Create temporary database file
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let db_path = temp_file.path().to_str().unwrap();
        let connection_string = format!("sqlite://{db_path}");

        // Create a test table using sqlx
        let pool = SqlitePool::connect(&connection_string)
            .await
            .expect("Failed to connect to database");

        sqlx::query(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                price REAL,
                quantity INTEGER DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test table");

        sqlx::query("INSERT INTO products (name, price, quantity) VALUES (?, ?, ?)")
            .bind("Test Product")
            .bind(19.99)
            .bind(10)
            .execute(&pool)
            .await
            .expect("Failed to insert test data");

        pool.close().await;

        // Now use our adapter to collect metadata
        let config = ConnectionConfig::default();
        let adapter = SqliteAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "sqlite");

        // Find the main schema
        let main_schema = metadata
            .schemas
            .iter()
            .find(|s| s.name == "main")
            .expect("Main schema not found");

        // Find the products table
        let products_table = main_schema
            .tables
            .iter()
            .find(|t| t.name == "products")
            .expect("products table not found");

        assert_eq!(products_table.columns.len(), 4);

        // Verify columns
        let id_column = products_table
            .columns
            .iter()
            .find(|c| c.name == "id")
            .expect("id column not found");
        assert_eq!(id_column.data_type, "INTEGER");
    }
}

#[cfg(all(test, feature = "mongodb"))]
mod mongodb_integration {
    use dbsurveyor_collect::adapters::{ConnectionConfig, SchemaCollector, mongodb::MongoAdapter};

    #[tokio::test]
    #[ignore = "MongoDB requires running container, run with --ignored flag"]
    async fn test_mongodb_connection() {
        use testcontainers_modules::{mongo::Mongo, testcontainers::runners::AsyncRunner};

        // Start MongoDB container
        let container = Mongo::default()
            .start()
            .await
            .expect("Failed to start MongoDB container");

        let port = container
            .get_host_port_ipv4(27017)
            .await
            .expect("Failed to get port");

        let connection_string = format!("mongodb://localhost:{port}/testdb");

        let config = ConnectionConfig::default();
        let adapter = MongoAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        assert_eq!(adapter.database_type(), "mongodb");

        adapter
            .test_connection()
            .await
            .expect("Connection test failed");

        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "mongodb");
        assert!(metadata.version.is_some());
    }
}
