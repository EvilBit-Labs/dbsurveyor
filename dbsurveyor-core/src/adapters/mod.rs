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

/// Pattern for detecting sensitive data fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivePattern {
    pub pattern: String,
    pub description: String,
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

        self.connection.validate()?;

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
pub async fn create_adapter(connection_string: &str) -> Result<Box<dyn DatabaseAdapter>> {
    let database_type = detect_database_type(connection_string)?;

    match database_type {
        #[cfg(feature = "postgresql")]
        crate::models::DatabaseType::PostgreSQL => {
            let adapter = postgres::PostgresAdapter::new(connection_string).await?;
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
            let adapter = mysql::MySqlAdapter::new(connection_string).await?;
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
            let adapter = sqlite::SqliteAdapter::new(connection_string).await?;
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
            let adapter = mongodb::MongoAdapter::new(connection_string).await?;
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
                let adapter = mssql::SqlServerAdapter::new(connection_string).await?;
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

// Database-specific adapter modules
#[cfg(feature = "postgresql")]
pub mod postgres;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "mongodb")]
pub mod mongodb;

#[cfg(feature = "mssql")]
pub mod mssql;

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
    }
}
