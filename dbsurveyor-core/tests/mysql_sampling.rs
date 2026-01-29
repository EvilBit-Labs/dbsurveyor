//! MySQL sampling and ordering strategy detection tests.
//!
//! This test suite covers:
//! - Ordering strategy detection for tables with different structures
//! - Primary key detection (simple and composite)
//! - Timestamp column detection
//! - Auto-increment column detection
//! - Fallback to unordered for tables without reliable ordering
//! - ORDER BY clause generation

#![cfg(feature = "mysql")]

use dbsurveyor_core::{
    Result,
    adapters::mysql::MySqlAdapter,
    error::DbSurveyorError,
    models::{OrderingStrategy, SortDirection},
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

/// Test ordering strategy detection for tables with primary key
#[tokio::test]
async fn test_detect_ordering_strategy_primary_key() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with primary key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_pk (id INT AUTO_INCREMENT PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter.detect_ordering_strategy("test", "test_pk").await?;

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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with composite primary key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_composite_pk (
            tenant_id INT,
            user_id INT,
            name TEXT,
            PRIMARY KEY (tenant_id, user_id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "test_composite_pk")
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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with timestamp column but no primary key
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_ts (
            name TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter.detect_ordering_strategy("test", "test_ts").await?;

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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with only updated_at timestamp
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_updated (
            name TEXT,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "test_updated")
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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with auto_increment column but no primary key constraint
    // Note: In MySQL, AUTO_INCREMENT requires a key, so we use a UNIQUE key instead of PK
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_auto (
            row_id INT AUTO_INCREMENT,
            name TEXT,
            UNIQUE KEY (row_id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "test_auto")
        .await?;

    // Since AUTO_INCREMENT in MySQL requires a key, this might be detected as PrimaryKey
    // or AutoIncrement depending on the detection logic
    assert!(
        matches!(strategy, OrderingStrategy::AutoIncrement { ref column } if column == "row_id")
            || matches!(strategy, OrderingStrategy::PrimaryKey { .. }),
        "Expected AutoIncrement or PrimaryKey strategy with 'row_id' column, got {:?}",
        strategy
    );

    Ok(())
}

/// Test ordering strategy detection falls back to unordered for tables without reliable ordering
#[tokio::test]
async fn test_detect_ordering_strategy_unordered_fallback() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with no primary key, no timestamp, no auto_increment
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_noorder (
            name TEXT,
            value INT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "test_noorder")
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
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create test table with both primary key and timestamp
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_pk_ts (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "test_pk_ts")
        .await?;

    // Primary key should take priority over timestamp
    assert!(
        matches!(strategy, OrderingStrategy::PrimaryKey { ref columns } if columns == &vec!["id".to_string()]),
        "Expected PrimaryKey strategy (priority over timestamp), got {:?}",
        strategy
    );

    Ok(())
}

/// Test ORDER BY clause generation
#[tokio::test]
async fn test_generate_order_by_clause() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    let adapter = MySqlAdapter::new(&database_url).await?;

    // Test primary key ordering
    let pk_strategy = OrderingStrategy::PrimaryKey {
        columns: vec!["id".to_string()],
    };
    let clause = adapter.generate_order_by(&pk_strategy, true);
    assert_eq!(clause, "ORDER BY `id` DESC");

    let clause = adapter.generate_order_by(&pk_strategy, false);
    assert_eq!(clause, "ORDER BY `id` ASC");

    // Test timestamp ordering
    let ts_strategy = OrderingStrategy::Timestamp {
        column: "created_at".to_string(),
        direction: SortDirection::Descending,
    };
    let clause = adapter.generate_order_by(&ts_strategy, true);
    assert_eq!(clause, "ORDER BY `created_at` DESC");

    // Test unordered
    let unordered = OrderingStrategy::Unordered;
    let clause = adapter.generate_order_by(&unordered, true);
    assert_eq!(clause, "ORDER BY RAND()");

    Ok(())
}

/// Test detection for table that doesn't exist returns unordered
#[tokio::test]
async fn test_detect_ordering_strategy_nonexistent_table() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let strategy = adapter
        .detect_ordering_strategy("test", "nonexistent_table")
        .await?;

    // Non-existent table should return Unordered (no PK, no timestamps, etc.)
    assert!(
        matches!(strategy, OrderingStrategy::Unordered),
        "Expected Unordered for non-existent table, got {:?}",
        strategy
    );

    Ok(())
}

// =============================================================================
// Data Sampling Tests
// =============================================================================

use dbsurveyor_core::{SamplingConfig, SamplingStrategy};

/// Test basic data sampling with rate limiting
#[tokio::test]
async fn test_sample_table_with_rate_limit() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create and populate test table
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_sample (id INT AUTO_INCREMENT PRIMARY KEY, value TEXT)")
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

    let adapter = MySqlAdapter::new(&database_url).await?;

    // Configure sampling with rate limiting
    let sampling_config = SamplingConfig::new()
        .with_sample_size(10)
        .with_throttle_ms(10); // 100 queries per second

    let sample = adapter
        .sample_table("test", "test_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 10, "Should have sampled 10 rows");
    assert_eq!(sample.sample_size, 10, "Sample size should match row count");
    assert!(
        sample.total_rows.is_some(),
        "Total rows should be available"
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
    assert_eq!(sample.schema_name, Some("test".to_string()));

    Ok(())
}

/// Test sampling returns correct row data as JSON
#[tokio::test]
async fn test_sample_table_returns_json_rows() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create and populate test table with known data
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_json_sample (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            score INT
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

    let adapter = MySqlAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("test", "test_json_sample", &sampling_config)
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

/// Test sampling with unordered table (falls back to random)
#[tokio::test]
async fn test_sample_table_unordered() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create table with no reliable ordering
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE test_unordered_sample (
            name TEXT,
            value INT
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

    let adapter = MySqlAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("test", "test_unordered_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 5, "Should have sampled 5 rows");
    assert!(
        !sample.warnings.is_empty(),
        "Should have warning about no reliable ordering"
    );

    Ok(())
}

/// Test sampling an empty table
#[tokio::test]
async fn test_sample_table_empty() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create empty table
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_empty_sample (id INT AUTO_INCREMENT PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;
    let sampling_config = SamplingConfig::new().with_sample_size(10);

    let sample = adapter
        .sample_table("test", "test_empty_sample", &sampling_config)
        .await?;

    assert_eq!(sample.rows.len(), 0, "Should have 0 rows for empty table");
    assert_eq!(sample.sample_size, 0, "Sample size should be 0");

    Ok(())
}

/// Test sampling respects sample_size limit
#[tokio::test]
async fn test_sample_table_respects_limit() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create and populate test table with more rows than we'll sample
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_limit_sample (id INT AUTO_INCREMENT PRIMARY KEY, data TEXT)")
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

    let adapter = MySqlAdapter::new(&database_url).await?;

    // Request only 5 samples
    let sampling_config = SamplingConfig::new().with_sample_size(5);

    let sample = adapter
        .sample_table("test", "test_limit_sample", &sampling_config)
        .await?;

    assert_eq!(
        sample.rows.len(),
        5,
        "Should have exactly 5 rows as requested"
    );
    assert_eq!(sample.sample_size, 5);
    assert!(
        sample.total_rows.is_some(),
        "Total rows should be available"
    );

    Ok(())
}

/// Test rate limiting delay is applied (basic timing check)
#[tokio::test]
async fn test_sample_table_rate_limiting() -> Result<()> {
    let mysql = Mysql::default().start().await.unwrap();
    let port = mysql.get_host_port_ipv4(3306).await.unwrap();
    let database_url = format!("mysql://root@localhost:{}/test", port);

    wait_for_mysql_ready(&database_url, 30).await?;

    // Create simple table
    let pool = MySqlPool::connect(&database_url).await.unwrap();
    sqlx::query("CREATE TABLE test_rate_limit (id INT AUTO_INCREMENT PRIMARY KEY, value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO test_rate_limit (value) VALUES ('test')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let adapter = MySqlAdapter::new(&database_url).await?;

    // Configure with 100ms throttle (10 queries per second max)
    let sampling_config = SamplingConfig::new()
        .with_sample_size(1)
        .with_throttle_ms(100);

    let start = std::time::Instant::now();
    let _sample = adapter
        .sample_table("test", "test_rate_limit", &sampling_config)
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
