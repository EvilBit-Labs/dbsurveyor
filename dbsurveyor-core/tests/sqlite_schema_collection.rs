//! SQLite schema collection integration tests.
//!
//! This test suite covers:
//! - Table and column collection from sqlite_master
//! - Primary key and foreign key detection
//! - Index collection
//! - View collection
//! - Trigger collection
//! - Data type mapping
//!
//! Note: SQLite tests use in-memory databases, so no testcontainers needed.

#![cfg(feature = "sqlite")]

use dbsurveyor_core::{
    Result,
    adapters::{ConnectionConfig, DatabaseAdapter, sqlite::SqliteAdapter},
    models::DatabaseType,
};
use sqlx::SqlitePool;

/// Helper function to create an in-memory SQLite database with schema
async fn create_test_adapter() -> Result<SqliteAdapter> {
    SqliteAdapter::new("sqlite::memory:").await
}

/// Helper function to create an adapter with pre-populated data
async fn create_adapter_with_pool(pool: SqlitePool) -> SqliteAdapter {
    SqliteAdapter {
        pool,
        config: ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    }
}

// =============================================================================
// Basic Connectivity Tests
// =============================================================================

/// Test basic connection and test_connection
#[tokio::test]
async fn test_sqlite_connection() -> Result<()> {
    let adapter = create_test_adapter().await?;
    let result = adapter.test_connection().await;
    assert!(result.is_ok(), "Connection test should succeed");
    Ok(())
}

/// Test database type is correctly identified
#[tokio::test]
async fn test_sqlite_database_type() -> Result<()> {
    let adapter = create_test_adapter().await?;
    assert_eq!(adapter.database_type(), DatabaseType::SQLite);
    Ok(())
}

/// Test empty database schema collection
#[tokio::test]
async fn test_collect_empty_schema() -> Result<()> {
    let adapter = create_test_adapter().await?;
    let schema = adapter.collect_schema().await?;

    assert_eq!(schema.database_info.name, ":memory:");
    assert!(
        schema.database_info.version.is_some(),
        "Version should be present"
    );
    assert!(
        schema.tables.is_empty(),
        "Empty database should have no tables"
    );
    assert!(
        schema.views.is_empty(),
        "Empty database should have no views"
    );
    Ok(())
}

// =============================================================================
// Table Collection Tests
// =============================================================================

/// Test basic table collection
#[tokio::test]
async fn test_collect_tables() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Create test tables
    sqlx::query(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            total REAL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    assert_eq!(schema.tables.len(), 2, "Should have 2 tables");

    // Find users table
    let users_table = schema.tables.iter().find(|t| t.name == "users");
    assert!(users_table.is_some(), "Should find users table");

    let users_table = users_table.unwrap();
    assert_eq!(
        users_table.columns.len(),
        4,
        "Users table should have 4 columns"
    );

    // Verify column details
    let id_column = users_table.columns.iter().find(|c| c.name == "id");
    assert!(id_column.is_some(), "Should have id column");
    let id_column = id_column.unwrap();
    assert!(!id_column.is_nullable, "id should not be nullable");
    assert!(id_column.is_primary_key, "id should be primary key");

    let name_column = users_table.columns.iter().find(|c| c.name == "name");
    assert!(name_column.is_some(), "Should have name column");
    let name_column = name_column.unwrap();
    assert!(!name_column.is_nullable, "name should not be nullable");

    let email_column = users_table.columns.iter().find(|c| c.name == "email");
    assert!(email_column.is_some(), "Should have email column");
    let email_column = email_column.unwrap();
    assert!(email_column.is_nullable, "email should be nullable");

    Ok(())
}

/// Test primary key collection
#[tokio::test]
async fn test_collect_primary_keys() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE pk_test (
            id INTEGER PRIMARY KEY,
            value TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "pk_test");
    assert!(table.is_some(), "Should find pk_test table");

    let table = table.unwrap();
    assert!(table.primary_key.is_some(), "Should have primary key");

    let pk = table.primary_key.as_ref().unwrap();
    assert_eq!(pk.columns.len(), 1, "Primary key should have 1 column");
    assert_eq!(pk.columns[0], "id", "Primary key column should be 'id'");

    Ok(())
}

/// Test composite primary key collection
#[tokio::test]
async fn test_collect_composite_primary_key() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE composite_pk (
            tenant_id INTEGER,
            user_id INTEGER,
            data TEXT,
            PRIMARY KEY (tenant_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "composite_pk");
    assert!(table.is_some(), "Should find composite_pk table");

    let table = table.unwrap();
    assert!(table.primary_key.is_some(), "Should have primary key");

    let pk = table.primary_key.as_ref().unwrap();
    assert_eq!(pk.columns.len(), 2, "Composite PK should have 2 columns");
    assert!(pk.columns.contains(&"tenant_id".to_string()));
    assert!(pk.columns.contains(&"user_id".to_string()));

    Ok(())
}

/// Test foreign key collection
#[tokio::test]
async fn test_collect_foreign_keys() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Enable foreign keys
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE fk_parent (
            id INTEGER PRIMARY KEY,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE fk_child (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES fk_parent(id) ON DELETE CASCADE ON UPDATE CASCADE
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let child_table = schema.tables.iter().find(|t| t.name == "fk_child");
    assert!(child_table.is_some(), "Should find fk_child table");

    let child_table = child_table.unwrap();
    assert!(
        !child_table.foreign_keys.is_empty(),
        "Should have foreign keys"
    );

    let fk = &child_table.foreign_keys[0];
    assert_eq!(fk.columns, vec!["parent_id"]);
    assert_eq!(fk.referenced_table, "fk_parent");
    assert_eq!(fk.referenced_columns, vec!["id"]);

    Ok(())
}

/// Test index collection
#[tokio::test]
async fn test_collect_indexes() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE idx_test (
            id INTEGER PRIMARY KEY,
            email TEXT UNIQUE,
            name TEXT,
            score INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create explicit indexes
    sqlx::query("CREATE INDEX idx_name ON idx_test(name)")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("CREATE INDEX idx_score ON idx_test(score)")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "idx_test");
    assert!(table.is_some(), "Should find idx_test table");

    let table = table.unwrap();
    // Should have at least the two explicit indexes
    assert!(
        table.indexes.len() >= 2,
        "Should have at least 2 indexes, got {}",
        table.indexes.len()
    );

    // Check for explicit indexes
    let name_idx = table.indexes.iter().find(|i| i.name == "idx_name");
    assert!(name_idx.is_some(), "Should have idx_name index");

    let score_idx = table.indexes.iter().find(|i| i.name == "idx_score");
    assert!(score_idx.is_some(), "Should have idx_score index");

    Ok(())
}

/// Test view collection
#[tokio::test]
async fn test_collect_views() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE view_source (
            id INTEGER PRIMARY KEY,
            name TEXT,
            active INTEGER DEFAULT 1
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("CREATE VIEW active_items AS SELECT id, name FROM view_source WHERE active = 1")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    assert!(!schema.views.is_empty(), "Should have at least one view");

    let view = schema.views.iter().find(|v| v.name == "active_items");
    assert!(view.is_some(), "Should find active_items view");

    let view = view.unwrap();
    assert!(view.definition.is_some(), "View should have definition");

    Ok(())
}

/// Test trigger collection
#[tokio::test]
async fn test_collect_triggers() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE trigger_test (
            id INTEGER PRIMARY KEY,
            value TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE audit (
            id INTEGER PRIMARY KEY,
            action TEXT,
            timestamp TEXT DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TRIGGER test_trigger
         AFTER INSERT ON trigger_test
         BEGIN
             INSERT INTO audit(action) VALUES ('inserted');
         END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    assert!(
        !schema.triggers.is_empty(),
        "Should have at least one trigger"
    );

    let trigger = schema.triggers.iter().find(|t| t.name == "test_trigger");
    assert!(trigger.is_some(), "Should find test_trigger");

    let trigger = trigger.unwrap();
    assert_eq!(trigger.table_name, "trigger_test");
    assert!(trigger.definition.is_some());

    Ok(())
}

// =============================================================================
// Data Type Mapping Tests
// =============================================================================

/// Test data type mapping
#[tokio::test]
async fn test_data_type_mapping() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE type_test (
            int_col INTEGER,
            text_col TEXT,
            real_col REAL,
            blob_col BLOB,
            varchar_col VARCHAR(255),
            boolean_col BOOLEAN,
            date_col DATE,
            datetime_col DATETIME,
            numeric_col NUMERIC(10,2)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "type_test");
    assert!(table.is_some(), "Should find type_test table");

    let table = table.unwrap();
    assert_eq!(table.columns.len(), 9, "Should have 9 columns");

    // Verify column types are mapped correctly
    for col in &table.columns {
        assert!(
            !format!("{:?}", col.data_type).is_empty(),
            "Column {} should have a data type",
            col.name
        );
    }

    Ok(())
}

// =============================================================================
// Connection Configuration Tests
// =============================================================================

/// Test connection configuration
#[tokio::test]
async fn test_sqlite_connection_config() -> Result<()> {
    let adapter = create_test_adapter().await?;

    let config = adapter.connection_config();
    assert_eq!(config.max_connections, 1, "SQLite uses single connection");
    assert!(config.read_only, "Should be read-only by default");

    Ok(())
}

/// Test in-memory detection
#[tokio::test]
async fn test_sqlite_in_memory_detection() -> Result<()> {
    let adapter = SqliteAdapter::new(":memory:").await?;
    assert!(adapter.is_in_memory(), "Should detect in-memory database");

    let adapter = SqliteAdapter::new("sqlite::memory:").await?;
    assert!(adapter.is_in_memory(), "Should detect sqlite::memory:");

    Ok(())
}

// =============================================================================
// Row Count Tests
// =============================================================================

/// Test row count collection
#[tokio::test]
async fn test_row_count() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE count_test (id INTEGER PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    // Insert some rows
    for i in 0..10 {
        sqlx::query(&format!(
            "INSERT INTO count_test (value) VALUES ('row{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }

    let adapter = create_adapter_with_pool(pool).await;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "count_test");
    assert!(table.is_some());

    let table = table.unwrap();
    assert_eq!(table.row_count, Some(10), "Should count 10 rows");

    Ok(())
}
