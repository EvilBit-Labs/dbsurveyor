//! Database adapter traits and factory for unified database access.
//!
//! This module defines the core traits that all database adapters must implement
//! to provide a unified interface for schema collection across different database
//! engines. The design emphasizes object safety and security.
//!
//! # Module Structure
//! - `config`: Configuration types (ConnectionConfig, SamplingConfig, CollectionConfig)
//! - `helpers`: Shared helper utilities
//! - `placeholder`: Placeholder adapter macro for unimplemented databases
//! - Database-specific modules (postgres, mysql, sqlite, mongodb, mssql)

use crate::{Result, models::{DatabaseSchema, TableSample}};
use async_trait::async_trait;

// Configuration module
pub mod config;

// Re-export configuration types for convenience
pub use config::{
    CollectionConfig, ConnectionConfig, DatabaseCollectionResult, DatabaseFailure,
    MultiDatabaseConfig, MultiDatabaseMetadata, MultiDatabaseResult, OutputFormat, SamplingConfig,
    SensitivePattern,
};

/// Features that database adapters may support.
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

/// Main trait for database adapters with object-safe design.
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
    /// Tests the database connection without collecting schema.
    ///
    /// # Security
    /// - Uses read-only connection if supported
    /// - Times out after configured duration
    /// - Never logs connection details
    ///
    /// # Errors
    /// Returns error if connection fails or times out
    async fn test_connection(&self) -> Result<()>;

    /// Collects comprehensive database schema metadata.
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

    /// Returns the database type this adapter handles.
    fn database_type(&self) -> crate::models::DatabaseType;

    /// Checks if the adapter supports a specific feature.
    fn supports_feature(&self, feature: AdapterFeature) -> bool;

    /// Gets the connection configuration (credentials sanitized).
    fn connection_config(&self) -> ConnectionConfig;

    /// Samples data from tables in the collected schema.
    ///
    /// This method iterates over the tables in the schema and collects
    /// sample rows using the provided sampling configuration. Each adapter
    /// implementation handles sampling according to its database engine's
    /// capabilities.
    ///
    /// # Arguments
    /// * `schema` - The collected schema containing table metadata
    /// * `config` - Sampling configuration (sample size, throttle, etc.)
    ///
    /// # Returns
    /// A vector of `TableSample` results, one per sampled table.
    /// Tables that fail to sample are logged and skipped.
    ///
    /// # Default Implementation
    /// Returns an empty vector (no sampling support).
    async fn sample_tables(
        &self,
        _schema: &DatabaseSchema,
        _config: &SamplingConfig,
    ) -> Result<Vec<TableSample>> {
        Ok(Vec::new())
    }
}

/// Factory function to create database adapters based on connection string.
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

/// Safely redacts credentials from database connection URLs.
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
///
/// # Note
/// This function delegates to `crate::error::redact_database_url` for consistency.
/// Invalid URLs are fully redacted as "<redacted>" for security.
#[inline]
pub fn redact_database_url(url: &str) -> String {
    crate::error::redact_database_url(url)
}

/// Detects database type from connection string.
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
        || connection_string.starts_with("sqlite:")
        || connection_string == ":memory:"
        || connection_string.ends_with(".db")
        || connection_string.ends_with(".sqlite")
        || connection_string.ends_with(".sqlite3")
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

// Shared helper utilities
pub mod helpers;

// Placeholder adapter macro
pub mod placeholder;

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
            detect_database_type(":memory:").unwrap(),
            crate::models::DatabaseType::SQLite
        );

        assert_eq!(
            detect_database_type("sqlite::memory:").unwrap(),
            crate::models::DatabaseType::SQLite
        );

        assert_eq!(
            detect_database_type("test.sqlite3").unwrap(),
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

        // Test SQLite file path (not a valid URL, gets fully redacted for security)
        let url = "/path/to/database.db";
        let redacted = redact_database_url(url);
        assert_eq!(redacted, "<redacted>"); // Invalid URLs are fully redacted
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
