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

    // Create test table in public schema with constraints and indexes (ignore error if exists)
    let _ = sqlx::query(
        "CREATE TABLE public.test_table (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL UNIQUE,
            email VARCHAR(255) UNIQUE,
            created_at TIMESTAMP DEFAULT NOW(),
            CONSTRAINT check_name_length CHECK (length(name) > 0)
        )",
    )
    .execute(&pool)
    .await; // Ignore error if table already exists

    // Create additional index
    let _ = sqlx::query(
        "CREATE INDEX idx_test_table_created_at ON public.test_table (created_at DESC)",
    )
    .execute(&pool)
    .await; // Ignore error if index already exists

    // Create test table in custom schema with foreign key (ignore error if exists)
    let _ = sqlx::query(
        "CREATE TABLE test_schema.custom_table (
            id SERIAL PRIMARY KEY,
            test_table_id INTEGER REFERENCES public.test_table(id) ON DELETE CASCADE,
            data TEXT,
            active BOOLEAN DEFAULT TRUE,
            CONSTRAINT check_data_not_empty CHECK (data IS NULL OR length(data) > 0)
        )",
    )
    .execute(&pool)
    .await; // Ignore error if table already exists

    // Create composite index
    let _ = sqlx::query(
        "CREATE INDEX idx_custom_table_composite ON test_schema.custom_table (test_table_id, active)"
    )
    .execute(&pool)
    .await; // Ignore error if index already exists

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

    let email_column = public_table
        .columns
        .iter()
        .find(|c| c.name == "email")
        .expect("email column not found");
    assert_eq!(email_column.ordinal_position, 3);
    assert!(!email_column.is_primary_key);
    assert!(!email_column.is_auto_increment);
    assert!(email_column.is_nullable);

    let created_at_column = public_table
        .columns
        .iter()
        .find(|c| c.name == "created_at")
        .expect("created_at column not found");
    assert_eq!(created_at_column.ordinal_position, 4);
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

    // Test constraint and index collection

    // Verify primary key is collected
    assert!(public_table.primary_key.is_some());
    let pk = public_table.primary_key.as_ref().unwrap();
    assert_eq!(pk.columns, vec!["id"]);

    // Verify constraints are collected
    assert!(!public_table.constraints.is_empty());

    // Find primary key constraint
    let pk_constraint = public_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::PrimaryKey
            )
        })
        .expect("Primary key constraint not found");
    assert_eq!(pk_constraint.columns, vec!["id"]);

    // Find unique constraints
    let unique_constraints: Vec<_> = public_table
        .constraints
        .iter()
        .filter(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::Unique
            )
        })
        .collect();
    assert!(
        !unique_constraints.is_empty(),
        "Should have unique constraints"
    );

    // Find check constraint - look for our specific constraint
    let check_constraint = public_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::Check
            ) && c
                .check_clause
                .as_ref()
                .is_some_and(|clause| clause.to_lowercase().contains("length"))
        })
        .expect("Check constraint with length check not found");
    assert!(check_constraint.check_clause.is_some());
    assert!(
        check_constraint
            .check_clause
            .as_ref()
            .unwrap()
            .to_lowercase()
            .contains("length")
    );

    // Verify indexes are collected
    assert!(!public_table.indexes.is_empty());

    // Find primary key index
    let pk_index = public_table
        .indexes
        .iter()
        .find(|i| i.is_primary)
        .expect("Primary key index not found");
    assert!(pk_index.is_unique);
    assert_eq!(pk_index.columns.len(), 1);
    assert_eq!(pk_index.columns[0].name, "id");

    // Find our custom index
    let custom_index = public_table
        .indexes
        .iter()
        .find(|i| i.name.contains("created_at"))
        .expect("Custom index on created_at not found");
    assert!(!custom_index.is_primary);
    assert_eq!(custom_index.columns.len(), 1);
    assert_eq!(custom_index.columns[0].name, "created_at");
    assert_eq!(
        custom_index.columns[0].sort_order,
        Some(dbsurveyor_core::models::SortOrder::Descending)
    );

    // Test custom table constraints and indexes
    assert!(custom_table.primary_key.is_some());
    assert!(!custom_table.constraints.is_empty());
    assert!(!custom_table.indexes.is_empty());

    // Find foreign key constraint
    let fk_constraint = custom_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::ForeignKey
            )
        })
        .expect("Foreign key constraint not found");
    assert_eq!(fk_constraint.columns, vec!["test_table_id"]);

    // Verify foreign key relationships are collected (Task 2.5)
    assert_eq!(custom_table.foreign_keys.len(), 1);
    let fk_relationship = &custom_table.foreign_keys[0];
    assert_eq!(fk_relationship.columns, vec!["test_table_id"]);
    assert_eq!(fk_relationship.referenced_table, "test_table");
    assert_eq!(fk_relationship.referenced_schema, None); // public schema normalized to None
    assert_eq!(fk_relationship.referenced_columns, vec!["id"]);
    assert_eq!(
        fk_relationship.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::Cascade)
    );
    assert!(fk_relationship.name.is_some()); // Should have a constraint name

    // Find composite index
    let composite_index = custom_table
        .indexes
        .iter()
        .find(|i| i.name.contains("composite"))
        .expect("Composite index not found");
    assert_eq!(composite_index.columns.len(), 2);
    assert!(
        composite_index
            .columns
            .iter()
            .any(|c| c.name == "test_table_id")
    );
    assert!(composite_index.columns.iter().any(|c| c.name == "active"));

    // Verify schema-level aggregation
    assert!(
        !schema.indexes.is_empty(),
        "Schema should have aggregated indexes"
    );
    assert!(
        !schema.constraints.is_empty(),
        "Schema should have aggregated constraints"
    );

    // Count total indexes and constraints
    let total_table_indexes: usize = schema.tables.iter().map(|t| t.indexes.len()).sum();
    let total_table_constraints: usize = schema.tables.iter().map(|t| t.constraints.len()).sum();

    assert_eq!(
        schema.indexes.len(),
        total_table_indexes,
        "Schema indexes should match sum of table indexes"
    );
    assert_eq!(
        schema.constraints.len(),
        total_table_constraints,
        "Schema constraints should match sum of table constraints"
    );

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

/// Test foreign key relationship mapping with comprehensive scenarios
#[tokio::test]
async fn test_postgres_foreign_key_relationships() -> Result<(), Box<dyn std::error::Error>> {
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

    // Clean up any existing test tables
    let _ = sqlx::query("DROP TABLE IF EXISTS fk_child_table CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS fk_parent_table CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS fk_self_ref_table CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS fk_multi_col_child CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS fk_multi_col_parent CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP SCHEMA IF EXISTS fk_test_schema CASCADE")
        .execute(&pool)
        .await;

    // Create test schema
    sqlx::query("CREATE SCHEMA fk_test_schema")
        .execute(&pool)
        .await?;

    // Create parent table
    sqlx::query(
        "CREATE TABLE fk_parent_table (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            code VARCHAR(10) UNIQUE NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Create child table with various foreign key scenarios
    sqlx::query(
        "CREATE TABLE fk_child_table (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER NOT NULL REFERENCES fk_parent_table(id) ON DELETE CASCADE ON UPDATE RESTRICT,
            parent_code VARCHAR(10) REFERENCES fk_parent_table(code) ON DELETE SET NULL ON UPDATE CASCADE,
            data TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;

    // Create self-referencing table
    sqlx::query(
        "CREATE TABLE fk_self_ref_table (
            id SERIAL PRIMARY KEY,
            parent_id INTEGER REFERENCES fk_self_ref_table(id) ON DELETE SET NULL,
            name VARCHAR(100) NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Create multi-column foreign key scenario
    sqlx::query(
        "CREATE TABLE fk_multi_col_parent (
            tenant_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            username VARCHAR(50) NOT NULL,
            PRIMARY KEY (tenant_id, user_id)
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE fk_multi_col_child (
            id SERIAL PRIMARY KEY,
            tenant_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            message TEXT,
            FOREIGN KEY (tenant_id, user_id) REFERENCES fk_multi_col_parent(tenant_id, user_id) ON DELETE CASCADE ON UPDATE NO ACTION
        )",
    )
    .execute(&pool)
    .await?;

    // Create cross-schema foreign key
    sqlx::query(
        "CREATE TABLE fk_test_schema.cross_schema_table (
            id SERIAL PRIMARY KEY,
            parent_ref INTEGER REFERENCES public.fk_parent_table(id) ON DELETE RESTRICT ON UPDATE SET DEFAULT,
            description TEXT
        )",
    )
    .execute(&pool)
    .await?;

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Find our test tables
    let parent_table = schema
        .tables
        .iter()
        .find(|t| t.name == "fk_parent_table")
        .expect("fk_parent_table not found");

    let child_table = schema
        .tables
        .iter()
        .find(|t| t.name == "fk_child_table")
        .expect("fk_child_table not found");

    let self_ref_table = schema
        .tables
        .iter()
        .find(|t| t.name == "fk_self_ref_table")
        .expect("fk_self_ref_table not found");

    let multi_col_parent = schema
        .tables
        .iter()
        .find(|t| t.name == "fk_multi_col_parent")
        .expect("fk_multi_col_parent not found");

    let multi_col_child = schema
        .tables
        .iter()
        .find(|t| t.name == "fk_multi_col_child")
        .expect("fk_multi_col_child not found");

    let cross_schema_table = schema
        .tables
        .iter()
        .find(|t| t.name == "cross_schema_table" && t.schema.as_deref() == Some("fk_test_schema"))
        .expect("cross_schema_table not found in fk_test_schema");

    // Test 1: Parent table should have no foreign keys
    assert!(parent_table.foreign_keys.is_empty());

    // Test 2: Child table should have two foreign keys
    assert_eq!(child_table.foreign_keys.len(), 2);

    // Find the foreign key to parent_id
    let parent_id_fk = child_table
        .foreign_keys
        .iter()
        .find(|fk| fk.columns == vec!["parent_id"])
        .expect("Foreign key on parent_id not found");

    assert_eq!(parent_id_fk.referenced_table, "fk_parent_table");
    assert_eq!(parent_id_fk.referenced_schema, None); // Should be None for public schema
    assert_eq!(parent_id_fk.referenced_columns, vec!["id"]);
    assert_eq!(
        parent_id_fk.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::Cascade)
    );
    assert_eq!(
        parent_id_fk.on_update,
        Some(dbsurveyor_core::models::ReferentialAction::Restrict)
    );

    // Find the foreign key to parent_code
    let parent_code_fk = child_table
        .foreign_keys
        .iter()
        .find(|fk| fk.columns == vec!["parent_code"])
        .expect("Foreign key on parent_code not found");

    assert_eq!(parent_code_fk.referenced_table, "fk_parent_table");
    assert_eq!(parent_code_fk.referenced_schema, None);
    assert_eq!(parent_code_fk.referenced_columns, vec!["code"]);
    assert_eq!(
        parent_code_fk.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::SetNull)
    );
    assert_eq!(
        parent_code_fk.on_update,
        Some(dbsurveyor_core::models::ReferentialAction::Cascade)
    );

    // Test 3: Self-referencing table
    assert_eq!(self_ref_table.foreign_keys.len(), 1);
    let self_ref_fk = &self_ref_table.foreign_keys[0];
    assert_eq!(self_ref_fk.columns, vec!["parent_id"]);
    assert_eq!(self_ref_fk.referenced_table, "fk_self_ref_table"); // Self-reference
    assert_eq!(self_ref_fk.referenced_schema, None);
    assert_eq!(self_ref_fk.referenced_columns, vec!["id"]);
    assert_eq!(
        self_ref_fk.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::SetNull)
    );

    // Test 4: Multi-column foreign key
    assert!(multi_col_parent.foreign_keys.is_empty()); // Parent has no FKs
    assert_eq!(multi_col_child.foreign_keys.len(), 1);

    let multi_col_fk = &multi_col_child.foreign_keys[0];
    assert_eq!(multi_col_fk.columns, vec!["tenant_id", "user_id"]);
    assert_eq!(multi_col_fk.referenced_table, "fk_multi_col_parent");
    assert_eq!(multi_col_fk.referenced_schema, None);
    assert_eq!(
        multi_col_fk.referenced_columns,
        vec!["tenant_id", "user_id"]
    );
    assert_eq!(
        multi_col_fk.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::Cascade)
    );
    assert_eq!(
        multi_col_fk.on_update,
        Some(dbsurveyor_core::models::ReferentialAction::NoAction)
    );

    // Test 5: Cross-schema foreign key
    assert_eq!(cross_schema_table.foreign_keys.len(), 1);
    let cross_schema_fk = &cross_schema_table.foreign_keys[0];
    assert_eq!(cross_schema_fk.columns, vec!["parent_ref"]);
    assert_eq!(cross_schema_fk.referenced_table, "fk_parent_table");
    assert_eq!(cross_schema_fk.referenced_schema, None); // Should be None for public schema
    assert_eq!(cross_schema_fk.referenced_columns, vec!["id"]);
    assert_eq!(
        cross_schema_fk.on_delete,
        Some(dbsurveyor_core::models::ReferentialAction::Restrict)
    );
    assert_eq!(
        cross_schema_fk.on_update,
        Some(dbsurveyor_core::models::ReferentialAction::SetDefault)
    );

    // Test 6: Verify foreign key constraint names are captured
    for table in [
        child_table,
        self_ref_table,
        multi_col_child,
        cross_schema_table,
    ] {
        for fk in &table.foreign_keys {
            assert!(fk.name.is_some(), "Foreign key should have a name");
            assert!(
                !fk.name.as_ref().unwrap().is_empty(),
                "Foreign key name should not be empty"
            );
        }
    }

    // Test 7: Verify no credentials in schema output
    let schema_json = serde_json::to_string(&schema)?;
    assert!(!schema_json.contains("postgres:postgres"));
    assert!(!schema_json.contains("password"));

    // Test 8: Verify referential action mapping
    use dbsurveyor_core::adapters::postgres::PostgresAdapter;
    assert_eq!(
        PostgresAdapter::map_referential_action("CASCADE"),
        Some(dbsurveyor_core::models::ReferentialAction::Cascade)
    );
    assert_eq!(
        PostgresAdapter::map_referential_action("SET NULL"),
        Some(dbsurveyor_core::models::ReferentialAction::SetNull)
    );
    assert_eq!(
        PostgresAdapter::map_referential_action("SET DEFAULT"),
        Some(dbsurveyor_core::models::ReferentialAction::SetDefault)
    );
    assert_eq!(
        PostgresAdapter::map_referential_action("RESTRICT"),
        Some(dbsurveyor_core::models::ReferentialAction::Restrict)
    );
    assert_eq!(
        PostgresAdapter::map_referential_action("NO ACTION"),
        Some(dbsurveyor_core::models::ReferentialAction::NoAction)
    );
    assert_eq!(PostgresAdapter::map_referential_action("UNKNOWN"), None);

    Ok(())
}

/// Test constraint and index collection with edge cases
#[tokio::test]
async fn test_postgres_constraints_and_indexes() -> Result<(), Box<dyn std::error::Error>> {
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

    // Create test table with various constraints and indexes
    let _ = sqlx::query("DROP TABLE IF EXISTS constraint_test CASCADE")
        .execute(&pool)
        .await;

    sqlx::query(
        "CREATE TABLE constraint_test (
            id SERIAL PRIMARY KEY,
            email VARCHAR(255) UNIQUE NOT NULL,
            age INTEGER CHECK (age >= 0 AND age <= 150),
            status VARCHAR(20) DEFAULT 'active',
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await?;

    // Create additional indexes
    sqlx::query("CREATE INDEX idx_constraint_test_status ON constraint_test (status)")
        .execute(&pool)
        .await?;

    sqlx::query(
        "CREATE INDEX idx_constraint_test_created_desc ON constraint_test (created_at DESC)",
    )
    .execute(&pool)
    .await?;

    // Create composite index
    sqlx::query(
        "CREATE INDEX idx_constraint_test_composite ON constraint_test (status, created_at)",
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
        .find(|t| t.name == "constraint_test")
        .expect("constraint_test table not found");

    // Verify constraints were collected
    assert!(
        !test_table.constraints.is_empty(),
        "Should have constraints"
    );

    // Check for primary key constraint
    let pk_constraint = test_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::PrimaryKey
            )
        })
        .expect("Primary key constraint not found");
    assert_eq!(pk_constraint.columns, vec!["id"]);

    // Check for unique constraint
    let unique_constraint = test_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::Unique
            )
        })
        .expect("Unique constraint not found");
    assert_eq!(unique_constraint.columns, vec!["email"]);

    // Check for check constraint
    let check_constraint = test_table
        .constraints
        .iter()
        .find(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::Check
            )
        })
        .expect("Check constraint not found");
    assert!(check_constraint.check_clause.is_some());

    // Verify indexes were collected
    assert!(!test_table.indexes.is_empty(), "Should have indexes");

    // Check for primary key index
    let pk_index = test_table
        .indexes
        .iter()
        .find(|i| i.is_primary)
        .expect("Primary key index not found");
    assert!(pk_index.is_unique);
    assert_eq!(pk_index.columns.len(), 1);
    assert_eq!(pk_index.columns[0].name, "id");

    // Check for descending index
    let desc_index = test_table
        .indexes
        .iter()
        .find(|i| i.name.contains("created_desc"))
        .expect("Descending index not found");
    assert_eq!(desc_index.columns.len(), 1);
    assert_eq!(desc_index.columns[0].name, "created_at");
    assert_eq!(
        desc_index.columns[0].sort_order,
        Some(dbsurveyor_core::models::SortOrder::Descending)
    );

    // Check for composite index
    let composite_index = test_table
        .indexes
        .iter()
        .find(|i| i.name.contains("composite"))
        .expect("Composite index not found");
    assert_eq!(composite_index.columns.len(), 2);
    assert!(composite_index.columns.iter().any(|c| c.name == "status"));
    assert!(
        composite_index
            .columns
            .iter()
            .any(|c| c.name == "created_at")
    );

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
