# Design Document

## Overview

The database schema collection and documentation system implements a dual-binary architecture with a collector (dbsurveyor-collect) and postprocessor (dbsurveyor). The system provides secure, offline-capable database introspection across multiple database engines including relational databases (PostgreSQL, MySQL, SQLite, SQL Server, Oracle), NoSQL databases (MongoDB, Cassandra), and columnar databases (ClickHouse, BigQuery).

The design emphasizes security-first principles with zero telemetry, offline operation, and comprehensive credential protection while maintaining high performance and extensibility through a plugin architecture.

## Architecture

### High-Level System Architecture

```mermaid
graph TB
    subgraph "Target Environment"
        DB[(Database)]
        Collector[dbsurveyor-collect]
        DB --> Collector
    end
    
    subgraph "Operator Environment"
        PostProcessor[dbsurveyor]
        Reports[Documentation<br/>Reports<br/>Diagrams]
        PostProcessor --> Reports
    end
    
    subgraph "Data Exchange"
        JSON[.dbsurveyor.json]
        Compressed[.dbsurveyor.json.zst]
        Encrypted[.dbsurveyor.enc]
    end
    
    Collector --> JSON
    Collector --> Compressed
    Collector --> Encrypted
    JSON --> PostProcessor
    Compressed --> PostProcessor
    Encrypted --> PostProcessor
```

### Dual-Binary Architecture

The system consists of two independent executables that communicate through structured file interchange:

1. **Collector Binary (dbsurveyor-collect)**

   - Connects to target databases
   - Extracts comprehensive metadata
   - Generates structured output files
   - Operates with minimal network footprint

2. **Postprocessor Binary (dbsurveyor)**

   - Processes collected metadata offline
   - Generates documentation and reports
   - Creates visualizations and diagrams
   - Operates with zero network connectivity

## Components and Interfaces

### CLI Framework

Using `clap` with derive macros for command-line interface:

```rust
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dbsurveyor-collect")]
#[command(about = "Database schema collection tool")]
#[command(version)]
pub struct CollectorCli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<CollectorCommand>,

    /// Database connection URL
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    /// Output file path
    #[arg(short, long, default_value = "schema.dbsurveyor.json")]
    pub output: PathBuf,

    /// Number of sample rows per table
    #[arg(long, default_value = "100")]
    pub sample: u32,

    /// Throttle delay between operations (ms)
    #[arg(long)]
    pub throttle: Option<u64>,

    /// Enable compression
    #[arg(long)]
    pub compress: bool,

    /// Enable encryption
    #[arg(long)]
    pub encrypt: bool,

    /// Collect all accessible databases (requires appropriate privileges)
    #[arg(long)]
    pub all_databases: bool,

    /// Include system databases in collection
    #[arg(long)]
    pub include_system_databases: bool,

    /// Exclude specific databases (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub exclude_databases: Vec<String>,
}

#[derive(Subcommand)]
pub enum CollectorCommand {
    /// Collect schema from database
    Collect(CollectArgs),
    /// Test database connection
    Test(TestArgs),
    /// List supported database types
    List,
}

#[derive(Args)]
pub struct GlobalArgs {
    /// Increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress output
    #[arg(short, long)]
    pub quiet: bool,
}
```

### Template Engine

Using `askama` for Markdown and HTML template generation:

```rust
use askama::Template;
use serde::Serialize;

#[derive(Template, Serialize)]
#[template(path = "schema_report.md")]
pub struct SchemaReportTemplate {
    pub database_info: DatabaseInfo,
    pub tables: Vec<Table>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub generation_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Template, Serialize)]
#[template(path = "table_detail.md")]
pub struct TableDetailTemplate {
    pub table: Table,
    pub columns: Vec<Column>,
    pub foreign_keys: Vec<ForeignKey>,
    pub samples: Vec<TableSample>,
}

// Template files in templates/ directory:
// - schema_report.md: Main report template
// - table_detail.md: Individual table documentation
// - sql_reconstruction.sql: SQL DDL generation template
```

### Logging Framework

Using `tracing` ecosystem for structured logging:

```rust
use tracing::{info, warn, error, debug, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logging(verbose: u8, quiet: bool) -> Result<(), LoggingError> {
    let level = match (quiet, verbose) {
        (true, _) => tracing::Level::ERROR,
        (false, 0) => tracing::Level::INFO,
        (false, 1) => tracing::Level::DEBUG,
        (false, _) => tracing::Level::TRACE,
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("dbsurveyor={}", level).into())
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    Ok(())
}

#[instrument(skip(connection_string))]
pub async fn collect_database_schema(
    connection_string: &str,
    config: CollectionConfig,
) -> Result<DatabaseSchema, CollectionError> {
    info!("Starting schema collection");
    debug!("Collection config: {:?}", config);
    
    // Implementation with structured logging
    let adapter = create_adapter(connection_string)?;
    info!("Created adapter for database type: {:?}", adapter.database_type());
    
    let metadata = adapter.collect_metadata().await?;
    info!("Collected metadata for {} tables", metadata.tables.len());
    
    Ok(metadata)
}
```

### Database Adapter System

The core of the collector is a trait-based adapter system that provides unified access to diverse database engines using the established technology stack:

```rust
use async_trait::async_trait;
use sqlx::{Pool, Postgres, MySql, Sqlite};
use tiberius::Client;
use mongodb::Client as MongoClient;

#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    type Connection;
    type Error: std::error::Error + Send + Sync + 'static;
    
    async fn connect(&self, connection_string: &str) -> Result<Self::Connection, Self::Error>;
    async fn collect_metadata(&self, conn: &Self::Connection) -> Result<DatabaseMetadata, Self::Error>;
    async fn sample_data(&self, conn: &Self::Connection, config: &SamplingConfig) -> Result<Vec<TableSample>, Self::Error>;
    async fn test_connection(&self, conn: &Self::Connection) -> Result<(), Self::Error>;
    
    fn database_type(&self) -> DatabaseType;
    fn supports_feature(&self, feature: AdapterFeature) -> bool;
}

// Concrete adapter implementations using preferred crates
pub struct PostgresAdapter {
    pool_options: sqlx::postgres::PgPoolOptions,
}

impl PostgresAdapter {
    /// Lists all accessible databases on the PostgreSQL server
    async fn list_databases(
        &self,
        conn: &sqlx::PgPool,
    ) -> Result<Vec<DatabaseInfo>, AdapterError> {
        let query = r#"
            SELECT 
                d.datname as name,
                pg_database_size(d.datname) as size_bytes,
                r.rolname as owner,
                d.encoding,
                d.datcollate as collation,
                d.datistemplate as is_template,
                has_database_privilege(d.datname, 'CONNECT') as can_connect
            FROM pg_database d
            LEFT JOIN pg_roles r ON d.datdba = r.oid
            WHERE d.datallowconn = true
            ORDER BY d.datname
        "#;
        
        let rows = sqlx::query(query).fetch_all(conn).await?;
        let mut databases = Vec::new();
        
        for row in rows {
            let name: String = row.get("name");
            let size_bytes: Option<i64> = row.get("size_bytes");
            let owner: Option<String> = row.get("owner");
            let encoding: Option<i32> = row.get("encoding");
            let collation: Option<String> = row.get("collation");
            let is_template: bool = row.get("is_template");
            let can_connect: bool = row.get("can_connect");
            
            // Determine if this is a system database
            let is_system_database = matches!(name.as_str(), 
                "template0" | "template1" | "postgres"
            ) || is_template;
            
            let access_level = if can_connect {
                AccessLevel::Full // We'll determine actual access when we connect
            } else {
                AccessLevel::None
            };
            
            databases.push(DatabaseInfo {
                name,
                size_bytes: size_bytes.map(|s| s as u64),
                owner,
                encoding: encoding.map(|e| format!("Encoding {}", e)),
                collation,
                is_system_database,
                access_level,
                collection_status: CollectionStatus::Success, // Will be updated during collection
            });
        }
        
        Ok(databases)
    }
    
    /// Connects to a specific database using the base connection parameters
    async fn connect_to_database(
        &self,
        base_connection_string: &str,
        database_name: &str,
    ) -> Result<sqlx::PgPool, AdapterError> {
        // Parse the base connection string and replace the database name
        let mut url = url::Url::parse(base_connection_string)
            .map_err(|e| AdapterError::InvalidConnectionString(e.to_string()))?;
        
        // Set the database name in the path
        url.set_path(&format!("/{}", database_name));
        
        // Create connection pool for this specific database
        let pool = sqlx::PgPool::connect(&url.to_string()).await
            .map_err(|e| AdapterError::ConnectionFailed {
                database: database_name.to_string(),
                source: Box::new(e),
            })?;
        
        Ok(pool)
    }
    
    /// Collects schemas from all accessible databases
    pub async fn collect_all_databases(
        &self,
        connection_string: &str,
        config: &CollectionConfig,
    ) -> Result<DatabaseServerSchema, AdapterError> {
        // First, connect to the server (usually to 'postgres' database or without specifying database)
        let server_conn = self.connect(connection_string).await?;
        
        // Check if user has superuser privileges
        let is_superuser: bool = sqlx::query_scalar(
            "SELECT usesuper FROM pg_user WHERE usename = current_user"
        ).fetch_one(&server_conn).await.unwrap_or(false);
        
        // Get server information
        let version: String = sqlx::query_scalar("SELECT version()")
            .fetch_one(&server_conn).await?;
        
        let current_user: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(&server_conn).await?;
        
        // List all databases
        let mut database_infos = self.list_databases(&server_conn).await?;
        
        // Filter databases based on configuration
        if !config.include_system_databases {
            database_infos.retain(|db| !db.is_system_database);
        }
        
        let total_databases = database_infos.len();
        let mut collected_databases = Vec::new();
        let mut collection_errors = Vec::new();
        
        // Collect schema from each database
        for mut db_info in database_infos {
            if config.exclude_databases.contains(&db_info.name) {
                db_info.collection_status = CollectionStatus::Skipped {
                    reason: "Excluded by configuration".to_string(),
                };
                continue;
            }
            
            match self.collect_single_database(connection_string, &db_info.name, config).await {
                Ok(mut schema) => {
                    schema.database_info = db_info.clone();
                    db_info.collection_status = CollectionStatus::Success;
                    collected_databases.push(schema);
                }
                Err(e) => {
                    let error_msg = format!("Failed to collect database '{}': {}", db_info.name, e);
                    collection_errors.push(error_msg.clone());
                    db_info.collection_status = CollectionStatus::Failed { error: error_msg };
                    
                    // Still add the database info even if collection failed
                    let failed_schema = DatabaseSchema {
                        format_version: "1.0".to_string(),
                        database_info: db_info.clone(),
                        tables: Vec::new(),
                        views: Vec::new(),
                        indexes: Vec::new(),
                        constraints: Vec::new(),
                        procedures: Vec::new(),
                        functions: Vec::new(),
                        triggers: Vec::new(),
                        custom_types: Vec::new(),
                        collection_metadata: CollectionMetadata {
                            collected_at: chrono::Utc::now(),
                            collection_duration_ms: 0,
                            collector_version: env!("CARGO_PKG_VERSION").to_string(),
                            warnings: vec![format!("Collection failed: {}", e)],
                        },
                    };
                    collected_databases.push(failed_schema);
                }
            }
            
            // Throttle between database collections if configured
            if let Some(throttle_ms) = config.throttle_ms {
                tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
            }
        }
        
        // Parse connection string for server info
        let url = url::Url::parse(connection_string)
            .map_err(|e| AdapterError::InvalidConnectionString(e.to_string()))?;
        
        let server_info = ServerInfo {
            server_type: DatabaseType::PostgreSQL,
            version,
            host: url.host_str().unwrap_or("localhost").to_string(),
            port: url.port(),
            total_databases,
            collected_databases: collected_databases.len(),
            system_databases_excluded: if config.include_system_databases { 0 } else { 
                // Count system databases that were filtered out
                database_infos.iter().filter(|db| db.is_system_database).count()
            },
            connection_user: current_user,
            has_superuser_privileges: is_superuser,
            collection_mode: CollectionMode::MultiDatabase {
                discovered: total_databases,
                collected: collected_databases.len(),
                failed: collection_errors.len(),
            },
        };
        
        Ok(DatabaseServerSchema {
            format_version: "1.0".to_string(),
            server_info,
            databases: collected_databases,
            collection_metadata: CollectionMetadata {
                collected_at: chrono::Utc::now(),
                collection_duration_ms: 0, // TODO: Track total duration
                collector_version: env!("CARGO_PKG_VERSION").to_string(),
                warnings: collection_errors,
            },
        })
    }
    
    /// Collects schema from a single database
    async fn collect_single_database(
        &self,
        base_connection_string: &str,
        database_name: &str,
        config: &CollectionConfig,
    ) -> Result<DatabaseSchema, AdapterError> {
        let db_conn = self.connect_to_database(base_connection_string, database_name).await?;
        
        // Use existing metadata collection logic
        let metadata = self.collect_metadata(&db_conn).await?;
        
        Ok(metadata)
    }
    
    /// Determines the best ordering strategy for a PostgreSQL table
    async fn determine_ordering_strategy(
        &self,
        conn: &sqlx::PgPool,
        table: &Table,
        config: &SamplingConfig,
    ) -> Result<OrderingStrategy, AdapterError> {
        // 1. Try primary key first
        if let Some(pk) = &table.primary_key {
            return Ok(OrderingStrategy::PrimaryKey {
                columns: pk.columns.clone(),
            });
        }
        
        // 2. Look for timestamp columns
        for ts_col in &config.timestamp_columns {
            if let Some(column) = table.columns.iter().find(|c| 
                c.name.to_lowercase() == ts_col.to_lowercase()
            ) {
                if matches!(column.data_type, UnifiedDataType::DateTime { .. } | UnifiedDataType::Date) {
                    return Ok(OrderingStrategy::Timestamp {
                        column: column.name.clone(),
                        direction: SortDirection::Descending, // Most recent first
                    });
                }
            }
        }
        
        // 3. Look for auto-increment columns
        for column in &table.columns {
            if column.is_auto_increment {
                return Ok(OrderingStrategy::AutoIncrement {
                    column: column.name.clone(),
                });
            }
        }
        
        // 4. Try system row ID columns
        let system_columns = ["ctid", "rowid", "_rowid_"];
        for sys_col in &system_columns {
            // Check if system column exists (PostgreSQL has ctid)
            let query = format!(
                "SELECT 1 FROM information_schema.columns 
                 WHERE table_name = $1 AND column_name = $2"
            );
            
            let exists: bool = sqlx::query_scalar(&query)
                .bind(&table.name)
                .bind(sys_col)
                .fetch_optional(conn)
                .await?
                .unwrap_or(false);
                
            if exists {
                return Ok(OrderingStrategy::SystemRowId {
                    column: sys_col.to_string(),
                });
            }
        }
        
        // 5. No reliable ordering available
        Ok(OrderingStrategy::Unordered)
    }
    
    /// Samples data from a PostgreSQL table using the determined ordering strategy
    async fn sample_table_data(
        &self,
        conn: &sqlx::PgPool,
        table: &Table,
        config: &SamplingConfig,
    ) -> Result<TableSample, AdapterError> {
        let start_time = std::time::Instant::now();
        
        // Determine ordering strategy
        let ordering_strategy = self.determine_ordering_strategy(conn, table, config).await?;
        
        // Build sampling query
        let (query, sampling_strategy) = match &ordering_strategy {
            OrderingStrategy::PrimaryKey { columns } => {
                let order_clause = columns.join(", ");
                let query = format!(
                    "SELECT * FROM {} ORDER BY {} DESC LIMIT $1",
                    quote_identifier(&table.name),
                    order_clause
                );
                (query, SamplingStrategy::MostRecent { limit: config.sample_size })
            }
            
            OrderingStrategy::Timestamp { column, direction } => {
                let dir = match direction {
                    SortDirection::Descending => "DESC",
                    SortDirection::Ascending => "ASC",
                };
                let query = format!(
                    "SELECT * FROM {} ORDER BY {} {} LIMIT $1",
                    quote_identifier(&table.name),
                    quote_identifier(column),
                    dir
                );
                (query, SamplingStrategy::MostRecent { limit: config.sample_size })
            }
            
            OrderingStrategy::AutoIncrement { column } => {
                let query = format!(
                    "SELECT * FROM {} ORDER BY {} DESC LIMIT $1",
                    quote_identifier(&table.name),
                    quote_identifier(column)
                );
                (query, SamplingStrategy::MostRecent { limit: config.sample_size })
            }
            
            OrderingStrategy::SystemRowId { column } => {
                let query = format!(
                    "SELECT * FROM {} ORDER BY {} DESC LIMIT $1",
                    quote_identifier(&table.name),
                    column // System columns don't need quoting
                );
                (query, SamplingStrategy::MostRecent { limit: config.sample_size })
            }
            
            OrderingStrategy::Unordered => {
                // Use TABLESAMPLE for random sampling on large tables
                let query = format!(
                    "SELECT * FROM {} TABLESAMPLE SYSTEM(1) LIMIT $1",
                    quote_identifier(&table.name)
                );
                (query, SamplingStrategy::Random { limit: config.sample_size })
            }
        };
        
        // Execute sampling query with timeout
        let rows = tokio::time::timeout(
            Duration::from_secs(config.query_timeout_secs),
            sqlx::query(&query)
                .bind(config.sample_size as i64)
                .fetch_all(conn)
        ).await
        .map_err(|_| AdapterError::QueryTimeout)?
        .map_err(AdapterError::Database)?;
        
        // Convert rows to JSON - COLLECTOR NEVER REDACTS DATA
        let mut sample_rows = Vec::new();
        let mut warnings = Vec::new();
        
        for row in rows {
            let mut row_data = serde_json::Map::new();
            
            for (i, column) in table.columns.iter().enumerate() {
                let value: Option<serde_json::Value> = match &column.data_type {
                    UnifiedDataType::String { .. } => {
                        let str_val: Option<String> = row.try_get(i).ok();
                        str_val.map(serde_json::Value::String)
                    }
                    UnifiedDataType::Integer { .. } => {
                        let int_val: Option<i64> = row.try_get(i).ok();
                        int_val.map(serde_json::Value::from)
                    }
                    UnifiedDataType::Boolean => {
                        let bool_val: Option<bool> = row.try_get(i).ok();
                        bool_val.map(serde_json::Value::from)
                    }
                    UnifiedDataType::Float { .. } => {
                        let float_val: Option<f64> = row.try_get(i).ok();
                        float_val.map(serde_json::Value::from)
                    }
                    UnifiedDataType::DateTime { .. } => {
                        let dt_val: Option<chrono::DateTime<chrono::Utc>> = row.try_get(i).ok();
                        dt_val.map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                    }
                    _ => {
                        // Generic handling for complex types - preserve as string
                        let str_val: Option<String> = row.try_get(i).ok();
                        str_val.map(serde_json::Value::String)
                    }
                };
                
                row_data.insert(column.name.clone(), value.unwrap_or(serde_json::Value::Null));
            }
            
            sample_rows.push(serde_json::Value::Object(row_data));
        }
        
        // Generate warnings about potentially sensitive data (detection only, no redaction)
        if config.warn_sensitive {
            for pattern in &config.sensitive_detection_patterns {
                if table.columns.iter().any(|col| {
                    if let Ok(regex) = regex::Regex::new(&pattern.pattern) {
                        regex.is_match(&col.name)
                    } else {
                        false
                    }
                }) {
                    warnings.push(format!(
                        "WARNING: {} - Table '{}' may contain sensitive data. Review samples before sharing.",
                        pattern.description,
                        table.name
                    ));
                }
            }
        }
        
        // Get total row count for context
        let count_query = format!("SELECT COUNT(*) FROM {}", quote_identifier(&table.name));
        let total_rows: Option<i64> = sqlx::query_scalar(&count_query)
            .fetch_optional(conn)
            .await?;
        
        let query_duration = start_time.elapsed();
        
        Ok(TableSample {
            table_name: table.name.clone(),
            schema_name: table.schema.clone(),
            rows: sample_rows,
            sample_size: config.sample_size,
            total_rows: total_rows.map(|n| n as u64),
            sampling_strategy,
            collected_at: chrono::Utc::now(),
            warnings: warnings.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect(),
        })
    }
}

// Helper functions for data sampling
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace("\"", "\"\""))
}

/// Postprocessor-only redaction functionality
/// The collector NEVER redacts data - this is purely for postprocessor use
pub mod postprocessor_redaction {
    use super::*;
    
    pub fn apply_redaction(
        sample: &TableSample,
        config: &RedactionConfig,
    ) -> Result<TableSample, RedactionError> {
        if !config.enabled || matches!(config.mode, RedactionMode::None) {
            return Ok(sample.clone());
        }
        
        let mut redacted_sample = sample.clone();
        redacted_sample.rows = sample.rows.iter()
            .map(|row| redact_row_data(row, config))
            .collect::<Result<Vec<_>, _>>()?;
            
        // Add redaction notice to warnings
        redacted_sample.warnings.push(
            "Data has been redacted for privacy. Use --no-redact to see original data.".to_string()
        );
        
        Ok(redacted_sample)
    }
    
    fn redact_row_data(
        row: &serde_json::Value,
        config: &RedactionConfig,
    ) -> Result<serde_json::Value, RedactionError> {
        if let serde_json::Value::Object(obj) = row {
            let mut redacted_obj = serde_json::Map::new();
            
            for (key, value) in obj {
                let redacted_value = redact_field_value(key, value, config)?;
                redacted_obj.insert(key.clone(), redacted_value);
            }
            
            Ok(serde_json::Value::Object(redacted_obj))
        } else {
            Ok(row.clone())
        }
    }
    
    fn redact_field_value(
        field_name: &str,
        value: &serde_json::Value,
        config: &RedactionConfig,
    ) -> Result<serde_json::Value, RedactionError> {
        for pattern in &config.patterns {
            // Check if field name matches
            if let Ok(field_regex) = regex::Regex::new(&pattern.field_pattern) {
                if field_regex.is_match(field_name) {
                    // If there's a value pattern, check the value too
                    if let Some(value_pattern) = &pattern.value_pattern {
                        if let serde_json::Value::String(s) = value {
                            if let Ok(value_regex) = regex::Regex::new(value_pattern) {
                                if value_regex.is_match(s) {
                                    return Ok(serde_json::Value::String(pattern.replacement.clone()));
                                }
                            }
                        }
                    } else {
                        // No value pattern, redact based on field name only
                        return Ok(serde_json::Value::String(pattern.replacement.clone()));
                    }
                }
            }
        }
        
        Ok(value.clone())
    }
    
    #[derive(Debug, thiserror::Error)]
    pub enum RedactionError {
        #[error("Invalid regex pattern: {0}")]
        InvalidRegex(String),
        
        #[error("Redaction failed: {0}")]
        RedactionFailed(String),
    }
}

pub struct MySqlAdapter {
    pool_options: sqlx::mysql::MySqlPoolOptions,
}

pub struct SqliteAdapter {
    pool_options: sqlx::sqlite::SqlitePoolOptions,
}

pub struct SqlServerAdapter {
    config: tiberius::Config,
}

pub struct MongoAdapter {
    client_options: mongodb::options::ClientOptions,
}
```

### Adapter Implementations

#### Relational Database Adapters

- **PostgreSQL Adapter**: Uses sqlx with information_schema and pg_catalog queries
- **MySQL Adapter**: Uses sqlx with information_schema queries
- **SQLite Adapter**: Uses sqlx with sqlite_master table introspection
- **SQL Server Adapter**: Uses tiberius with sys.tables and INFORMATION_SCHEMA
- **Oracle Adapter**: Uses oracle crate with ALL_TABLES and USER_TABLES views

#### NoSQL Database Adapters

- **MongoDB Adapter**: Uses mongodb crate with collection introspection and document sampling
- **Cassandra Adapter**: Uses cassandra-cpp with system keyspace queries

#### Columnar Database Adapters

- **ClickHouse Adapter**: Uses HTTP interface with system.tables queries
- **BigQuery Adapter**: Uses REST API with INFORMATION_SCHEMA dataset

### Plugin Architecture

The system supports extensibility through multiple plugin mechanisms, following idiomatic Rust patterns for dynamic dispatch and plugin systems:

```rust
// Primary trait for database adapters - object-safe for dynamic dispatch
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    // ... trait methods as defined above
}

// Plugin loading strategies using enum dispatch for performance
pub enum AdapterPlugin {
    // Static compilation - zero-cost abstraction (primary)
    Compiled(Box<dyn DatabaseAdapter>),
    // WASM runtime - sandboxed execution (extension)
    #[cfg(feature = "wasm-plugins")]
    Wasm(WasmAdapter),
}

// Plugin manager with lazy loading and caching
pub struct PluginManager {
    // Static adapters compiled at build time (primary)
    static_adapters: HashMap<DatabaseType, Box<dyn DatabaseAdapter>>,
    // WASM adapters loaded at runtime (optional extension)
    #[cfg(feature = "wasm-plugins")]
    wasm_adapters: HashMap<DatabaseType, WasmAdapter>,
    #[cfg(feature = "wasm-plugins")]
    wasm_engine: wasmtime::Engine,
    // Configuration for plugin loading
    config: PluginConfig,
}

impl PluginManager {
    pub fn new(config: PluginConfig) -> Self {
        let mut manager = Self {
            static_adapters: HashMap::new(),
            dynamic_adapters: HashMap::new(),
            wasm_engine: None,
            config,
        };
        
        // Register built-in adapters based on feature flags
        #[cfg(feature = "postgres")]
        manager.register_static(DatabaseType::PostgreSQL, Box::new(PostgresAdapter::new()));
        
        #[cfg(feature = "mysql")]
        manager.register_static(DatabaseType::MySQL, Box::new(MySqlAdapter::new()));
        
        manager
    }
    
    pub async fn get_adapter(&mut self, db_type: DatabaseType) -> Result<&dyn DatabaseAdapter, PluginError> {
        // Try static adapters first (fastest, zero-cost)
        if let Some(adapter) = self.static_adapters.get(&db_type) {
            return Ok(adapter.as_ref());
        }
        
        // Try WASM adapters if feature is enabled
        #[cfg(feature = "wasm-plugins")]
        {
            if let Some(wasm_adapter) = self.wasm_adapters.get(&db_type) {
                return Ok(wasm_adapter.as_adapter());
            }
            
            // Attempt to load WASM plugin dynamically
            if let Ok(adapter) = self.load_wasm_plugin(db_type).await {
                self.wasm_adapters.insert(db_type, adapter);
                return Ok(self.wasm_adapters.get(&db_type).unwrap().as_adapter());
            }
        }
        
        Err(PluginError::AdapterNotFound(db_type))
    }
}
```

#### Plugin Loading Strategies

For a single binary collector, we recommend two complementary plugin architectures:

1. **Static Compilation (Primary Approach)**

   - Zero-cost abstraction using Cargo feature flags
   - Compile-time optimization and monomorphization
   - Best performance and security
   - Used for all built-in database adapters
   - Enables selective compilation: `cargo build --features postgres,mysql,sqlite`
   - No runtime overhead or dynamic loading complexity

2. **WASM Plugins (Extension Mechanism)**

   - Runtime loading via wasmtime for third-party adapters
   - Memory and capability isolation with WASI
   - Cross-platform compatibility
   - Suitable for experimental or proprietary database adapters
   - Sandboxed execution prevents security issues
   - Optional feature flag: `--features wasm-plugins`

**Rationale for Excluding Stdio Plugins:**

- Adds complexity of process management and IPC
- Increases attack surface with external processes
- Complicates deployment and distribution
- Performance overhead of serialization/deserialization
- Static compilation + WASM covers all practical use cases

#### Object-Safe Trait Design

Following Rust best practices for trait objects and dynamic dispatch:

```rust
// Object-safe trait design
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    // Avoid generic methods in trait objects
    async fn connect_boxed(&self, params: ConnectionParams) -> Result<Box<dyn Connection>, AdapterError>;
    async fn collect_metadata_boxed(&self, conn: &dyn Connection) -> Result<DatabaseMetadata, AdapterError>;
    
    // Non-generic methods are object-safe
    fn database_type(&self) -> DatabaseType;
    fn adapter_info(&self) -> AdapterInfo;
    fn supports_feature(&self, feature: AdapterFeature) -> bool;
}

// Separate trait for type-safe operations when concrete type is known
pub trait TypedDatabaseAdapter<C: Connection>: DatabaseAdapter {
    async fn connect(&self, params: ConnectionParams) -> Result<C, AdapterError>;
    async fn collect_metadata(&self, conn: &C) -> Result<DatabaseMetadata, AdapterError>;
}

// Blanket implementation to bridge typed and object-safe traits
#[async_trait]
impl<T, C> DatabaseAdapter for T 
where 
    T: TypedDatabaseAdapter<C> + Send + Sync,
    C: Connection + 'static,
{
    async fn connect_boxed(&self, params: ConnectionParams) -> Result<Box<dyn Connection>, AdapterError> {
        let conn = self.connect(params).await?;
        Ok(Box::new(conn))
    }
    
    async fn collect_metadata_boxed(&self, conn: &dyn Connection) -> Result<DatabaseMetadata, AdapterError> {
        let typed_conn = conn.as_any().downcast_ref::<C>()
            .ok_or(AdapterError::InvalidConnectionType)?;
        self.collect_metadata(typed_conn).await
    }
}
```

### Data Models

#### Core Schema Representation

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseServerSchema {
    pub format_version: String, // "1.0"
    pub server_info: ServerInfo,
    pub databases: Vec<DatabaseSchema>,
    pub collection_metadata: CollectionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub format_version: String, // "1.0"
    pub database_info: DatabaseInfo,
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub procedures: Vec<StoredProcedure>,
    pub functions: Vec<Function>,
    pub triggers: Vec<Trigger>,
    pub custom_types: Vec<CustomType>,
    pub collection_metadata: CollectionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub server_type: DatabaseType,
    pub version: String,
    pub host: String,
    pub port: Option<u16>,
    pub total_databases: usize,
    pub collected_databases: usize,
    pub system_databases_excluded: usize,
    pub connection_user: String, // Username used for connection (no password)
    pub has_superuser_privileges: bool,
    pub collection_mode: CollectionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub size_bytes: Option<u64>,
    pub owner: Option<String>,
    pub encoding: Option<String>,
    pub collation: Option<String>,
    pub is_system_database: bool,
    pub access_level: AccessLevel,
    pub collection_status: CollectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionMode {
    /// Single database specified in connection string
    SingleDatabase,
    /// Multiple databases discovered via server enumeration
    MultiDatabase {
        discovered: usize,
        collected: usize,
        failed: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessLevel {
    /// Full read access to database
    Full,
    /// Limited access (some tables/views may be restricted)
    Limited,
    /// No access (database exists but cannot be read)
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionStatus {
    /// Successfully collected schema
    Success,
    /// Partial collection (some objects failed)
    Partial { errors: Vec<String> },
    /// Collection failed
    Failed { error: String },
    /// Skipped (system database or access denied)
    Skipped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<String>, // References to Index objects
    pub row_count: Option<u64>,
    pub size_bytes: Option<u64>,
    pub samples: Vec<TableSample>,
    pub sampling_metadata: Option<SamplingMetadata>,
    pub metadata: TableMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSample {
    pub table_name: String,
    pub schema_name: Option<String>,
    pub rows: Vec<serde_json::Value>, // Sample row data as JSON
    pub sample_size: usize,
    pub total_rows: Option<u64>,
    pub sampling_strategy: SamplingStrategy,
    pub collected_at: chrono::DateTime<chrono::Utc>,
    pub warnings: Vec<String>, // Security warnings about sensitive data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMetadata {
    pub ordering_column: Option<String>,
    pub ordering_strategy: OrderingStrategy,
    pub query_duration_ms: u64,
    pub has_sensitive_data_warning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderingStrategy {
    /// Primary key ordering (preferred)
    PrimaryKey { columns: Vec<String> },
    /// Timestamp column ordering (second choice)
    Timestamp {
        column: String,
        direction: SortDirection,
    },
    /// Auto-increment column (third choice)
    AutoIncrement { column: String },
    /// Row ID or system column (fallback)
    SystemRowId { column: String },
    /// No reliable ordering available
    Unordered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Most recent N rows based on ordering strategy
    MostRecent { limit: usize },
    /// Random sampling (when no good ordering available)
    Random { limit: usize },
    /// First N rows (fallback)
    FirstRows { limit: usize },
    /// No sampling performed
    None { reason: String },
}
```

#### Sampling Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Number of sample rows per table (default: 100)
    pub sample_size: usize,

    /// Maximum total rows to sample across all tables
    pub max_total_samples: Option<usize>,

    /// Tables to include (if empty, include all)
    pub include_tables: Vec<String>,

    /// Tables to exclude
    pub exclude_tables: Vec<String>,

    /// Patterns for tables to exclude (regex)
    pub exclude_patterns: Vec<String>,

    /// Whether to sample system/internal tables
    pub include_system_tables: bool,

    /// Throttle delay between sampling queries (ms)
    pub throttle_ms: Option<u64>,

    /// Query timeout for sampling operations (seconds)
    pub query_timeout_secs: u64,

    /// Preferred timestamp column names for ordering
    pub timestamp_columns: Vec<String>, // ["created_at", "updated_at", "timestamp", "date_created"]

    /// Whether to warn about potentially sensitive data (collector only warns, never redacts)
    pub warn_sensitive: bool,

    /// Patterns for sensitive data detection (for warnings only)
    pub sensitive_detection_patterns: Vec<SensitiveDetectionPattern>,
}

/// Overall collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    /// Sampling configuration
    pub sampling: SamplingConfig,

    /// Whether to collect all databases (multi-database mode)
    pub collect_all_databases: bool,

    /// Whether to include system databases
    pub include_system_databases: bool,

    /// Databases to exclude from collection
    pub exclude_databases: Vec<String>,

    /// Throttle delay between database collections (ms)
    pub throttle_ms: Option<u64>,

    /// Maximum concurrent database connections
    pub max_concurrent_connections: usize,

    /// Connection timeout per database (seconds)
    pub connection_timeout_secs: u64,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            sampling: SamplingConfig::default(),
            collect_all_databases: false,
            include_system_databases: false,
            exclude_databases: Vec::new(),
            throttle_ms: Some(1000),       // 1 second between databases
            max_concurrent_connections: 3, // Conservative for server load
            connection_timeout_secs: 30,
        }
    }
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            sample_size: 100,
            max_total_samples: Some(10_000),
            include_tables: Vec::new(),
            exclude_tables: Vec::new(),
            exclude_patterns: vec![
                "^sys_".to_string(),
                "^information_schema".to_string(),
                "^pg_".to_string(),
                "^mysql\\.".to_string(),
            ],
            include_system_tables: false,
            throttle_ms: None,
            query_timeout_secs: 30,
            timestamp_columns: vec![
                "created_at".to_string(),
                "updated_at".to_string(),
                "timestamp".to_string(),
                "date_created".to_string(),
                "date_modified".to_string(),
                "created".to_string(),
                "modified".to_string(),
            ],
            warn_sensitive: true,
            sensitive_detection_patterns: vec![
                SensitiveDetectionPattern::new("password_field", r"(?i)password|passwd|pwd"),
                SensitiveDetectionPattern::new("email_field", r"(?i)email|e_mail"),
                SensitiveDetectionPattern::new("ssn_field", r"(?i)ssn|social_security"),
                SensitiveDetectionPattern::new(
                    "credit_card_field",
                    r"(?i)credit_card|cc_number|card_num",
                ),
                SensitiveDetectionPattern::new("phone_field", r"(?i)phone|telephone|mobile"),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveDetectionPattern {
    pub name: String,
    pub pattern: String,
    pub description: String,
}

impl SensitiveDetectionPattern {
    pub fn new(name: &str, pattern: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            description: format!("Potential {} detected", name.replace("_", " ")),
        }
    }
}

/// Postprocessor-only redaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    /// Whether to enable redaction in postprocessor output
    pub enabled: bool,

    /// Redaction patterns with replacement values
    pub patterns: Vec<RedactionPattern>,

    /// Whether to allow user override of redaction
    pub allow_override: bool,

    /// Redaction mode
    pub mode: RedactionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RedactionMode {
    /// Conservative: Redact anything that might be sensitive
    Conservative,
    /// Balanced: Redact obvious sensitive data
    Balanced,
    /// Minimal: Only redact explicitly configured patterns
    Minimal,
    /// None: No redaction (user explicitly disabled)
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPattern {
    pub name: String,
    pub field_pattern: String,         // Pattern to match field names
    pub value_pattern: Option<String>, // Optional pattern to match values
    pub replacement: String,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_override: true,
            mode: RedactionMode::Balanced,
            patterns: vec![
                RedactionPattern {
                    name: "password_fields".to_string(),
                    field_pattern: r"(?i)password|passwd|pwd".to_string(),
                    value_pattern: None,
                    replacement: "[REDACTED_PASSWORD]".to_string(),
                },
                RedactionPattern {
                    name: "email_values".to_string(),
                    field_pattern: r"(?i)email".to_string(),
                    value_pattern: Some(
                        r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b".to_string(),
                    ),
                    replacement: "[REDACTED_EMAIL]".to_string(),
                },
                RedactionPattern {
                    name: "ssn_values".to_string(),
                    field_pattern: r"(?i)ssn|social".to_string(),
                    value_pattern: Some(r"\b\d{3}-\d{2}-\d{4}\b".to_string()),
                    replacement: "[REDACTED_SSN]".to_string(),
                },
            ],
        }
    }
}
```

#### NoSQL Schema Adaptation

For NoSQL databases, the system infers schema from document structure:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSchema {
    pub collection_name: String,
    pub inferred_fields: Vec<InferredField>,
    pub document_samples: Vec<serde_json::Value>,
    pub field_statistics: HashMap<String, FieldStats>,
    pub index_information: Vec<IndexInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredField {
    pub name: String,
    pub field_type: InferredType,
    pub occurrence_rate: f64, // Percentage of documents containing this field
    pub nullable: bool,
    pub nested_schema: Option<Box<DocumentSchema>>,
}
```

### Security Layer

#### Credential Management

```rust
pub struct SecureCredentials {
    connection_params: ConnectionParams,
    // Credentials are never stored in structs
}

impl SecureCredentials {
    pub fn from_connection_string(
        url: &str,
    ) -> Result<(ConnectionParams, Credentials), SecurityError> {
        // Parse and separate credentials from connection parameters
    }

    pub fn sanitized_display(&self) -> String {
        // Return connection info without credentials
    }
}

impl Drop for Credentials {
    fn drop(&mut self) {
        // Zero out sensitive data
        self.username.zeroize();
        if let Some(ref mut password) = self.password {
            password.zeroize();
        }
    }
}
```

#### Encryption Implementation

Using the preferred cryptographic libraries (`aes-gcm` and `ring`):

```rust
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, NewAead}};
use ring::{pbkdf2, rand::{SecureRandom, SystemRandom}};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedOutput {
    pub algorithm: String, // "AES-GCM-256"
    pub nonce: Vec<u8>,    // 96-bit random nonce
    pub ciphertext: Vec<u8>,
    pub tag: Vec<u8>,      // Authentication tag (included in ciphertext for AES-GCM)
    pub kdf_params: KdfParams,
    pub format_version: String, // "1.0"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String, // "PBKDF2-SHA256" or "Argon2id"
    pub salt: Vec<u8>,     // Random salt
    pub iterations: u32,   // PBKDF2 iterations or Argon2 time cost
    pub memory_cost: Option<u32>, // Argon2 memory cost
    pub parallelism: Option<u32>, // Argon2 parallelism
}

pub async fn encrypt_schema_data(
    plaintext: &[u8],
    password: &str,
) -> Result<EncryptedOutput, EncryptionError> {
    let rng = SystemRandom::new();
    
    // Generate random salt and nonce
    let mut salt = [0u8; 32];
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut salt)?;
    rng.fill(&mut nonce_bytes)?;
    
    // Derive key using PBKDF2 with ring
    let mut key_bytes = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(100_000).unwrap(),
        &salt,
        password.as_bytes(),
        &mut key_bytes,
    );
    
    // Encrypt with AES-GCM
    let key = Key::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;
    
    Ok(EncryptedOutput {
        algorithm: "AES-GCM-256".to_string(),
        nonce: nonce_bytes.to_vec(),
        ciphertext,
        tag: vec![], // Included in ciphertext for AES-GCM
        kdf_params: KdfParams {
            algorithm: "PBKDF2-SHA256".to_string(),
            salt: salt.to_vec(),
            iterations: 100_000,
            memory_cost: None,
            parallelism: None,
        },
        format_version: "1.0".to_string(),
    })
}

// Compression using zstd (preferred library)
pub fn compress_schema_data(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    zstd::encode_all(data, 3) // Level 3 for good compression/speed balance
        .map_err(|e| CompressionError::CompressionFailed(e.to_string()))
}
```

## Data Models

### Unified Schema Representation

The system uses a unified schema representation that can accommodate both relational and NoSQL database structures:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SchemaObject {
    Table(Table),
    Collection(Collection),
    View(View),
    MaterializedView(MaterializedView),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub database: String,
    pub document_count: Option<u64>,
    pub size_bytes: Option<u64>,
    pub inferred_schema: DocumentSchema,
    pub indexes: Vec<CollectionIndex>,
    pub sharding_info: Option<ShardingInfo>,
}
```

### Data Type Mapping

The system maintains a unified type system that maps database-specific types to common representations:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedDataType {
    // Primitive types
    Integer {
        size: Option<u8>,
        signed: bool,
    },
    Float {
        precision: Option<u8>,
    },
    Decimal {
        precision: Option<u8>,
        scale: Option<u8>,
    },
    String {
        max_length: Option<u32>,
        fixed_length: bool,
    },
    Boolean,
    DateTime {
        timezone_aware: bool,
    },
    Date,
    Time,
    Binary {
        max_length: Option<u32>,
    },

    // Complex types
    Array {
        element_type: Box<UnifiedDataType>,
    },
    Object {
        fields: HashMap<String, UnifiedDataType>,
    },
    Json,
    Xml,

    // Database-specific types
    Custom {
        type_name: String,
        database_type: DatabaseType,
    },
}
```

## Error Handling

### Comprehensive Error System

```rust
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("Database connection failed")]
    Connection(#[from] ConnectionError),

    #[error("Schema collection failed: {context}")]
    Collection {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Data sampling failed: {table}")]
    Sampling {
        table: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Output generation failed")]
    Output(#[from] OutputError),

    #[error("Security violation: {message}")]
    Security { message: String },
}

impl CollectorError {
    pub fn sanitized_message(&self) -> String {
        match self {
            CollectorError::Connection(_) => "Database connection failed".to_string(),
            CollectorError::Security { .. } => "Security policy violation".to_string(),
            _ => self.to_string(),
        }
    }
}
```

## Testing Strategy

### Testing Architecture Overview

The testing strategy employs a multi-layered approach with specialized tools for different testing needs:

- **Testcontainers**: Realistic integration tests with actual database instances
- **Nextest**: Fast, parallel test execution with enhanced reporting
- **Criterion**: Performance benchmarking and regression detection
- **Insta**: Snapshot testing for CLI output validation
- **Proptest**: Property-based testing for edge cases

### Unit Testing Framework

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::*;
    
    #[tokio::test]
    async fn test_postgres_schema_collection() {
        let docker = clients::Cli::default();
        let postgres = docker.run(images::postgres::Postgres::default());
        
        let database_url = format!(
            "postgres://postgres:postgres@localhost:{}/postgres",
            postgres.get_host_port_ipv4(5432)
        );
        
        let adapter = PostgresAdapter::new();
        let connection = adapter.connect(&database_url).await?;
        let metadata = adapter.collect_metadata(&connection).await?;
        
        assert!(!metadata.tables.is_empty());
        assert_eq!(metadata.database_info.database_type, DatabaseType::PostgreSQL);
    }
    
    #[tokio::test]
    async fn test_credential_sanitization() {
        let connection_string = "postgres://user:secret@localhost/db";
        let (params, _creds) = SecureCredentials::from_connection_string(connection_string)?;
        
        let display = params.sanitized_display();
        assert!(!display.contains("secret"));
        assert!(!display.contains("user:secret"));
    }
}
```

### Integration Testing with Testcontainers

Using the modern `testcontainers-modules` crate for realistic database testing:

```rust
use testcontainers_modules::{
    postgres::Postgres,
    mysql::Mysql, 
    mongo::Mongo,
    testcontainers::{
        runners::AsyncRunner,
        core::{WaitFor, IntoContainerPort},
        ImageExt,
    },
};
use std::time::Duration;

pub struct DatabaseTestSuite {
    postgres: testcontainers::ContainerAsync<Postgres>,
    mysql: testcontainers::ContainerAsync<Mysql>,
    mongo: testcontainers::ContainerAsync<Mongo>,
}

impl DatabaseTestSuite {
    pub async fn new() -> Result<Self, TestError> {
        // Start PostgreSQL with custom configuration
        let postgres = Postgres::default()
            .with_env_var("POSTGRES_DB", "testdb")
            .with_env_var("POSTGRES_USER", "testuser")
            .with_env_var("POSTGRES_PASSWORD", "testpass")
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"))
            .with_startup_timeout(Duration::from_secs(60))
            .start()
            .await?;
        
        // Start MySQL with custom configuration
        let mysql = Mysql::default()
            .with_env_var("MYSQL_DATABASE", "testdb")
            .with_env_var("MYSQL_USER", "testuser")
            .with_env_var("MYSQL_PASSWORD", "testpass")
            .with_env_var("MYSQL_ROOT_PASSWORD", "rootpass")
            .with_wait_for(WaitFor::message_on_stdout("ready for connections"))
            .with_startup_timeout(Duration::from_secs(60))
            .start()
            .await?;
        
        // Start MongoDB
        let mongo = Mongo::default()
            .with_wait_for(WaitFor::message_on_stdout("Waiting for connections"))
            .with_startup_timeout(Duration::from_secs(60))
            .start()
            .await?;
        
        Ok(Self { postgres, mysql, mongo })
    }
    
    pub fn postgres_url(&self) -> String {
        let host_port = self.postgres.get_host_port_ipv4(5432).unwrap();
        format!("postgres://testuser:testpass@127.0.0.1:{}/testdb", host_port)
    }
    
    pub fn mysql_url(&self) -> String {
        let host_port = self.mysql.get_host_port_ipv4(3306).unwrap();
        format!("mysql://testuser:testpass@127.0.0.1:{}/testdb", host_port)
    }
    
    pub fn mongo_url(&self) -> String {
        let host_port = self.mongo.get_host_port_ipv4(27017).unwrap();
        format!("mongodb://127.0.0.1:{}", host_port)
    }
}

#[tokio::test]
async fn test_multi_database_collection() -> Result<(), Box<dyn std::error::Error>> {
    let suite = DatabaseTestSuite::new().await?;
    
    // Test PostgreSQL adapter
    let pg_adapter = PostgresAdapter::new();
    let pg_metadata = pg_adapter.collect_from_url(&suite.postgres_url()).await?;
    assert_eq!(pg_metadata.database_type, DatabaseType::PostgreSQL);
    assert_eq!(pg_metadata.format_version, "1.0");
    
    // Test MySQL adapter
    let mysql_adapter = MySqlAdapter::new();
    let mysql_metadata = mysql_adapter.collect_from_url(&suite.mysql_url()).await?;
    assert_eq!(mysql_metadata.database_type, DatabaseType::MySQL);
    assert_eq!(mysql_metadata.format_version, "1.0");
    
    // Test MongoDB adapter
    let mongo_adapter = MongoAdapter::new();
    let mongo_metadata = mongo_adapter.collect_from_url(&suite.mongo_url()).await?;
    assert_eq!(mongo_metadata.database_type, DatabaseType::MongoDB);
    assert_eq!(mongo_metadata.format_version, "1.0");
    
    // Verify unified schema format across all databases
    assert_eq!(pg_metadata.format_version, mysql_metadata.format_version);
    assert_eq!(mysql_metadata.format_version, mongo_metadata.format_version);
    
    Ok(())
}

#[tokio::test]
async fn test_postgres_schema_collection_with_real_data() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = Postgres::default()
        .with_env_var("POSTGRES_DB", "testdb")
        .with_env_var("POSTGRES_USER", "testuser")
        .with_env_var("POSTGRES_PASSWORD", "testpass")
        .start()
        .await?;
    
    let host_port = postgres.get_host_port_ipv4(5432)?;
    let connection_string = format!(
        "postgres://testuser:testpass@127.0.0.1:{}/testdb",
        host_port
    );
    
    // Create test schema
    let pool = sqlx::PgPool::connect(&connection_string).await?;
    sqlx::query(r#"
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            username VARCHAR(50) UNIQUE NOT NULL,
            email VARCHAR(100) NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        
        CREATE TABLE posts (
            id SERIAL PRIMARY KEY,
            user_id INTEGER REFERENCES users(id),
            title VARCHAR(200) NOT NULL,
            content TEXT,
            published BOOLEAN DEFAULT FALSE
        );
        
        INSERT INTO users (username, email) VALUES 
            ('alice', 'alice@example.com'),
            ('bob', 'bob@example.com');
            
        INSERT INTO posts (user_id, title, content, published) VALUES
            (1, 'Hello World', 'This is my first post', true),
            (2, 'Draft Post', 'This is a draft', false);
    "#).execute(&pool).await?;
    
    // Test schema collection
    let adapter = PostgresAdapter::new();
    let metadata = adapter.collect_from_url(&connection_string).await?;
    
    // Verify collected metadata
    assert_eq!(metadata.tables.len(), 2);
    
    let users_table = metadata.tables.iter()
        .find(|t| t.name == "users")
        .expect("users table should be found");
    assert_eq!(users_table.columns.len(), 4);
    
    let posts_table = metadata.tables.iter()
        .find(|t| t.name == "posts")
        .expect("posts table should be found");
    assert_eq!(posts_table.columns.len(), 5);
    
    // Verify foreign key relationships
    assert!(!posts_table.foreign_keys.is_empty());
    
    // Verify sample data collection
    if let Some(users_samples) = users_table.samples.first() {
        assert_eq!(users_samples.rows.len(), 2); // Should have 2 sample rows
    }
    
    Ok(())
}
```

### CLI Testing with Insta Snapshots

```rust
use insta::{assert_snapshot, with_settings};
use std::process::Command;

#[test]
fn test_cli_help_output() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "dbsurveyor-collect", "--", "--help"])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8(output.stdout).unwrap();
    
    with_settings!({
        description => "CLI help output should be stable and informative"
    }, {
        assert_snapshot!(stdout);
    });
}

#[test]
fn test_cli_version_output() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "dbsurveyor", "--", "--version"])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_snapshot!(stdout);
}

#[tokio::test]
async fn test_cli_error_messages() {
    let suite = DatabaseTestSuite::new().await?;
    
    // Test invalid connection string
    let output = Command::new("cargo")
        .args(&[
            "run", "--bin", "dbsurveyor-collect", "--",
            "--database-url", "invalid://connection",
            "--output", "/tmp/test.json"
        ])
        .output()
        .expect("Failed to execute command");
    
    let stderr = String::from_utf8(output.stderr).unwrap();
    
    with_settings!({
        description => "Error messages should be helpful and not leak credentials"
    }, {
        assert_snapshot!(stderr);
    });
    
    // Verify no credentials in error output
    assert!(!stderr.contains("password"));
    assert!(!stderr.contains("secret"));
}
```

### Performance Benchmarking with Criterion

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_schema_collection(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("schema_collection");
    
    // Benchmark different database sizes
    for table_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("postgres", table_count),
            table_count,
            |b, &table_count| {
                b.to_async(&rt).iter(|| async {
                    let test_db = create_test_db_with_tables(table_count).await;
                    let adapter = PostgresAdapter::new();
                    let metadata = adapter.collect_from_url(&test_db.url()).await.unwrap();
                    black_box(metadata)
                });
            },
        );
    }
    
    group.finish();
}

fn bench_output_formats(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("json_serialization", |b| {
        b.to_async(&rt).iter(|| async {
            let schema = create_large_test_schema();
            let json = serde_json::to_string(&schema).unwrap();
            black_box(json)
        });
    });
    
    c.bench_function("compression", |b| {
        b.to_async(&rt).iter(|| async {
            let schema = create_large_test_schema();
            let json = serde_json::to_vec(&schema).unwrap();
            let compressed = zstd::encode_all(&json[..], 3).unwrap();
            black_box(compressed)
        });
    });
    
    c.bench_function("encryption", |b| {
        b.to_async(&rt).iter(|| async {
            let schema = create_large_test_schema();
            let json = serde_json::to_vec(&schema).unwrap();
            let key = EncryptionKey::generate().unwrap();
            let encrypted = encrypt_schema_data(&json, &key).await.unwrap();
            black_box(encrypted)
        });
    });
}

criterion_group!(benches, bench_schema_collection, bench_output_formats);
criterion_main!(benches);
```

### Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_connection_string_parsing(
        username in "[a-zA-Z][a-zA-Z0-9_]{0,30}",
        password in "[a-zA-Z0-9!@#$%^&*()_+]{8,50}",
        host in "[a-zA-Z0-9.-]{1,253}",
        port in 1024u16..65535,
        database in "[a-zA-Z][a-zA-Z0-9_]{0,63}"
    ) {
        let connection_string = format!(
            "postgres://{}:{}@{}:{}/{}",
            username, password, host, port, database
        );

        let (params, creds) = SecureCredentials::from_connection_string(&connection_string)?;

        // Verify parsing correctness
        prop_assert_eq!(params.host, host);
        prop_assert_eq!(params.port, port);
        prop_assert_eq!(params.database, database);
        prop_assert_eq!(creds.username, username);
        prop_assert_eq!(creds.password.as_ref().unwrap(), &password);

        // Verify credential sanitization
        let display = params.sanitized_display();
        prop_assert!(!display.contains(&password));
        prop_assert!(!display.contains(&format!("{}:{}", username, password)));
    }
}
```

### Security Testing

```rust
#[tokio::test]
async fn test_no_network_calls_in_postprocessor() {
    // Mock network to fail all connections
    let _guard = MockNetworkGuard::block_all();
    
    let schema_file = create_test_schema_file().await?;
    let postprocessor = PostProcessor::new(PostProcessorConfig::default());
    
    // Should succeed despite blocked network
    let result = postprocessor.generate_report(&schema_file).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_encryption_randomness() {
    let data = b"test schema data";
    let key = EncryptionKey::generate()?;
    
    let encrypted1 = encrypt_schema_data(data, &key).await?;
    let encrypted2 = encrypt_schema_data(data, &key).await?;
    
    // Same data should produce different ciphertext (random nonce)
    assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    assert_ne!(encrypted1.nonce, encrypted2.nonce);
    
    // Both should decrypt to same plaintext
    assert_eq!(decrypt_schema_data(&encrypted1, &key).await?, data);
    assert_eq!(decrypt_schema_data(&encrypted2, &key).await?, data);
}

#[test]
fn test_memory_safety_with_credentials() {
    let mut credentials = Credentials {
        username: "testuser".to_string(),
        password: Some("supersecret".to_string()),
    };
    
    // Verify credentials are zeroed on drop
    let password_ptr = credentials.password.as_ref().unwrap().as_ptr();
    drop(credentials);
    
    // Note: This is a conceptual test - actual memory verification
    // would require unsafe code and platform-specific techniques
}
```

### Test Configuration

```toml
# Cargo.toml test configuration
[dev-dependencies]
# Modern testcontainers with modules
testcontainers = "0.17"
testcontainers-modules = { version = "0.4", features = [
  "postgres",
  "mysql",
  "mongo",
] }

# Performance benchmarking
criterion = { version = "0.5", features = ["html_reports"] }

# Snapshot testing for CLI
insta = { version = "1.34", features = ["yaml", "json"] }

# Property-based testing
proptest = "1.4"

# Additional testing utilities
tokio-test = "0.4"
tempfile = "3.8"

# Database clients for integration tests
sqlx = { version = "0.7", features = [
  "postgres",
  "mysql",
  "sqlite",
  "runtime-tokio-rustls",
] }
mongodb = "2.8"

[[bench]]
name = "schema_collection"
harness = false

[[bench]]
name = "output_formats"
harness = false

[package.metadata.nextest]
# Nextest configuration for parallel test execution
test-threads = 4
retries = 2
slow-timeout = { period = "120s", terminate-after = 2 }
success-output = "never"
failure-output = "immediate"

# Test groups for different types of tests
[[package.metadata.nextest.test-groups]]
name = "integration"
max-threads = 2      # Limit concurrent container tests

[[package.metadata.nextest.test-groups]]
name = "unit"
max-threads = 8 # Allow more parallel unit tests

# Profile for CI environments
[package.metadata.nextest.profiles.ci]
retries = 3
test-threads = 2
slow-timeout = { period = "180s", terminate-after = 3 }
```

### Distribution and Release Management

Using `cargo-dist` and `cargo-release` for automated distribution with specialized collector binaries:

```toml
# Cargo.toml distribution configuration
[workspace.metadata.dist]
# The preferred cargo-dist version to use in CI (Cargo.toml SemVer syntax)
cargo-dist-version = "0.8.0"
# CI backends to support
ci = ["github"]
# The installers to generate for each app
installers = ["shell", "powershell", "homebrew"]
# Target platforms to build apps for ("rustc -vV" to find yours)
targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]
# Publish jobs to run in CI
pr-run-mode = "plan"
# Whether to install an updater program
install-updater = false

# Multiple collector binaries for different database types
[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect-postgres"
features = ["postgres"]

[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect-mysql"
features = ["mysql"]

[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect-sqlite"
features = ["sqlite"]

[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect-mongodb"
features = ["mongodb"]

[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect-mssql"
features = ["mssql"]

# Universal collector with all databases (larger binary)
[[workspace.metadata.dist.bin]]
name = "dbsurveyor-collect"
features = ["full"]

# Single postprocessor works with all collector outputs
[[workspace.metadata.dist.bin]]
name = "dbsurveyor"
features = ["compression", "encryption"]

[workspace.metadata.release]
# Shared release configuration
shared-version = true
consolidate-commits = true
pre-release-commit-message = "chore: Release {{crate_name}} version {{version}}"
post-release-commit-message = "chore: Bump {{crate_name}} to {{next_version}}"
tag-message = "{{crate_name}} {{version}}"
tag-prefix = "v"
```

### Specialized Collector Binaries

The distribution strategy creates optimized binaries for specific use cases:

```toml
# Binary configurations in Cargo.toml
[[bin]]
name = "dbsurveyor-collect-postgres"
path = "src/bin/collect.rs"
required-features = ["postgres"]

[[bin]]
name = "dbsurveyor-collect-mysql"
path = "src/bin/collect.rs"
required-features = ["mysql"]

[[bin]]
name = "dbsurveyor-collect-sqlite"
path = "src/bin/collect.rs"
required-features = ["sqlite"]

[[bin]]
name = "dbsurveyor-collect-mongodb"
path = "src/bin/collect.rs"
required-features = ["mongodb"]

[[bin]]
name = "dbsurveyor-collect-mssql"
path = "src/bin/collect.rs"
required-features = ["mssql"]

# Universal collector (larger but supports all databases)
[[bin]]
name = "dbsurveyor-collect"
path = "src/bin/collect.rs"
required-features = ["full"]

# Single postprocessor for all formats
[[bin]]
name = "dbsurveyor"
path = "src/bin/postprocess.rs"
```

### Binary Size Optimization

```rust
// src/bin/collect.rs - Conditional compilation for minimal binaries
use dbsurveyor_core::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CollectorCli::parse();

    // Only compile adapters for enabled features
    let adapter = match detect_database_type(&cli.database_url)? {
        #[cfg(feature = "postgres")]
        DatabaseType::PostgreSQL => Box::new(PostgresAdapter::new()) as Box<dyn DatabaseAdapter>,

        #[cfg(feature = "mysql")]
        DatabaseType::MySQL => Box::new(MySqlAdapter::new()) as Box<dyn DatabaseAdapter>,

        #[cfg(feature = "sqlite")]
        DatabaseType::SQLite => Box::new(SqliteAdapter::new()) as Box<dyn DatabaseAdapter>,

        #[cfg(feature = "mongodb")]
        DatabaseType::MongoDB => Box::new(MongoAdapter::new()) as Box<dyn DatabaseAdapter>,

        #[cfg(feature = "mssql")]
        DatabaseType::SqlServer => Box::new(SqlServerAdapter::new()) as Box<dyn DatabaseAdapter>,

        db_type => {
            eprintln!(
                "Error: Database type {:?} not supported in this binary.",
                db_type
            );
            eprintln!("Try using:");
            eprintln!("  - dbsurveyor-collect-postgres for PostgreSQL");
            eprintln!("  - dbsurveyor-collect-mysql for MySQL");
            eprintln!("  - dbsurveyor-collect-sqlite for SQLite");
            eprintln!("  - dbsurveyor-collect-mongodb for MongoDB");
            eprintln!("  - dbsurveyor-collect-mssql for SQL Server");
            eprintln!("  - dbsurveyor-collect for universal support");
            std::process::exit(1);
        }
    };

    // Rest of collection logic...
    Ok(())
}
```

### Installation Strategy

Users can install only what they need:

```bash
# Install PostgreSQL-only collector (smallest binary)
curl -L https://github.com/user/dbsurveyor/releases/latest/download/dbsurveyor-collect-postgres-x86_64-unknown-linux-gnu.tar.gz | tar xz

# Install universal collector (larger but supports all databases)
curl -L https://github.com/user/dbsurveyor/releases/latest/download/dbsurveyor-collect-x86_64-unknown-linux-gnu.tar.gz | tar xz

# Install postprocessor (works with all collector outputs)
curl -L https://github.com/user/dbsurveyor/releases/latest/download/dbsurveyor-x86_64-unknown-linux-gnu.tar.gz | tar xz
```

### Binary Size Comparison

Expected binary sizes (approximate):

- `dbsurveyor-collect-postgres`: ~8MB (PostgreSQL + core)
- `dbsurveyor-collect-mysql`: ~8MB (MySQL + core)
- `dbsurveyor-collect-sqlite`: ~6MB (SQLite + core, no network deps)
- `dbsurveyor-collect-mongodb`: ~12MB (MongoDB + core)
- `dbsurveyor-collect`: ~25MB (all databases + core)
- `dbsurveyor`: ~5MB (postprocessor, no database drivers)

### CI Integration

```yaml
# .github/workflows/test.yml
name: Test Suite

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy, rustfmt
    
    - name: Install nextest
      uses: taiki-e/install-action@nextest
    
    - name: Install cargo-insta
      uses: taiki-e/install-action@cargo-insta
    
    - name: Run tests with nextest
      run: |
        cargo nextest run --all-features --workspace --profile ci
    
    - name: Run benchmarks
      run: |
        cargo bench --all-features --no-run
    
    - name: Check snapshot tests
      run: |
        cargo insta test --all-features --check
    
    - name: Security audit
      run: |
        cargo audit
    
    - name: Check formatting
      run: |
        cargo fmt --all -- --check
    
    - name: Clippy
      run: |
        cargo clippy --all-features --all-targets -- -D warnings

# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - "v*.*.*"

jobs:
  # Build and upload binaries to GitHub Releases
  upload-artifacts:
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
      with:
        fetch-depth: 0
    
    - name: Install cargo-dist
      run: "curl --proto '=https' --tlsv1.2 -LsSf https://github.com/axodotdev/cargo-dist/releases/latest/download/cargo-dist-installer.sh | sh"
    
    - name: Run cargo-dist
      run: |
        cargo dist build --tag=${{ github.ref_name }} --output-format=json > dist-manifest.json
        echo "dist ran successfully"
        cat dist-manifest.json
    
    - name: Create Release
      uses: ncipollo/release-action@v1
      with:
        artifacts: "target/distrib/*"
        bodyFile: "CHANGELOG.md"
        token: ${{ secrets.GITHUB_TOKEN }}
```

### Build Configuration

```toml
# Cross-compilation and optimization settings
[profile.release]
lto = true        # Link-time optimization
codegen-units = 1 # Better optimization
panic = "abort"   # Smaller binary, immediate failure
strip = "symbols" # Remove debug symbols for smaller binaries

[profile.dist]
inherits = "release"
debug = 1            # Keep some debug info for better stack traces

# Feature flags for selective compilation
[features]
default = ["postgres", "mysql", "sqlite", "mongodb"]

# Database drivers
postgres = ["sqlx/postgres", "sqlx/runtime-tokio-rustls"]
mysql = ["sqlx/mysql", "sqlx/runtime-tokio-rustls"]
sqlite = ["sqlx/sqlite", "sqlx/runtime-tokio-rustls"]
mssql = ["tiberius", "tiberius/rustls"]
mongodb = ["dep:mongodb"]

# Optional features
compression = ["zstd"]
encryption = ["aes-gcm", "ring"]
wasm-plugins = ["wasmtime"]
tui = ["ratatui", "crossterm"]

# All features for development
full = [
  "postgres",
  "mysql",
  "sqlite",
  "mssql",
  "mongodb",
  "compression",
  "encryption",
  "wasm-plugins",
  "tui",
]
```

This comprehensive testing architecture ensures reliability, performance, and security across all components of the database schema collection system.

## Documentation Architecture

### Documentation Strategy

The project uses a dual documentation approach:

- **Rustdoc**: API documentation for developers and contributors
- **mdbook**: User-facing guides and tutorials

### Rustdoc Configuration

```toml
# Cargo.toml documentation configuration
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]

[workspace.metadata.docs.rs]
# Generate documentation for all features
all-features = true
# Include private items in documentation
rustdoc-args = [
  "--cfg",
  "docsrs",
  "--document-private-items",
  "--enable-index-page",
  "--crate-name",
  "dbsurveyor",
]
```

### API Documentation Standards

````rust
//! # DBSurveyor Core Library
//!
//! DBSurveyor provides secure, offline-capable database schema collection and documentation
//! generation across multiple database engines.
//!
//! ## Security Guarantees
//!
//! - **Offline-Only Operation**: No network calls except to target databases
//! - **No Telemetry**: Zero data collection or external reporting
//! - **Credential Protection**: Database credentials never stored in outputs
//! - **AES-GCM Encryption**: Industry-standard authenticated encryption
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use dbsurveyor_core::{PostgresAdapter, DatabaseAdapter};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let adapter = PostgresAdapter::new();
//!     let metadata = adapter.collect_from_url(
//!         "postgres://user:pass@localhost/db"
//!     ).await?;
//!     
//!     println!("Found {} tables", metadata.tables.len());
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! The system consists of two main components:
//! - **Collector**: Extracts database metadata and generates structured output
//! - **Postprocessor**: Generates documentation from collected metadata
//!
//! ## Supported Databases
//!
//! | Database | Feature Flag | Status |
//! |----------|--------------|--------|
//! | PostgreSQL | `postgres` | ✅ Full Support |
//! | MySQL | `mysql` | ✅ Full Support |
//! | SQLite | `sqlite` | ✅ Full Support |
//! | MongoDB | `mongodb` | ✅ Full Support |
//! | SQL Server | `mssql` | ✅ Full Support |

/// Database adapter trait for unified schema collection across different database engines.
///
/// This trait provides a consistent interface for connecting to and collecting metadata
/// from various database types while maintaining security and performance guarantees.
///
/// # Security
///
/// All implementations must ensure:
/// - Database credentials are never logged or stored
/// - All operations are read-only
/// - Connection strings are sanitized in error messages
///
/// # Example
///
/// ```rust,no_run
/// use dbsurveyor_core::{DatabaseAdapter, PostgresAdapter};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let adapter = PostgresAdapter::new();
///     let connection = adapter.connect("postgres://localhost/db").await?;
///     let metadata = adapter.collect_metadata(&connection).await?;
///     
///     println!("Database: {}", metadata.database_info.name);
///     println!("Tables: {}", metadata.tables.len());
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// The connection type used by this adapter
    type Connection;
    /// Error type for adapter operations
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Establishes a connection to the database.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - Database connection URL (credentials will not be logged)
    ///
    /// # Security
    ///
    /// Connection strings containing credentials are automatically sanitized in error messages.
    /// No credentials are stored or logged during the connection process.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The connection string is malformed
    /// - The database is unreachable
    /// - Authentication fails
    /// - Connection timeout is exceeded
    async fn connect(&self, connection_string: &str) -> Result<Self::Connection, Self::Error>;
    
    /// Lists all accessible databases on the server.
    ///
    /// This method attempts to enumerate all databases that the connected user has access to.
    /// If the connection string doesn't specify a database, or if the user has superuser
    /// privileges, this method will discover all available databases.
    ///
    /// # Arguments
    ///
    /// * `conn` - Database connection (should be to a system database or server-level connection)
    ///
    /// # Returns
    ///
    /// Returns a list of database names that can be accessed for schema collection.
    /// System databases may be filtered out based on configuration.
    ///
    /// # Security
    ///
    /// Only databases that the authenticated user has read access to will be returned.
    /// No privilege escalation is attempted.
    async fn list_databases(&self, conn: &Self::Connection) -> Result<Vec<DatabaseInfo>, Self::Error>;
    
    /// Connects to a specific database by name.
    ///
    /// Used in multi-database collection scenarios where we need to connect to each
    /// discovered database individually for schema collection.
    ///
    /// # Arguments
    ///
    /// * `base_connection_string` - Original connection string (may not specify database)
    /// * `database_name` - Specific database to connect to
    async fn connect_to_database(&self, base_connection_string: &str, database_name: &str) -> Result<Self::Connection, Self::Error>;
    
    /// Collects comprehensive metadata from the connected database.
    ///
    /// This method performs deep introspection of the database structure, extracting:
    /// - Table and column definitions
    /// - Indexes and constraints
    /// - Foreign key relationships
    /// - Database-specific objects (views, procedures, functions)
    ///
    /// # Performance
    ///
    /// Collection time scales with database size:
    /// - Small databases (<100 tables): <1 second
    /// - Medium databases (100-1000 tables): 1-10 seconds  
    /// - Large databases (>1000 tables): 10-60 seconds
    ///
    /// # Security
    ///
    /// All operations are read-only. No data modification occurs during collection.
    async fn collect_metadata(&self, conn: &Self::Connection) -> Result<DatabaseMetadata, Self::Error>;
    
    /// Collects sample data from tables using intelligent ordering strategies.
    ///
    /// This method samples the most recent N rows from each table by:
    /// 1. Identifying the primary key or best available ordering column
    /// 2. Using timestamp columns when available (created_at, updated_at, etc.)
    /// 3. Falling back to auto-increment columns or row IDs
    /// 4. Using LIMIT with appropriate ORDER BY clauses
    ///
    /// # Arguments
    ///
    /// * `conn` - Database connection
    /// * `config` - Sampling configuration (row count, tables to include/exclude)
    ///
    /// # Security
    ///
    /// - Sample data may contain sensitive information
    /// - Users should review outputs before sharing
    /// - Configurable redaction patterns available
    /// - All sampling queries are read-only
    ///
    /// # Performance
    ///
    /// Sampling is optimized to minimize database load:
    /// - Uses indexed columns for ordering when possible
    /// - Configurable throttling between queries
    /// - Respects query timeouts to avoid slow log entries
    async fn sample_data(&self, conn: &Self::Connection, config: &SamplingConfig) -> Result<Vec<TableSample>, Self::Error>;
}
````

### mdbook Documentation Structure

```toml
# book.toml configuration
[book]
title = "DBSurveyor User Guide"
description = "Secure database schema collection and documentation"
authors = ["DBSurveyor Contributors"]
language = "en"
multilingual = false
src = "docs/src"

[preprocessor.mermaid]
command = "mdbook-mermaid"

[preprocessor.admonish]
command = "mdbook-admonish"
assets_version = "3.0.2"

[output.html]
theme = "docs/theme"
default-theme = "navy"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/user/dbsurveyor"
edit-url-template = "https://github.com/user/dbsurveyor/edit/main/docs/src/{path}"

[output.html.search]
enable = true
limit-results = 30
teaser-word-count = 30
use-boolean-and = true
boost-title = 2
boost-hierarchy = 1
boost-paragraph = 1
expand = true
heading-split-level = 3

[output.html.print]
enable = true
```

### Documentation Content Structure

```
docs/
├── book.toml
├── src/
│   ├── SUMMARY.md
│   ├── introduction.md
│   ├── installation/
│   │   ├── README.md
│   │   ├── binary-releases.md
│   │   ├── from-source.md
│   │   └── docker.md
│   ├── getting-started/
│   │   ├── README.md
│   │   ├── quick-start.md
│   │   ├── first-collection.md
│   │   └── basic-reporting.md
│   ├── user-guide/
│   │   ├── README.md
│   │   ├── collector/
│   │   │   ├── README.md
│   │   │   ├── postgres.md
│   │   │   ├── mysql.md
│   │   │   ├── sqlite.md
│   │   │   ├── mongodb.md
│   │   │   └── mssql.md
│   │   ├── postprocessor/
│   │   │   ├── README.md
│   │   │   ├── reports.md
│   │   │   ├── diagrams.md
│   │   │   └── sql-reconstruction.md
│   │   └── security/
│   │       ├── README.md
│   │       ├── encryption.md
│   │       ├── credential-management.md
│   │       └── airgap-operation.md
│   ├── reference/
│   │   ├── README.md
│   │   ├── cli-reference.md
│   │   ├── output-formats.md
│   │   ├── configuration.md
│   │   └── troubleshooting.md
│   ├── examples/
│   │   ├── README.md
│   │   ├── red-team-scenarios.md
│   │   ├── compliance-auditing.md
│   │   └── schema-migration.md
│   └── development/
│       ├── README.md
│       ├── contributing.md
│       ├── architecture.md
│       └── plugin-development.md
└── theme/
    ├── index.hbs
    ├── css/
    └── js/
```

### Documentation Build Integration

```yaml
# .github/workflows/docs.yml
name: Documentation

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  rustdoc:
    name: Build API Documentation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build rustdoc
        run: |
          cargo doc --all-features --no-deps --document-private-items

      - name: Deploy to GitHub Pages
        if: github.ref == 'refs/heads/main'
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./target/doc
          destination_dir: api

  mdbook:
    name: Build User Guide
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install mdbook
        run: |
          curl -L https://github.com/rust-lang/mdBook/releases/latest/download/mdbook-x86_64-unknown-linux-gnu.tar.gz | tar xz
          curl -L https://github.com/badboy/mdbook-mermaid/releases/latest/download/mdbook-mermaid-x86_64-unknown-linux-gnu.tar.gz | tar xz
          curl -L https://github.com/tommilligan/mdbook-admonish/releases/latest/download/mdbook-admonish-x86_64-unknown-linux-gnu.tar.gz | tar xz
          chmod +x mdbook mdbook-mermaid mdbook-admonish
          sudo mv mdbook mdbook-mermaid mdbook-admonish /usr/local/bin/

      - name: Build book
        run: |
          cd docs
          mdbook build

      - name: Deploy to GitHub Pages
        if: github.ref == 'refs/heads/main'
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./docs/book
          destination_dir: guide
```

### Documentation Quality Standards

````rust
// Documentation examples must be tested
#[doc = include_str!("../examples/basic_usage.rs")]
/// # Examples
///
/// Basic usage with PostgreSQL:
///
/// ```rust,no_run
/// # use dbsurveyor_core::*;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let adapter = PostgresAdapter::new();
/// let metadata = adapter.collect_from_url(
///     "postgres://user:pass@localhost/db"
/// ).await?;
///
/// // Generate markdown report
/// let report = generate_markdown_report(&metadata)?;
/// std::fs::write("schema_report.md", report)?;
/// # Ok(())
/// # }
/// ```
///
/// With encryption:
///
/// ```rust,no_run
/// # use dbsurveyor_core::*;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let adapter = PostgresAdapter::new();
/// let metadata = adapter.collect_from_url(
///     "postgres://user:pass@localhost/db"
/// ).await?;
///
/// // Encrypt output
/// let encrypted = encrypt_schema_data(
///     &serde_json::to_vec(&metadata)?,
///     "secure_password"
/// ).await?;
///
/// std::fs::write("schema.dbsurveyor.enc", encrypted.to_bytes())?;
/// # Ok(())
/// # }
/// ```
pub struct PostgresAdapter {
    // Implementation details...
}
````

This comprehensive documentation architecture ensures both developers and end-users have excellent documentation resources while maintaining the security-first principles of the project.

This design provides a robust, secure, and extensible foundation for the database schema collection and documentation system while maintaining the strict security and offline operation requirements.
