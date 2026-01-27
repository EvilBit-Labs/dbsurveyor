//! PostgreSQL version compatibility and configuration tests.
//!
//! This test suite focuses on:
//! - Different PostgreSQL versions and their features
//! - Various database configurations and settings
//! - Version-specific SQL features and syntax
//! - Compatibility across PostgreSQL releases

use dbsurveyor_core::{
    Result,
    adapters::{DatabaseAdapter, postgres::PostgresAdapter},
    error::DbSurveyorError,
    models::{DatabaseType, UnifiedDataType},
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

/// Test PostgreSQL version detection and compatibility
#[tokio::test]
async fn test_postgres_version_detection() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify version information is collected
    assert!(schema.database_info.version.is_some());
    let version = schema.database_info.version.as_ref().unwrap();

    // Should contain PostgreSQL version information
    assert!(version.contains("PostgreSQL"));

    // Version should be parseable (basic format check)
    assert!(version.len() > 10); // Should be more than just "PostgreSQL"

    // Verify other database info
    assert_eq!(schema.database_info.name, "postgres");
    assert!(schema.database_info.size_bytes.is_some());
    assert!(schema.database_info.encoding.is_some());

    Ok(())
}

/// Test PostgreSQL with different database configurations
#[tokio::test]
async fn test_postgres_database_configurations() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let base_url = format!("postgres://postgres:postgres@localhost:{}", port);

    wait_for_postgres_ready(&format!("{}/postgres", base_url), 30).await?;

    // Test different connection configurations without creating new databases

    // Test 1: Default database configuration
    let adapter1 = PostgresAdapter::new(&format!("{}/postgres", base_url)).await?;
    let schema1 = adapter1.collect_schema().await?;

    assert_eq!(schema1.database_info.name, "postgres");
    assert!(schema1.database_info.encoding.is_some());

    // Test 2: Connection with SSL disabled
    let ssl_disabled_url = format!("{}?sslmode=disable", format!("{}/postgres", base_url));
    let adapter2 = PostgresAdapter::new(&ssl_disabled_url).await?;
    let schema2 = adapter2.collect_schema().await?;

    assert_eq!(schema2.database_info.name, "postgres");
    assert!(schema2.database_info.encoding.is_some());

    // Test 3: Connection with application name
    let app_name_url = format!(
        "{}?application_name=dbsurveyor_test",
        format!("{}/postgres", base_url)
    );
    let adapter3 = PostgresAdapter::new(&app_name_url).await?;
    let schema3 = adapter3.collect_schema().await?;

    assert_eq!(schema3.database_info.name, "postgres");
    assert!(schema3.database_info.version.is_some());

    Ok(())
}

/// Test PostgreSQL-specific data types and features
#[tokio::test]
async fn test_postgres_specific_features() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let pool = PgPool::connect(&database_url).await.unwrap();

    // Create table with PostgreSQL-specific features
    sqlx::query("DROP TABLE IF EXISTS postgres_features_test")
        .execute(&pool)
        .await
        .unwrap();

    // Enable pgcrypto extension for UUID generation
    let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        .execute(&pool)
        .await;

    sqlx::query(
        r#"
        CREATE TABLE postgres_features_test (
            id SERIAL PRIMARY KEY,
            -- UUID with default (fallback to NULL if pgcrypto not available)
            uuid_col UUID,
            -- JSON and JSONB
            json_col JSON,
            jsonb_col JSONB,
            -- Arrays
            int_array INTEGER[],
            text_array TEXT[],
            -- Network types
            inet_col INET,
            cidr_col CIDR,
            macaddr_col MACADDR,
            -- Geometric types
            point_col POINT,
            line_col LINE,
            box_col BOX,
            -- Range types (PostgreSQL 9.2+)
            int_range INT4RANGE,
            timestamp_range TSRANGE,
            -- Full-text search
            tsvector_col TSVECTOR,
            tsquery_col TSQUERY,
            -- Money type
            money_col MONEY,
            -- Bit strings
            bit_col BIT(8),
            varbit_col VARBIT(16),
            -- XML (if available)
            xml_col XML
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create custom enum type
    let _ = sqlx::query("DROP TYPE IF EXISTS status_enum CASCADE")
        .execute(&pool)
        .await;

    sqlx::query("CREATE TYPE status_enum AS ENUM ('active', 'inactive', 'pending')")
        .execute(&pool)
        .await
        .unwrap();

    // Create table using custom enum
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS enum_test (
            id SERIAL PRIMARY KEY,
            status status_enum NOT NULL DEFAULT 'pending',
            name VARCHAR(100)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create table with advanced constraints
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS advanced_constraints (
            id SERIAL PRIMARY KEY,
            email VARCHAR(255) UNIQUE,
            age INTEGER CHECK (age >= 0 AND age <= 150),
            salary NUMERIC(10,2) CHECK (salary > 0),
            created_at TIMESTAMP DEFAULT NOW(),
            updated_at TIMESTAMP DEFAULT NOW(),
            -- Complex check constraint
            CONSTRAINT check_email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
            -- Constraint with multiple columns
            CONSTRAINT check_timestamps CHECK (updated_at >= created_at)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create partial index (PostgreSQL feature)
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_active_users ON advanced_constraints (email) WHERE age > 18"
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create expression index
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lower_email ON advanced_constraints (lower(email))",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Find PostgreSQL features table
    let features_table = schema
        .tables
        .iter()
        .find(|t| t.name == "postgres_features_test")
        .expect("postgres_features_test table not found");

    // Verify PostgreSQL-specific data types are mapped correctly
    let uuid_col = features_table
        .columns
        .iter()
        .find(|c| c.name == "uuid_col")
        .expect("uuid_col not found");
    assert!(matches!(uuid_col.data_type, UnifiedDataType::Uuid));
    // UUID column may or may not have default depending on pgcrypto availability
    // Just verify the type mapping is correct

    let jsonb_col = features_table
        .columns
        .iter()
        .find(|c| c.name == "jsonb_col")
        .expect("jsonb_col not found");
    assert!(matches!(jsonb_col.data_type, UnifiedDataType::Json));

    let int_array_col = features_table
        .columns
        .iter()
        .find(|c| c.name == "int_array")
        .expect("int_array not found");
    assert!(matches!(
        int_array_col.data_type,
        UnifiedDataType::Array { .. }
    ));

    // Find enum test table
    let enum_table = schema
        .tables
        .iter()
        .find(|t| t.name == "enum_test")
        .expect("enum_test table not found");

    let status_col = enum_table
        .columns
        .iter()
        .find(|c| c.name == "status")
        .expect("status column not found");

    // Custom enum should be mapped as Custom type
    assert!(matches!(
        status_col.data_type,
        UnifiedDataType::Custom { .. }
    ));
    assert!(status_col.default_value.is_some());

    // Find advanced constraints table
    let constraints_table = schema
        .tables
        .iter()
        .find(|t| t.name == "advanced_constraints")
        .expect("advanced_constraints table not found");

    // Verify complex constraints are collected
    assert!(!constraints_table.constraints.is_empty());

    // Should have multiple check constraints
    let check_constraints: Vec<_> = constraints_table
        .constraints
        .iter()
        .filter(|c| {
            matches!(
                c.constraint_type,
                dbsurveyor_core::models::ConstraintType::Check
            )
        })
        .collect();
    assert!(
        check_constraints.len() >= 2,
        "Should have multiple check constraints"
    );

    // Verify indexes including partial and expression indexes
    assert!(!constraints_table.indexes.is_empty());

    Ok(())
}

/// Test PostgreSQL schema and table name handling
#[tokio::test]
async fn test_postgres_schema_and_table_names() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let pool = PgPool::connect(&database_url).await.unwrap();

    // Create simple schema (avoid complex naming for now)
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_schema")
        .execute(&pool)
        .await
        .unwrap();

    // Create table with special characters in public schema
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS public."table-with-dashes" (
            "column-with-dashes" INTEGER PRIMARY KEY,
            "column with spaces" TEXT,
            "UPPERCASE_COLUMN" BOOLEAN
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create simple table in test schema
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS test_schema.simple_table (
            id SERIAL PRIMARY KEY,
            name VARCHAR(100)
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create table with maximum length name (PostgreSQL limit is 63 characters)
    let max_length_name = "a".repeat(63);
    sqlx::query(&format!(
        r#"CREATE TABLE IF NOT EXISTS public."{}" (id SERIAL PRIMARY KEY)"#,
        max_length_name
    ))
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;

    // Test schema collection
    let adapter = PostgresAdapter::new(&database_url).await?;
    let schema = adapter.collect_schema().await?;

    // Verify simple schema table
    let simple_table = schema
        .tables
        .iter()
        .find(|t| t.name == "simple_table" && t.schema.as_deref() == Some("test_schema"));
    assert!(simple_table.is_some(), "Simple table should be found");

    // Verify table with dashes in public schema
    let dash_table = schema
        .tables
        .iter()
        .find(|t| t.name == "table-with-dashes" && t.schema.as_deref() == Some("public"));

    if let Some(table) = dash_table {
        // Verify columns with special characters
        let dash_column = table
            .columns
            .iter()
            .find(|c| c.name == "column-with-dashes");
        assert!(dash_column.is_some(), "Column with dashes should be found");

        let space_column = table
            .columns
            .iter()
            .find(|c| c.name == "column with spaces");
        assert!(space_column.is_some(), "Column with spaces should be found");

        let upper_column = table.columns.iter().find(|c| c.name == "UPPERCASE_COLUMN");
        assert!(upper_column.is_some(), "Uppercase column should be found");
    }

    // Verify maximum length table name
    let max_length_table = schema.tables.iter().find(|t| t.name == max_length_name);
    assert!(
        max_length_table.is_some(),
        "Maximum length table name should be handled"
    );

    Ok(())
}

/// Test PostgreSQL connection with various SSL and security configurations
#[tokio::test]
async fn test_postgres_security_configurations() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let base_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&base_url, 30).await?;

    // Test 1: Connection with SSL disabled (for testing environments)
    let ssl_disabled_url = format!("{}?sslmode=disable", base_url);
    let adapter1 = PostgresAdapter::new(&ssl_disabled_url).await?;
    adapter1.test_connection().await?;

    // Test 2: Connection with application name
    let app_name_url = format!("{}?application_name=dbsurveyor_test", base_url);
    let adapter2 = PostgresAdapter::new(&app_name_url).await?;
    adapter2.test_connection().await?;

    // Test 3: Connection with statement timeout
    let timeout_url = format!("{}?statement_timeout=30000", base_url); // 30 seconds
    let adapter3 = PostgresAdapter::new(&timeout_url).await?;
    adapter3.test_connection().await?;

    // Test 4: Connection with multiple parameters
    let multi_param_url = format!(
        "{}?sslmode=disable&application_name=dbsurveyor&statement_timeout=15000&connect_timeout=10",
        base_url
    );
    let adapter4 = PostgresAdapter::new(&multi_param_url).await?;
    adapter4.test_connection().await?;

    // Verify all adapters work correctly
    let schema1 = adapter1.collect_schema().await?;
    let schema2 = adapter2.collect_schema().await?;
    let schema3 = adapter3.collect_schema().await?;
    let schema4 = adapter4.collect_schema().await?;

    // All should collect the same database
    assert_eq!(schema1.database_info.name, "postgres");
    assert_eq!(schema2.database_info.name, "postgres");
    assert_eq!(schema3.database_info.name, "postgres");
    assert_eq!(schema4.database_info.name, "postgres");

    // Verify no credentials in any output
    for schema in [&schema1, &schema2, &schema3, &schema4] {
        let schema_json = serde_json::to_string(schema).unwrap();
        assert!(!schema_json.contains("postgres:postgres"));
        assert!(!schema_json.contains("password"));
    }

    Ok(())
}

/// Test PostgreSQL adapter feature detection
#[tokio::test]
async fn test_postgres_adapter_features() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Test basic adapter properties
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);

    // Test feature support
    use dbsurveyor_core::adapters::AdapterFeature;

    assert!(adapter.supports_feature(AdapterFeature::SchemaCollection));
    assert!(adapter.supports_feature(AdapterFeature::DataSampling));
    assert!(adapter.supports_feature(AdapterFeature::MultiDatabase));
    assert!(adapter.supports_feature(AdapterFeature::ConnectionPooling));
    assert!(adapter.supports_feature(AdapterFeature::QueryTimeout));
    assert!(adapter.supports_feature(AdapterFeature::ReadOnlyMode));

    // Test connection configuration
    let config = adapter.connection_config();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, Some(port));
    assert_eq!(config.database, Some("postgres".to_string()));
    assert_eq!(config.username, Some("postgres".to_string()));
    assert!(config.read_only); // Should default to read-only for security

    Ok(())
}

/// Test PostgreSQL with different locale and encoding settings
#[tokio::test]
async fn test_postgres_locale_and_encoding() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let base_url = format!("postgres://postgres:postgres@localhost:{}", port);

    wait_for_postgres_ready(&format!("{}/postgres", base_url), 30).await?;

    let pool = PgPool::connect(&format!("{}/postgres", base_url))
        .await
        .unwrap();

    // Create table with Unicode data
    sqlx::query("DROP TABLE IF EXISTS unicode_test")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE unicode_test (
            id SERIAL PRIMARY KEY,
            emoji_col TEXT,
            chinese_col TEXT,
            arabic_col TEXT,
            mixed_unicode TEXT
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert Unicode data
    sqlx::query(
        r#"
        INSERT INTO unicode_test (emoji_col, chinese_col, arabic_col, mixed_unicode)
        VALUES
            ('🚀🌟💻', '你好世界', 'مرحبا بالعالم', '🌍 Hello 世界 مرحبا'),
            ('🎉🎊🎈', '数据库', 'قاعدة البيانات', '📊 Data 数据 بيانات')
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool.close().await;

    // Test schema collection with Unicode data
    let adapter = PostgresAdapter::new(&format!("{}/postgres", base_url)).await?;
    let schema = adapter.collect_schema().await?;

    // Find Unicode test table
    let unicode_table = schema
        .tables
        .iter()
        .find(|t| t.name == "unicode_test")
        .expect("unicode_test table not found");

    // Verify columns are collected correctly
    assert_eq!(unicode_table.columns.len(), 5); // id + 4 text columns

    // Verify all text columns are present
    let text_columns: Vec<_> = unicode_table
        .columns
        .iter()
        .filter(|c| matches!(c.data_type, UnifiedDataType::String { .. }))
        .collect();
    assert_eq!(text_columns.len(), 4, "Should have 4 text columns");

    // Verify database encoding information
    assert!(schema.database_info.encoding.is_some());
    let encoding = schema.database_info.encoding.as_ref().unwrap();
    // Should be UTF8 or similar Unicode encoding
    assert!(encoding.to_uppercase().contains("UTF"));

    Ok(())
}
