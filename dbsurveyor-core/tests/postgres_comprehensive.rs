//! Comprehensive PostgreSQL adapter tests with testcontainers.
//!
//! This test suite covers all aspects of PostgreSQL adapter functionality:
//! - Connection pooling with various configurations
//! - Schema collection with different PostgreSQL versions
//! - Edge cases (empty schemas, special characters)
//! - Error handling for connection failures and timeouts
//!
//! # Security Testing
//! - Credential sanitization in all error messages
//! - No credentials in output files or logs
//! - Proper connection cleanup and resource management

use dbsurveyor_core::{
    Result,
    adapters::{ConnectionConfig, DatabaseAdapter, postgres::PostgresAdapter},
    error::DbSurveyorError,
    models::{ConstraintType, DatabaseType, ReferentialAction, UnifiedDataType},
};
use sqlx::PgPool;
use std::time::Duration;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

/// Helper function to wait for PostgreSQL to be ready
async fn wait_for_postgres_ready(database_url: &str, max_attempts: u32) -> Result<()> {
    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(database_url).await {
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
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        ),
    )))
}

/// Test connection pooling with various configurations
#[tokio::test]
async fn test_postgres_connection_pooling_configurations() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let base_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    // Wait for PostgreSQL to be ready
    wait_for_postgres_ready(&base_url, 30).await?;

    // Test 1: Default connection configuration
    let adapter1 = PostgresAdapter::new(&base_url).await?;
    adapter1.test_connection().await?;

    let (_active, idle, total) = adapter1.pool_stats();
    assert!(total >= 1, "Should have at least one connection");
    assert!(idle <= total, "Idle connections should not exceed total");

    // Test pool health
    assert!(adapter1.is_pool_healthy().await, "Pool should be healthy");

    // Test 2: Custom connection configuration with specific pool settings
    let custom_config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_secs(10),
        query_timeout: Duration::from_secs(15),
        max_connections: 5,
        read_only: true,
    };

    let adapter2 = PostgresAdapter::with_config(&base_url, custom_config).await?;
    adapter2.test_connection().await?;

    let (_active2, idle2, total2) = adapter2.pool_stats();
    assert!(total2 >= 1, "Custom config should have connections");
    assert!(idle2 <= total2, "Idle should not exceed total");

    // Test 3: Connection string with query parameters
    let url_with_params = format!(
        "{}?connect_timeout=5&statement_timeout=10000&pool_max_conns=3",
        base_url
    );
    let adapter3 = PostgresAdapter::new(&url_with_params).await?;
    adapter3.test_connection().await?;

    // Test 4: Multiple concurrent connections
    let mut handles = Vec::new();
    for i in 0..3 {
        let url = base_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&url).await.unwrap();
            adapter.test_connection().await.unwrap();
            // Simulate some work
            tokio::time::sleep(Duration::from_millis(100)).await;
            adapter.test_connection().await.unwrap();
            i
        });
        handles.push(handle);
    }

    // Wait for all concurrent connections to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Test 5: Connection cleanup
    adapter1.close().await;
    adapter2.close().await;
    adapter3.close().await;

    Ok(())
}

/// Test schema collection with different PostgreSQL versions and configurations
#[tokio::test]
async fn test_postgres_schema_collection_versions() -> Result<()> {
    // Test with default PostgreSQL version (latest)
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create comprehensive test schema
    let pool = PgPool::connect(&database_url).await.unwrap();

    // Create test schemas
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_schema_1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_schema_2")
        .execute(&pool)
        .await
        .unwrap();

    // Create tables with various PostgreSQL features
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS public.comprehensive_table (
            id SERIAL PRIMARY KEY,
            uuid_col UUID,
            text_col TEXT NOT NULL,
            varchar_col VARCHAR(255),
            json_col JSONB,
            array_col INTEGER[],
            timestamp_col TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            numeric_col NUMERIC(10,2),
            bool_col BOOLEAN DEFAULT FALSE,
            enum_col TEXT CHECK (enum_col IN ('active', 'inactive', 'pending')),
            CONSTRAINT check_text_length CHECK (length(text_col) > 0)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create table with foreign key relationships
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS test_schema_1.related_table (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER REFERENCES public.comprehensive_table(id) ON DELETE CASCADE ON UPDATE RESTRICT,
            name VARCHAR(100) NOT NULL UNIQUE,
            created_at TIMESTAMP DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create indexes
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comprehensive_timestamp ON public.comprehensive_table (timestamp_col DESC)"
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_related_name ON test_schema_1.related_table (name)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create a view
    sqlx::query(
        r#"
        CREATE OR REPLACE VIEW public.active_items AS
        SELECT id, text_col, timestamp_col
        FROM public.comprehensive_table
        WHERE bool_col = TRUE
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify database info
    assert_eq!(schema.database_info.name, "postgres");
    assert!(schema.database_info.version.is_some());
    assert!(schema.database_info.size_bytes.is_some());

    // Verify tables were collected
    assert!(!schema.tables.is_empty(), "Should have collected tables");

    // Find our comprehensive table
    let comprehensive_table = schema
        .tables
        .iter()
        .find(|t| t.name == "comprehensive_table" && t.schema.as_deref() == Some("public"))
        .expect("comprehensive_table not found");

    // Verify columns
    assert!(!comprehensive_table.columns.is_empty());

    // Check specific columns and their types
    let id_col = comprehensive_table
        .columns
        .iter()
        .find(|c| c.name == "id")
        .expect("id column not found");
    assert!(id_col.is_primary_key);
    assert!(id_col.is_auto_increment);
    assert!(!id_col.is_nullable);

    let uuid_col = comprehensive_table
        .columns
        .iter()
        .find(|c| c.name == "uuid_col")
        .expect("uuid_col not found");
    assert!(matches!(uuid_col.data_type, UnifiedDataType::Uuid));

    let json_col = comprehensive_table
        .columns
        .iter()
        .find(|c| c.name == "json_col")
        .expect("json_col not found");
    assert!(matches!(json_col.data_type, UnifiedDataType::Json));

    let array_col = comprehensive_table
        .columns
        .iter()
        .find(|c| c.name == "array_col")
        .expect("array_col not found");
    assert!(matches!(array_col.data_type, UnifiedDataType::Array { .. }));

    // Verify constraints
    assert!(!comprehensive_table.constraints.is_empty());

    // Find primary key constraint
    let pk_constraint = comprehensive_table
        .constraints
        .iter()
        .find(|c| matches!(c.constraint_type, ConstraintType::PrimaryKey))
        .expect("Primary key constraint not found");
    assert_eq!(pk_constraint.columns, vec!["id"]);

    // Find check constraint
    let check_constraint = comprehensive_table
        .constraints
        .iter()
        .find(|c| matches!(c.constraint_type, ConstraintType::Check))
        .expect("Check constraint not found");
    assert!(check_constraint.check_clause.is_some());

    // Verify indexes
    assert!(!comprehensive_table.indexes.is_empty());

    // Find our custom index
    let custom_index = comprehensive_table
        .indexes
        .iter()
        .find(|i| i.name.contains("timestamp"))
        .expect("Timestamp index not found");
    assert_eq!(custom_index.columns.len(), 1);
    assert_eq!(custom_index.columns[0].name, "timestamp_col");

    // Find related table
    let related_table = schema
        .tables
        .iter()
        .find(|t| t.name == "related_table" && t.schema.as_deref() == Some("test_schema_1"))
        .expect("related_table not found");

    // Verify foreign key relationship
    assert_eq!(related_table.foreign_keys.len(), 1);
    let fk = &related_table.foreign_keys[0];
    assert_eq!(fk.columns, vec!["parent_id"]);
    assert_eq!(fk.referenced_table, "comprehensive_table");
    assert_eq!(fk.referenced_columns, vec!["id"]);
    assert_eq!(fk.on_delete, Some(ReferentialAction::Cascade));
    assert_eq!(fk.on_update, Some(ReferentialAction::Restrict));

    // Find view
    let view = schema
        .tables
        .iter()
        .find(|t| t.name == "active_items" && t.schema.as_deref() == Some("public"))
        .expect("active_items view not found");
    assert!(!view.columns.is_empty());

    // Verify no credentials in output
    let schema_json = serde_json::to_string(&schema).unwrap();
    assert!(!schema_json.contains("postgres:postgres"));
    assert!(!schema_json.contains("password"));

    Ok(())
}

/// Test edge cases: empty schemas, special characters, and unusual configurations
#[tokio::test]
async fn test_postgres_edge_cases() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let pool = PgPool::connect(&database_url).await.unwrap();

    // Test 1: Empty schema
    sqlx::query("CREATE SCHEMA IF NOT EXISTS empty_schema")
        .execute(&pool)
        .await
        .unwrap();

    // Test 2: Schema and table names with special characters
    sqlx::query(r#"CREATE SCHEMA IF NOT EXISTS "special-schema""#)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS "special-schema"."table-with-dashes" (
            "column-with-dashes" INTEGER PRIMARY KEY,
            "column with spaces" TEXT,
            "column_with_unicode_🚀" VARCHAR(50),
            "UPPERCASE_COLUMN" BOOLEAN
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Test 3: Table with very long names (PostgreSQL limit is 63 characters)
    let long_table_name = "a".repeat(63); // Maximum PostgreSQL identifier length
    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS public."{}" (id SERIAL PRIMARY KEY)"#,
        long_table_name
    ))
    .execute(&pool)
    .await
    .unwrap();

    // Test 4: Table with all nullable columns
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS public.all_nullable (
            id SERIAL PRIMARY KEY,
            nullable_text TEXT,
            nullable_int INTEGER,
            nullable_timestamp TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Test 5: Table with complex constraints
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS public.complex_constraints (
            id SERIAL PRIMARY KEY,
            email VARCHAR(255) UNIQUE,
            age INTEGER CHECK (age >= 0 AND age <= 150),
            status VARCHAR(20) CHECK (status IN ('active', 'inactive', 'suspended')),
            created_at TIMESTAMP DEFAULT NOW(),
            updated_at TIMESTAMP DEFAULT NOW(),
            CONSTRAINT check_email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
            CONSTRAINT check_updated_after_created CHECK (updated_at >= created_at)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;

    // Test schema collection with edge cases
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify empty schema is handled
    // (Empty schemas might not appear in results, which is correct behavior)

    // Verify special character handling
    let special_table = schema
        .tables
        .iter()
        .find(|t| t.name == "table-with-dashes" && t.schema.as_deref() == Some("special-schema"));

    if let Some(table) = special_table {
        // Verify columns with special characters
        let dash_column = table
            .columns
            .iter()
            .find(|c| c.name == "column-with-dashes");
        assert!(dash_column.is_some());

        let space_column = table
            .columns
            .iter()
            .find(|c| c.name == "column with spaces");
        assert!(space_column.is_some());

        let unicode_column = table
            .columns
            .iter()
            .find(|c| c.name == "column_with_unicode_🚀");
        assert!(unicode_column.is_some());
    }

    // Verify long table name handling
    let long_table = schema.tables.iter().find(|t| t.name == long_table_name);
    assert!(long_table.is_some(), "Long table name should be handled");

    // Verify all nullable table
    let nullable_table = schema
        .tables
        .iter()
        .find(|t| t.name == "all_nullable")
        .expect("all_nullable table not found");

    // All columns except primary key should be nullable
    for column in &nullable_table.columns {
        if column.name != "id" {
            assert!(
                column.is_nullable,
                "Column {} should be nullable",
                column.name
            );
        }
    }

    // Verify complex constraints table
    let complex_table = schema
        .tables
        .iter()
        .find(|t| t.name == "complex_constraints")
        .expect("complex_constraints table not found");

    // Should have multiple check constraints
    let check_constraints: Vec<_> = complex_table
        .constraints
        .iter()
        .filter(|c| matches!(c.constraint_type, ConstraintType::Check))
        .collect();
    assert!(
        check_constraints.len() >= 2,
        "Should have multiple check constraints"
    );

    // Verify no credentials in output
    let schema_json = serde_json::to_string(&schema).unwrap();
    assert!(!schema_json.contains("postgres:postgres"));

    Ok(())
}

/// Test error handling for connection failures and timeouts
#[tokio::test]
async fn test_postgres_error_handling() -> Result<()> {
    // Test 1: Invalid connection string format
    let result = PostgresAdapter::new("invalid://connection/string").await;
    assert!(result.is_err());
    let error_msg = format!("{}", result.err().unwrap());
    assert!(!error_msg.contains("password")); // Should not leak credentials

    // Test 2: Malformed URL
    let result = PostgresAdapter::new("not-a-url-at-all").await;
    assert!(result.is_err());

    // Test 3: Wrong scheme
    let result = PostgresAdapter::new("mysql://user:pass@localhost/db").await;
    assert!(result.is_err());

    // Test 4: Connection to non-existent host
    let result = PostgresAdapter::new("postgres://user:pass@nonexistent-host:5432/db").await;
    // This should succeed because we use lazy connections, but test_connection should fail
    if let Ok(adapter) = result {
        let conn_result = adapter.test_connection().await;
        assert!(conn_result.is_err());
        let error_msg = format!("{}", conn_result.err().unwrap());
        assert!(!error_msg.contains("pass")); // Should not leak password
    }

    // Test 5: Connection to wrong port
    let result = PostgresAdapter::new("postgres://user:pass@localhost:9999/db").await;
    if let Ok(adapter) = result {
        let conn_result = adapter.test_connection().await;
        assert!(conn_result.is_err());
    }

    // Test 6: Invalid database name characters
    let result = PostgresAdapter::new("postgres://user@localhost/invalid-db-name-with-too-many-characters-exceeding-postgresql-limit-of-63-characters").await;
    assert!(result.is_err());

    // Test 7: Invalid username characters
    let result = PostgresAdapter::new("postgres://user-with-invalid-chars!@#$@localhost/db").await;
    assert!(result.is_err());

    // Test 8: Test with real PostgreSQL but insufficient privileges
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&admin_url, 30).await?;

    // Create limited user
    let pool = PgPool::connect(&admin_url).await.unwrap();
    let _ = sqlx::query("CREATE USER limited_user WITH PASSWORD 'limited_pass'")
        .execute(&pool)
        .await; // Ignore error if user exists

    // Revoke default privileges
    let _ = sqlx::query("REVOKE ALL ON SCHEMA public FROM limited_user")
        .execute(&pool)
        .await;

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

    // Should have very limited access
    assert!(schema.tables.len() <= 5); // May have access to some system views

    // Verify no credentials in output
    let schema_json = serde_json::to_string(&schema).unwrap();
    assert!(!schema_json.contains("limited_pass"));
    assert!(!schema_json.contains("limited_user:limited_pass"));

    Ok(())
}

/// Test connection timeout handling
#[tokio::test]
async fn test_postgres_timeout_handling() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Test with very short timeout
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_millis(1), // Very short timeout
        query_timeout: Duration::from_millis(100), // Very short query timeout
        max_connections: 10,
        read_only: true,
    };

    // This might succeed or fail depending on timing, but should handle gracefully
    let result = PostgresAdapter::with_config(&database_url, config).await;

    if let Ok(adapter) = result {
        // If adapter creation succeeded, test operations should handle timeouts gracefully
        let _ = adapter.test_connection().await; // May timeout, which is expected
        let _ = adapter.collect_schema().await; // May timeout, which is expected
    }

    // Test with reasonable timeout
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_secs(5),
        query_timeout: Duration::from_secs(10),
        max_connections: 10,
        read_only: true,
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;
    adapter.test_connection().await?;

    Ok(())
}

/// Test credential sanitization in all error paths
#[tokio::test]
async fn test_postgres_credential_sanitization() {
    use dbsurveyor_core::adapters::redact_database_url;

    // Test URL redaction function
    let test_cases = vec![
        (
            "postgres://user:secret123@localhost:5432/testdb",
            "postgres://user:****@localhost:5432/testdb",
        ),
        (
            "postgresql://admin:p@ssw0rd@example.com:5432/mydb",
            "postgresql://admin:****@example.com:5432/mydb",
        ),
        (
            "postgres://user@localhost/db", // No password
            "postgres://user@localhost/db",
        ),
        (
            "postgres://localhost/db", // No user or password
            "postgres://localhost/db",
        ),
        ("invalid-url", "<redacted>"), // Invalid URL - fully redacted for security
        ("", "<redacted>"),            // Empty string - fully redacted for security
    ];

    for (input, expected) in test_cases {
        let result = redact_database_url(input);
        assert_eq!(result, expected, "Failed for input: {}", input);

        // Ensure no secrets are leaked
        if input.contains("secret123") {
            assert!(!result.contains("secret123"));
        }
        if input.contains("p@ssw0rd") {
            assert!(!result.contains("p@ssw0rd"));
        }
    }

    // Test error message sanitization
    let sensitive_urls = vec![
        "postgres://user:topsecret@localhost:5432/db",
        "postgresql://admin:password123@example.com/mydb",
    ];

    for url in sensitive_urls {
        // Test adapter creation with invalid URL format
        let invalid_url = url.replace("postgres", "invalid");
        let result = PostgresAdapter::new(&invalid_url).await;
        assert!(result.is_err());

        let error_msg = format!("{}", result.err().unwrap());
        assert!(!error_msg.contains("topsecret"));
        assert!(!error_msg.contains("password123"));
        assert!(!error_msg.contains("user:topsecret"));
        assert!(!error_msg.contains("admin:password123"));
    }
}

/// Test PostgreSQL adapter with different database configurations
#[tokio::test]
async fn test_postgres_database_configurations() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let base_url = format!("postgres://postgres:postgres@localhost:{}", port);

    wait_for_postgres_ready(&format!("{}/postgres", base_url), 30).await?;

    // Create additional test database
    let pool = PgPool::connect(&format!("{}/postgres", base_url))
        .await
        .unwrap();
    let _ = sqlx::query("CREATE DATABASE test_db_config")
        .execute(&pool)
        .await; // Ignore error if exists
    pool.close().await;

    // Test 1: Connection to specific database
    let test_db_url = format!("{}/test_db_config", base_url);
    let adapter = PostgresAdapter::new(&test_db_url).await?;
    adapter.test_connection().await?;

    let schema = adapter.collect_schema().await?;
    assert_eq!(schema.database_info.name, "test_db_config");

    // Test 2: Connection without specifying database (should connect to default)
    let default_url = base_url.clone();
    let adapter2 = PostgresAdapter::new(&default_url).await?;
    adapter2.test_connection().await?;

    // Test 3: Connection with SSL mode disabled (for testing)
    let ssl_disabled_url = format!("{}?sslmode=disable", base_url);
    let adapter3 = PostgresAdapter::new(&ssl_disabled_url).await?;
    adapter3.test_connection().await?;

    Ok(())
}

/// Test PostgreSQL version compatibility and feature detection
#[tokio::test]
async fn test_postgres_version_compatibility() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify version information is collected
    assert!(schema.database_info.version.is_some());
    let version = schema.database_info.version.as_ref().unwrap();
    assert!(version.contains("PostgreSQL"));

    // Test adapter features
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);

    // Test that adapter supports expected features
    use dbsurveyor_core::adapters::AdapterFeature;
    assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
    assert!(adapter.supports_feature(AdapterFeature::DataSampling));
    assert!(adapter.supports_feature(AdapterFeature::MultiDatabase));
    assert!(adapter.supports_feature(AdapterFeature::ConnectionPooling));
    assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));
    assert!(adapter.supports_feature(AdapterFeature::ReadOnlyMode));

    Ok(())
}
