//! Integration tests for PostgreSQL schema enumeration functionality.
//!
//! These tests verify that the PostgreSQL adapter correctly enumerates schemas
//! and tables with proper error handling and security guarantees.

use dbsurveyor_core::adapters::{DatabaseAdapter, postgres::PostgresAdapter};
use dbsurveyor_core::models::DatabaseType;
use sqlx::PgPool;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

/// Test basic schema enumeration with a real PostgreSQL database
#[tokio::test]
async fn test_postgres_schema_enumeration() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    // Create test schema and tables
    let pool = PgPool::connect(&database_url).await?;

    // Create test schema (ignore error if it already exists)
    let _ = sqlx::query("CREATE SCHEMA test_schema")
        .execute(&pool)
        .await; // Ignore error if schema already exists

    // Create test table in public schema (ignore error if exists)
    let _ = sqlx::query(
        "CREATE TABLE public.test_table (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await; // Ignore error if table already exists

    // Create test table in custom schema (ignore error if exists)
    let _ = sqlx::query(
        "CREATE TABLE test_schema.custom_table (
            id SERIAL PRIMARY KEY,
            data TEXT,
            active BOOLEAN DEFAULT TRUE
        )",
    )
    .execute(&pool)
    .await; // Ignore error if table already exists

    // Create a view (ignore error if exists)
    let _ = sqlx::query(
        "CREATE VIEW public.test_view AS
         SELECT id, name FROM public.test_table WHERE id > 0",
    )
    .execute(&pool)
    .await; // Ignore error if view already exists

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;

    // Test connection
    adapter.test_connection().await?;

    // Verify adapter properties
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);

    // Collect schema
    let schema = adapter.collect_schema().await?;

    // Verify basic schema properties
    assert_eq!(schema.database_info.name, "postgres");
    assert!(!schema.tables.is_empty());

    // Verify we found our test tables
    let public_table = schema
        .tables
        .iter()
        .find(|t| t.name == "test_table" && t.schema.as_deref() == Some("public"))
        .expect("test_table not found in public schema");

    assert_eq!(public_table.name, "test_table");
    assert_eq!(public_table.schema.as_deref(), Some("public"));

    // Verify column introspection is working
    assert!(
        !public_table.columns.is_empty(),
        "test_table should have columns"
    );

    // Check for expected columns
    let id_column = public_table
        .columns
        .iter()
        .find(|c| c.name == "id")
        .expect("id column not found");
    assert_eq!(id_column.ordinal_position, 1);
    assert!(id_column.is_primary_key);
    assert!(id_column.is_auto_increment);
    assert!(!id_column.is_nullable);

    let name_column = public_table
        .columns
        .iter()
        .find(|c| c.name == "name")
        .expect("name column not found");
    assert_eq!(name_column.ordinal_position, 2);
    assert!(!name_column.is_primary_key);
    assert!(!name_column.is_auto_increment);
    assert!(!name_column.is_nullable);

    let created_at_column = public_table
        .columns
        .iter()
        .find(|c| c.name == "created_at")
        .expect("created_at column not found");
    assert_eq!(created_at_column.ordinal_position, 3);
    assert!(!created_at_column.is_primary_key);
    assert!(!created_at_column.is_auto_increment);
    assert!(created_at_column.is_nullable); // Default allows NULL

    let custom_table = schema
        .tables
        .iter()
        .find(|t| t.name == "custom_table" && t.schema.as_deref() == Some("test_schema"))
        .expect("custom_table not found in test_schema");

    assert_eq!(custom_table.name, "custom_table");
    assert_eq!(custom_table.schema.as_deref(), Some("test_schema"));

    // Verify column introspection for custom table
    assert!(
        !custom_table.columns.is_empty(),
        "custom_table should have columns"
    );

    let id_column = custom_table
        .columns
        .iter()
        .find(|c| c.name == "id")
        .expect("id column not found in custom_table");
    assert!(id_column.is_primary_key);
    assert!(id_column.is_auto_increment);

    let data_column = custom_table
        .columns
        .iter()
        .find(|c| c.name == "data")
        .expect("data column not found in custom_table");
    assert!(!data_column.is_primary_key);
    assert!(data_column.is_nullable);

    let active_column = custom_table
        .columns
        .iter()
        .find(|c| c.name == "active")
        .expect("active column not found in custom_table");
    assert!(!active_column.is_primary_key);
    assert!(active_column.is_nullable); // Has default but still nullable

    // Verify we found the view
    let test_view = schema
        .tables
        .iter()
        .find(|t| t.name == "test_view" && t.schema.as_deref() == Some("public"))
        .expect("test_view not found in public schema");

    assert_eq!(test_view.name, "test_view");
    assert_eq!(test_view.schema.as_deref(), Some("public"));

    // Verify no credentials in schema output
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("postgres:postgres"));
    assert!(!schema_json.contains("password"));
    assert!(!schema_json.contains("secret"));

    Ok(())
}

/// Test schema collection with insufficient privileges
#[tokio::test]
async fn test_postgres_insufficient_privileges() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let admin_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&admin_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    // Create limited user
    let pool = PgPool::connect(&admin_url).await?;

    sqlx::query("CREATE USER limited_user WITH PASSWORD 'limited_pass'")
        .execute(&pool)
        .await?;

    // Don't grant any privileges - user should have very limited access

    pool.close().await;

    // Test with limited user
    let limited_url = format!(
        "postgres://limited_user:limited_pass@localhost:{}/postgres",
        port
    );

    let adapter = PostgresAdapter::new(&limited_url).await?;

    // Connection should work
    adapter.test_connection().await?;

    // Schema collection should work but return limited results
    let schema = adapter.collect_schema().await?;

    // Should have very few or no tables due to limited privileges
    // This tests that the adapter handles privilege restrictions gracefully
    assert!(schema.tables.len() <= 1); // May have access to some system views

    // Verify no credentials in error messages or output
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("limited_pass"));
    assert!(!schema_json.contains("limited_user:limited_pass"));

    Ok(())
}

/// Test connection string redaction in error messages
#[tokio::test]
async fn test_postgres_credential_redaction() {
    use dbsurveyor_core::adapters::redact_database_url;

    // Test URL redaction
    let url = "postgres://user:secret123@localhost:5432/testdb";
    let redacted = redact_database_url(url);

    assert!(!redacted.contains("secret123"));
    assert!(redacted.contains("user:****"));
    assert!(redacted.contains("localhost:5432"));
    assert!(redacted.contains("/testdb"));

    // Test with invalid connection string format (should fail during parsing)
    let invalid_url = "invalid://user:secret@nonexistent:5432/db";
    let result = PostgresAdapter::new(invalid_url).await;

    // Should fail but not expose credentials
    assert!(result.is_err());
    let error_msg = format!("{}", result.err().unwrap());
    assert!(!error_msg.contains("secret"));
    assert!(!error_msg.contains("user:secret"));

    // Test edge cases for URL redaction
    assert_eq!(
        redact_database_url("postgres://user@localhost/db"),
        "postgres://user@localhost/db"
    );
    assert_eq!(redact_database_url("invalid-url"), "invalid-url");
    assert_eq!(redact_database_url(""), "");
}

/// Test connection configuration and basic functionality
#[tokio::test]
async fn test_postgres_connection_config() -> Result<(), Box<dyn std::error::Error>> {
    // Test that we can create a PostgresAdapter with a valid connection string
    // Note: connect_lazy() doesn't actually test the connection, so this will succeed
    let connection_string = "postgres://testuser@localhost:5432/testdb";
    let result = PostgresAdapter::new(connection_string).await;
    assert!(result.is_ok()); // Should succeed with lazy connection

    // Test that invalid connection strings are rejected during parsing
    let invalid_result = PostgresAdapter::new("invalid://url").await;
    assert!(invalid_result.is_err());

    // Test that malformed URLs are rejected
    let malformed_result = PostgresAdapter::new("not-a-url").await;
    assert!(malformed_result.is_err());

    Ok(())
}

/// Test pool statistics and connection management
#[tokio::test]
async fn test_postgres_pool_management() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Test connection first to establish pool connections
    adapter.test_connection().await?;

    // Now test pool statistics after connections are established
    let pool_size = adapter.pool.size() as usize;
    let idle_connections = adapter.pool.num_idle() as usize;
    assert!(pool_size >= 1); // Should have at least one connection after use
    assert!(idle_connections <= pool_size);

    // Test another connection to verify pool is functional
    adapter.test_connection().await?;

    // Pool should still be functional
    let pool_size_after = adapter.pool.size() as usize;
    assert!(pool_size_after >= 1); // Should still have connections

    // Test graceful shutdown
    adapter.pool.close().await;

    Ok(())
}

/// Test handling of NULL values in database metadata
#[tokio::test]
async fn test_postgres_null_handling() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    let pool = PgPool::connect(&database_url).await?;

    // Create table with potential NULL values (ignore error if exists)
    let _ = sqlx::query(
        "CREATE TABLE test_nulls (
            id SERIAL PRIMARY KEY,
            nullable_text TEXT,
            nullable_int INTEGER,
            nullable_timestamp TIMESTAMP
        )",
    )
    .execute(&pool)
    .await; // Ignore error if table already exists

    // Insert row with NULL values
    sqlx::query("INSERT INTO test_nulls (nullable_text, nullable_int, nullable_timestamp) VALUES (NULL, NULL, NULL)")
        .execute(&pool)
        .await?;

    pool.close().await;

    // Test schema collection handles NULLs gracefully
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Should successfully collect schema even with NULL metadata
    assert!(!schema.tables.is_empty());

    // Find our test table
    let test_table = schema
        .tables
        .iter()
        .find(|t| t.name == "test_nulls")
        .expect("test_nulls table not found");

    assert_eq!(test_table.name, "test_nulls");

    // Verify no credentials in output even with NULL handling
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("postgres:postgres"));

    Ok(())
}

/// Test PostgreSQL data type mapping to unified types
#[tokio::test]
async fn test_postgres_data_type_mapping() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    // Create test table with various data types
    let pool = PgPool::connect(&database_url).await?;

    let _ = sqlx::query("DROP TABLE IF EXISTS data_types_test")
        .execute(&pool)
        .await;

    sqlx::query(
        "CREATE TABLE data_types_test (
            id SERIAL PRIMARY KEY,
            text_col TEXT,
            varchar_col VARCHAR(100),
            char_col CHAR(10),
            int_col INTEGER,
            bigint_col BIGINT,
            smallint_col SMALLINT,
            numeric_col NUMERIC(10,2),
            decimal_col DECIMAL(8,3),
            real_col REAL,
            double_col DOUBLE PRECISION,
            bool_col BOOLEAN,
            date_col DATE,
            timestamp_col TIMESTAMP,
            timestamptz_col TIMESTAMP WITH TIME ZONE,
            time_col TIME,
            timetz_col TIME WITH TIME ZONE,
            uuid_col UUID,
            json_col JSON,
            jsonb_col JSONB,
            bytea_col BYTEA,
            array_col INTEGER[],
            text_array_col TEXT[]
        )",
    )
    .execute(&pool)
    .await?;

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Find our test table
    let test_table = schema
        .tables
        .iter()
        .find(|t| t.name == "data_types_test")
        .expect("data_types_test table not found");

    // Verify we have all expected columns
    assert_eq!(test_table.columns.len(), 23); // All columns including id

    // Test specific data type mappings
    use dbsurveyor_core::models::UnifiedDataType;

    // String types
    let text_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "text_col")
        .unwrap();
    assert!(matches!(
        text_col.data_type,
        UnifiedDataType::String { max_length: None }
    ));

    let varchar_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "varchar_col")
        .unwrap();
    assert!(matches!(
        varchar_col.data_type,
        UnifiedDataType::String {
            max_length: Some(100)
        }
    ));

    let char_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "char_col")
        .unwrap();
    assert!(matches!(
        char_col.data_type,
        UnifiedDataType::String {
            max_length: Some(10)
        }
    ));

    // Integer types
    let int_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "int_col")
        .unwrap();
    assert!(matches!(
        int_col.data_type,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));

    let bigint_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "bigint_col")
        .unwrap();
    assert!(matches!(
        bigint_col.data_type,
        UnifiedDataType::Integer {
            bits: 64,
            signed: true
        }
    ));

    let smallint_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "smallint_col")
        .unwrap();
    assert!(matches!(
        smallint_col.data_type,
        UnifiedDataType::Integer {
            bits: 16,
            signed: true
        }
    ));

    // Numeric types
    let numeric_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "numeric_col")
        .unwrap();
    assert!(matches!(
        numeric_col.data_type,
        UnifiedDataType::Float { .. }
    ));

    let real_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "real_col")
        .unwrap();
    assert!(matches!(
        real_col.data_type,
        UnifiedDataType::Float {
            precision: Some(24)
        }
    ));

    let double_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "double_col")
        .unwrap();
    assert!(matches!(
        double_col.data_type,
        UnifiedDataType::Float {
            precision: Some(53)
        }
    ));

    // Boolean type
    let bool_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "bool_col")
        .unwrap();
    assert!(matches!(bool_col.data_type, UnifiedDataType::Boolean));

    // Date/time types
    let date_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "date_col")
        .unwrap();
    assert!(matches!(date_col.data_type, UnifiedDataType::Date));

    let timestamp_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "timestamp_col")
        .unwrap();
    assert!(matches!(
        timestamp_col.data_type,
        UnifiedDataType::DateTime {
            with_timezone: false
        }
    ));

    let timestamptz_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "timestamptz_col")
        .unwrap();
    assert!(matches!(
        timestamptz_col.data_type,
        UnifiedDataType::DateTime {
            with_timezone: true
        }
    ));

    let time_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "time_col")
        .unwrap();
    assert!(matches!(
        time_col.data_type,
        UnifiedDataType::Time {
            with_timezone: false
        }
    ));

    let timetz_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "timetz_col")
        .unwrap();
    assert!(matches!(
        timetz_col.data_type,
        UnifiedDataType::Time {
            with_timezone: true
        }
    ));

    // UUID type
    let uuid_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "uuid_col")
        .unwrap();
    assert!(matches!(uuid_col.data_type, UnifiedDataType::Uuid));

    // JSON types
    let json_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "json_col")
        .unwrap();
    assert!(matches!(json_col.data_type, UnifiedDataType::Json));

    let jsonb_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "jsonb_col")
        .unwrap();
    assert!(matches!(jsonb_col.data_type, UnifiedDataType::Json));

    // Binary type
    let bytea_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "bytea_col")
        .unwrap();
    assert!(matches!(
        bytea_col.data_type,
        UnifiedDataType::Binary { max_length: None }
    ));

    // Array types
    let array_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "array_col")
        .unwrap();
    assert!(matches!(array_col.data_type, UnifiedDataType::Array { .. }));

    let text_array_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "text_array_col")
        .unwrap();
    assert!(matches!(
        text_array_col.data_type,
        UnifiedDataType::Array { .. }
    ));

    // Verify ordinal positions are correct
    for (i, column) in test_table.columns.iter().enumerate() {
        assert_eq!(column.ordinal_position as usize, i + 1);
    }

    // Verify no credentials in output
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("postgres:postgres"));

    Ok(())
}

/// Test PostgreSQL-specific types and edge cases
#[tokio::test]
async fn test_postgres_specific_types() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default().start().await?;
    let port = postgres.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    let max_attempts = 30;
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(&database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                break;
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if attempts >= max_attempts {
        panic!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        );
    }

    // Create test table with PostgreSQL-specific types
    let pool = PgPool::connect(&database_url).await?;

    let _ = sqlx::query("DROP TABLE IF EXISTS postgres_specific_types")
        .execute(&pool)
        .await;

    // Create enum type first
    let _ = sqlx::query("DROP TYPE IF EXISTS test_enum")
        .execute(&pool)
        .await;
    sqlx::query("CREATE TYPE test_enum AS ENUM ('active', 'inactive', 'pending')")
        .execute(&pool)
        .await?;

    sqlx::query(
        "CREATE TABLE postgres_specific_types (
            id SERIAL PRIMARY KEY,
            inet_col INET,
            cidr_col CIDR,
            macaddr_col MACADDR,
            point_col POINT,
            xml_col XML,
            enum_col test_enum,
            int_array INTEGER[],
            text_array TEXT[],
            multidim_array INTEGER[][],
            nullable_with_default INTEGER DEFAULT 42,
            not_null_no_default TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Find our test table
    let test_table = schema
        .tables
        .iter()
        .find(|t| t.name == "postgres_specific_types")
        .expect("postgres_specific_types table not found");

    // Verify we have all expected columns
    assert_eq!(test_table.columns.len(), 12);

    use dbsurveyor_core::models::UnifiedDataType;

    // Test PostgreSQL-specific types map to Custom
    let inet_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "inet_col")
        .unwrap();
    assert!(matches!(inet_col.data_type, UnifiedDataType::Custom { .. }));

    let cidr_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "cidr_col")
        .unwrap();
    assert!(matches!(cidr_col.data_type, UnifiedDataType::Custom { .. }));

    let macaddr_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "macaddr_col")
        .unwrap();
    assert!(matches!(
        macaddr_col.data_type,
        UnifiedDataType::Custom { .. }
    ));

    let point_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "point_col")
        .unwrap();
    assert!(matches!(
        point_col.data_type,
        UnifiedDataType::Custom { .. }
    ));

    let xml_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "xml_col")
        .unwrap();
    assert!(matches!(xml_col.data_type, UnifiedDataType::Custom { .. }));

    // Test enum type maps to Custom
    let enum_col = test_table
        .columns
        .iter()
        .find(|c| c.name == "enum_col")
        .unwrap();
    assert!(matches!(enum_col.data_type, UnifiedDataType::Custom { .. }));

    // Test array types
    let int_array = test_table
        .columns
        .iter()
        .find(|c| c.name == "int_array")
        .unwrap();
    assert!(matches!(int_array.data_type, UnifiedDataType::Array { .. }));

    let text_array = test_table
        .columns
        .iter()
        .find(|c| c.name == "text_array")
        .unwrap();
    assert!(matches!(
        text_array.data_type,
        UnifiedDataType::Array { .. }
    ));

    // Test multidimensional arrays (should still be Array type)
    let multidim_array = test_table
        .columns
        .iter()
        .find(|c| c.name == "multidim_array")
        .unwrap();
    assert!(matches!(
        multidim_array.data_type,
        UnifiedDataType::Array { .. }
    ));

    // Test nullable and default value handling
    let nullable_with_default = test_table
        .columns
        .iter()
        .find(|c| c.name == "nullable_with_default")
        .unwrap();
    assert!(nullable_with_default.is_nullable);
    assert!(nullable_with_default.default_value.is_some());

    let not_null_no_default = test_table
        .columns
        .iter()
        .find(|c| c.name == "not_null_no_default")
        .unwrap();
    assert!(!not_null_no_default.is_nullable);
    assert!(not_null_no_default.default_value.is_none());

    // Test primary key detection
    let id_col = test_table.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.is_primary_key);
    assert!(id_col.is_auto_increment);

    // Verify ordinal positions are sequential
    let mut positions: Vec<u32> = test_table
        .columns
        .iter()
        .map(|c| c.ordinal_position)
        .collect();
    positions.sort();
    for (i, pos) in positions.iter().enumerate() {
        assert_eq!(*pos, (i + 1) as u32);
    }

    // Verify no credentials in output
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("postgres:postgres"));

    Ok(())
}
