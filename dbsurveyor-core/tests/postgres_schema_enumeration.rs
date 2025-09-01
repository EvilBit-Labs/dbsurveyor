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

    let custom_table = schema
        .tables
        .iter()
        .find(|t| t.name == "custom_table" && t.schema.as_deref() == Some("test_schema"))
        .expect("custom_table not found in test_schema");

    assert_eq!(custom_table.name, "custom_table");
    assert_eq!(custom_table.schema.as_deref(), Some("test_schema"));

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
