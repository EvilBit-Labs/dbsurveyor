# PostgreSQL Advanced Features Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement advanced connection pooling, data sampling, and multi-database collection for PostgreSQL adapter.

**Architecture:** Extend existing PostgreSQL adapter with configurable connection pooling, intelligent data sampling based on ordering strategies, and server-level database enumeration for multi-database collection scenarios.

**Tech Stack:** Rust, sqlx with PostgreSQL, testcontainers-modules for testing, tokio for async runtime.

---

## Task 5.1: Implement Advanced Connection Pooling Configuration

**Files:**
- Modify: `dbsurveyor-core/src/adapters/config/connection.rs`
- Modify: `dbsurveyor-core/src/adapters/postgres/connection.rs`
- Test: `dbsurveyor-core/tests/postgres_connection_pooling.rs`

### Step 1: Write failing test for pool limit configuration

```rust
// In dbsurveyor-core/tests/postgres_connection_pooling.rs
#[tokio::test]
async fn test_connection_pool_max_connections_limit() {
    // This test verifies that when max_connections connections are acquired,
    // the next acquire attempt times out per the configured acquire_timeout
    let container = postgres_container().await;
    let url = container.connection_string();

    let config = ConnectionConfig::builder()
        .max_connections(2)
        .acquire_timeout(Duration::from_millis(100))
        .build()
        .unwrap();

    let adapter = PostgresAdapter::connect(&url, config).await.unwrap();

    // Acquire all available connections
    let _conn1 = adapter.acquire().await.unwrap();
    let _conn2 = adapter.acquire().await.unwrap();

    // Third acquire should timeout
    let result = adapter.acquire().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DbSurveyorError::ConnectionTimeout { .. }));
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_connection_pool_max_connections_limit --features postgresql --no-capture`
Expected: FAIL - acquire() method doesn't exist yet

### Step 3: Add acquire method to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/connection.rs
impl PostgresAdapter {
    /// Acquire a connection from the pool
    ///
    /// Returns a pooled connection that will be returned to the pool on drop.
    /// Respects the configured acquire_timeout.
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>, DbSurveyorError> {
        self.pool
            .acquire()
            .await
            .map_err(|e| {
                if e.to_string().contains("timed out") {
                    DbSurveyorError::connection_timeout(
                        "connection pool",
                        self.config.acquire_timeout,
                    )
                } else {
                    DbSurveyorError::connection_failed(&format!("pool acquire failed: {}", e))
                }
            })
    }
}
```

### Step 4: Run test to verify it passes

Run: `cd dbsurveyor-core && cargo nextest run test_connection_pool_max_connections_limit --features postgresql --no-capture`
Expected: PASS

### Step 5: Write test for idle connection configuration

```rust
#[tokio::test]
async fn test_connection_pool_idle_timeout() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let config = ConnectionConfig::builder()
        .max_connections(5)
        .min_idle_connections(2)
        .idle_timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    let adapter = PostgresAdapter::connect(&url, config).await.unwrap();

    // Pool should have min_idle connections ready
    let stats = adapter.pool_stats();
    assert!(stats.idle_connections >= 2);

    // After idle timeout, excess connections should be dropped
    tokio::time::sleep(Duration::from_millis(600)).await;
    let stats = adapter.pool_stats();
    assert_eq!(stats.idle_connections, 2);
}
```

### Step 6: Add pool_stats method to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/connection.rs
/// Pool statistics for monitoring
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub idle_connections: u32,
    pub active_connections: u32,
    pub max_connections: u32,
}

impl PostgresAdapter {
    /// Get current pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        PoolStats {
            idle_connections: self.pool.num_idle() as u32,
            active_connections: self.pool.size() as u32 - self.pool.num_idle() as u32,
            max_connections: self.config.max_connections,
        }
    }
}
```

### Step 7: Run tests and verify

Run: `cd dbsurveyor-core && cargo nextest run test_connection_pool --features postgresql --no-capture`
Expected: PASS

### Step 8: Commit

```bash
git add dbsurveyor-core/src/adapters/config/connection.rs dbsurveyor-core/src/adapters/postgres/connection.rs dbsurveyor-core/tests/postgres_connection_pooling.rs
git commit -m "feat(postgres): add advanced connection pool configuration

- Add acquire() method with timeout handling
- Add pool_stats() for monitoring idle/active connections
- Support min_idle_connections and idle_timeout configuration
- Add ConnectionTimeout error variant"
```

---

## Task 5.2: Add Environment Variable Configuration Support

**Files:**
- Modify: `dbsurveyor-core/src/adapters/config/connection.rs`
- Test: `dbsurveyor-core/tests/postgres_connection_pooling.rs`

### Step 1: Write failing test for env var configuration

```rust
#[test]
fn test_connection_config_from_env() {
    // Set environment variables
    std::env::set_var("DBSURVEYOR_MAX_CONNECTIONS", "20");
    std::env::set_var("DBSURVEYOR_CONNECT_TIMEOUT_SECS", "60");
    std::env::set_var("DBSURVEYOR_IDLE_TIMEOUT_SECS", "300");

    let config = ConnectionConfig::from_env().unwrap();

    assert_eq!(config.max_connections, 20);
    assert_eq!(config.connect_timeout, Duration::from_secs(60));
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(300)));

    // Clean up
    std::env::remove_var("DBSURVEYOR_MAX_CONNECTIONS");
    std::env::remove_var("DBSURVEYOR_CONNECT_TIMEOUT_SECS");
    std::env::remove_var("DBSURVEYOR_IDLE_TIMEOUT_SECS");
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_connection_config_from_env --features postgresql`
Expected: FAIL - from_env() method doesn't exist

### Step 3: Implement from_env method

```rust
// In dbsurveyor-core/src/adapters/config/connection.rs
impl ConnectionConfig {
    /// Create configuration from environment variables
    ///
    /// Supported variables:
    /// - DBSURVEYOR_MAX_CONNECTIONS (default: 10)
    /// - DBSURVEYOR_MIN_IDLE_CONNECTIONS (default: 2)
    /// - DBSURVEYOR_CONNECT_TIMEOUT_SECS (default: 30)
    /// - DBSURVEYOR_ACQUIRE_TIMEOUT_SECS (default: 30)
    /// - DBSURVEYOR_IDLE_TIMEOUT_SECS (default: 600)
    /// - DBSURVEYOR_MAX_LIFETIME_SECS (default: 3600)
    pub fn from_env() -> Result<Self, DbSurveyorError> {
        let mut builder = Self::builder();

        if let Ok(val) = std::env::var("DBSURVEYOR_MAX_CONNECTIONS") {
            let max_conn: u32 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_MAX_CONNECTIONS"))?;
            builder = builder.max_connections(max_conn);
        }

        if let Ok(val) = std::env::var("DBSURVEYOR_MIN_IDLE_CONNECTIONS") {
            let min_idle: u32 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_MIN_IDLE_CONNECTIONS"))?;
            builder = builder.min_idle_connections(min_idle);
        }

        if let Ok(val) = std::env::var("DBSURVEYOR_CONNECT_TIMEOUT_SECS") {
            let secs: u64 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_CONNECT_TIMEOUT_SECS"))?;
            builder = builder.connect_timeout(Duration::from_secs(secs));
        }

        if let Ok(val) = std::env::var("DBSURVEYOR_ACQUIRE_TIMEOUT_SECS") {
            let secs: u64 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_ACQUIRE_TIMEOUT_SECS"))?;
            builder = builder.acquire_timeout(Duration::from_secs(secs));
        }

        if let Ok(val) = std::env::var("DBSURVEYOR_IDLE_TIMEOUT_SECS") {
            let secs: u64 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_IDLE_TIMEOUT_SECS"))?;
            builder = builder.idle_timeout(Duration::from_secs(secs));
        }

        if let Ok(val) = std::env::var("DBSURVEYOR_MAX_LIFETIME_SECS") {
            let secs: u64 = val.parse()
                .map_err(|_| DbSurveyorError::configuration("invalid DBSURVEYOR_MAX_LIFETIME_SECS"))?;
            builder = builder.max_lifetime(Duration::from_secs(secs));
        }

        builder.build()
    }
}
```

### Step 4: Run test to verify it passes

Run: `cd dbsurveyor-core && cargo nextest run test_connection_config_from_env --features postgresql`
Expected: PASS

### Step 5: Commit

```bash
git add dbsurveyor-core/src/adapters/config/connection.rs dbsurveyor-core/tests/postgres_connection_pooling.rs
git commit -m "feat(config): add environment variable configuration support

- Add from_env() constructor for ConnectionConfig
- Support DBSURVEYOR_MAX_CONNECTIONS, DBSURVEYOR_CONNECT_TIMEOUT_SECS, etc.
- Add validation for environment variable parsing"
```

---

## Task 6.1: Implement Ordering Strategy Detection

**Files:**
- Create: `dbsurveyor-core/src/adapters/postgres/sampling.rs`
- Modify: `dbsurveyor-core/src/adapters/postgres/mod.rs`
- Test: `dbsurveyor-core/tests/postgres_sampling.rs`

### Step 1: Write failing test for ordering strategy detection

```rust
// In dbsurveyor-core/tests/postgres_sampling.rs
use dbsurveyor_core::adapters::postgres::{PostgresAdapter, OrderingStrategy};

#[tokio::test]
async fn test_detect_ordering_strategy_primary_key() {
    let container = postgres_container().await;
    let url = container.connection_string();

    // Create test table with primary key
    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();
    adapter.execute("CREATE TABLE test_pk (id SERIAL PRIMARY KEY, name TEXT)").await.unwrap();

    let strategy = adapter.detect_ordering_strategy("public", "test_pk").await.unwrap();

    assert!(matches!(strategy, OrderingStrategy::PrimaryKey { columns } if columns == vec!["id".to_string()]));
}

#[tokio::test]
async fn test_detect_ordering_strategy_timestamp() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();
    adapter.execute("CREATE TABLE test_ts (name TEXT, created_at TIMESTAMP DEFAULT NOW())").await.unwrap();

    let strategy = adapter.detect_ordering_strategy("public", "test_ts").await.unwrap();

    assert!(matches!(strategy, OrderingStrategy::Timestamp { column } if column == "created_at"));
}

#[tokio::test]
async fn test_detect_ordering_strategy_random_fallback() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();
    adapter.execute("CREATE TABLE test_noorder (name TEXT, value INT)").await.unwrap();

    let strategy = adapter.detect_ordering_strategy("public", "test_noorder").await.unwrap();

    assert!(matches!(strategy, OrderingStrategy::Random));
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_detect_ordering_strategy --features postgresql --no-capture`
Expected: FAIL - OrderingStrategy and detect_ordering_strategy don't exist

### Step 3: Create OrderingStrategy enum and detection logic

```rust
// In dbsurveyor-core/src/adapters/postgres/sampling.rs
use crate::error::DbSurveyorError;
use sqlx::{PgPool, Row};

/// Strategy for ordering rows during sampling
#[derive(Debug, Clone, PartialEq)]
pub enum OrderingStrategy {
    /// Order by primary key columns (most reliable for most recent)
    PrimaryKey { columns: Vec<String> },
    /// Order by timestamp column (created_at, updated_at, etc.)
    Timestamp { column: String },
    /// Order by auto-increment column
    AutoIncrement { column: String },
    /// Random sampling (fallback when no reliable ordering exists)
    Random,
}

impl OrderingStrategy {
    /// Generate ORDER BY clause for this strategy
    pub fn order_by_clause(&self, descending: bool) -> String {
        let direction = if descending { "DESC" } else { "ASC" };
        match self {
            Self::PrimaryKey { columns } => {
                let cols: Vec<String> = columns.iter()
                    .map(|c| format!("\"{}\" {}", c, direction))
                    .collect();
                format!("ORDER BY {}", cols.join(", "))
            }
            Self::Timestamp { column } => {
                format!("ORDER BY \"{}\" {}", column, direction)
            }
            Self::AutoIncrement { column } => {
                format!("ORDER BY \"{}\" {}", column, direction)
            }
            Self::Random => {
                "ORDER BY RANDOM()".to_string()
            }
        }
    }
}

/// Detect the best ordering strategy for a table
pub async fn detect_ordering_strategy(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<OrderingStrategy, DbSurveyorError> {
    // 1. Check for primary key
    let pk_query = r#"
        SELECT a.attname
        FROM pg_index i
        JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
        JOIN pg_class c ON c.oid = i.indrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE i.indisprimary
          AND n.nspname = $1
          AND c.relname = $2
        ORDER BY array_position(i.indkey, a.attnum)
    "#;

    let pk_columns: Vec<String> = sqlx::query_scalar(pk_query)
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    if !pk_columns.is_empty() {
        return Ok(OrderingStrategy::PrimaryKey { columns: pk_columns });
    }

    // 2. Check for timestamp columns
    let ts_query = r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
          AND data_type IN ('timestamp without time zone', 'timestamp with time zone', 'timestamptz')
          AND column_name IN ('created_at', 'updated_at', 'inserted_at', 'modified_at', 'timestamp', 'date_created', 'created')
        ORDER BY
            CASE column_name
                WHEN 'created_at' THEN 1
                WHEN 'inserted_at' THEN 2
                WHEN 'date_created' THEN 3
                WHEN 'created' THEN 4
                WHEN 'updated_at' THEN 5
                WHEN 'modified_at' THEN 6
                WHEN 'timestamp' THEN 7
                ELSE 8
            END
        LIMIT 1
    "#;

    let ts_column: Option<String> = sqlx::query_scalar(ts_query)
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    if let Some(column) = ts_column {
        return Ok(OrderingStrategy::Timestamp { column });
    }

    // 3. Check for auto-increment/serial columns
    let serial_query = r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
          AND column_default LIKE 'nextval%'
        ORDER BY ordinal_position
        LIMIT 1
    "#;

    let serial_column: Option<String> = sqlx::query_scalar(serial_query)
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    if let Some(column) = serial_column {
        return Ok(OrderingStrategy::AutoIncrement { column });
    }

    // 4. Fallback to random sampling
    Ok(OrderingStrategy::Random)
}
```

### Step 4: Add to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/mod.rs
mod sampling;
pub use sampling::{OrderingStrategy, detect_ordering_strategy};

impl PostgresAdapter {
    /// Detect the best ordering strategy for sampling a table
    pub async fn detect_ordering_strategy(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<OrderingStrategy, DbSurveyorError> {
        sampling::detect_ordering_strategy(&self.pool, schema, table).await
    }
}
```

### Step 5: Run tests to verify

Run: `cd dbsurveyor-core && cargo nextest run test_detect_ordering_strategy --features postgresql --no-capture`
Expected: PASS

### Step 6: Commit

```bash
git add dbsurveyor-core/src/adapters/postgres/sampling.rs dbsurveyor-core/src/adapters/postgres/mod.rs dbsurveyor-core/tests/postgres_sampling.rs
git commit -m "feat(postgres): add intelligent ordering strategy detection

- Add OrderingStrategy enum (PrimaryKey, Timestamp, AutoIncrement, Random)
- Detect primary key columns for optimal ordering
- Detect timestamp columns (created_at, updated_at, etc.)
- Fallback to auto-increment or random sampling
- Add order_by_clause() generation for SQL queries"
```

---

## Task 6.2: Implement Data Sampling with Rate Limiting

**Files:**
- Modify: `dbsurveyor-core/src/adapters/postgres/sampling.rs`
- Modify: `dbsurveyor-core/src/adapters/config/sampling.rs`
- Test: `dbsurveyor-core/tests/postgres_sampling.rs`

### Step 1: Write failing test for rate-limited sampling

```rust
#[tokio::test]
async fn test_sample_data_with_rate_limit() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();

    // Create and populate test table
    adapter.execute("CREATE TABLE test_sample (id SERIAL PRIMARY KEY, value TEXT)").await.unwrap();
    for i in 0..100 {
        adapter.execute(&format!("INSERT INTO test_sample (value) VALUES ('row{}')", i)).await.unwrap();
    }

    let sampling_config = SamplingConfig::builder()
        .sample_size(10)
        .queries_per_second(100.0)
        .build()
        .unwrap();

    let samples = adapter.sample_table("public", "test_sample", &sampling_config).await.unwrap();

    assert_eq!(samples.rows.len(), 10);
    assert_eq!(samples.total_rows, 100);
    assert!(matches!(samples.strategy, OrderingStrategy::PrimaryKey { .. }));
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_sample_data_with_rate_limit --features postgresql --no-capture`
Expected: FAIL - sample_table method doesn't exist

### Step 3: Implement TableSample structure

```rust
// In dbsurveyor-core/src/adapters/postgres/sampling.rs
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Result of sampling a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSample {
    /// Table name
    pub table_name: String,
    /// Schema name
    pub schema_name: String,
    /// Sampled rows as JSON objects
    pub rows: Vec<JsonValue>,
    /// Number of rows sampled
    pub sample_size: usize,
    /// Estimated total rows in table
    pub total_rows: u64,
    /// Strategy used for ordering
    pub strategy: OrderingStrategy,
    /// When sampling was performed
    pub collected_at: chrono::DateTime<chrono::Utc>,
    /// Any warnings during sampling
    pub warnings: Vec<String>,
}
```

### Step 4: Implement sample_table method

```rust
// In dbsurveyor-core/src/adapters/postgres/sampling.rs
use crate::adapters::config::SamplingConfig;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Sample data from a table with rate limiting
pub async fn sample_table(
    pool: &PgPool,
    schema: &str,
    table: &str,
    config: &SamplingConfig,
) -> Result<TableSample, DbSurveyorError> {
    let mut warnings = Vec::new();

    // Detect ordering strategy
    let strategy = detect_ordering_strategy(pool, schema, table).await?;

    if matches!(strategy, OrderingStrategy::Random) {
        warnings.push("No reliable ordering found - using random sampling".to_string());
    }

    // Get total row count estimate
    let count_query = format!(
        "SELECT reltuples::bigint FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 AND c.relname = $2"
    );
    let total_rows: i64 = sqlx::query_scalar(&count_query)
        .bind(schema)
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    // Build sample query
    let order_clause = strategy.order_by_clause(true); // DESC for most recent
    let sample_query = format!(
        "SELECT row_to_json(t.*) FROM \"{}\".\"{}\" t {} LIMIT $1",
        schema, table, order_clause
    );

    // Apply rate limiting
    let delay = Duration::from_secs_f64(1.0 / config.queries_per_second);
    sleep(delay).await;

    // Execute sample query
    let rows: Vec<JsonValue> = sqlx::query_scalar(&sample_query)
        .bind(config.sample_size as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    Ok(TableSample {
        table_name: table.to_string(),
        schema_name: schema.to_string(),
        sample_size: rows.len(),
        rows,
        total_rows: total_rows.max(0) as u64,
        strategy,
        collected_at: chrono::Utc::now(),
        warnings,
    })
}
```

### Step 5: Add to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/mod.rs
impl PostgresAdapter {
    /// Sample data from a table with rate limiting and intelligent ordering
    pub async fn sample_table(
        &self,
        schema: &str,
        table: &str,
        config: &SamplingConfig,
    ) -> Result<TableSample, DbSurveyorError> {
        sampling::sample_table(&self.pool, schema, table, config).await
    }
}
```

### Step 6: Run tests to verify

Run: `cd dbsurveyor-core && cargo nextest run test_sample_data --features postgresql --no-capture`
Expected: PASS

### Step 7: Commit

```bash
git add dbsurveyor-core/src/adapters/postgres/sampling.rs dbsurveyor-core/src/adapters/config/sampling.rs dbsurveyor-core/tests/postgres_sampling.rs
git commit -m "feat(postgres): add data sampling with rate limiting

- Add TableSample structure for sample results
- Implement sample_table() with intelligent ordering
- Add rate limiting via queries_per_second config
- Include row count estimates and warnings"
```

---

## Task 7.1: Implement Database Enumeration

**Files:**
- Create: `dbsurveyor-core/src/adapters/postgres/enumeration.rs`
- Modify: `dbsurveyor-core/src/adapters/postgres/mod.rs`
- Test: `dbsurveyor-core/tests/postgres_multi_database.rs`

### Step 1: Write failing test for database enumeration

```rust
// In dbsurveyor-core/tests/postgres_multi_database.rs
#[tokio::test]
async fn test_list_databases() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();

    // Create additional databases
    adapter.execute("CREATE DATABASE testdb1").await.ok();
    adapter.execute("CREATE DATABASE testdb2").await.ok();

    let databases = adapter.list_databases().await.unwrap();

    // Should include postgres and our test databases, but not templates
    assert!(databases.iter().any(|d| d.name == "postgres"));
    assert!(databases.iter().any(|d| d.name == "testdb1"));
    assert!(databases.iter().any(|d| d.name == "testdb2"));
    assert!(!databases.iter().any(|d| d.name == "template0"));
    assert!(!databases.iter().any(|d| d.name == "template1"));
}

#[tokio::test]
async fn test_list_databases_with_system_included() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();

    let databases = adapter.list_databases_with_options(true).await.unwrap();

    // Should include template databases when include_system is true
    assert!(databases.iter().any(|d| d.is_system_database));
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_list_databases --features postgresql --no-capture`
Expected: FAIL - list_databases doesn't exist

### Step 3: Create database enumeration types and implementation

```rust
// In dbsurveyor-core/src/adapters/postgres/enumeration.rs
use crate::error::DbSurveyorError;
use sqlx::{PgPool, Row};

/// Information about a discovered database
#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub name: String,
    pub owner: String,
    pub encoding: String,
    pub collation: String,
    pub size_bytes: Option<u64>,
    pub is_system_database: bool,
    pub is_accessible: bool,
}

/// System databases that are excluded by default
const SYSTEM_DATABASES: &[&str] = &["template0", "template1"];

/// List all accessible databases on the server
pub async fn list_databases(
    pool: &PgPool,
    include_system: bool,
) -> Result<Vec<DatabaseInfo>, DbSurveyorError> {
    let query = r#"
        SELECT
            d.datname as name,
            r.rolname as owner,
            pg_encoding_to_char(d.encoding) as encoding,
            d.datcollate as collation,
            pg_database_size(d.datname) as size_bytes,
            d.datistemplate as is_template,
            has_database_privilege(d.datname, 'CONNECT') as is_accessible
        FROM pg_database d
        JOIN pg_roles r ON d.datdba = r.oid
        WHERE d.datallowconn = true
        ORDER BY d.datname
    "#;

    let rows = sqlx::query(query)
        .fetch_all(pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    let mut databases = Vec::new();

    for row in rows {
        let name: String = row.get("name");
        let is_system = SYSTEM_DATABASES.contains(&name.as_str()) || row.get::<bool, _>("is_template");

        if !include_system && is_system {
            continue;
        }

        databases.push(DatabaseInfo {
            name,
            owner: row.get("owner"),
            encoding: row.get("encoding"),
            collation: row.get("collation"),
            size_bytes: row.get::<Option<i64>, _>("size_bytes").map(|v| v as u64),
            is_system_database: is_system,
            is_accessible: row.get("is_accessible"),
        });
    }

    Ok(databases)
}
```

### Step 4: Add to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/mod.rs
mod enumeration;
pub use enumeration::DatabaseInfo;

impl PostgresAdapter {
    /// List all accessible databases, excluding system databases by default
    pub async fn list_databases(&self) -> Result<Vec<DatabaseInfo>, DbSurveyorError> {
        enumeration::list_databases(&self.pool, false).await
    }

    /// List databases with option to include system databases
    pub async fn list_databases_with_options(
        &self,
        include_system: bool,
    ) -> Result<Vec<DatabaseInfo>, DbSurveyorError> {
        enumeration::list_databases(&self.pool, include_system).await
    }
}
```

### Step 5: Run tests to verify

Run: `cd dbsurveyor-core && cargo nextest run test_list_databases --features postgresql --no-capture`
Expected: PASS

### Step 6: Commit

```bash
git add dbsurveyor-core/src/adapters/postgres/enumeration.rs dbsurveyor-core/src/adapters/postgres/mod.rs dbsurveyor-core/tests/postgres_multi_database.rs
git commit -m "feat(postgres): add database enumeration for multi-db collection

- Add DatabaseInfo structure with size, encoding, accessibility
- Implement list_databases() with system database filtering
- Query pg_database for server-level enumeration
- Check has_database_privilege for access detection"
```

---

## Task 7.2: Implement Per-Database Connection Management

**Files:**
- Modify: `dbsurveyor-core/src/adapters/postgres/enumeration.rs`
- Modify: `dbsurveyor-core/src/adapters/postgres/connection.rs`
- Test: `dbsurveyor-core/tests/postgres_multi_database.rs`

### Step 1: Write failing test for connecting to specific database

```rust
#[tokio::test]
async fn test_connect_to_database() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();

    // Create a test database
    adapter.execute("CREATE DATABASE testdb_connect").await.ok();

    // Connect to the specific database
    let db_adapter = adapter.connect_to_database("testdb_connect").await.unwrap();

    // Verify we're connected to the right database
    let current_db: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db_adapter.pool)
        .await
        .unwrap();

    assert_eq!(current_db, "testdb_connect");
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_connect_to_database --features postgresql --no-capture`
Expected: FAIL - connect_to_database doesn't exist

### Step 3: Implement connect_to_database method

```rust
// In dbsurveyor-core/src/adapters/postgres/connection.rs
impl PostgresAdapter {
    /// Create a new adapter connected to a specific database
    ///
    /// Uses the same connection configuration but targets a different database.
    pub async fn connect_to_database(&self, database: &str) -> Result<PostgresAdapter, DbSurveyorError> {
        // Build new connection string with different database
        let new_url = self.connection_url_for_database(database)?;
        Self::connect(&new_url, self.config.clone()).await
    }

    /// Generate connection URL for a different database
    fn connection_url_for_database(&self, database: &str) -> Result<String, DbSurveyorError> {
        // Validate database name
        if database.is_empty() || database.len() > 63 {
            return Err(DbSurveyorError::configuration("invalid database name length"));
        }

        // Check for dangerous characters
        if database.contains(';') || database.contains('\'') || database.contains('"') {
            return Err(DbSurveyorError::configuration("database name contains invalid characters"));
        }

        // Replace database in URL
        // This is a simplified implementation - production would parse URL properly
        let base_url = self.safe_connection_info.to_string();
        let url = url::Url::parse(&base_url)
            .map_err(|_| DbSurveyorError::configuration("invalid base URL"))?;

        let mut new_url = url.clone();
        new_url.set_path(&format!("/{}", database));

        Ok(new_url.to_string())
    }
}
```

### Step 4: Run tests to verify

Run: `cd dbsurveyor-core && cargo nextest run test_connect_to_database --features postgresql --no-capture`
Expected: PASS

### Step 5: Commit

```bash
git add dbsurveyor-core/src/adapters/postgres/connection.rs dbsurveyor-core/tests/postgres_multi_database.rs
git commit -m "feat(postgres): add per-database connection management

- Add connect_to_database() for multi-db collection
- Validate database names for security
- Preserve connection configuration across databases"
```

---

## Task 7.3: Implement Multi-Database Collection Orchestration

**Files:**
- Create: `dbsurveyor-core/src/adapters/postgres/multi_database.rs`
- Modify: `dbsurveyor-core/src/adapters/postgres/mod.rs`
- Test: `dbsurveyor-core/tests/postgres_multi_database.rs`

### Step 1: Write failing test for multi-database collection

```rust
#[tokio::test]
async fn test_collect_all_databases() {
    let container = postgres_container().await;
    let url = container.connection_string();

    let adapter = PostgresAdapter::connect(&url, ConnectionConfig::default()).await.unwrap();

    // Create test databases with tables
    adapter.execute("CREATE DATABASE multidb1").await.ok();
    adapter.execute("CREATE DATABASE multidb2").await.ok();

    let multidb1_adapter = adapter.connect_to_database("multidb1").await.unwrap();
    multidb1_adapter.execute("CREATE TABLE test1 (id INT)").await.unwrap();

    let multidb2_adapter = adapter.connect_to_database("multidb2").await.unwrap();
    multidb2_adapter.execute("CREATE TABLE test2 (id INT)").await.unwrap();

    // Collect from all databases
    let config = MultiDatabaseConfig::default();
    let results = adapter.collect_all_databases(&config).await.unwrap();

    assert!(results.databases.len() >= 2);
    assert!(results.databases.iter().any(|d| d.database_info.name == "multidb1"));
    assert!(results.databases.iter().any(|d| d.database_info.name == "multidb2"));
    assert_eq!(results.collection_metadata.collected_databases, results.databases.len());
}
```

### Step 2: Run test to verify it fails

Run: `cd dbsurveyor-core && cargo nextest run test_collect_all_databases --features postgresql --no-capture`
Expected: FAIL - MultiDatabaseConfig and collect_all_databases don't exist

### Step 3: Implement multi-database collection types

```rust
// In dbsurveyor-core/src/adapters/postgres/multi_database.rs
use crate::error::DbSurveyorError;
use crate::models::DatabaseSchema;
use super::{PostgresAdapter, DatabaseInfo};
use std::time::{Duration, Instant};

/// Configuration for multi-database collection
#[derive(Debug, Clone)]
pub struct MultiDatabaseConfig {
    /// Maximum concurrent database collections
    pub max_concurrency: usize,
    /// Include system databases (template0, template1, postgres)
    pub include_system: bool,
    /// Database name patterns to exclude (glob patterns)
    pub exclude_patterns: Vec<String>,
    /// Continue on error (collect remaining databases)
    pub continue_on_error: bool,
}

impl Default for MultiDatabaseConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            include_system: false,
            exclude_patterns: Vec::new(),
            continue_on_error: true,
        }
    }
}

/// Result of multi-database collection
#[derive(Debug)]
pub struct MultiDatabaseResult {
    /// Server information
    pub server_info: ServerInfo,
    /// Collected database schemas
    pub databases: Vec<DatabaseSchema>,
    /// Failed database collections
    pub failures: Vec<DatabaseFailure>,
    /// Collection metadata
    pub collection_metadata: MultiDatabaseMetadata,
}

/// Server-level information
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub server_type: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub total_databases: usize,
    pub connection_user: String,
    pub has_superuser_privileges: bool,
}

/// Failed database collection
#[derive(Debug, Clone)]
pub struct DatabaseFailure {
    pub database_name: String,
    pub error: String,
}

/// Multi-database collection metadata
#[derive(Debug, Clone)]
pub struct MultiDatabaseMetadata {
    pub collected_at: chrono::DateTime<chrono::Utc>,
    pub collection_duration_ms: u64,
    pub total_databases: usize,
    pub collected_databases: usize,
    pub failed_databases: usize,
    pub system_databases_excluded: usize,
}
```

### Step 4: Implement collect_all_databases

```rust
// In dbsurveyor-core/src/adapters/postgres/multi_database.rs
use futures::stream::{self, StreamExt};

/// Collect schemas from all accessible databases
pub async fn collect_all_databases(
    adapter: &PostgresAdapter,
    config: &MultiDatabaseConfig,
) -> Result<MultiDatabaseResult, DbSurveyorError> {
    let start_time = Instant::now();

    // Get server info
    let server_info = get_server_info(adapter).await?;

    // List all databases
    let all_databases = adapter.list_databases_with_options(config.include_system).await?;
    let system_excluded = if config.include_system { 0 } else {
        adapter.list_databases_with_options(true).await?.len() - all_databases.len()
    };

    // Filter by exclude patterns
    let databases: Vec<_> = all_databases.into_iter()
        .filter(|db| {
            !config.exclude_patterns.iter().any(|pattern| {
                glob_match::glob_match(pattern, &db.name)
            })
        })
        .filter(|db| db.is_accessible)
        .collect();

    let total_databases = databases.len();

    // Collect from each database with concurrency limit
    let results: Vec<_> = stream::iter(databases)
        .map(|db| {
            let adapter = adapter.clone();
            async move {
                collect_single_database(&adapter, &db).await
            }
        })
        .buffer_unordered(config.max_concurrency)
        .collect()
        .await;

    // Separate successes and failures
    let mut schemas = Vec::new();
    let mut failures = Vec::new();

    for result in results {
        match result {
            Ok(schema) => schemas.push(schema),
            Err((db_name, error)) => {
                if !config.continue_on_error {
                    return Err(error);
                }
                failures.push(DatabaseFailure {
                    database_name: db_name,
                    error: error.to_string(),
                });
            }
        }
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    Ok(MultiDatabaseResult {
        server_info,
        databases: schemas,
        failures: failures.clone(),
        collection_metadata: MultiDatabaseMetadata {
            collected_at: chrono::Utc::now(),
            collection_duration_ms: duration_ms,
            total_databases,
            collected_databases: schemas.len(),
            failed_databases: failures.len(),
            system_databases_excluded: system_excluded,
        },
    })
}

async fn collect_single_database(
    adapter: &PostgresAdapter,
    db: &DatabaseInfo,
) -> Result<DatabaseSchema, (String, DbSurveyorError)> {
    let db_adapter = adapter.connect_to_database(&db.name).await
        .map_err(|e| (db.name.clone(), e))?;

    db_adapter.collect_schema().await
        .map_err(|e| (db.name.clone(), e))
}

async fn get_server_info(adapter: &PostgresAdapter) -> Result<ServerInfo, DbSurveyorError> {
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    let user: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| DbSurveyorError::query_failed(&e.to_string()))?;

    let is_superuser: bool = sqlx::query_scalar("SELECT usesuper FROM pg_user WHERE usename = current_user")
        .fetch_one(&adapter.pool)
        .await
        .unwrap_or(false);

    Ok(ServerInfo {
        server_type: "PostgreSQL".to_string(),
        version,
        host: adapter.safe_connection_info.host.clone(),
        port: adapter.safe_connection_info.port,
        total_databases: 0, // Will be filled by caller
        connection_user: user,
        has_superuser_privileges: is_superuser,
    })
}
```

### Step 5: Add to PostgresAdapter

```rust
// In dbsurveyor-core/src/adapters/postgres/mod.rs
mod multi_database;
pub use multi_database::{MultiDatabaseConfig, MultiDatabaseResult, ServerInfo, DatabaseFailure, MultiDatabaseMetadata};

impl PostgresAdapter {
    /// Collect schemas from all accessible databases on the server
    pub async fn collect_all_databases(
        &self,
        config: &MultiDatabaseConfig,
    ) -> Result<MultiDatabaseResult, DbSurveyorError> {
        multi_database::collect_all_databases(self, config).await
    }
}
```

### Step 6: Run tests to verify

Run: `cd dbsurveyor-core && cargo nextest run test_collect_all_databases --features postgresql --no-capture`
Expected: PASS

### Step 7: Commit

```bash
git add dbsurveyor-core/src/adapters/postgres/multi_database.rs dbsurveyor-core/src/adapters/postgres/mod.rs dbsurveyor-core/tests/postgres_multi_database.rs
git commit -m "feat(postgres): add multi-database collection orchestration

- Add MultiDatabaseConfig for concurrency and filtering
- Implement collect_all_databases() with parallel collection
- Add ServerInfo for server-level metadata
- Handle partial failures with continue_on_error
- Track collection statistics in MultiDatabaseMetadata"
```

---

## Summary

This plan covers Tasks 5.1-5.2 (connection pooling), 6.1-6.2 (data sampling), and 7.1-7.3 (multi-database collection). Each task is broken into small, testable steps following TDD principles.

**Key features implemented:**
1. **Connection Pooling**: acquire(), pool_stats(), environment variable configuration
2. **Data Sampling**: OrderingStrategy detection, rate-limited sampling, TableSample results
3. **Multi-Database**: Database enumeration, per-database connections, parallel collection

**Total commits:** 8 focused commits with clear, atomic changes.

---

Plan complete and saved to `docs/plans/2026-01-27-postgresql-advanced-features.md`. Two execution options:

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

Which approach?
