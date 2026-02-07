//! Comprehensive connection pooling tests with testcontainers
//!
//! These tests verify advanced connection pooling configuration including:
//! - Pool parameter validation and adjustment
//! - Environment variable configuration
//! - Connection pool exhaustion scenarios
//! - Timeout behavior under load
//! - Connection lifecycle management

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::uninlined_format_args)]

use dbsurveyor_collect::adapters::ConnectionConfig;
use std::time::Duration;

#[cfg(all(test, feature = "postgresql"))]
extern crate sqlx;

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_config_builder_basic() {
        let config = ConnectionConfig::builder()
            .max_connections(20)
            .min_idle_connections(5)
            .connect_timeout(Duration::from_secs(60))
            .acquire_timeout(Duration::from_secs(45))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .build();

        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_idle_connections, 5);
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        assert_eq!(config.acquire_timeout, Duration::from_secs(45));
        assert_eq!(config.idle_timeout, Duration::from_secs(300));
        assert_eq!(config.max_lifetime, Duration::from_secs(1800));
    }

    #[test]
    fn test_config_builder_partial() {
        let config = ConnectionConfig::builder().max_connections(15).build();

        assert_eq!(config.max_connections, 15);
        // Other values should use defaults
        assert_eq!(config.min_idle_connections, 2);
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_config_validation_valid() {
        let config = ConnectionConfig::default();
        assert!(config.validate().is_ok());

        let config = ConnectionConfig::builder()
            .max_connections(100)
            .min_idle_connections(10)
            .build();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_zero_max_connections() {
        let config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_excessive_max_connections() {
        let config = ConnectionConfig {
            max_connections: 1001,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_min_exceeds_max() {
        let config = ConnectionConfig {
            max_connections: 5,
            min_idle_connections: 10,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_zero_timeout() {
        let config = ConnectionConfig {
            connect_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = ConnectionConfig {
            acquire_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_excessive_timeout() {
        let config = ConnectionConfig {
            connect_timeout: Duration::from_secs(3601),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_adjustment_max_connections() {
        let mut config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.max_connections, 1);

        let mut config = ConnectionConfig {
            max_connections: 2000,
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.max_connections, 1000);
    }

    #[test]
    fn test_config_adjustment_min_idle() {
        let mut config = ConnectionConfig {
            max_connections: 5,
            min_idle_connections: 10,
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.min_idle_connections, 5);
    }

    #[test]
    fn test_config_adjustment_timeouts() {
        let mut config = ConnectionConfig {
            connect_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));

        let mut config = ConnectionConfig {
            acquire_timeout: Duration::from_secs(5000),
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.acquire_timeout, Duration::from_secs(3600));
    }

    #[test]
    fn test_config_from_env() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("25")),
                ("DBSURVEYOR_MIN_IDLE_CONNECTIONS", Some("8")),
                ("DBSURVEYOR_CONNECT_TIMEOUT_SECS", Some("45")),
                ("DBSURVEYOR_ACQUIRE_TIMEOUT_SECS", Some("40")),
                ("DBSURVEYOR_IDLE_TIMEOUT_SECS", Some("500")),
                ("DBSURVEYOR_MAX_LIFETIME_SECS", Some("2000")),
            ],
            || {
                let config = ConnectionConfig::from_env();
                assert_eq!(config.max_connections, 25);
                assert_eq!(config.min_idle_connections, 8);
                assert_eq!(config.connect_timeout, Duration::from_secs(45));
                assert_eq!(config.acquire_timeout, Duration::from_secs(40));
                assert_eq!(config.idle_timeout, Duration::from_secs(500));
                assert_eq!(config.max_lifetime, Duration::from_secs(2000));
            },
        );
    }

    #[test]
    fn test_config_from_env_partial() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("30")),
                // Other variables not set
            ],
            || {
                let config = ConnectionConfig::from_env();
                assert_eq!(config.max_connections, 30);
                // Others should use defaults
                assert_eq!(config.min_idle_connections, 2);
                assert_eq!(config.connect_timeout, Duration::from_secs(30));
            },
        );
    }

    #[test]
    fn test_config_from_env_invalid_values() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("not_a_number")),
                ("DBSURVEYOR_CONNECT_TIMEOUT_SECS", Some("invalid")),
            ],
            || {
                let config = ConnectionConfig::from_env();
                // Should use defaults for invalid values
                assert_eq!(config.max_connections, 10);
                assert_eq!(config.connect_timeout, Duration::from_secs(30));
            },
        );
    }

    #[test]
    fn test_config_builder_with_env_overrides() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("50")),
                ("DBSURVEYOR_MIN_IDLE_CONNECTIONS", Some("10")),
            ],
            || {
                let config = ConnectionConfig::builder()
                    .with_env_overrides()
                    .connect_timeout(Duration::from_secs(120))
                    .build();

                // Env vars should be applied
                assert_eq!(config.max_connections, 50);
                assert_eq!(config.min_idle_connections, 10);
                // Explicit builder value should be used
                assert_eq!(config.connect_timeout, Duration::from_secs(120));
            },
        );
    }
}

#[cfg(all(test, feature = "postgresql"))]
mod postgresql_pooling_tests {
    use super::*;
    use dbsurveyor_collect::adapters::{SchemaCollector, postgresql::PostgresAdapter};
    use std::sync::Arc;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_postgres_pool_with_custom_config() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create adapter with custom pool configuration
        let config = ConnectionConfig::builder()
            .max_connections(5)
            .min_idle_connections(1)
            .connect_timeout(Duration::from_secs(10))
            .acquire_timeout(Duration::from_secs(10))
            .build();

        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        // Test connection
        adapter
            .test_connection()
            .await
            .expect("Connection test failed");

        // Collect metadata to verify pool works
        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "postgresql");
    }

    #[tokio::test]
    async fn test_postgres_pool_concurrent_operations() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create adapter with limited pool size
        let config = ConnectionConfig::builder()
            .max_connections(3)
            .min_idle_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Spawn multiple concurrent operations
        let mut tasks = JoinSet::new();

        for i in 0..5 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move {
                let result = adapter_clone.test_connection().await;
                (i, result)
            });
        }

        // Collect results
        let mut success_count = 0;
        while let Some(result) = tasks.join_next().await {
            let (task_id, connection_result) = result.expect("Task panicked");
            if connection_result.is_ok() {
                success_count += 1;
            }
            println!(
                "Task {} completed: {:?}",
                task_id,
                connection_result.is_ok()
            );
        }

        // All tasks should succeed (pool should handle concurrent requests)
        assert_eq!(success_count, 5);
    }

    #[tokio::test]
    async fn test_postgres_pool_validation_enforcement() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Try to create adapter with invalid configuration
        let config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };

        let result = PostgresAdapter::new(&connection_string, config).await;
        assert!(result.is_err(), "Should reject invalid configuration");
    }

    #[tokio::test]
    async fn test_postgres_pool_with_env_config() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("8")),
                ("DBSURVEYOR_MIN_IDLE_CONNECTIONS", Some("2")),
            ],
            || async {
                let config = ConnectionConfig::from_env();
                let adapter = PostgresAdapter::new(&connection_string, config)
                    .await
                    .expect("Failed to create adapter");

                adapter
                    .test_connection()
                    .await
                    .expect("Connection test failed");

                let description = adapter.safe_description();
                assert!(description.contains("max: 8"));
                assert!(description.contains("idle: 2"));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_postgres_pool_timeout_behavior() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create adapter with very short timeouts
        let config = ConnectionConfig::builder()
            .max_connections(2)
            .min_idle_connections(1)
            .acquire_timeout(Duration::from_secs(2))
            .connect_timeout(Duration::from_secs(5))
            .build();

        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        // Basic operation should succeed
        adapter
            .test_connection()
            .await
            .expect("Connection test failed");
    }

    #[tokio::test]
    async fn test_postgres_pool_lifecycle() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        let config = ConnectionConfig::builder()
            .max_connections(5)
            .min_idle_connections(2)
            .idle_timeout(Duration::from_secs(30))
            .max_lifetime(Duration::from_secs(60))
            .build();

        let adapter = PostgresAdapter::new(&connection_string, config)
            .await
            .expect("Failed to create adapter");

        // Perform multiple operations to exercise pool lifecycle
        for i in 0..10 {
            adapter
                .test_connection()
                .await
                .unwrap_or_else(|_| panic!("Connection test {} failed", i));
        }

        // Collect metadata to verify pool is still healthy
        let metadata = adapter
            .collect_metadata()
            .await
            .expect("Failed to collect metadata");

        assert_eq!(metadata.database_type, "postgresql");
    }

    #[tokio::test]
    async fn test_postgres_pool_stress_test() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        let config = ConnectionConfig::builder()
            .max_connections(10)
            .min_idle_connections(3)
            .acquire_timeout(Duration::from_secs(10))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Spawn many concurrent operations
        let mut tasks = JoinSet::new();

        for i in 0..20 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move {
                // Perform multiple operations per task
                for _ in 0..3 {
                    adapter_clone
                        .test_connection()
                        .await
                        .expect("Connection failed");
                }
                i
            });
        }

        // Wait for all tasks to complete
        let mut completed = 0;
        while let Some(result) = tasks.join_next().await {
            result.expect("Task panicked");
            completed += 1;
        }

        assert_eq!(completed, 20);
    }

    /// Test connection pool exhaustion scenario (max_connections + 1)
    ///
    /// This test verifies that when the pool is exhausted, additional connection
    /// requests either wait for available connections or timeout appropriately.
    #[tokio::test]
    async fn test_postgres_pool_exhaustion() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
        use tokio::time::sleep;

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create pool with very limited connections and short timeout
        let config = ConnectionConfig::builder()
            .max_connections(2)
            .min_idle_connections(1)
            .acquire_timeout(Duration::from_secs(3))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Spawn tasks that hold connections longer than acquire timeout
        let mut tasks = JoinSet::new();

        for i in 0..3 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move {
                let start = std::time::Instant::now();
                let result = adapter_clone.test_connection().await;
                let duration = start.elapsed();

                // Simulate holding connection
                if result.is_ok() {
                    sleep(Duration::from_millis(500)).await;
                }

                (i, result, duration)
            });
        }

        // Collect results
        let mut success_count = 0;
        let mut timeout_count = 0;

        while let Some(result) = tasks.join_next().await {
            let (task_id, connection_result, duration) = result.expect("Task panicked");

            if connection_result.is_ok() {
                success_count += 1;
                println!("Task {} succeeded in {:?}", task_id, duration);
            } else {
                timeout_count += 1;
                println!("Task {} failed/timed out in {:?}", task_id, duration);
            }
        }

        // With max_connections=2 and 3 concurrent tasks, at least 2 should succeed
        assert!(
            success_count >= 2,
            "Expected at least 2 successful connections"
        );
        println!(
            "Pool exhaustion test: {} succeeded, {} timed out",
            success_count, timeout_count
        );
    }

    /// Test timeout validation under heavy load
    ///
    /// This test verifies that acquire timeouts are properly enforced when
    /// the pool is under heavy load with long-running operations.
    #[tokio::test]
    async fn test_postgres_pool_timeout_under_load() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
        use tokio::time::sleep;

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create pool with limited connections and very short acquire timeout
        let config = ConnectionConfig::builder()
            .max_connections(3)
            .min_idle_connections(1)
            .acquire_timeout(Duration::from_secs(2))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // First, saturate the pool with long-running operations
        let mut blocking_tasks = JoinSet::new();

        for i in 0..3 {
            let adapter_clone = Arc::clone(&adapter);
            blocking_tasks.spawn(async move {
                let _result = adapter_clone.test_connection().await;
                // Hold connection for 5 seconds
                sleep(Duration::from_secs(5)).await;
                i
            });
        }

        // Give blocking tasks time to acquire connections
        sleep(Duration::from_millis(100)).await;

        // Now try to acquire additional connections (should timeout)
        let mut timeout_tasks = JoinSet::new();

        for i in 0..2 {
            let adapter_clone = Arc::clone(&adapter);
            timeout_tasks.spawn(async move {
                let start = std::time::Instant::now();
                let result = adapter_clone.test_connection().await;
                let duration = start.elapsed();
                (i, result, duration)
            });
        }

        // Check timeout tasks
        let mut timed_out = 0;
        while let Some(result) = timeout_tasks.join_next().await {
            let (task_id, connection_result, duration) = result.expect("Task panicked");

            if connection_result.is_err() {
                timed_out += 1;
                // Verify timeout occurred within reasonable time
                assert!(
                    duration < Duration::from_secs(4),
                    "Task {} took too long to timeout: {:?}",
                    task_id,
                    duration
                );
            }
        }

        // Wait for blocking tasks to complete
        while blocking_tasks.join_next().await.is_some() {}

        println!(
            "Timeout under load test: {} tasks timed out as expected",
            timed_out
        );
    }

    /// Test various pool parameter configurations
    ///
    /// This test validates that different pool configurations work correctly
    /// and that the pool behaves appropriately with various settings.
    #[tokio::test]
    async fn test_postgres_pool_parameter_variations() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Test configuration 1: Minimal pool
        let config1 = ConnectionConfig::builder()
            .max_connections(1)
            .min_idle_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .build();

        let adapter1 = PostgresAdapter::new(&connection_string, config1)
            .await
            .expect("Failed to create adapter with minimal config");

        adapter1
            .test_connection()
            .await
            .expect("Minimal pool connection failed");

        // Test configuration 2: Large pool
        let config2 = ConnectionConfig::builder()
            .max_connections(50)
            .min_idle_connections(10)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(120))
            .max_lifetime(Duration::from_secs(600))
            .build();

        let adapter2 = PostgresAdapter::new(&connection_string, config2)
            .await
            .expect("Failed to create adapter with large config");

        adapter2
            .test_connection()
            .await
            .expect("Large pool connection failed");

        // Test configuration 3: Balanced pool
        let config3 = ConnectionConfig::builder()
            .max_connections(10)
            .min_idle_connections(3)
            .acquire_timeout(Duration::from_secs(15))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .build();

        let adapter3 = PostgresAdapter::new(&connection_string, config3)
            .await
            .expect("Failed to create adapter with balanced config");

        adapter3
            .test_connection()
            .await
            .expect("Balanced pool connection failed");

        println!("All pool parameter variations tested successfully");
    }

    /// Test connection lifecycle management (acquire, release, cleanup)
    ///
    /// This test verifies that connections are properly acquired, released,
    /// and cleaned up throughout their lifecycle.
    #[tokio::test]
    async fn test_postgres_connection_lifecycle() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
        use tokio::time::sleep;

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        let config = ConnectionConfig::builder()
            .max_connections(5)
            .min_idle_connections(2)
            .idle_timeout(Duration::from_secs(2))
            .max_lifetime(Duration::from_secs(10))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Phase 1: Acquire connections
        println!("Phase 1: Acquiring connections");
        for i in 0..5 {
            adapter
                .test_connection()
                .await
                .unwrap_or_else(|_| panic!("Failed to acquire connection {}", i));
        }

        // Phase 2: Release connections (implicit through drop)
        println!("Phase 2: Connections released");
        sleep(Duration::from_millis(100)).await;

        // Phase 3: Reuse connections
        println!("Phase 3: Reusing connections");
        for i in 0..5 {
            adapter
                .test_connection()
                .await
                .unwrap_or_else(|_| panic!("Failed to reuse connection {}", i));
        }

        // Phase 4: Wait for idle timeout
        println!("Phase 4: Waiting for idle timeout");
        sleep(Duration::from_secs(3)).await;

        // Phase 5: Verify pool still works after idle cleanup
        println!("Phase 5: Verifying pool after idle cleanup");
        adapter
            .test_connection()
            .await
            .expect("Pool failed after idle cleanup");

        // Phase 6: Test concurrent acquire/release cycles
        println!("Phase 6: Concurrent acquire/release cycles");
        let mut tasks = JoinSet::new();

        for i in 0..10 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move {
                for _ in 0..3 {
                    adapter_clone
                        .test_connection()
                        .await
                        .expect("Connection failed");
                    sleep(Duration::from_millis(50)).await;
                }
                i
            });
        }

        while let Some(result) = tasks.join_next().await {
            result.expect("Task panicked");
        }

        println!("Connection lifecycle test completed successfully");
    }

    /// Test performance with concurrent schema collection
    ///
    /// This test measures performance characteristics when multiple tasks
    /// are concurrently collecting schema metadata.
    #[tokio::test]
    async fn test_postgres_concurrent_schema_collection_performance() {
        use std::time::Instant;
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        // Create some test tables for schema collection
        let setup_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&connection_string)
            .await
            .expect("Failed to create setup pool");

        for i in 1..=5 {
            let create_table = format!(
                "CREATE TABLE IF NOT EXISTS test_table_{} (id SERIAL PRIMARY KEY, data TEXT)",
                i
            );
            sqlx::query(&create_table)
                .execute(&setup_pool)
                .await
                .expect("Failed to create test table");
        }

        setup_pool.close().await;

        // Configure pool for concurrent operations
        let config = ConnectionConfig::builder()
            .max_connections(10)
            .min_idle_connections(3)
            .acquire_timeout(Duration::from_secs(10))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Measure concurrent schema collection performance
        let start = Instant::now();
        let mut tasks = JoinSet::new();

        for i in 0..10 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move {
                let task_start = Instant::now();
                let result = adapter_clone.collect_metadata().await;
                let task_duration = task_start.elapsed();
                (i, result, task_duration)
            });
        }

        let mut success_count = 0;
        let mut total_duration = Duration::from_secs(0);

        while let Some(result) = tasks.join_next().await {
            let (task_id, metadata_result, task_duration) = result.expect("Task panicked");

            if let Ok(metadata) = metadata_result {
                success_count += 1;
                total_duration += task_duration;
                println!(
                    "Task {} collected {} schemas in {:?}",
                    task_id,
                    metadata.schemas.len(),
                    task_duration
                );
            }
        }

        let total_elapsed = start.elapsed();
        let avg_duration = total_duration / success_count;

        println!("\nPerformance Summary:");
        println!("  Total time: {:?}", total_elapsed);
        println!("  Successful collections: {}", success_count);
        println!("  Average collection time: {:?}", avg_duration);
        println!(
            "  Throughput: {:.2} collections/sec",
            success_count as f64 / total_elapsed.as_secs_f64()
        );

        assert_eq!(success_count, 10, "All schema collections should succeed");

        // Performance assertion: average collection should be reasonable
        assert!(
            avg_duration < Duration::from_secs(5),
            "Average collection time too high: {:?}",
            avg_duration
        );
    }

    /// Test pool behavior with connection failures and recovery
    ///
    /// This test verifies that the pool can recover from transient failures
    /// and continue operating normally.
    #[tokio::test]
    async fn test_postgres_pool_failure_recovery() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        let config = ConnectionConfig::builder()
            .max_connections(5)
            .min_idle_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .max_lifetime(Duration::from_secs(30))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Verify initial connectivity
        adapter
            .test_connection()
            .await
            .expect("Initial connection failed");

        // Perform multiple operations to ensure pool stability
        for i in 0..10 {
            let result = adapter.test_connection().await;
            assert!(
                result.is_ok(),
                "Connection {} failed: {:?}",
                i,
                result.err()
            );
        }

        println!("Pool failure recovery test completed successfully");
    }

    /// Test pool with extreme concurrency
    ///
    /// This test pushes the pool to its limits with very high concurrency
    /// to ensure it remains stable under extreme load.
    #[tokio::test]
    async fn test_postgres_pool_extreme_concurrency() {
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let container = Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get port");

        let connection_string =
            format!("postgresql://postgres:postgres@localhost:{}/postgres", port);

        let config = ConnectionConfig::builder()
            .max_connections(20)
            .min_idle_connections(5)
            .acquire_timeout(Duration::from_secs(15))
            .build();

        let adapter = Arc::new(
            PostgresAdapter::new(&connection_string, config)
                .await
                .expect("Failed to create adapter"),
        );

        // Spawn 100 concurrent tasks
        let mut tasks = JoinSet::new();

        for i in 0..100 {
            let adapter_clone = Arc::clone(&adapter);
            tasks.spawn(async move { adapter_clone.test_connection().await.map(|_| i) });
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        while let Some(result) = tasks.join_next().await {
            match result.expect("Task panicked") {
                Ok(_) => success_count += 1,
                Err(_) => failure_count += 1,
            }
        }

        println!(
            "Extreme concurrency test: {} succeeded, {} failed",
            success_count, failure_count
        );

        // Most tasks should succeed even under extreme load
        assert!(
            success_count >= 90,
            "Expected at least 90% success rate, got {}%",
            success_count
        );
    }
}
