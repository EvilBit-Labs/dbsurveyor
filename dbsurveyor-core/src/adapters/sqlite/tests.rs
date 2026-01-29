//! Unit tests for SQLite adapter.
//!
//! These tests verify the SQLite adapter functionality including:
//! - Type mapping from SQLite types to UnifiedDataType
//! - Connection configuration parsing
//! - Order by clause generation

use crate::adapters::{AdapterFeature, DatabaseAdapter};
use crate::models::{DatabaseType, UnifiedDataType};

use super::SqliteAdapter;
use super::type_mapping::map_sqlite_type;

// =============================================================================
// Type Mapping Tests
// =============================================================================

#[test]
fn test_map_sqlite_integer_type() {
    let result = map_sqlite_type("INTEGER");
    assert!(matches!(
        result,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));
}

#[test]
fn test_map_sqlite_text_type() {
    let result = map_sqlite_type("TEXT");
    assert!(matches!(
        result,
        UnifiedDataType::String { max_length: None }
    ));
}

#[test]
fn test_map_sqlite_varchar_with_length() {
    let result = map_sqlite_type("VARCHAR(255)");
    assert!(matches!(
        result,
        UnifiedDataType::String {
            max_length: Some(255)
        }
    ));
}

#[test]
fn test_map_sqlite_real_type() {
    let result = map_sqlite_type("REAL");
    assert!(matches!(
        result,
        UnifiedDataType::Float {
            precision: Some(53)
        }
    ));
}

#[test]
fn test_map_sqlite_blob_type() {
    let result = map_sqlite_type("BLOB");
    assert!(matches!(
        result,
        UnifiedDataType::Binary { max_length: None }
    ));
}

#[test]
fn test_map_sqlite_boolean_type() {
    let result = map_sqlite_type("BOOLEAN");
    assert!(matches!(result, UnifiedDataType::Boolean));
}

#[test]
fn test_map_sqlite_datetime_types() {
    let result = map_sqlite_type("DATE");
    assert!(matches!(result, UnifiedDataType::Date));

    let result = map_sqlite_type("DATETIME");
    assert!(matches!(
        result,
        UnifiedDataType::DateTime {
            with_timezone: false
        }
    ));

    let result = map_sqlite_type("TIMESTAMP");
    assert!(matches!(
        result,
        UnifiedDataType::DateTime {
            with_timezone: false
        }
    ));
}

#[test]
fn test_map_sqlite_empty_type() {
    // Empty type defaults to BLOB in SQLite
    let result = map_sqlite_type("");
    assert!(matches!(
        result,
        UnifiedDataType::Binary { max_length: None }
    ));
}

#[test]
fn test_map_sqlite_case_insensitivity() {
    let upper = map_sqlite_type("INTEGER");
    let lower = map_sqlite_type("integer");
    let mixed = map_sqlite_type("Integer");

    assert!(matches!(
        upper,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));
    assert!(matches!(
        lower,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));
    assert!(matches!(
        mixed,
        UnifiedDataType::Integer {
            bits: 32,
            signed: true
        }
    ));
}

// =============================================================================
// Connection Configuration Tests
// =============================================================================

#[test]
fn test_parse_sqlite_connection_config_memory() {
    use super::connection::parse_sqlite_connection_config;

    let config = parse_sqlite_connection_config(":memory:").unwrap();
    assert_eq!(config.database, Some(":memory:".to_string()));
    assert_eq!(config.max_connections, 1);
    assert_eq!(config.port, None);
}

#[test]
fn test_parse_sqlite_connection_config_file() {
    use super::connection::parse_sqlite_connection_config;

    let config = parse_sqlite_connection_config("sqlite:///path/to/test.db").unwrap();
    assert_eq!(config.database, Some("test.db".to_string()));
    assert_eq!(config.max_connections, 1);
}

#[test]
fn test_validate_sqlite_connection_string_valid() {
    use super::connection::validate_sqlite_connection_string;

    assert!(validate_sqlite_connection_string(":memory:").is_ok());
    assert!(validate_sqlite_connection_string("sqlite::memory:").is_ok());
    assert!(validate_sqlite_connection_string("sqlite:///path/to/db.sqlite").is_ok());
    assert!(validate_sqlite_connection_string("/path/to/database.db").is_ok());
    assert!(validate_sqlite_connection_string("./local.sqlite").is_ok());
}

#[test]
fn test_validate_sqlite_connection_string_invalid() {
    use super::connection::validate_sqlite_connection_string;

    assert!(validate_sqlite_connection_string("postgres://localhost/db").is_err());
    assert!(validate_sqlite_connection_string("mysql://localhost/db").is_err());
}

// =============================================================================
// Adapter Feature Tests
// =============================================================================

#[tokio::test]
async fn test_sqlite_adapter_database_type() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    assert_eq!(adapter.database_type(), DatabaseType::SQLite);
}

#[tokio::test]
async fn test_sqlite_adapter_supports_features() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();

    // SQLite should support these features
    assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
    assert!(adapter.supports_feature(AdapterFeature::DataSampling));
    assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));
    assert!(adapter.supports_feature(AdapterFeature::ReadOnlyMode));

    // SQLite does NOT support connection pooling or multi-database
    assert!(!adapter.supports_feature(AdapterFeature::ConnectionPooling));
    assert!(!adapter.supports_feature(AdapterFeature::MultiDatabase));
}

#[tokio::test]
async fn test_sqlite_adapter_test_connection() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    let result = adapter.test_connection().await;
    assert!(
        result.is_ok(),
        "Connection test should succeed for in-memory DB"
    );
}

#[tokio::test]
async fn test_sqlite_adapter_is_in_memory() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    assert!(adapter.is_in_memory());

    let adapter = SqliteAdapter::new("sqlite::memory:").await.unwrap();
    assert!(adapter.is_in_memory());
}

#[tokio::test]
async fn test_sqlite_adapter_connection_config() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    let config = adapter.connection_config();

    assert_eq!(config.max_connections, 1);
    assert!(config.read_only);
}

// =============================================================================
// Order By Clause Tests
// =============================================================================

#[test]
fn test_generate_order_by_clause_variants() {
    use super::sampling::generate_order_by_clause;
    use crate::models::{OrderingStrategy, SortDirection};

    // Primary key
    let strategy = OrderingStrategy::PrimaryKey {
        columns: vec!["id".to_string()],
    };
    assert_eq!(
        generate_order_by_clause(&strategy, true),
        "ORDER BY \"id\" DESC"
    );
    assert_eq!(
        generate_order_by_clause(&strategy, false),
        "ORDER BY \"id\" ASC"
    );

    // Timestamp
    let strategy = OrderingStrategy::Timestamp {
        column: "created_at".to_string(),
        direction: SortDirection::Descending,
    };
    assert_eq!(
        generate_order_by_clause(&strategy, true),
        "ORDER BY \"created_at\" DESC"
    );

    // Auto-increment
    let strategy = OrderingStrategy::AutoIncrement {
        column: "row_id".to_string(),
    };
    assert_eq!(
        generate_order_by_clause(&strategy, true),
        "ORDER BY \"row_id\" DESC"
    );

    // System ROWID
    let strategy = OrderingStrategy::SystemRowId {
        column: "rowid".to_string(),
    };
    assert_eq!(
        generate_order_by_clause(&strategy, true),
        "ORDER BY rowid DESC"
    );

    // Unordered
    let strategy = OrderingStrategy::Unordered;
    assert_eq!(
        generate_order_by_clause(&strategy, true),
        "ORDER BY RANDOM()"
    );
}

// =============================================================================
// In-Memory Schema Collection Tests
// =============================================================================

#[tokio::test]
async fn test_sqlite_collect_empty_schema() {
    let adapter = SqliteAdapter::new(":memory:").await.unwrap();
    let schema = adapter.collect_schema().await.unwrap();

    assert_eq!(schema.database_info.name, ":memory:");
    assert!(schema.database_info.version.is_some());
    assert!(schema.tables.is_empty());
    assert!(schema.views.is_empty());
    assert!(schema.triggers.is_empty());
}

#[tokio::test]
async fn test_sqlite_collect_schema_with_table() {
    // Create adapter with writable connection for setup
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Create a test table
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

    // Create adapter from the pool
    let adapter = SqliteAdapter {
        pool,
        config: crate::adapters::ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    };

    let schema = adapter.collect_schema().await.unwrap();

    assert_eq!(schema.tables.len(), 1);
    let table = &schema.tables[0];
    assert_eq!(table.name, "users");
    assert_eq!(table.columns.len(), 4);

    // Verify columns
    let id_col = table.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.is_primary_key);
    assert!(!id_col.is_nullable);

    let name_col = table.columns.iter().find(|c| c.name == "name").unwrap();
    assert!(!name_col.is_nullable);

    let email_col = table.columns.iter().find(|c| c.name == "email").unwrap();
    assert!(email_col.is_nullable);
}

#[tokio::test]
async fn test_sqlite_collect_schema_with_foreign_key() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Enable foreign keys
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    // Create parent and child tables
    sqlx::query(
        "CREATE TABLE authors (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE books (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            author_id INTEGER NOT NULL,
            FOREIGN KEY (author_id) REFERENCES authors(id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = SqliteAdapter {
        pool,
        config: crate::adapters::ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    };

    let schema = adapter.collect_schema().await.unwrap();

    assert_eq!(schema.tables.len(), 2);

    let books_table = schema.tables.iter().find(|t| t.name == "books").unwrap();
    assert_eq!(books_table.foreign_keys.len(), 1);

    let fk = &books_table.foreign_keys[0];
    assert_eq!(fk.columns, vec!["author_id"]);
    assert_eq!(fk.referenced_table, "authors");
    assert_eq!(fk.referenced_columns, vec!["id"]);
}

#[tokio::test]
async fn test_sqlite_collect_schema_with_index() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            sku TEXT UNIQUE,
            price REAL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("CREATE INDEX idx_products_name ON products(name)")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = SqliteAdapter {
        pool,
        config: crate::adapters::ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    };

    let schema = adapter.collect_schema().await.unwrap();

    let table = &schema.tables[0];
    // Should have at least the custom index plus auto-created indexes
    assert!(
        !table.indexes.is_empty(),
        "Should have indexes, got {:?}",
        table.indexes
    );

    let name_idx = table.indexes.iter().find(|i| i.name == "idx_products_name");
    assert!(name_idx.is_some(), "Should find idx_products_name index");
}

#[tokio::test]
async fn test_sqlite_collect_schema_with_view() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE employees (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            department TEXT,
            salary REAL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE VIEW high_earners AS
         SELECT id, name, department FROM employees WHERE salary > 100000",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = SqliteAdapter {
        pool,
        config: crate::adapters::ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    };

    let schema = adapter.collect_schema().await.unwrap();

    assert_eq!(schema.views.len(), 1);
    let view = &schema.views[0];
    assert_eq!(view.name, "high_earners");
    assert!(view.definition.is_some());
}

#[tokio::test]
async fn test_sqlite_collect_schema_with_trigger() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE audit_log (
            id INTEGER PRIMARY KEY,
            action TEXT,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TRIGGER log_user_insert
         AFTER INSERT ON users
         BEGIN
             INSERT INTO audit_log(action) VALUES ('INSERT user ' || NEW.name);
         END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = SqliteAdapter {
        pool,
        config: crate::adapters::ConnectionConfig::new("localhost".to_string())
            .with_database(":memory:".to_string()),
        connection_string: "sqlite::memory:".to_string(),
    };

    let schema = adapter.collect_schema().await.unwrap();

    assert_eq!(schema.triggers.len(), 1);
    let trigger = &schema.triggers[0];
    assert_eq!(trigger.name, "log_user_insert");
    assert_eq!(trigger.table_name, "users");
}
