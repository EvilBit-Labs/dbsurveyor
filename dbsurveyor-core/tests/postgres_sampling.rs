//! PostgreSQL sampling and ordering strategy detection tests.
//!
//! This test suite covers:
//! - Ordering strategy detection for tables with different structures
//! - Primary key detection (simple and composite)
//! - Timestamp column detection
//! - Auto-increment/serial column detection
//! - Fallback to unordered for tables without reliable ordering
//! - ORDER BY clause generation

use dbsurveyor_core::{
    Result,
    adapters::postgres::PostgresAdapter,
    error::DbSurveyorError,
    models::{OrderingStrategy, SortDirection},
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

/// Test ordering strategy detection for tables with primary key
#[tokio::test]
async fn test_detect_ordering_strategy_primary_key() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with primary key
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_pk (id SERIAL PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_pk")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()]),
        "Expected PrimaryKey strategy with 'id' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with composite primary key
#[tokio::test]
async fn test_detect_ordering_strategy_composite_primary_key() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with composite primary key
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_composite_pk (
            tenant_id INTEGER,
            user_id INTEGER,
            name TEXT,
            PRIMARY KEY (tenant_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_composite_pk")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns }
            if columns == &vec!["tenant_id".to_string(), "user_id".to_string()]),
        "Expected PrimaryKey strategy with composite columns, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with timestamp column
#[tokio::test]
async fn test_detect_ordering_strategy_timestamp() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with timestamp column but no primary key
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_ts (
            name TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_ts")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::Timestamp { ref column, direction: SortDirection::Descending }
            if column == "created_at"),
        "Expected Timestamp strategy with 'created_at' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with updated_at timestamp
#[tokio::test]
async fn test_detect_ordering_strategy_timestamp_updated_at() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with only updated_at timestamp
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_updated (
            name TEXT,
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_updated")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::Timestamp { ref column, .. } if column == "updated_at"),
        "Expected Timestamp strategy with 'updated_at' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with auto-increment column
#[tokio::test]
async fn test_detect_ordering_strategy_auto_increment() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with serial column but no primary key constraint
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_serial (
            row_id SERIAL,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_serial")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "row_id"),
        "Expected AutoIncrement strategy with 'row_id' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection falls back to unordered for tables without reliable ordering
#[tokio::test]
async fn test_detect_ordering_strategy_unordered_fallback() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with no primary key, no timestamp, no serial
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_noorder (
            name TEXT,
            value INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_noorder")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::Unordered),
        "Expected Unordered strategy, got {:?}",
        strategy
    );

    Ok(())
}

/// Test that primary key takes priority over timestamp
#[tokio::test]
async fn test_detect_ordering_strategy_priority_pk_over_timestamp() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with both primary key and timestamp
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_pk_ts (
            id SERIAL PRIMARY KEY,
            name TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_pk_ts")
        .await?;

    // Primary key should take priority over timestamp
    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()]),
        "Expected PrimaryKey strategy (priority over timestamp), got {:?}",
        strategy
    );

    Ok(())
}

/// Test that timestamp takes priority over auto-increment (when no PK)
#[tokio::test]
async fn test_detect_ordering_strategy_priority_timestamp_over_auto() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with both serial column and timestamp, but no PK
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_ts_serial (
            row_id SERIAL,
            name TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_ts_serial")
        .await?;

    // Timestamp should take priority over auto-increment when there's no PK
    assert!(
        matches!(strategy, OrderingStrategy::Timestamp { ref column, .. } if column == "created_at"),
        "Expected Timestamp strategy (priority over auto-increment), got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection in non-public schema
#[tokio::test]
async fn test_detect_ordering_strategy_custom_schema() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create custom schema and table
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE SCHEMA IF NOT EXISTS custom_schema")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE custom_schema.test_custom (
            id SERIAL PRIMARY KEY,
            data TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("custom_schema", "test_custom")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()]),
        "Expected PrimaryKey strategy in custom schema, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection with IDENTITY column (PostgreSQL 10+)
#[tokio::test]
async fn test_detect_ordering_strategy_identity_column() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create test table with IDENTITY column (no PK constraint)
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_identity (
            row_id INTEGER GENERATED ALWAYS AS IDENTITY,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "test_identity")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "row_id"),
        "Expected AutoIncrement strategy for IDENTITY column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ORDER BY clause generation via adapter method
#[tokio::test]
async fn test_generate_order_by_clause() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Test primary key ordering
    let pk_strategy = OrderingStrategy::PrimaryKey {
        columns: vec!["id".to_string()],
    };
    let clause = adapter.generate_order_by(&pk_strategy, true);
    assert_eq!(clause, "ORDER BY \"id\" DESC");

    let clause = adapter.generate_order_by(&pk_strategy, false);
    assert_eq!(clause, "ORDER BY \"id\" ASC");

    // Test timestamp ordering
    let ts_strategy = OrderingStrategy::Timestamp {
        column: "created_at".to_string(),
        direction: SortDirection::Descending,
    };
    let clause = adapter.generate_order_by(&ts_strategy, true);
    assert_eq!(clause, "ORDER BY \"created_at\" DESC");

    // Test unordered
    let unordered = OrderingStrategy::Unordered;
    let clause = adapter.generate_order_by(&unordered, true);
    assert_eq!(clause, "ORDER BY RANDOM()");

    Ok(())
}

/// Test detection for table that doesn't exist returns error
#[tokio::test]
async fn test_detect_ordering_strategy_nonexistent_table() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("public", "nonexistent_table")
        .await?;

    // Non-existent table should return Unordered (no PK, no timestamps, etc.)
    assert!(
        matches!(strategy, OrderingStrategy::Unordered),
        "Expected Unordered for non-existent table, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy with various timestamp column naming patterns
#[tokio::test]
async fn test_detect_ordering_strategy_timestamp_patterns() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let pool = PgPool::connect(&database_url).await.unwrap();

    // Test various timestamp column naming patterns
    let test_cases = vec![
        ("test_ts_inserted", "inserted_at", "inserted_at"),
        ("test_ts_modified", "modified_at", "modified_at"),
        ("test_ts_date_created", "date_created", "date_created"),
    ];

    for (table_name, column_name, expected_column) in test_cases {
        let create_sql = format!(
            "CREATE TABLE {} ({} TIMESTAMP DEFAULT NOW(), data TEXT)",
            table_name, column_name
        );
        sqlx::query(&create_sql).execute(&pool).await.unwrap();

        let adapter = PostgresAdapter::new(&database_url).await?;
        let strategy = adapter
            .detect_ordering_strategy("public", table_name)
            .await?;

        assert!(
            matches!(strategy, OrderingStrategy::Timestamp { ref column, .. } if column == expected_column),
            "Expected Timestamp strategy with '{}' column for table '{}', got {:?}",
            expected_column,
            table_name,
            strategy
        );
    }

    pool.close().await;

    Ok(())
}

// =============================================================================
// Data Sampling Tests (Task 6.2)
// =============================================================================

use dbsurveyor_core::{SamplingConfig, SamplingStrategy};

/// Test basic data sampling with rate limiting
#[tokio::test]
async fn test_sample_table_with_rate_limit() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create and populate test table
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_sample (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    // Insert 100 rows
    for i in 0..100 {
        sqlx::query(&format!(
            "INSERT INTO test_sample (value) VALUES ('row{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Configure sampling with rate limiting
    let sampling_config = SamplingConfig::new()
        .with_sample_size(10)
        .with_throttle_ms(10); // 100 queries per second

    let sample = adapter
        .sample_table("public", "test_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 10, "Should have sampled 10 rows");
    assert_eq!(sample.sample_size, 10, "Sample size should match row count");
    // Note: pg_class.reltuples is an estimate and may not be updated immediately
    // after inserts. We just verify it's available (Some) - the actual count
    // may be stale until ANALYZE is run.
    assert!(
        sample.total_rows.is_some(),
        "Total rows estimate should be available"
    );
    assert!(
        matches!(
            sample.sampling_strategy,
            SamplingStrategy::MostRecent { .. }
        ),
        "Expected MostRecent strategy for table with primary key, got {:?}",
        sample.sampling_strategy
    );
    assert_eq!(sample.table_name, "test_sample");
    assert_eq!(sample.schema_name, Some("public".to_string()));

    Ok(())
}

/// Test sampling returns correct row data as JSON
#[tokio::test]
async fn test_sample_table_returns_json_rows() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create and populate test table with known data
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_json_sample (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            score INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO test_json_sample (name, score) VALUES ('Alice', 95)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_json_sample (name, score) VALUES ('Bob', 87)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_json_sample (name, score) VALUES ('Charlie', 92)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("public", "test_json_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 3, "Should have sampled all 3 rows");

    // Verify JSON structure - rows are ordered DESC by id so Charlie is first
    let first_row = &sample.rows[0];
    assert!(first_row.get("id").is_some(), "Row should have 'id' field");
    assert!(
        first_row.get("name").is_some(),
        "Row should have 'name' field"
    );
    assert!(
        first_row.get("score").is_some(),
        "Row should have 'score' field"
    );

    // First row (DESC order) should be Charlie (id=3)
    assert_eq!(first_row["id"].as_i64().unwrap(), 3);
    assert_eq!(first_row["name"].as_str().unwrap(), "Charlie");
    assert_eq!(first_row["score"].as_i64().unwrap(), 92);

    Ok(())
}

/// Test sampling with timestamp ordering
#[tokio::test]
async fn test_sample_table_with_timestamp_ordering() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create table with only timestamp (no primary key)
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_ts_sample (
            name TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert rows with slight delay to ensure different timestamps
    for i in 0..5 {
        sqlx::query(&format!(
            "INSERT INTO test_ts_sample (name) VALUES ('item{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(3);

    let sample = adapter
        .sample_table("public", "test_ts_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 3, "Should have sampled 3 rows");
    assert!(
        matches!(
            sample.sampling_strategy,
            SamplingStrategy::MostRecent { .. }
        ),
        "Expected MostRecent strategy for timestamp ordering"
    );

    // Verify timestamp columns are present
    let first_row = &sample.rows[0];
    assert!(
        first_row.get("name").is_some(),
        "Row should have 'name' field"
    );
    assert!(
        first_row.get("created_at").is_some(),
        "Row should have 'created_at' field"
    );

    Ok(())
}

/// Test sampling with unordered table (falls back to random)
#[tokio::test]
async fn test_sample_table_unordered() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create table with no reliable ordering
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_unordered_sample (
            name TEXT,
            value INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    for i in 0..10 {
        sqlx::query(&format!(
            "INSERT INTO test_unordered_sample (name, value) VALUES ('item{}', {})",
            i, i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("public", "test_unordered_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 5, "Should have sampled 5 rows");
    assert!(
        matches!(sample.sampling_strategy, SamplingStrategy::Random { .. }),
        "Expected Random strategy for unordered table, got {:?}",
        sample.sampling_strategy
    );
    assert!(
        !sample.warnings.is_empty(),
        "Should have warning about no reliable ordering"
    );

    Ok(())
}

/// Test sampling an empty table
#[tokio::test]
async fn test_sample_table_empty() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create empty table
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_empty_sample (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("public", "test_empty_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 0, "Should have 0 rows for empty table");
    assert_eq!(sample.sample_size, 0, "Sample size should be 0");

    Ok(())
}

/// Test sampling respects sample_size limit
#[tokio::test]
async fn test_sample_table_respects_limit() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create and populate test table with more rows than we'll sample
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_limit_sample (id SERIAL PRIMARY KEY, data TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    for i in 0..50 {
        sqlx::query(&format!(
            "INSERT INTO test_limit_sample (data) VALUES ('data{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Request only 5 samples
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("public", "test_limit_sample", &sampling_config)
        .await?;

    assert_eq!(
        sample.rows.len(),
        5,
        "Should have exactly 5 rows as requested"
    );
    assert_eq!(sample.sample_size, 5);
    // Note: pg_class.reltuples estimate may not be updated immediately after inserts
    assert!(
        sample.total_rows.is_some(),
        "Total rows estimate should be available"
    );

    Ok(())
}

/// Test sampling in custom schema
#[tokio::test]
async fn test_sample_table_custom_schema() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create custom schema and table
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE SCHEMA IF NOT EXISTS sample_schema")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE sample_schema.test_custom (
            id SERIAL PRIMARY KEY,
            info TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    for i in 0..10 {
        sqlx::query(&format!(
            "INSERT INTO sample_schema.test_custom (info) VALUES ('info{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("sample_schema", "test_custom", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 5);
    assert_eq!(sample.schema_name, Some("sample_schema".to_string()));
    assert_eq!(sample.table_name, "test_custom");

    Ok(())
}

/// Test rate limiting delay is applied (basic timing check)
#[tokio::test]
async fn test_sample_table_rate_limiting() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create simple table
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_rate_limit (id SERIAL PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_rate_limit (value) VALUES ('test')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Configure with 100ms throttle (10 queries per second max)
    let sampling_config = SamplingConfig::new()
        .with_sample_size(1)
        .with_throttle_ms(100);

    let start = std::time::Instant::now();
    let _sample = adapter
        .sample_table("public", "test_rate_limit", &sampling_config)
        .await?;
    let elapsed = start.elapsed();

    // Should have taken at least 100ms due to throttle
    assert!(
        elapsed >= Duration::from_millis(100),
        "Sampling should take at least 100ms due to rate limiting, took {:?}",
        elapsed
    );

    Ok(())
}
