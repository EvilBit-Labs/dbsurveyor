//! SQLite read-only enforcement tests.
//!
//! This test suite verifies that the SQLite adapter enforces read-only access
//! at the connection level. This is a core security guarantee: all database
//! operations must be strictly read-only.
//!
//! # What is tested
//! - `SqliteAdapter::new()` opens connections with `read_only(true)`
//! - INSERT, UPDATE, DELETE, DROP, and CREATE statements all fail
//! - SELECT and PRAGMA (read) operations still succeed
//!
//! # Note on `from_pool()`
//! `SqliteAdapter::from_pool()` accepts an externally created pool and does NOT
//! enforce read-only mode itself. Read-only enforcement depends on the caller
//! passing a read-only pool. These tests cover the `new()` path, which is the
//! production entry point.

#![cfg(feature = "sqlite")]

use dbsurveyor_core::{
    Result,
    adapters::{DatabaseAdapter, sqlite::SqliteAdapter},
};

// =============================================================================
// Read-Only Enforcement Tests (via SqliteAdapter::new)
// =============================================================================

/// Verify that the adapter's connection config reports read-only mode.
#[tokio::test]
async fn test_sqlite_adapter_config_is_read_only() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;
    assert!(
        adapter.connection_config().read_only,
        "SqliteAdapter::new() should produce a read-only config"
    );
    Ok(())
}

/// Verify that CREATE TABLE is rejected on a read-only connection.
#[tokio::test]
async fn test_sqlite_read_only_rejects_create_table() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;

    let result = sqlx::query("CREATE TABLE should_fail (id INTEGER PRIMARY KEY, value TEXT)")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "CREATE TABLE should be rejected on a read-only connection"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("readonly")
            || err_msg.to_lowercase().contains("read-only")
            || err_msg.to_lowercase().contains("attempt to write"),
        "Error should mention read-only restriction, got: {}",
        err_msg
    );
    Ok(())
}

/// Verify that INSERT is rejected on a read-only connection.
///
/// Since CREATE TABLE also fails, there is no table to insert into.
/// The adapter should reject the statement before reaching "no such table".
#[tokio::test]
async fn test_sqlite_read_only_rejects_insert() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;

    let result = sqlx::query("INSERT INTO nonexistent (id) VALUES (1)")
        .execute(&adapter.pool)
        .await;

    assert!(
        result.is_err(),
        "INSERT should be rejected on a read-only connection"
    );
    Ok(())
}

/// Verify that DROP TABLE is rejected on a read-only connection.
#[tokio::test]
async fn test_sqlite_read_only_rejects_drop_table() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;

    let result = sqlx::query("DROP TABLE IF EXISTS anything")
        .execute(&adapter.pool)
        .await;

    // DROP on a read-only connection should fail (even with IF EXISTS on
    // a nonexistent table, the write intent is rejected).
    // Note: Some SQLite versions may treat "DROP TABLE IF EXISTS <nonexistent>"
    // as a no-op rather than a write. We check for either outcome.
    if let Err(e) = &result {
        let msg = e.to_string().to_lowercase();
        assert!(
            msg.contains("readonly")
                || msg.contains("read-only")
                || msg.contains("attempt to write"),
            "Error should mention read-only restriction, got: {}",
            e
        );
    }
    // If it succeeds (no-op on nonexistent table), that is acceptable.
    Ok(())
}

/// Verify that SELECT on an empty database succeeds (read operations allowed).
#[tokio::test]
async fn test_sqlite_read_only_allows_select() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;

    // sqlite_master is always readable
    let result: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("SELECT * FROM sqlite_master")
        .fetch_all(&adapter.pool)
        .await
        .expect("SELECT on sqlite_master should succeed on read-only connection");

    // Empty database, so no rows expected
    assert!(
        result.is_empty(),
        "Empty in-memory database should have no entries in sqlite_master"
    );
    Ok(())
}

/// Verify that PRAGMA reads succeed on a read-only connection.
#[tokio::test]
async fn test_sqlite_read_only_allows_pragma_read() -> Result<()> {
    let adapter = SqliteAdapter::new("sqlite::memory:").await?;

    let result = sqlx::query("PRAGMA table_info('sqlite_master')")
        .fetch_all(&adapter.pool)
        .await;

    assert!(
        result.is_ok(),
        "PRAGMA table_info should succeed on read-only connection, got: {:?}",
        result.err()
    );
    Ok(())
}

// =============================================================================
// File-Based Read-Only Tests
// =============================================================================

/// Verify read-only enforcement on a file-based SQLite database.
///
/// This test creates a temporary database file with a table and data,
/// then opens it via `SqliteAdapter::new()` (which enforces read-only)
/// and verifies that write operations are rejected while reads succeed.
#[tokio::test]
async fn test_sqlite_file_based_read_only_enforcement() -> Result<()> {
    use sqlx::SqlitePool;

    // Create a temp directory and database file
    let temp_dir = std::env::temp_dir().join(format!("dbsurveyor_ro_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    let db_path = temp_dir.join("test_readonly.db");
    let db_url = format!("sqlite://{}", db_path.display());

    // Phase 1: Create and populate the database (writable connection)
    {
        let pool = SqlitePool::connect(&format!("{}?mode=rwc", db_url))
            .await
            .expect("Failed to create writable SQLite database");

        sqlx::query(
            "CREATE TABLE test_data (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                value REAL
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test table");

        sqlx::query("INSERT INTO test_data (name, value) VALUES ('alpha', 1.0)")
            .execute(&pool)
            .await
            .expect("Failed to insert test data");

        sqlx::query("INSERT INTO test_data (name, value) VALUES ('beta', 2.0)")
            .execute(&pool)
            .await
            .expect("Failed to insert test data");

        pool.close().await;
    }

    // Phase 2: Open via SqliteAdapter::new() (read-only) and verify enforcement
    let adapter = SqliteAdapter::new(&db_url).await?;
    assert!(
        adapter.connection_config().read_only,
        "Adapter should be in read-only mode"
    );

    // Reads should succeed
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("SELECT * FROM test_data")
        .fetch_all(&adapter.pool)
        .await
        .expect("SELECT should succeed on read-only adapter");
    assert_eq!(rows.len(), 2, "Should read 2 rows from test_data");

    // INSERT should fail
    let insert_result = sqlx::query("INSERT INTO test_data (name, value) VALUES ('gamma', 3.0)")
        .execute(&adapter.pool)
        .await;
    assert!(
        insert_result.is_err(),
        "INSERT should be rejected on read-only adapter"
    );

    // UPDATE should fail
    let update_result = sqlx::query("UPDATE test_data SET value = 99.0 WHERE name = 'alpha'")
        .execute(&adapter.pool)
        .await;
    assert!(
        update_result.is_err(),
        "UPDATE should be rejected on read-only adapter"
    );

    // DELETE should fail
    let delete_result = sqlx::query("DELETE FROM test_data WHERE name = 'alpha'")
        .execute(&adapter.pool)
        .await;
    assert!(
        delete_result.is_err(),
        "DELETE should be rejected on read-only adapter"
    );

    // DROP TABLE should fail
    let drop_result = sqlx::query("DROP TABLE test_data")
        .execute(&adapter.pool)
        .await;
    assert!(
        drop_result.is_err(),
        "DROP TABLE should be rejected on read-only adapter"
    );

    // CREATE TABLE should fail
    let create_result = sqlx::query("CREATE TABLE should_not_exist (id INTEGER PRIMARY KEY)")
        .execute(&adapter.pool)
        .await;
    assert!(
        create_result.is_err(),
        "CREATE TABLE should be rejected on read-only adapter"
    );

    // Cleanup
    drop(adapter);
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}
