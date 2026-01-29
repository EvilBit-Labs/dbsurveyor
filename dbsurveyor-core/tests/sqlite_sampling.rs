//! SQLite sampling and ordering strategy detection tests.
//!
//! This test suite covers:
//! - Ordering strategy detection for tables with different structures
//! - Primary key detection (simple and composite)
//! - Timestamp column detection
//! - Auto-increment column detection
//! - ROWID fallback detection
//! - Fallback to unordered for tables without reliable ordering
//! - ORDER BY clause generation

#![cfg(feature = "sqlite")]

use dbsurveyor_core::{
    Result, SamplingConfig, SamplingStrategy,
    adapters::{ConnectionConfig, sqlite::SqliteAdapter},
    models::{OrderingStrategy, SortDirection},
};
use sqlx::SqlitePool;

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
// Ordering Strategy Detection Tests
// =============================================================================

/// Test ordering strategy detection for tables with primary key
#[tokio::test]
async fn test_detect_ordering_strategy_primary_key() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE test_pk (id INTEGER PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_pk").await?;

    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()])
            || matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "id"),
        "Expected PrimaryKey or AutoIncrement strategy with 'id' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with composite primary key
#[tokio::test]
async fn test_detect_ordering_strategy_composite_primary_key() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

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

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter
        .detect_ordering_strategy("test_composite_pk")
        .await?;

    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns }
            if columns.contains(&"tenant_id".to_string()) && columns.contains(&"user_id".to_string())),
        "Expected PrimaryKey strategy with composite columns, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with timestamp column
#[tokio::test]
async fn test_detect_ordering_strategy_timestamp() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Create table with timestamp column but no primary key
    sqlx::query(
        "CREATE TABLE test_ts (
            name TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_ts").await?;

    // Could be timestamp or rowid
    assert!(
        matches!(strategy, OrderingStrategy::Timestamp { ref column, direction: SortDirection::Descending }
            if column == "created_at")
            || matches!(strategy, OrderingStrategy::SystemRowId { .. }),
        "Expected Timestamp or SystemRowId strategy, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for tables with updated_at timestamp
#[tokio::test]
async fn test_detect_ordering_strategy_timestamp_updated_at() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE test_updated (
            name TEXT,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_updated").await?;

    // Could be timestamp or rowid
    assert!(
        matches!(strategy, OrderingStrategy::Timestamp { ref column, .. } if column == "updated_at")
            || matches!(strategy, OrderingStrategy::SystemRowId { .. }),
        "Expected Timestamp or SystemRowId strategy, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection for INTEGER PRIMARY KEY (auto-increment in SQLite)
#[tokio::test]
async fn test_detect_ordering_strategy_auto_increment() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // INTEGER PRIMARY KEY is auto-increment in SQLite
    sqlx::query(
        "CREATE TABLE test_auto (
            row_id INTEGER PRIMARY KEY,
            name TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_auto").await?;

    // Should be detected as AutoIncrement or PrimaryKey
    assert!(
        matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "row_id")
            || matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns.contains(&"row_id".to_string())),
        "Expected AutoIncrement or PrimaryKey strategy with 'row_id' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection falls back to ROWID for regular tables
#[tokio::test]
async fn test_detect_ordering_strategy_rowid_fallback() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Create table without PK, timestamp, or auto-increment
    // But regular tables have ROWID
    sqlx::query(
        "CREATE TABLE test_rowid (
            name TEXT,
            value INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_rowid").await?;

    assert!(
        matches!(strategy, OrderingStrategy::SystemRowId { ref column } if column == "rowid"),
        "Expected SystemRowId strategy with 'rowid', got {:?}",
        strategy
    );

    Ok(())
}

/// Test that primary key takes priority over timestamp
#[tokio::test]
async fn test_detect_ordering_strategy_priority_pk_over_timestamp() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE test_pk_ts (
            id INTEGER PRIMARY KEY,
            name TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let strategy = adapter.detect_ordering_strategy("test_pk_ts").await?;

    // Primary key should take priority
    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()])
            || matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "id"),
        "Expected PrimaryKey or AutoIncrement strategy (priority over timestamp), got {:?}",
        strategy
    );

    Ok(())
}

/// Test ORDER BY clause generation
#[tokio::test]
async fn test_generate_order_by_clause() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    let adapter = create_adapter_with_pool(pool).await;

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

    // Test ROWID ordering
    let rowid_strategy = OrderingStrategy::SystemRowId {
        column: "rowid".to_string(),
    };
    let clause = adapter.generate_order_by(&rowid_strategy, true);
    assert_eq!(clause, "ORDER BY rowid DESC");

    // Test unordered
    let unordered = OrderingStrategy::Unordered;
    let clause = adapter.generate_order_by(&unordered, true);
    assert_eq!(clause, "ORDER BY RANDOM()");

    Ok(())
}

// =============================================================================
// Data Sampling Tests
// =============================================================================

/// Test basic data sampling with rate limiting
#[tokio::test]
async fn test_sample_table_with_rate_limit() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE test_sample (id INTEGER PRIMARY KEY, value TEXT)")
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

    let adapter = create_adapter_with_pool(pool).await;

    // Configure sampling with rate limiting
    let sampling_config = SamplingConfig::new()
        .with_sample_size(10)
        .with_throttle_ms(10);

    let sample = adapter
        .sample_table("test_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 10, "Should have sampled 10 rows");
    assert_eq!(sample.sample_size, 10, "Sample size should match row count");
    assert!(
        sample.total_rows.is_some(),
        "Total rows should be available"
    );
    assert_eq!(sample.total_rows, Some(100));
    assert!(
        matches!(
            sample.sampling_strategy,
            SamplingStrategy::MostRecent { .. }
        ),
        "Expected MostRecent strategy, got {:?}",
        sample.sampling_strategy
    );
    assert_eq!(sample.table_name, "test_sample");

    Ok(())
}

/// Test sampling returns correct row data as JSON
#[tokio::test]
async fn test_sample_table_returns_json_rows() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE test_json_sample (
            id INTEGER PRIMARY KEY,
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

    let adapter = create_adapter_with_pool(pool).await;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("test_json_sample", &sampling_config)
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

/// Test sampling an empty table
#[tokio::test]
async fn test_sample_table_empty() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE test_empty_sample (id INTEGER PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("test_empty_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 0, "Should have 0 rows for empty table");
    assert_eq!(sample.sample_size, 0, "Sample size should be 0");
    assert_eq!(sample.total_rows, Some(0));

    Ok(())
}

/// Test sampling respects sample_size limit
#[tokio::test]
async fn test_sample_table_respects_limit() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE test_limit_sample (id INTEGER PRIMARY KEY, data TEXT)")
        .execute(&pool)
        .await
        .unwrap();

    // Insert 50 rows
    for i in 0..50 {
        sqlx::query(&format!(
            "INSERT INTO test_limit_sample (data) VALUES ('data{}')",
            i
        ))
        .execute(&pool)
        .await
        .unwrap();
    }

    let adapter = create_adapter_with_pool(pool).await;

    // Request only 5 samples
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("test_limit_sample", &sampling_config)
        .await?;

    assert_eq!(
        sample.rows.len(),
        5,
        "Should have exactly 5 rows as requested"
    );
    assert_eq!(sample.sample_size, 5);
    assert_eq!(sample.total_rows, Some(50));

    Ok(())
}

/// Test rate limiting delay is applied (basic timing check)
#[tokio::test]
async fn test_sample_table_rate_limiting() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query("CREATE TABLE test_rate_limit (id INTEGER PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_rate_limit (value) VALUES ('test')")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;

    // Configure with 100ms throttle
    let sampling_config = SamplingConfig::new()
        .with_sample_size(1)
        .with_throttle_ms(100);

    let start = std::time::Instant::now();
    let _sample = adapter
        .sample_table("test_rate_limit", &sampling_config)
        .await?;
    let elapsed = start.elapsed();

    // Should have taken at least 100ms due to throttle
    assert!(
        elapsed >= std::time::Duration::from_millis(100),
        "Sampling should take at least 100ms due to rate limiting, took {:?}",
        elapsed
    );

    Ok(())
}

/// Test sampling with various data types
#[tokio::test]
async fn test_sample_table_data_types() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE test_types (
            id INTEGER PRIMARY KEY,
            int_val INTEGER,
            real_val REAL,
            text_val TEXT,
            blob_val BLOB
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO test_types (int_val, real_val, text_val, blob_val)
         VALUES (42, 3.14, 'hello', X'DEADBEEF')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter.sample_table("test_types", &sampling_config).await?;

    assert_eq!(sample.rows.len(), 1);
    let row = &sample.rows[0];

    assert_eq!(row["int_val"].as_i64().unwrap(), 42);
    // REAL values - using a test value that isn't close to PI
    let real_val = row["real_val"].as_f64().unwrap();
    #[allow(clippy::approx_constant)]
    let expected = 3.14;
    assert!((real_val - expected).abs() < 0.001);
    assert_eq!(row["text_val"].as_str().unwrap(), "hello");
    // BLOB should be base64 encoded
    let blob_val = row["blob_val"].as_str().unwrap();
    assert!(blob_val.starts_with("base64:"));

    Ok(())
}

/// Test sampling with NULL values
#[tokio::test]
async fn test_sample_table_null_values() -> Result<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE test_nulls (
            id INTEGER PRIMARY KEY,
            nullable_text TEXT,
            nullable_int INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO test_nulls (nullable_text, nullable_int) VALUES (NULL, NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = create_adapter_with_pool(pool).await;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter.sample_table("test_nulls", &sampling_config).await?;

    assert_eq!(sample.rows.len(), 1);
    let row = &sample.rows[0];

    assert!(row["nullable_text"].is_null());
    assert!(row["nullable_int"].is_null());

    Ok(())
}
