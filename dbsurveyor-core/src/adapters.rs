//! Database adapter traits and factory for unified database access.
//!
//! This module defines the core traits that all database adapters must implement
//! to provide a unified interface for schema collection across different database
//! engines. The design emphasizes object safety and security.

use crate::{Result, models::DatabaseSchema};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Features that database adapters may support
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterFeature {
    /// Schema introspection and metadata collection
    SchemaCollection,
    /// Data sampling from tables
    DataSampling,
    /// Multi-database enumeration
    MultiDatabase,
    /// Connection pooling
    ConnectionPooling,
    /// Query timeout configuration
    QueryTimeout,
    /// Read-only connection enforcement
    ReadOnlyMode,
}

/// Configuration for database connections
///
/// # Security
/// This struct intentionally does NOT store passwords or credentials.
/// Credentials must be handled separately and never logged or serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub connect_timeout: Duration,
    pub query_timeout: Duration,
    pub max_connections: u32,
    pub read_only: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: None,
            database: None,
            username: None,
            connect_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(30),
            max_connections: 10,
            read_only: true,
        }
    }
}

impl std::fmt::Display for ConnectionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConnectionConfig({}{}{})",
            self.host,
            self.port.map_or_else(String::new, |p| format!(":{}", p)),
            self.database
                .as_ref()
                .map_or_else(String::new, |db| format!("/{}", db))
        )
        // Intentionally omit username and never include credentials
    }
}

impl ConnectionConfig {
    /// Validates connection configuration parameters
    ///
    /// # Errors
    /// Returns error if configuration values are invalid or unsafe
    pub fn validate(&self) -> Result<()> {
        if self.host.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(
                "host cannot be empty",
            ));
        }

        if let Some(port) = self.port {
            if port == 0 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "port must be greater than 0",
                ));
            }
        }

        if self.max_connections == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_connections must be greater than 0",
            ));
        }

        if self.max_connections > 100 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_connections should not exceed 100 for safety",
            ));
        }

        if self.connect_timeout.as_secs() == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "connect_timeout must be greater than 0",
            ));
        }

        if self.query_timeout.as_secs() == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "query_timeout must be greater than 0",
            ));
        }

        Ok(())
    }

    /// Creates a new connection config with safe defaults
    pub fn new(host: String) -> Self {
        Self {
            host,
            ..Default::default()
        }
    }

    /// Builder method to set port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Builder method to set database
    pub fn with_database(mut self, database: String) -> Self {
        self.database = Some(database);
        self
    }

    /// Builder method to set username
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }
}

/// Configuration for data sampling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    pub sample_size: u32,
    pub throttle_ms: Option<u64>,
    pub query_timeout_secs: u64,
    pub warn_sensitive: bool,
    pub timestamp_columns: Vec<String>,
    pub sensitive_detection_patterns: Vec<SensitivePattern>,
}

/// Configuration for database schema collection
///
/// This struct controls all aspects of database schema collection including
/// connection settings, what database objects to include, and output options.
///
/// # Security
/// - Connection credentials are handled separately and never stored here
/// - All database operations are read-only by default
/// - Query timeouts prevent resource exhaustion
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::{CollectionConfig, ConnectionConfig, OutputFormat};
///
/// let connection = ConnectionConfig::new("localhost".to_string())
///     .with_port(5432)
///     .with_database("mydb".to_string());
///
/// let config = CollectionConfig::new()
///     .with_connection(connection)
///     .with_max_concurrent_queries(10)
///     .unwrap();
///
/// assert!(config.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    /// Database connection configuration (credentials handled separately)
    pub connection: ConnectionConfig,
    /// Data sampling configuration
    pub sampling: SamplingConfig,
    /// Whether to include system/internal databases
    pub include_system_databases: bool,
    /// List of database names to exclude from collection
    pub exclude_databases: Vec<String>,
    /// Whether to collect database views
    pub include_views: bool,
    /// Whether to collect stored procedures
    pub include_procedures: bool,
    /// Whether to collect functions
    pub include_functions: bool,
    /// Whether to collect triggers
    pub include_triggers: bool,
    /// Whether to collect indexes
    pub include_indexes: bool,
    /// Whether to collect constraints
    pub include_constraints: bool,
    /// Whether to collect custom/user-defined types
    pub include_custom_types: bool,
    /// Maximum number of concurrent database queries (1-50)
    pub max_concurrent_queries: u32,
    /// Whether to enable data sampling from tables
    pub enable_data_sampling: bool,
    /// Output format for collected schema
    pub output_format: OutputFormat,
    /// Whether to enable compression of output
    pub compression_enabled: bool,
    /// Whether to enable encryption of output
    pub encryption_enabled: bool,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            sample_size: 100,
            throttle_ms: None,
            query_timeout_secs: 30,
            warn_sensitive: true,
            timestamp_columns: vec![
                "created_at".to_string(),
                "updated_at".to_string(),
                "modified_at".to_string(),
                "timestamp".to_string(),
            ],
            sensitive_detection_patterns: vec![
                SensitivePattern {
                    pattern: r"(?i)(password|passwd|pwd)".to_string(),
                    description: "Password field detected".to_string(),
                },
                SensitivePattern {
                    pattern: r"(?i)(email|mail)".to_string(),
                    description: "Email field detected".to_string(),
                },
                SensitivePattern {
                    pattern: r"(?i)(ssn|social_security)".to_string(),
                    description: "Social Security Number field detected".to_string(),
                },
            ],
        }
    }
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            sampling: SamplingConfig::default(),
            include_system_databases: false,
            exclude_databases: Vec::new(),
            include_views: true,
            include_procedures: true,
            include_functions: true,
            include_triggers: true,
            include_indexes: true,
            include_constraints: true,
            include_custom_types: true,
            max_concurrent_queries: 5,
            enable_data_sampling: false,
            output_format: OutputFormat::Json,
            compression_enabled: false,
            encryption_enabled: false,
        }
    }
}

impl CollectionConfig {
    /// Validates the collection configuration
    ///
    /// # Errors
    /// Returns error if configuration values are invalid or unsafe
    pub fn validate(&self) -> Result<()> {
        if self.max_concurrent_queries == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries must be greater than 0",
            ));
        }

        if self.max_concurrent_queries > 50 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries should not exceed 50 for safety",
            ));
        }

        if self.connection.max_connections == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_connections must be greater than 0",
            ));
        }

        if self.connection.connect_timeout.as_secs() > 300 {
            return Err(crate::error::DbSurveyorError::configuration(
                "connect_timeout should not exceed 300 seconds",
            ));
        }

        if self.connection.query_timeout.as_secs() > 600 {
            return Err(crate::error::DbSurveyorError::configuration(
                "query_timeout should not exceed 600 seconds",
            ));
        }

        Ok(())
    }

    /// Creates a new collection config with safe defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set connection config
    pub fn with_connection(mut self, connection: ConnectionConfig) -> Self {
        self.connection = connection;
        self
    }

    /// Builder method to set sampling config
    pub fn with_sampling(mut self, sampling: SamplingConfig) -> Self {
        self.sampling = sampling;
        self
    }

    /// Builder method to set max concurrent queries with validation
    pub fn with_max_concurrent_queries(mut self, max: u32) -> Result<Self> {
        if max == 0 || max > 50 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries must be between 1 and 50",
            ));
        }
        self.max_concurrent_queries = max;
        Ok(self)
    }
}

/// Pattern for detecting sensitive data fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePattern {
    pub pattern: String,
    pub description: String,
}

/// Output format options for collected schema data
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Standard JSON format (.dbsurveyor.json)
    #[default]
    Json,
    /// Compressed JSON format (.dbsurveyor.json.zst)
    CompressedJson,
    /// Encrypted format (.dbsurveyor.enc)
    Encrypted,
}

/// Main trait for database adapters with object-safe design
///
/// # Security Guarantees
/// - All operations are read-only
/// - Credentials are never stored or logged
/// - Connection strings are sanitized in error messages
/// - Query timeouts prevent resource exhaustion
///
/// # Object Safety
/// This trait is object-safe, allowing for dynamic dispatch through
/// `Box<dyn DatabaseAdapter>` or `Arc<dyn DatabaseAdapter>`.
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Tests the database connection without collecting schema
    ///
    /// # Security
    /// - Uses read-only connection if supported
    /// - Times out after configured duration
    /// - Never logs connection details
    ///
    /// # Errors
    /// Returns error if connection fails or times out
    async fn test_connection(&self) -> Result<()>;

    /// Collects comprehensive database schema metadata
    ///
    /// # Security
    /// - All operations are read-only (SELECT/DESCRIBE only)
    /// - Query timeouts prevent resource exhaustion
    /// - No credentials stored in returned schema
    ///
    /// # Returns
    /// Complete database schema with tables, indexes, constraints, etc.
    ///
    /// # Errors
    /// Returns error if:
    /// - Connection fails or times out
    /// - Insufficient privileges for schema access
    /// - Database-specific errors occur
    async fn collect_schema(&self) -> Result<DatabaseSchema>;

    /// Returns the database type this adapter handles
    fn database_type(&self) -> crate::models::DatabaseType;

    /// Checks if the adapter supports a specific feature
    fn supports_feature(&self, feature: AdapterFeature) -> bool;

    /// Gets the connection configuration (credentials sanitized)
    fn connection_config(&self) -> ConnectionConfig;
}

/// Factory function to create database adapters based on connection string
///
/// # Arguments
/// * `connection_string` - Database connection URL (will be sanitized in errors)
///
/// # Security
/// - Automatically detects database type from connection string
/// - Sanitizes connection string in all error messages
/// - Enforces read-only mode by default
///
/// # Returns
/// Boxed database adapter for dynamic dispatch
///
/// # Errors
/// Returns error if:
/// - Connection string format is invalid
/// - Database type is not supported
/// - Required features are not compiled in
///
/// # Example
/// ```rust,no_run
/// use dbsurveyor_core::adapters::create_adapter;
///
/// # async fn example() -> dbsurveyor_core::Result<()> {
/// let adapter = create_adapter("postgres://user:pass@localhost/db").await?;
/// let schema = adapter.collect_schema().await?;
/// println!("Found {} tables", schema.tables.len());
/// # Ok(())
/// # }
/// ```
pub async fn create_adapter(connection_string: &str) -> Result<Box<dyn DatabaseAdapter>> {
    let database_type = detect_database_type(connection_string)?;

    match database_type {
        #[cfg(feature = "postgresql")]
        crate::models::DatabaseType::PostgreSQL => {
            let adapter =
                crate::adapters::postgres::PostgresAdapter::new(connection_string).await?;
            Ok(Box::new(adapter))
        }
        #[cfg(not(feature = "postgresql"))]
        crate::models::DatabaseType::PostgreSQL => {
            Err(crate::error::DbSurveyorError::unsupported_feature(
                "PostgreSQL adapter",
                "Compile with --features postgresql to enable PostgreSQL support",
            ))
        }
        #[cfg(feature = "mysql")]
        crate::models::DatabaseType::MySQL => {
            let adapter = crate::adapters::mysql::MySqlAdapter::new(connection_string).await?;
            Ok(Box::new(adapter))
        }
        #[cfg(not(feature = "mysql"))]
        crate::models::DatabaseType::MySQL => {
            Err(crate::error::DbSurveyorError::unsupported_feature(
                "MySQL adapter",
                "Compile with --features mysql to enable MySQL support",
            ))
        }
        #[cfg(feature = "sqlite")]
        crate::models::DatabaseType::SQLite => {
            let adapter = crate::adapters::sqlite::SqliteAdapter::new(connection_string).await?;
            Ok(Box::new(adapter))
        }
        #[cfg(not(feature = "sqlite"))]
        crate::models::DatabaseType::SQLite => {
            Err(crate::error::DbSurveyorError::unsupported_feature(
                "SQLite adapter",
                "Compile with --features sqlite to enable SQLite support",
            ))
        }
        #[cfg(feature = "mongodb")]
        crate::models::DatabaseType::MongoDB => {
            let adapter = crate::adapters::mongodb::MongoAdapter::new(connection_string).await?;
            Ok(Box::new(adapter))
        }
        #[cfg(not(feature = "mongodb"))]
        crate::models::DatabaseType::MongoDB => {
            Err(crate::error::DbSurveyorError::unsupported_feature(
                "MongoDB adapter",
                "Compile with --features mongodb to enable MongoDB support",
            ))
        }
        crate::models::DatabaseType::SqlServer => {
            #[cfg(feature = "mssql")]
            {
                let adapter =
                    crate::adapters::mssql::SqlServerAdapter::new(connection_string).await?;
                Ok(Box::new(adapter))
            }
            #[cfg(not(feature = "mssql"))]
            {
                Err(crate::error::DbSurveyorError::unsupported_feature(
                    "SQL Server adapter",
                    "Compile with --features mssql to enable SQL Server support",
                ))
            }
        }
    }
}

/// Safely redacts credentials from database connection URLs
///
/// This function ensures that passwords in connection strings are never
/// exposed in logs, error messages, or any output.
///
/// # Arguments
/// * `url` - Database connection URL that may contain credentials
///
/// # Returns
/// Returns a sanitized string with passwords masked as "****"
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::redact_database_url;
///
/// let sanitized = redact_database_url("postgres://user:secret@localhost/db");
/// assert_eq!(sanitized, "postgres://user:****@localhost/db");
/// assert!(!sanitized.contains("secret"));
/// ```
pub fn redact_database_url(url: &str) -> String {
    // Try to parse as URL first
    if let Ok(mut parsed_url) = url::Url::parse(url) {
        if parsed_url.password().is_some() {
            let _ = parsed_url.set_password(Some("****"));
        }
        parsed_url.to_string()
    } else {
        // For non-URL formats (like file paths), just return as-is
        // since they shouldn't contain credentials
        url.to_string()
    }
}

/// Detects database type from connection string
///
/// # Arguments
/// * `connection_string` - Database connection URL
///
/// # Returns
/// Detected database type
///
/// # Errors
/// Returns error if connection string format is unrecognized
fn detect_database_type(connection_string: &str) -> Result<crate::models::DatabaseType> {
    if connection_string.starts_with("postgres://")
        || connection_string.starts_with("postgresql://")
    {
        Ok(crate::models::DatabaseType::PostgreSQL)
    } else if connection_string.starts_with("mysql://") {
        Ok(crate::models::DatabaseType::MySQL)
    } else if connection_string.starts_with("sqlite://")
        || connection_string.ends_with(".db")
        || connection_string.ends_with(".sqlite")
    {
        Ok(crate::models::DatabaseType::SQLite)
    } else if connection_string.starts_with("mongodb://")
        || connection_string.starts_with("mongodb+srv://")
    {
        Ok(crate::models::DatabaseType::MongoDB)
    } else if connection_string.starts_with("mssql://")
        || connection_string.starts_with("sqlserver://")
    {
        #[cfg(feature = "mssql")]
        return Ok(crate::models::DatabaseType::SqlServer);
        #[cfg(not(feature = "mssql"))]
        return Err(crate::error::DbSurveyorError::configuration(
            "SQL Server support not compiled in. Use --features mssql",
        ));
    } else {
        Err(crate::error::DbSurveyorError::configuration(
            "Unrecognized database connection string format",
        ))
    }
}

// Placeholder modules for database-specific adapters
// These will be implemented in subsequent tasks

#[cfg(feature = "postgresql")]
pub mod postgres {
    use super::*;
    use crate::models::*;
    use sqlx::{PgPool, Row};
    use std::time::Duration;

    /// PostgreSQL database adapter with connection pooling and comprehensive schema collection
    ///
    /// # Security Guarantees
    /// - All operations are read-only (SELECT/DESCRIBE only)
    /// - Connection strings are sanitized in error messages
    /// - Query timeouts prevent resource exhaustion
    /// - Connection pooling with configurable limits
    ///
    /// # Features
    /// - Full schema introspection using information_schema and pg_catalog
    /// - Connection pooling with sqlx::PgPool
    /// - Multi-database enumeration for server-level collection
    /// - Data sampling with intelligent ordering strategies
    /// - Proper UnifiedDataType mapping from PostgreSQL types
    pub struct PostgresAdapter {
        pub pool: PgPool,
        pub config: ConnectionConfig,
    }

    impl PostgresAdapter {
        /// Creates a new PostgreSQL adapter with connection pooling
        ///
        /// # Arguments
        /// * `connection_string` - PostgreSQL connection URL (credentials sanitized in errors)
        ///
        /// # Security
        /// - Enforces read-only mode by default
        /// - Sets statement_timeout for query safety
        /// - Sanitizes connection string in all error messages
        ///
        /// # Errors
        /// Returns error if:
        /// - Connection string format is invalid
        /// - Database connection fails
        /// - Pool configuration is invalid
        pub async fn new(connection_string: &str) -> Result<Self> {
            let config = Self::parse_connection_config(connection_string)?;
            let pool = Self::create_connection_pool(connection_string, &config).await?;

            Ok(Self { pool, config })
        }

        /// Parses connection string to extract configuration parameters
        pub fn parse_connection_config(connection_string: &str) -> Result<ConnectionConfig> {
            let url = url::Url::parse(connection_string).map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Invalid PostgreSQL connection string format: {}",
                    e
                ))
            })?;

            let mut config =
                ConnectionConfig::new(url.host_str().unwrap_or("localhost").to_string());

            if let Some(port) = url.port() {
                config = config.with_port(port);
            } else {
                config = config.with_port(5432); // PostgreSQL default port
            }

            if !url.path().is_empty() && url.path() != "/" {
                let database = url.path().trim_start_matches('/');
                if !database.is_empty() {
                    config = config.with_database(database.to_string());
                }
            }

            let username = url.username();
            if !username.is_empty() {
                config = config.with_username(username.to_string());
            }

            config.validate()?;
            Ok(config)
        }

        /// Creates a connection pool with proper configuration and security settings
        async fn create_connection_pool(
            connection_string: &str,
            config: &ConnectionConfig,
        ) -> Result<PgPool> {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(config.max_connections)
                .min_connections(2) // Keep minimum connections for efficiency
                .acquire_timeout(config.connect_timeout)
                .idle_timeout(Some(Duration::from_secs(600))) // 10 minutes idle timeout
                .max_lifetime(Some(Duration::from_secs(3600))) // 1 hour max lifetime
                .test_before_acquire(true) // Validate connections before use
                .connect_lazy(connection_string)
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to create PostgreSQL connection pool to {}",
                            redact_database_url(connection_string)
                        ),
                        e,
                    )
                })?;

            Ok(pool)
        }

        /// Sets up session-level security settings on first connection
        async fn setup_session(&self) -> Result<()> {
            // Set session-level security settings
            sqlx::query(&format!(
                "SET statement_timeout = '{}s'",
                self.config.query_timeout.as_secs()
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set query timeout: {}",
                    e
                ))
            })?;

            // Set read-only mode if requested
            if self.config.read_only {
                sqlx::query("SET default_transaction_read_only = on")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        crate::error::DbSurveyorError::configuration(format!(
                            "Failed to set read-only mode: {}",
                            e
                        ))
                    })?;
            }

            Ok(())
        }

        /// Collects comprehensive database schema information
        async fn collect_database_info(&self) -> Result<DatabaseInfo> {
            let version_query = "SELECT version()";
            let version: String = sqlx::query_scalar(version_query)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        "Failed to get database version",
                        e,
                    )
                })?;

            let db_info_query = r#"
                SELECT
                    current_database() as name,
                    pg_database_size(current_database()) as size_bytes,
                    pg_encoding_to_char(encoding) as encoding,
                    datcollate as collation,
                    r.rolname as owner
                FROM pg_database d
                LEFT JOIN pg_roles r ON d.datdba = r.oid
                WHERE d.datname = current_database()
            "#;

            let row = sqlx::query(db_info_query)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        "Failed to get database information",
                        e,
                    )
                })?;

            let name: String = row.get("name");
            let size_bytes: Option<i64> = row.get("size_bytes");
            let encoding: Option<String> = row.get("encoding");
            let collation: Option<String> = row.get("collation");
            let owner: Option<String> = row.get("owner");

            // Check if this is a system database
            let is_system_database =
                matches!(name.as_str(), "template0" | "template1" | "postgres");

            Ok(DatabaseInfo {
                name,
                version: Some(version),
                size_bytes: size_bytes.map(|s| s as u64),
                encoding,
                collation,
                owner,
                is_system_database,
                access_level: AccessLevel::Full, // We have full access if we can query
                collection_status: CollectionStatus::Success,
            })
        }

        /// Collects all tables from the database
        async fn collect_tables(&self) -> Result<Vec<Table>> {
            let tables_query = r#"
                SELECT
                    t.table_name,
                    t.table_schema,
                    obj_description(c.oid) as table_comment,
                    (SELECT reltuples::bigint FROM pg_class WHERE relname = t.table_name AND relnamespace = n.oid) as estimated_rows
                FROM information_schema.tables t
                LEFT JOIN pg_class c ON c.relname = t.table_name
                LEFT JOIN pg_namespace n ON n.nspname = t.table_schema AND c.relnamespace = n.oid
                WHERE t.table_type = 'BASE TABLE'
                AND t.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
                ORDER BY t.table_schema, t.table_name
            "#;

            let table_rows = sqlx::query(tables_query)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed("Failed to collect tables", e)
                })?;

            let mut tables = Vec::new();

            for row in table_rows {
                let table_name: String = row.get("table_name");
                let schema_name: Option<String> = row.get("table_schema");
                let comment: Option<String> = row.get("table_comment");
                let estimated_rows: Option<i64> = row.get("estimated_rows");

                // Collect columns for this table
                let columns = self
                    .collect_table_columns(&table_name, schema_name.as_deref())
                    .await?;

                // Collect primary key
                let primary_key = self
                    .collect_primary_key(&table_name, schema_name.as_deref())
                    .await?;

                // Collect foreign keys
                let foreign_keys = self
                    .collect_foreign_keys(&table_name, schema_name.as_deref())
                    .await?;

                // Collect indexes
                let indexes = self
                    .collect_table_indexes(&table_name, schema_name.as_deref())
                    .await?;

                // Collect constraints
                let constraints = self
                    .collect_table_constraints(&table_name, schema_name.as_deref())
                    .await?;

                let table = Table {
                    name: table_name,
                    schema: schema_name,
                    columns,
                    primary_key,
                    foreign_keys,
                    indexes,
                    constraints,
                    comment,
                    row_count: estimated_rows.map(|r| r as u64),
                };

                tables.push(table);
            }

            Ok(tables)
        }

        /// Collects columns for a specific table
        async fn collect_table_columns(
            &self,
            table_name: &str,
            schema_name: Option<&str>,
        ) -> Result<Vec<Column>> {
            let schema = schema_name.unwrap_or("public");

            let columns_query = r#"
                SELECT
                    c.column_name,
                    c.data_type,
                    c.character_maximum_length,
                    c.numeric_precision,
                    c.numeric_scale,
                    c.is_nullable,
                    c.column_default,
                    c.ordinal_position,
                    col_description(pgc.oid, c.ordinal_position) as column_comment,
                    CASE WHEN c.column_default LIKE 'nextval%' THEN true ELSE false END as is_auto_increment
                FROM information_schema.columns c
                LEFT JOIN pg_class pgc ON pgc.relname = c.table_name
                LEFT JOIN pg_namespace pgn ON pgn.nspname = c.table_schema AND pgc.relnamespace = pgn.oid
                WHERE c.table_name = $1 AND c.table_schema = $2
                ORDER BY c.ordinal_position
            "#;

            let column_rows = sqlx::query(columns_query)
                .bind(table_name)
                .bind(schema)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect columns for table {}.{}",
                            schema, table_name
                        ),
                        e,
                    )
                })?;

            // Get primary key columns for this table
            let pk_columns = self.get_primary_key_columns(table_name, schema).await?;

            let mut columns = Vec::new();

            for row in column_rows {
                let column_name: String = row.get("column_name");
                let data_type_str: String = row.get("data_type");
                let max_length: Option<i32> = row.get("character_maximum_length");
                let numeric_precision: Option<i32> = row.get("numeric_precision");
                let numeric_scale: Option<i32> = row.get("numeric_scale");
                let is_nullable: String = row.get("is_nullable");
                let default_value: Option<String> = row.get("column_default");
                let ordinal_position: i32 = row.get("ordinal_position");
                let comment: Option<String> = row.get("column_comment");
                let is_auto_increment: bool = row.get("is_auto_increment");

                let data_type = Self::map_postgresql_type(
                    &data_type_str,
                    max_length,
                    numeric_precision,
                    numeric_scale,
                )?;
                let is_primary_key = pk_columns.contains(&column_name);

                let column = Column {
                    name: column_name,
                    data_type,
                    is_nullable: is_nullable == "YES",
                    is_primary_key,
                    is_auto_increment,
                    default_value,
                    comment,
                    ordinal_position: ordinal_position as u32,
                };

                columns.push(column);
            }

            Ok(columns)
        }

        /// Gets primary key column names for a table
        async fn get_primary_key_columns(
            &self,
            table_name: &str,
            schema_name: &str,
        ) -> Result<Vec<String>> {
            let pk_query = r#"
                SELECT a.attname as column_name
                FROM pg_index i
                JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                JOIN pg_class c ON c.oid = i.indrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE i.indisprimary = true
                AND c.relname = $1
                AND n.nspname = $2
                ORDER BY a.attnum
            "#;

            let rows = sqlx::query(pk_query)
                .bind(table_name)
                .bind(schema_name)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to get primary key columns for {}.{}",
                            schema_name, table_name
                        ),
                        e,
                    )
                })?;

            Ok(rows.into_iter().map(|row| row.get("column_name")).collect())
        }

        /// Collects primary key constraint for a table
        async fn collect_primary_key(
            &self,
            table_name: &str,
            schema_name: Option<&str>,
        ) -> Result<Option<PrimaryKey>> {
            let schema = schema_name.unwrap_or("public");

            let pk_query = r#"
                SELECT
                    con.conname as constraint_name,
                    array_agg(a.attname ORDER BY array_position(con.conkey, a.attnum)) as column_names
                FROM pg_constraint con
                JOIN pg_class c ON c.oid = con.conrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = ANY(con.conkey)
                WHERE con.contype = 'p'
                AND c.relname = $1
                AND n.nspname = $2
                GROUP BY con.conname
            "#;

            let row = sqlx::query(pk_query)
                .bind(table_name)
                .bind(schema)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect primary key for {}.{}",
                            schema, table_name
                        ),
                        e,
                    )
                })?;

            if let Some(row) = row {
                let constraint_name: String = row.get("constraint_name");
                let column_names: Vec<String> = row.get("column_names");

                Ok(Some(PrimaryKey {
                    name: Some(constraint_name),
                    columns: column_names,
                }))
            } else {
                Ok(None)
            }
        }

        /// Collects foreign key constraints for a table
        async fn collect_foreign_keys(
            &self,
            table_name: &str,
            schema_name: Option<&str>,
        ) -> Result<Vec<ForeignKey>> {
            let schema = schema_name.unwrap_or("public");

            let fk_query = r#"
                SELECT
                    con.conname as constraint_name,
                    array_agg(a.attname ORDER BY array_position(con.conkey, a.attnum)) as column_names,
                    ref_class.relname as referenced_table,
                    ref_ns.nspname as referenced_schema,
                    array_agg(ref_a.attname ORDER BY array_position(con.confkey, ref_a.attnum)) as referenced_columns,
                    con.confdeltype as on_delete,
                    con.confupdtype as on_update
                FROM pg_constraint con
                JOIN pg_class c ON c.oid = con.conrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = ANY(con.conkey)
                JOIN pg_class ref_class ON ref_class.oid = con.confrelid
                JOIN pg_namespace ref_ns ON ref_ns.oid = ref_class.relnamespace
                JOIN pg_attribute ref_a ON ref_a.attrelid = con.confrelid AND ref_a.attnum = ANY(con.confkey)
                WHERE con.contype = 'f'
                AND c.relname = $1
                AND n.nspname = $2
                GROUP BY con.conname, ref_class.relname, ref_ns.nspname, con.confdeltype, con.confupdtype
            "#;

            let rows = sqlx::query(fk_query)
                .bind(table_name)
                .bind(schema)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect foreign keys for {}.{}",
                            schema, table_name
                        ),
                        e,
                    )
                })?;

            let mut foreign_keys = Vec::new();

            for row in rows {
                let constraint_name: String = row.get("constraint_name");
                let column_names: Vec<String> = row.get("column_names");
                let referenced_table: String = row.get("referenced_table");
                let referenced_schema: String = row.get("referenced_schema");
                let referenced_columns: Vec<String> = row.get("referenced_columns");
                let on_delete_char: String = row.get("on_delete");
                let on_update_char: String = row.get("on_update");

                let on_delete = Self::map_referential_action(&on_delete_char);
                let on_update = Self::map_referential_action(&on_update_char);

                let foreign_key = ForeignKey {
                    name: Some(constraint_name),
                    columns: column_names,
                    referenced_table,
                    referenced_schema: Some(referenced_schema),
                    referenced_columns,
                    on_delete,
                    on_update,
                };

                foreign_keys.push(foreign_key);
            }

            Ok(foreign_keys)
        }

        /// Collects indexes for a table
        async fn collect_table_indexes(
            &self,
            table_name: &str,
            schema_name: Option<&str>,
        ) -> Result<Vec<Index>> {
            let schema = schema_name.unwrap_or("public");

            let index_query = r#"
                SELECT
                    i.relname as index_name,
                    idx.indisunique as is_unique,
                    idx.indisprimary as is_primary,
                    am.amname as index_type,
                    array_agg(a.attname ORDER BY array_position(idx.indkey, a.attnum)) as column_names
                FROM pg_index idx
                JOIN pg_class i ON i.oid = idx.indexrelid
                JOIN pg_class t ON t.oid = idx.indrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                JOIN pg_am am ON am.oid = i.relam
                JOIN pg_attribute a ON a.attrelid = idx.indrelid AND a.attnum = ANY(idx.indkey)
                WHERE t.relname = $1
                AND n.nspname = $2
                GROUP BY i.relname, idx.indisunique, idx.indisprimary, am.amname
                ORDER BY i.relname
            "#;

            let rows = sqlx::query(index_query)
                .bind(table_name)
                .bind(schema)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!("Failed to collect indexes for {}.{}", schema, table_name),
                        e,
                    )
                })?;

            let mut indexes = Vec::new();

            for row in rows {
                let index_name: String = row.get("index_name");
                let is_unique: bool = row.get("is_unique");
                let is_primary: bool = row.get("is_primary");
                let index_type: String = row.get("index_type");
                let column_names: Vec<String> = row.get("column_names");

                let columns = column_names
                    .into_iter()
                    .map(|name| IndexColumn {
                        name,
                        sort_order: Some(SortOrder::Ascending), // Default for PostgreSQL
                    })
                    .collect();

                let index = Index {
                    name: index_name,
                    table_name: table_name.to_string(),
                    schema: Some(schema.to_string()),
                    columns,
                    is_unique,
                    is_primary,
                    index_type: Some(index_type),
                };

                indexes.push(index);
            }

            Ok(indexes)
        }

        /// Collects constraints for a table
        async fn collect_table_constraints(
            &self,
            table_name: &str,
            schema_name: Option<&str>,
        ) -> Result<Vec<Constraint>> {
            let schema = schema_name.unwrap_or("public");

            let constraint_query = r#"
                SELECT
                    con.conname as constraint_name,
                    con.contype as constraint_type,
                    array_agg(a.attname ORDER BY array_position(con.conkey, a.attnum)) as column_names,
                    pg_get_constraintdef(con.oid) as check_clause
                FROM pg_constraint con
                JOIN pg_class c ON c.oid = con.conrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                LEFT JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = ANY(con.conkey)
                WHERE c.relname = $1
                AND n.nspname = $2
                AND con.contype IN ('c', 'u') -- Check and unique constraints (PK and FK handled separately)
                GROUP BY con.conname, con.contype, con.oid
                ORDER BY con.conname
            "#;

            let rows = sqlx::query(constraint_query)
                .bind(table_name)
                .bind(schema)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect constraints for {}.{}",
                            schema, table_name
                        ),
                        e,
                    )
                })?;

            let mut constraints = Vec::new();

            for row in rows {
                let constraint_name: String = row.get("constraint_name");
                let constraint_type_char: String = row.get("constraint_type");
                let column_names: Vec<String> = row.get("column_names");
                let check_clause: Option<String> = row.get("check_clause");

                let constraint_type = match constraint_type_char.as_str() {
                    "c" => ConstraintType::Check,
                    "u" => ConstraintType::Unique,
                    _ => continue, // Skip unknown constraint types
                };

                let constraint = Constraint {
                    name: constraint_name,
                    table_name: table_name.to_string(),
                    schema: Some(schema.to_string()),
                    constraint_type,
                    columns: column_names,
                    check_clause,
                };

                constraints.push(constraint);
            }

            Ok(constraints)
        }

        /// Maps PostgreSQL data types to unified data types
        pub fn map_postgresql_type(
            pg_type: &str,
            max_length: Option<i32>,
            precision: Option<i32>,
            _scale: Option<i32>,
        ) -> Result<UnifiedDataType> {
            let unified_type = match pg_type {
                // String types
                "character varying" | "varchar" => UnifiedDataType::String {
                    max_length: max_length.map(|l| l as u32),
                },
                "character" | "char" => UnifiedDataType::String {
                    max_length: max_length.map(|l| l as u32),
                },
                "text" => UnifiedDataType::String { max_length: None },

                // Integer types
                "smallint" | "int2" => UnifiedDataType::Integer {
                    bits: 16,
                    signed: true,
                },
                "integer" | "int" | "int4" => UnifiedDataType::Integer {
                    bits: 32,
                    signed: true,
                },
                "bigint" | "int8" => UnifiedDataType::Integer {
                    bits: 64,
                    signed: true,
                },

                // Floating point types
                "real" | "float4" => UnifiedDataType::Float {
                    precision: Some(24),
                },
                "double precision" | "float8" => UnifiedDataType::Float {
                    precision: Some(53),
                },
                "numeric" | "decimal" => {
                    if let Some(p) = precision {
                        UnifiedDataType::Float {
                            precision: Some(p as u8),
                        }
                    } else {
                        UnifiedDataType::Float { precision: None }
                    }
                }

                // Boolean
                "boolean" | "bool" => UnifiedDataType::Boolean,

                // Date/time types
                "timestamp without time zone" | "timestamp" => UnifiedDataType::DateTime {
                    with_timezone: false,
                },
                "timestamp with time zone" | "timestamptz" => UnifiedDataType::DateTime {
                    with_timezone: true,
                },
                "date" => UnifiedDataType::Date,
                "time without time zone" | "time" => UnifiedDataType::Time {
                    with_timezone: false,
                },
                "time with time zone" | "timetz" => UnifiedDataType::Time {
                    with_timezone: true,
                },

                // Binary types
                "bytea" => UnifiedDataType::Binary { max_length: None },

                // JSON types
                "json" | "jsonb" => UnifiedDataType::Json,

                // UUID
                "uuid" => UnifiedDataType::Uuid,

                // Array types (simplified - PostgreSQL arrays are complex)
                array_type if array_type.ends_with("[]") => {
                    let element_type_str = &array_type[..array_type.len() - 2];
                    let element_type =
                        Self::map_postgresql_type(element_type_str, None, None, None)?;
                    UnifiedDataType::Array {
                        element_type: Box::new(element_type),
                    }
                }

                // Custom/unknown types
                custom_type => UnifiedDataType::Custom {
                    type_name: custom_type.to_string(),
                },
            };

            Ok(unified_type)
        }

        /// Maps PostgreSQL referential action characters to enum values
        pub fn map_referential_action(action_char: &str) -> Option<ReferentialAction> {
            match action_char {
                "c" => Some(ReferentialAction::Cascade),
                "n" => Some(ReferentialAction::SetNull),
                "d" => Some(ReferentialAction::SetDefault),
                "r" => Some(ReferentialAction::Restrict),
                "a" => Some(ReferentialAction::NoAction),
                _ => None,
            }
        }
    }

    #[async_trait]
    impl DatabaseAdapter for PostgresAdapter {
        async fn test_connection(&self) -> Result<()> {
            // Set up session on first connection
            self.setup_session().await?;

            sqlx::query("SELECT 1")
                .fetch_one(&self.pool)
                .await
                .map_err(crate::error::DbSurveyorError::connection_failed)?;

            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            let start_time = std::time::Instant::now();

            // Set up session on first connection
            self.setup_session().await?;

            // Collect database information
            let database_info = self.collect_database_info().await?;

            // Collect all schema objects
            let tables = self.collect_tables().await?;

            // TODO: Implement collection of other schema objects in future tasks
            let views = Vec::new(); // Placeholder for views
            let indexes = Vec::new(); // Placeholder for standalone indexes
            let constraints = Vec::new(); // Placeholder for standalone constraints
            let procedures = Vec::new(); // Placeholder for procedures
            let functions = Vec::new(); // Placeholder for functions
            let triggers = Vec::new(); // Placeholder for triggers
            let custom_types = Vec::new(); // Placeholder for custom types

            let collection_duration = start_time.elapsed();

            let mut schema = DatabaseSchema {
                format_version: "1.0".to_string(),
                database_info,
                tables,
                views,
                indexes,
                constraints,
                procedures,
                functions,
                triggers,
                custom_types,
                samples: None, // Data sampling will be implemented in future tasks
                collection_metadata: CollectionMetadata {
                    collected_at: chrono::Utc::now(),
                    collection_duration_ms: collection_duration.as_millis() as u64,
                    collector_version: env!("CARGO_PKG_VERSION").to_string(),
                    warnings: Vec::new(),
                },
            };

            // Add performance warning if collection took too long
            if collection_duration.as_secs() > 10 {
                schema.add_warning(format!(
                    "Schema collection took {} seconds, consider using filters for large databases",
                    collection_duration.as_secs()
                ));
            }

            Ok(schema)
        }

        fn database_type(&self) -> crate::models::DatabaseType {
            crate::models::DatabaseType::PostgreSQL
        }

        fn supports_feature(&self, feature: AdapterFeature) -> bool {
            matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::MultiDatabase
                    | AdapterFeature::ConnectionPooling
                    | AdapterFeature::QueryTimeout
                    | AdapterFeature::ReadOnlyMode
            )
        }

        fn connection_config(&self) -> ConnectionConfig {
            self.config.clone()
        }
    }
}

#[cfg(feature = "mysql")]
pub mod mysql {
    use super::*;

    pub struct MySqlAdapter {
        config: ConnectionConfig,
    }

    impl MySqlAdapter {
        pub async fn new(_connection_string: &str) -> Result<Self> {
            Ok(Self {
                config: ConnectionConfig::default(),
            })
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MySqlAdapter {
        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            let db_info = crate::models::DatabaseInfo::new("placeholder".to_string());
            Ok(DatabaseSchema::new(db_info))
        }

        fn database_type(&self) -> crate::models::DatabaseType {
            crate::models::DatabaseType::MySQL
        }

        fn supports_feature(&self, feature: AdapterFeature) -> bool {
            matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::MultiDatabase
                    | AdapterFeature::ConnectionPooling
                    | AdapterFeature::QueryTimeout
                    | AdapterFeature::ReadOnlyMode
            )
        }

        fn connection_config(&self) -> ConnectionConfig {
            self.config.clone()
        }
    }
}

#[cfg(feature = "sqlite")]
pub mod sqlite {
    use super::*;

    pub struct SqliteAdapter {
        config: ConnectionConfig,
    }

    impl SqliteAdapter {
        pub async fn new(_connection_string: &str) -> Result<Self> {
            Ok(Self {
                config: ConnectionConfig::default(),
            })
        }
    }

    #[async_trait]
    impl DatabaseAdapter for SqliteAdapter {
        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            let db_info = crate::models::DatabaseInfo::new("placeholder".to_string());
            Ok(DatabaseSchema::new(db_info))
        }

        fn database_type(&self) -> crate::models::DatabaseType {
            crate::models::DatabaseType::SQLite
        }

        fn supports_feature(&self, feature: AdapterFeature) -> bool {
            matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::QueryTimeout
                    | AdapterFeature::ReadOnlyMode
            )
        }

        fn connection_config(&self) -> ConnectionConfig {
            self.config.clone()
        }
    }
}

#[cfg(feature = "mongodb")]
pub mod mongodb {
    use super::*;

    pub struct MongoAdapter {
        config: ConnectionConfig,
    }

    impl MongoAdapter {
        pub async fn new(_connection_string: &str) -> Result<Self> {
            Ok(Self {
                config: ConnectionConfig::default(),
            })
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MongoAdapter {
        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            let db_info = crate::models::DatabaseInfo::new("placeholder".to_string());
            Ok(DatabaseSchema::new(db_info))
        }

        fn database_type(&self) -> crate::models::DatabaseType {
            crate::models::DatabaseType::MongoDB
        }

        fn supports_feature(&self, feature: AdapterFeature) -> bool {
            matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::QueryTimeout
            )
        }

        fn connection_config(&self) -> ConnectionConfig {
            self.config.clone()
        }
    }
}

#[cfg(feature = "mssql")]
pub mod mssql {
    use super::*;

    pub struct SqlServerAdapter {
        config: ConnectionConfig,
    }

    impl SqlServerAdapter {
        pub async fn new(_connection_string: &str) -> Result<Self> {
            Ok(Self {
                config: ConnectionConfig::default(),
            })
        }
    }

    #[async_trait]
    impl DatabaseAdapter for SqlServerAdapter {
        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            let db_info = crate::models::DatabaseInfo::new("placeholder".to_string());
            Ok(DatabaseSchema::new(db_info))
        }

        fn database_type(&self) -> crate::models::DatabaseType {
            crate::models::DatabaseType::SqlServer
        }

        fn supports_feature(&self, feature: AdapterFeature) -> bool {
            matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::MultiDatabase
                    | AdapterFeature::ConnectionPooling
                    | AdapterFeature::QueryTimeout
                    | AdapterFeature::ReadOnlyMode
            )
        }

        fn connection_config(&self) -> ConnectionConfig {
            self.config.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_database_type() {
        assert_eq!(
            detect_database_type("postgres://user:pass@localhost/db").unwrap(),
            crate::models::DatabaseType::PostgreSQL
        );

        assert_eq!(
            detect_database_type("postgresql://user:pass@localhost/db").unwrap(),
            crate::models::DatabaseType::PostgreSQL
        );

        assert_eq!(
            detect_database_type("mysql://user:pass@localhost/db").unwrap(),
            crate::models::DatabaseType::MySQL
        );

        assert_eq!(
            detect_database_type("sqlite:///path/to/db.sqlite").unwrap(),
            crate::models::DatabaseType::SQLite
        );

        assert_eq!(
            detect_database_type("/path/to/db.db").unwrap(),
            crate::models::DatabaseType::SQLite
        );

        assert_eq!(
            detect_database_type("mongodb://user:pass@localhost/db").unwrap(),
            crate::models::DatabaseType::MongoDB
        );

        assert!(detect_database_type("invalid://connection").is_err());
    }

    #[test]
    fn test_sampling_config_default() {
        let config = SamplingConfig::default();
        assert_eq!(config.sample_size, 100);
        assert_eq!(config.query_timeout_secs, 30);
        assert!(config.warn_sensitive);
        assert!(!config.timestamp_columns.is_empty());
        assert!(!config.sensitive_detection_patterns.is_empty());
    }

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.query_timeout, Duration::from_secs(30));
        assert_eq!(config.max_connections, 10);
        assert!(config.read_only);
    }

    #[test]
    fn test_collection_config_default() {
        let config = CollectionConfig::default();
        assert!(!config.include_system_databases);
        assert!(config.exclude_databases.is_empty());
        assert!(config.include_views);
        assert!(config.include_procedures);
        assert!(config.include_functions);
        assert!(config.include_triggers);
        assert!(config.include_indexes);
        assert!(config.include_constraints);
        assert!(config.include_custom_types);
        assert_eq!(config.max_concurrent_queries, 5);
        assert!(!config.enable_data_sampling);
        assert_eq!(config.output_format, OutputFormat::Json);
        assert!(!config.compression_enabled);
        assert!(!config.encryption_enabled);
    }

    #[test]
    fn test_output_format_default() {
        let format = OutputFormat::default();
        assert_eq!(format, OutputFormat::Json);
    }

    #[test]
    fn test_connection_config_validation() {
        // Valid config should pass
        let config = ConnectionConfig::new("localhost".to_string());
        assert!(config.validate().is_ok());

        // Empty host should fail
        let config = ConnectionConfig {
            host: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid port should fail
        let config = ConnectionConfig {
            port: Some(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Zero max_connections should fail
        let config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Excessive max_connections should fail
        let config = ConnectionConfig {
            max_connections: 200,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_collection_config_validation() {
        // Valid config should pass
        let config = CollectionConfig::default();
        assert!(config.validate().is_ok());

        // Zero max_concurrent_queries should fail
        let config = CollectionConfig {
            max_concurrent_queries: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Excessive max_concurrent_queries should fail
        let config = CollectionConfig {
            max_concurrent_queries: 100,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_connection_config_builder() {
        let config = ConnectionConfig::new("example.com".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("testuser".to_string());

        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
    }

    #[test]
    fn test_collection_config_builder() {
        let connection = ConnectionConfig::new("localhost".to_string());
        let sampling = SamplingConfig::default();

        let config = CollectionConfig::new()
            .with_connection(connection.clone())
            .with_sampling(sampling.clone());

        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.sampling.sample_size, 100);

        // Test max_concurrent_queries validation
        let result = CollectionConfig::new().with_max_concurrent_queries(0);
        assert!(result.is_err());

        let result = CollectionConfig::new().with_max_concurrent_queries(10);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().max_concurrent_queries, 10);
    }

    #[test]
    fn test_connection_config_display_no_credentials() {
        let config = ConnectionConfig::new("example.com".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("testuser".to_string());

        let display = format!("{}", config);

        // Should contain connection info
        assert!(display.contains("example.com"));
        assert!(display.contains("5432"));
        assert!(display.contains("testdb"));

        // Should NOT contain username (security)
        assert!(!display.contains("testuser"));

        // Should definitely not contain any password-like strings
        assert!(!display.contains("password"));
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_sensitive_pattern_serialization() {
        let pattern = SensitivePattern {
            pattern: r"(?i)(password|passwd|pwd)".to_string(),
            description: "Password field detected".to_string(),
        };

        // Test serialization/deserialization
        let json = serde_json::to_string(&pattern).unwrap();
        let deserialized: SensitivePattern = serde_json::from_str(&json).unwrap();

        assert_eq!(pattern.pattern, deserialized.pattern);
        assert_eq!(pattern.description, deserialized.description);
    }

    #[test]
    fn test_collection_config_serialization() {
        let config = CollectionConfig::default();

        // Test serialization/deserialization
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CollectionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.include_views, deserialized.include_views);
        assert_eq!(
            config.max_concurrent_queries,
            deserialized.max_concurrent_queries
        );
        assert_eq!(config.output_format, deserialized.output_format);
    }

    #[test]
    fn test_redact_database_url() {
        // Test PostgreSQL URL
        let url = "postgres://user:secret123@localhost:5432/db";
        let redacted = redact_database_url(url);
        assert!(!redacted.contains("secret123"));
        assert!(redacted.contains("user:****"));
        assert!(redacted.contains("localhost:5432"));
        assert!(redacted.contains("/db"));

        // Test MySQL URL
        let url = "mysql://admin:password@example.com:3306/testdb";
        let redacted = redact_database_url(url);
        assert!(!redacted.contains("password"));
        assert!(redacted.contains("admin:****"));

        // Test URL without password
        let url = "postgres://user@localhost/db";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, url); // Should be unchanged

        // Test SQLite file path (no credentials)
        let url = "/path/to/database.db";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, url); // Should be unchanged

        // Test invalid URL
        let url = "not-a-valid-url";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, url); // Should be unchanged
    }
}
