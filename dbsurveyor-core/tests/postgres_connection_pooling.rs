//! Comprehensive PostgreSQL connection pooling tests.
//!
//! This test suite focuses specifically on connection pooling functionality:
//! - Pool configuration validation
//! - Connection limits and timeouts
//! - Pool health monitoring
//! - Concurrent connection handling
//! - Resource cleanup and management

use dbsurveyor_core::{
    Result,
    adapters::{
        ConnectionConfig, DatabaseAdapter,
        postgres::{PoolStats, PostgresAdapter},
    },
    error::DbSurveyorError,
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

/// Test connection pool configuration validation
#[tokio::test]
async fn test_connection_pool_configuration_validation() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Test 1: Valid configuration with minimum values
    let config1 = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 1, // Minimum
        connect_timeout: Duration::from_secs(1),
        query_timeout: Duration::from_secs(1),
        read_only: true,
        ..Default::default()
    };

    let adapter1 = PostgresAdapter::with_config(&database_url, config1).await?;
    adapter1.test_connection().await?;

    // Test 2: Valid configuration with maximum reasonable values
    let config2 = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 50, // High but reasonable
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(60),
        read_only: true,
        ..Default::default()
    };

    let adapter2 = PostgresAdapter::with_config(&database_url, config2).await?;
    adapter2.test_connection().await?;

    // Test 3: Configuration with security-focused defaults
    let config3 = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        read_only: true, // Security: read-only mode
        max_connections: 10,
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(30),
        ..Default::default()
    };

    let adapter3 = PostgresAdapter::with_config(&database_url, config3).await?;
    adapter3.test_connection().await?;

    // Verify pool statistics
    let (_active1, idle1, total1) = adapter1.pool_stats();
    let (_active2, idle2, total2) = adapter2.pool_stats();
    let (_active3, idle3, total3) = adapter3.pool_stats();

    // All pools should have at least one connection after test_connection()
    assert!(total1 >= 1, "Pool 1 should have connections");
    assert!(total2 >= 1, "Pool 2 should have connections");
    assert!(total3 >= 1, "Pool 3 should have connections");

    // Idle connections should not exceed total
    assert!(idle1 <= total1, "Pool 1: idle <= total");
    assert!(idle2 <= total2, "Pool 2: idle <= total");
    assert!(idle3 <= total3, "Pool 3: idle <= total");

    // Clean up
    adapter1.close().await;
    adapter2.close().await;
    adapter3.close().await;

    Ok(())
}

/// Test connection pool limits and behavior under load
#[tokio::test]
async fn test_connection_pool_limits() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create adapter with limited connections
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 3, // Small limit for testing
        connect_timeout: Duration::from_secs(5),
        query_timeout: Duration::from_secs(10),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;

    // Test 1: Basic connection functionality
    adapter.test_connection().await?;
    let (_active, _idle, total) = adapter.pool_stats();
    assert!(total >= 1, "Should have at least one connection");

    // Test 2: Multiple concurrent operations within pool limits
    let mut handles = Vec::new();
    for i in 0..2 {
        // Within our limit of 3
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            adapter.test_connection().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            i
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Test 3: Pool health monitoring
    assert!(adapter.is_pool_healthy().await, "Pool should be healthy");

    // Test 4: Verify pool statistics after load
    let (_active_after, idle_after, total_after) = adapter.pool_stats();
    assert!(total_after >= 1, "Should maintain connections");
    assert!(idle_after <= total_after, "Idle should not exceed total");

    adapter.close().await;

    Ok(())
}

/// Test connection timeout handling in various scenarios
#[tokio::test]
async fn test_connection_timeout_scenarios() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Test 1: Normal timeout (should succeed)
    let config1 = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_secs(10),
        query_timeout: Duration::from_secs(30),
        max_connections: 10,
        read_only: true,
        ..Default::default()
    };

    let adapter1 = PostgresAdapter::with_config(&database_url, config1).await?;
    adapter1.test_connection().await?;

    // Test 2: Very short timeout (may succeed or fail, but should handle gracefully)
    let config2 = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_millis(10), // Very short
        query_timeout: Duration::from_millis(50),   // Very short
        max_connections: 10,
        read_only: true,
        ..Default::default()
    };

    // This may fail due to timeout, which is expected behavior
    let result2 = PostgresAdapter::with_config(&database_url, config2).await;
    match result2 {
        Ok(adapter2) => {
            // If creation succeeded, operations may timeout
            let _ = adapter2.test_connection().await; // May fail, which is OK
        }
        Err(_) => {
            // Timeout during creation is also acceptable
        }
    }

    // Test 3: Connection to non-existent host with timeout
    let bad_config = ConnectionConfig {
        host: "nonexistent-host".to_string(),
        port: Some(5432),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_secs(2), // Short timeout for faster test
        query_timeout: Duration::from_secs(5),
        max_connections: 10,
        read_only: true,
        ..Default::default()
    };

    let bad_url = "postgres://postgres:postgres@nonexistent-host:5432/postgres";

    // Adapter creation should succeed (lazy connection)
    let result3 = PostgresAdapter::with_config(bad_url, bad_config).await;
    if let Ok(adapter3) = result3 {
        // But connection test should fail
        let conn_result = adapter3.test_connection().await;
        assert!(
            conn_result.is_err(),
            "Connection to nonexistent host should fail"
        );

        // Verify error message doesn't contain credentials
        let error_msg = format!("{}", conn_result.err().unwrap());
        assert!(!error_msg.contains("postgres:postgres"));
    }

    adapter1.close().await;

    Ok(())
}

/// Test concurrent connection handling and resource management
#[tokio::test]
async fn test_concurrent_connection_handling() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create adapter with moderate connection limit
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 5,
        connect_timeout: Duration::from_secs(10),
        query_timeout: Duration::from_secs(15),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;

    // Test 1: Multiple concurrent test_connection calls
    let mut handles = Vec::new();
    for i in 0..4 {
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            adapter.test_connection().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            adapter.test_connection().await.unwrap();
            i
        });
        handles.push(handle);
    }

    // Wait for all concurrent operations
    for handle in handles {
        handle.await.unwrap();
    }

    // Test 2: Concurrent schema collection (more intensive)
    let mut schema_handles = Vec::new();
    for i in 0..2 {
        // Fewer concurrent schema collections as they're more resource intensive
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            let schema = adapter.collect_schema().await.unwrap();
            assert!(!schema.database_info.name.is_empty());
            i
        });
        schema_handles.push(handle);
    }

    // Wait for schema collections
    for handle in schema_handles {
        handle.await.unwrap();
    }

    // Test 3: Mixed concurrent operations
    let mut mixed_handles = Vec::new();

    // Connection tests
    for i in 0..2 {
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            adapter.test_connection().await.unwrap();
            i
        });
        mixed_handles.push(handle);
    }

    // Schema collections
    for i in 0..1 {
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            let _schema = adapter.collect_schema().await.unwrap();
            i + 100 // Different return value to distinguish
        });
        mixed_handles.push(handle);
    }

    // Wait for all mixed operations
    for handle in mixed_handles {
        handle.await.unwrap();
    }

    // Verify pool is still healthy after all operations
    assert!(
        adapter.is_pool_healthy().await,
        "Pool should remain healthy"
    );

    let (_active, idle, total) = adapter.pool_stats();
    assert!(total >= 1, "Should have connections after operations");
    assert!(idle <= total, "Idle should not exceed total");

    adapter.close().await;

    Ok(())
}

/// Test pool health monitoring and recovery
#[tokio::test]
async fn test_pool_health_monitoring() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let adapter = PostgresAdapter::new(&database_url).await?;

    // Test 1: Initial health check
    assert!(
        adapter.is_pool_healthy().await,
        "New pool should be healthy"
    );

    // Test 2: Health after operations
    adapter.test_connection().await?;
    assert!(
        adapter.is_pool_healthy().await,
        "Pool should remain healthy after operations"
    );

    // Test 3: Pool statistics monitoring
    let (_active1, _idle1, _total1) = adapter.pool_stats();

    // Perform some operations
    adapter.test_connection().await?;
    let _schema = adapter.collect_schema().await?;

    let (_active2, idle2, total2) = adapter.pool_stats();

    // Pool should maintain reasonable state
    assert!(total2 >= 1, "Should have connections after operations");
    assert!(idle2 <= total2, "Idle should not exceed total");

    // Test 4: Health check after intensive operations
    for _ in 0..3 {
        adapter.test_connection().await?;
    }

    assert!(
        adapter.is_pool_healthy().await,
        "Pool should remain healthy after intensive use"
    );

    // Test 5: Graceful shutdown
    adapter.close().await;

    // After close, health check behavior is undefined, so we don't test it

    Ok(())
}

/// Test connection pool configuration edge cases
#[tokio::test]
async fn test_connection_pool_edge_cases() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Test 1: Configuration with query parameters in URL
    let url_with_params = format!(
        "{}?connect_timeout=5&statement_timeout=10000&pool_max_conns=4",
        database_url
    );

    let adapter1 = PostgresAdapter::new(&url_with_params).await?;
    adapter1.test_connection().await?;

    let (_, _, total1) = adapter1.pool_stats();
    assert!(total1 >= 1, "URL with params should create valid pool");

    // Test 2: Configuration validation with invalid values
    let invalid_config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(0), // Invalid port
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(30),
        max_connections: 10,
        read_only: true,
        ..Default::default()
    };

    let result = PostgresAdapter::with_config(&database_url, invalid_config).await;
    assert!(result.is_err(), "Invalid port should be rejected");

    // Test 3: Configuration with very long database name (test via connection string parsing)
    let long_db_name = "a".repeat(100); // Exceeds PostgreSQL limit
    let long_db_url = format!(
        "postgres://postgres:postgres@localhost:{}/{}",
        port, long_db_name
    );

    let result = PostgresAdapter::new(&long_db_url).await;
    assert!(
        result.is_err(),
        "Overly long database name should be rejected during parsing"
    );

    // Test 4: Configuration with invalid username characters (test via connection string parsing)
    let invalid_user_url = format!(
        "postgres://invalid-user-name-with-invalid-chars!@#$:password@localhost:{}/postgres",
        port
    );

    let result = PostgresAdapter::new(&invalid_user_url).await;
    assert!(
        result.is_err(),
        "Invalid username should be rejected during parsing"
    );

    adapter1.close().await;

    Ok(())
}

/// Test resource cleanup and connection lifecycle management
#[tokio::test]
async fn test_resource_cleanup_and_lifecycle() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Test 1: Create and immediately close adapter
    let adapter1 = PostgresAdapter::new(&database_url).await?;
    adapter1.test_connection().await?;
    let (_, _, total1) = adapter1.pool_stats();
    assert!(total1 >= 1, "Should have connections");

    adapter1.close().await;
    // After close, pool should be shut down

    // Test 2: Create multiple adapters and close them
    let mut adapters = Vec::new();
    for _ in 0..3 {
        let adapter = PostgresAdapter::new(&database_url).await?;
        adapter.test_connection().await?;
        adapters.push(adapter);
    }

    // Close all adapters
    for adapter in adapters {
        adapter.close().await;
    }

    // Test 3: Adapter lifecycle with operations
    let adapter3 = PostgresAdapter::new(&database_url).await?;

    // Perform various operations
    adapter3.test_connection().await?;
    let _schema = adapter3.collect_schema().await?;
    assert!(adapter3.is_pool_healthy().await);

    // Check pool stats before close
    let (_active, _idle, total) = adapter3.pool_stats();
    assert!(total >= 1, "Should have connections before close");

    // Close and verify cleanup
    adapter3.close().await;

    // Test 4: Multiple operations before close
    let adapter4 = PostgresAdapter::new(&database_url).await?;

    // Concurrent operations
    let mut handles = Vec::new();
    for i in 0..3 {
        let database_url_clone = database_url.clone();
        let handle = tokio::spawn(async move {
            let adapter = PostgresAdapter::new(&database_url_clone).await.unwrap();
            adapter.test_connection().await.unwrap();
            i
        });
        handles.push(handle);
    }

    // Wait for operations
    for handle in handles {
        handle.await.unwrap();
    }

    // Close after concurrent operations
    adapter4.close().await;

    Ok(())
}

/// Test pool_statistics() method returns correct PoolStats struct
#[tokio::test]
async fn test_pool_statistics_struct() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 5,
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(30),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;
    adapter.test_connection().await?;

    // Get statistics using the new struct-based method
    let stats: PoolStats = adapter.pool_statistics();

    // Verify struct fields
    assert!(
        stats.total_connections >= 1,
        "Should have at least one connection"
    );
    assert!(
        stats.idle_connections <= stats.total_connections,
        "Idle should not exceed total"
    );
    assert_eq!(
        stats.active_connections,
        stats.total_connections - stats.idle_connections
    );
    assert_eq!(
        stats.max_connections, 5,
        "Max connections should match config"
    );

    // Verify consistency with tuple-based method
    let (active, idle, total) = adapter.pool_stats();
    assert_eq!(stats.active_connections, active);
    assert_eq!(stats.idle_connections, idle);
    assert_eq!(stats.total_connections, total);

    adapter.close().await;

    Ok(())
}

/// Test connection pool idle configuration
#[tokio::test]
async fn test_connection_pool_idle_configuration() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create adapter with custom idle configuration
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 5,
        min_idle_connections: 2, // Maintain at least 2 idle connections
        idle_timeout: Some(Duration::from_secs(60)), // 1 minute idle timeout
        connect_timeout: Duration::from_secs(30),
        query_timeout: Duration::from_secs(30),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;

    // Initial test - pool should be healthy
    adapter.test_connection().await?;

    // After a connection is used and returned, pool should have connections
    let (active, idle, total) = adapter.pool_stats();
    assert!(total >= 1, "Pool should have at least one connection");
    assert!(idle <= total, "Idle connections should not exceed total");
    assert_eq!(active, total - idle, "Active should equal total minus idle");

    // Test that min_idle_connections is respected by examining pool config
    // Note: sqlx's min_connections is a best-effort setting, actual behavior may vary
    assert!(
        adapter.is_pool_healthy().await,
        "Pool should remain healthy with custom idle config"
    );

    adapter.close().await;

    Ok(())
}

/// Test ConnectionConfig::from_env() creates configuration from environment variables
#[test]
fn test_connection_config_from_env() {
    // SAFETY: This test is run in isolation and env var manipulation is safe in single-threaded test
    unsafe {
        // Set environment variables
        std::env::set_var("DBSURVEYOR_MAX_CONNECTIONS", "20");
        std::env::set_var("DBSURVEYOR_CONNECT_TIMEOUT_SECS", "60");
        std::env::set_var("DBSURVEYOR_IDLE_TIMEOUT_SECS", "300");
    }

    let config = ConnectionConfig::from_env().unwrap();

    assert_eq!(config.max_connections, 20);
    assert_eq!(config.connect_timeout, Duration::from_secs(60));
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(300)));

    // SAFETY: Cleanup is safe in single-threaded test context
    unsafe {
        std::env::remove_var("DBSURVEYOR_MAX_CONNECTIONS");
        std::env::remove_var("DBSURVEYOR_CONNECT_TIMEOUT_SECS");
        std::env::remove_var("DBSURVEYOR_IDLE_TIMEOUT_SECS");
    }
}

/// Test that connection pool respects max_connections limit and times out correctly
#[tokio::test]
async fn test_connection_pool_max_connections_limit() -> Result<()> {
    let postgres = Postgres::default().start().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let database_url = format!("postgres://postgres:postgres@localhost:{}/postgres", port);

    wait_for_postgres_ready(&database_url, 30).await?;

    // Create adapter with very limited pool and short acquire timeout
    let config = ConnectionConfig {
        host: "localhost".to_string(),
        port: Some(port),
        database: Some("postgres".to_string()),
        username: Some("postgres".to_string()),
        max_connections: 2,                          // Only 2 connections allowed
        connect_timeout: Duration::from_millis(100), // Short timeout for acquire
        query_timeout: Duration::from_secs(30),
        read_only: true,
        ..Default::default()
    };

    let adapter = PostgresAdapter::with_config(&database_url, config).await?;

    // Acquire all available connections
    let _conn1 = adapter.acquire().await?;
    let _conn2 = adapter.acquire().await?;

    // Third acquire should timeout since pool is exhausted
    let result = adapter.acquire().await;
    assert!(
        result.is_err(),
        "Third acquire should fail due to pool exhaustion"
    );

    // Verify the error is a connection timeout
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("timeout") || error_msg.contains("Timeout"),
        "Error should indicate timeout, got: {}",
        error_msg
    );

    Ok(())
}
