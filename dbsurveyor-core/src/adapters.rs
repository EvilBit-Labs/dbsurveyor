//! Database adapter traits and factory for unified database access.
//!
//! This module defines the core traits that all database adapters must implement
//! to provide a unified interface for schema collection across different database
//! engines. The design emphasizes object safety and security.

use crate::{Result, models::DatabaseSchema};
use async_trait::async_trait;
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
#[derive(Debug, Clone)]
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

/// Configuration for data sampling
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct SensitivePattern {
    pub pattern: String,
    pub description: String,
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

    pub struct PostgresAdapter {
        config: ConnectionConfig,
    }

    impl PostgresAdapter {
        pub async fn new(_connection_string: &str) -> Result<Self> {
            // Placeholder implementation
            Ok(Self {
                config: ConnectionConfig::default(),
            })
        }
    }

    #[async_trait]
    impl DatabaseAdapter for PostgresAdapter {
        async fn test_connection(&self) -> Result<()> {
            // Placeholder implementation
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            // Placeholder implementation
            let db_info = crate::models::DatabaseInfo {
                name: "placeholder".to_string(),
                version: None,
                size_bytes: None,
                encoding: None,
                collation: None,
            };
            Ok(DatabaseSchema::new(db_info))
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
            let db_info = crate::models::DatabaseInfo {
                name: "placeholder".to_string(),
                version: None,
                size_bytes: None,
                encoding: None,
                collation: None,
            };
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
            let db_info = crate::models::DatabaseInfo {
                name: "placeholder".to_string(),
                version: None,
                size_bytes: None,
                encoding: None,
                collation: None,
            };
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
            let db_info = crate::models::DatabaseInfo {
                name: "placeholder".to_string(),
                version: None,
                size_bytes: None,
                encoding: None,
                collation: None,
            };
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
            let db_info = crate::models::DatabaseInfo {
                name: "placeholder".to_string(),
                version: None,
                size_bytes: None,
                encoding: None,
                collation: None,
            };
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
}
