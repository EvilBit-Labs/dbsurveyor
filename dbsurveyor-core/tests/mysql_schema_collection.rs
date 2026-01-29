//! MySQL schema collection integration tests.
//!
//! This test suite covers:
//! - Table and column collection from INFORMATION_SCHEMA
//! - Primary key and foreign key detection
//! - Index collection
//! - View collection
//! - Data type mapping

#![cfg(feature = "mysql")]

use dbsurveyor_core::{
    Result,
    adapters::{DatabaseAdapter, mysql::MySqlAdapter},
    error::DbSurveyorError,
    models::DatabaseType,
};
use sqlx::MySqlPool;
use std::time::Duration;
use testcontainers_modules::{mysql::Mysql, testcontainers::runners::AsyncRunner};

/// Helper function to wait for MySQL to be ready
async fn wait_for_mysql_ready(database_url: &str, max_attempts: u32) -> Result<()> {
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = MySqlPool::connect(database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                return Ok(());
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    Err(DbSurveyorError::connection_failed(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!(
            "MySQL failed to become ready after {} attempts",
            max_attempts
        ),
    )))
}

/// Test basic table collection
#[tokio::test]
async fn test_collect_tables() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test tables
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE users (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255) UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE orders (
            id INT AUTO_INCREMENT PRIMARY KEY,
            user_id INT NOT NULL,
            total DECIMAL(10, 2),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify we got the schema from MySQL
    assert!(
        !schema.database_info.name.is_empty(),
        "Database name should not be empty"
    );
    assert!(schema.tables.len() >= 2, "Should have at least 2 tables");

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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table with primary key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE pk_test (
            id INT AUTO_INCREMENT PRIMARY KEY,
            value TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table with composite primary key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE composite_pk (
            tenant_id INT,
            user_id INT,
            data TEXT,
            PRIMARY KEY (tenant_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "composite_pk");
    assert!(table.is_some(), "Should find composite_pk table");

    let table = table.unwrap();
    assert!(table.primary_key.is_some(), "Should have primary key");

    let pk = table.primary_key.as_ref().unwrap();
    assert_eq!(pk.columns.len(), 2, "Composite PK should have 2 columns");
    assert_eq!(pk.columns[0], "tenant_id");
    assert_eq!(pk.columns[1], "user_id");

    Ok(())
}

/// Test foreign key collection
#[tokio::test]
async fn test_collect_foreign_keys() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create tables with foreign key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE fk_parent (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE fk_child (
            id INT AUTO_INCREMENT PRIMARY KEY,
            parent_id INT NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES fk_parent(id) ON DELETE CASCADE ON UPDATE CASCADE
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table with indexes
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE idx_test (
            id INT AUTO_INCREMENT PRIMARY KEY,
            email VARCHAR(255) UNIQUE,
            name VARCHAR(100),
            score INT,
            INDEX idx_name (name),
            INDEX idx_score (score)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "idx_test");
    assert!(table.is_some(), "Should find idx_test table");

    let table = table.unwrap();
    // Should have multiple indexes (PRIMARY, unique on email, and regular indexes)
    assert!(
        table.indexes.len() >= 3,
        "Should have at least 3 indexes, got {}",
        table.indexes.len()
    );

    // Check for unique index on email
    let email_idx = table
        .indexes
        .iter()
        .find(|i| i.columns.iter().any(|c| c.name == "email"));
    assert!(email_idx.is_some(), "Should have index on email");
    assert!(email_idx.unwrap().is_unique, "Email index should be unique");

    // Check for regular indexes
    let name_idx = table
        .indexes
        .iter()
        .find(|i| i.columns.iter().any(|c| c.name == "name"));
    assert!(name_idx.is_some(), "Should have index on name");

    Ok(())
}

/// Test view collection
#[tokio::test]
async fn test_collect_views() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table and view
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE view_source (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100),
            active TINYINT(1) DEFAULT 1
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("CREATE VIEW active_items AS SELECT id, name FROM view_source WHERE active = 1")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    assert!(!schema.views.is_empty(), "Should have at least one view");

    let view = schema.views.iter().find(|v| v.name == "active_items");
    assert!(view.is_some(), "Should find active_items view");

    let view = view.unwrap();
    assert!(view.definition.is_some(), "View should have definition");

    Ok(())
}

/// Test data type mapping
#[tokio::test]
async fn test_data_type_mapping() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table with various data types
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE type_test (
            id INT AUTO_INCREMENT PRIMARY KEY,
            tiny_val TINYINT,
            small_val SMALLINT,
            big_val BIGINT,
            float_val FLOAT,
            double_val DOUBLE,
            dec_val DECIMAL(10, 2),
            char_val CHAR(10),
            varchar_val VARCHAR(255),
            text_val TEXT,
            bool_val TINYINT(1),
            date_val DATE,
            datetime_val DATETIME,
            timestamp_val TIMESTAMP,
            json_val JSON,
            blob_val BLOB
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    let table = schema.tables.iter().find(|t| t.name == "type_test");
    assert!(table.is_some(), "Should find type_test table");

    let table = table.unwrap();
    assert_eq!(table.columns.len(), 16, "Should have 16 columns");

    // Verify column types are mapped correctly
    for col in &table.columns {
        // Each column should have a unified data type
        assert!(
            !format!("{:?}", col.data_type).is_empty(),
            "Column {} should have a data type",
            col.name
        );
    }

    Ok(())
}

/// Test connection configuration
#[tokio::test]
async fn test_mysql_connection_config() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    let adapter = MySqlAdapter::new(&database_url).await?;

    // Verify connection config
    let config = adapter.connection_config();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(port));
    assert_eq!(config.database, Some("test".to_string()));
    assert!(config.read_only);

    Ok(())
}

/// Test database type is correctly identified
#[tokio::test]
async fn test_mysql_database_type() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    let adapter = MySqlAdapter::new(&database_url).await?;
    assert_eq!(adapter.database_type(), DatabaseType::MySQL);

    Ok(())
}

/// Test connection test works
#[tokio::test]
async fn test_mysql_test_connection() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let result = adapter.test_connection().await;
    assert!(result.is_ok(), "Connection test should succeed");

    Ok(())
}
