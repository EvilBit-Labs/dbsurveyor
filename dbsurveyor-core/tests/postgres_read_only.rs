//! PostgreSQL read-only enforcement tests.
//!
//! This test suite verifies that the PostgreSQL adapter enforces read-only access
//! at the connection level via `SET default_transaction_read_only = on`.
//! This is a core security guarantee: all database operations must be strictly
//! read-only.
//!
//! # What is tested
//! - The adapter's `after_connect` hook sets `default_transaction_read_only = on`
//! - INSERT, UPDATE, DELETE, DROP, and CREATE statements all fail
//! - SELECT operations still succeed
//!
//! # Requirements
//! These tests require Docker (testcontainers) to run a real PostgreSQL instance.

#![cfg(feature = "postgresql")]

use dbsurveyor_core::{
    Result,
    adapters::{ConnectionConfig, DatabaseAdapter, postgres::PostgresAdapter},
};
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

mod common;

// =============================================================================
// Helper
// =============================================================================

/// Start a PostgreSQL container and return (container, base_url).
///
/// The returned container must be kept alive for the duration of the test.
async fn start_postgres() -> (
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    String,
) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);
    common::wait_for_postgres_ready(&url, 30)
        .await
        .expect("PostgreSQL failed to become ready");
    (container, url)
}

// =============================================================================
// Read-Only Enforcement Tests
// =============================================================================

/// Verify that the adapter's connection config reports read-only mode.
#[tokio::test]
async fn test_postgres_adapter_config_is_read_only() -> Result<()> {
    let (_container, url) = start_postgres().await;
    let adapter = PostgresAdapter::new(&url).await?;

    assert!(
        adapter.connection_config().read_only,
        "PostgresAdapter::new() should produce a read-only config"
    );
    Ok(())
}

/// Verify that SELECT succeeds on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_allows_select() -> Result<()> {
    let (_container, url) = start_postgres().await;
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("SELECT 1 AS value")
        .fetch_one(&adapter.pool)
        .await;

    assert!(
        result.is_ok(),
        "SELECT should succeed on read-only connection, got: {:?}",
        result.err()
    );
    Ok(())
}

/// Verify that CREATE TABLE is rejected on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_rejects_create_table() -> Result<()> {
    let (_container, url) = start_postgres().await;
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("CREATE TABLE should_fail (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "CREATE TABLE should be rejected on read-only connection"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("read-only")
            || err_msg.contains("cannot execute")
            || err_msg.contains("read_only"),
        "Error should indicate read-only restriction, got: {}",
        err_msg
    );
    Ok(())
}

/// Verify that INSERT is rejected on a read-only connection.
///
/// We use a CTE to avoid needing a pre-existing table: the statement
/// attempts to write, which `default_transaction_read_only` blocks.
#[tokio::test]
async fn test_postgres_read_only_rejects_insert() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // First, create a table using a separate writable connection (no adapter).
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE ro_test_insert (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&writable_pool)
        .await
        .unwrap();
    writable_pool.close().await;

    // Now open via the adapter (read-only enforced)
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("INSERT INTO ro_test_insert (value) VALUES ('should_fail')")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "INSERT should be rejected on read-only connection"
    );
    Ok(())
}

/// Verify that UPDATE is rejected on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_rejects_update() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // Create and populate table via writable connection
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE ro_test_update (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&writable_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ro_test_update (value) VALUES ('original')")
        .execute(&writable_pool)
        .await
        .unwrap();
    writable_pool.close().await;

    // Open via adapter (read-only)
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("UPDATE ro_test_update SET value = 'modified' WHERE id = 1")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "UPDATE should be rejected on read-only connection"
    );
    Ok(())
}

/// Verify that DELETE is rejected on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_rejects_delete() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // Create and populate table via writable connection
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE ro_test_delete (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&writable_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ro_test_delete (value) VALUES ('to_delete')")
        .execute(&writable_pool)
        .await
        .unwrap();
    writable_pool.close().await;

    // Open via adapter (read-only)
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("DELETE FROM ro_test_delete WHERE id = 1")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "DELETE should be rejected on read-only connection"
    );
    Ok(())
}

/// Verify that DROP TABLE is rejected on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_rejects_drop_table() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // Create table via writable connection
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE ro_test_drop (id SERIAL PRIMARY KEY)")
        .execute(&writable_pool)
        .await
        .unwrap();
    writable_pool.close().await;

    // Open via adapter (read-only)
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("DROP TABLE ro_test_drop")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "DROP TABLE should be rejected on read-only connection"
    );
    Ok(())
}

/// Verify that TRUNCATE is rejected on a read-only connection.
#[tokio::test]
async fn test_postgres_read_only_rejects_truncate() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // Create table via writable connection
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("CREATE TABLE ro_test_truncate (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&writable_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ro_test_truncate (value) VALUES ('data')")
        .execute(&writable_pool)
        .await
        .unwrap();
    writable_pool.close().await;

    // Open via adapter (read-only)
    let adapter = PostgresAdapter::new(&url).await?;

    let result = sqlx::query("TRUNCATE ro_test_truncate")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "TRUNCATE should be rejected on read-only connection"
    );
    Ok(())
}

/// Verify that schema metadata reads succeed on a read-only connection.
///
/// This confirms that the adapter can still perform its core job (schema
/// collection) while write operations are blocked.
#[tokio::test]
async fn test_postgres_read_only_allows_schema_collection() -> Result<()> {
    let (_container, url) = start_postgres().await;

    // Create some schema objects via writable connection
    let writable_pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ro_test_schema (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&writable_pool)
    .await
    .unwrap();
    writable_pool.close().await;

    // Open via adapter (read-only) and collect schema
    let adapter = PostgresAdapter::new(&url).await?;
    let schema = adapter.collect_schema().await?;

    // Should successfully collect schema despite read-only mode
    let table = schema.tables.iter().find(|t| t.name == "ro_test_schema");
    assert!(
        table.is_some(),
        "Should find ro_test_schema in collected schema"
    );

    let table = table.unwrap();
    assert_eq!(
        table.columns.len(),
        3,
        "ro_test_schema should have 3 columns"
    );
    Ok(())
}

/// Verify read-only enforcement with a custom ConnectionConfig.
#[tokio::test]
async fn test_postgres_read_only_with_custom_config() -> Result<()> {
    let (_container, url) = start_postgres().await;

    let config = ConnectionConfig {
        host: "localhost".to_string(),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&url, config).await?;
    adapter.test_connection().await?;

    let result = sqlx::query("CREATE TABLE custom_config_fail (id SERIAL PRIMARY KEY)")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "CREATE TABLE should be rejected with custom read-only config"
    );
    Ok(())
}
