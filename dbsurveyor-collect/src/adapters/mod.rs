//! Database adapter implementations for unified schema collection
//!
//! This module provides trait-based adapters for connecting to and collecting
//! metadata from various database engines. All implementations follow security-first
//! principles with zero credential storage and comprehensive error sanitization.

use async_trait::async_trait;
use std::time::Duration;

/// Result type for adapter operations
pub type AdapterResult<T> = Result<T, AdapterError>;

/// Unified error type for database adapters with sanitized messages
#[derive(thiserror::Error, Debug)]
pub enum AdapterError {
    /// Connection failed without exposing credentials
    #[error("Database connection failed")]
    ConnectionFailed,

    /// Connection timeout
    #[error("Connection timeout after {0:?}")]
    ConnectionTimeout(Duration),

    /// Query execution failed
    #[error("Query execution failed")]
    QueryFailed,

    /// Invalid connection parameters (sanitized)
    #[error("Invalid connection parameters")]
    InvalidParameters,

    /// Feature not supported by this adapter
    #[error("Feature not supported: {0}")]
    UnsupportedFeature(String),

    /// Database-specific error (sanitized)
    #[error("Database error occurred")]
    DatabaseError,

    /// Pool exhaustion
    #[error("Connection pool exhausted")]
    PoolExhausted,

    /// Generic error with context
    #[error("Adapter error: {0}")]
    Generic(String),
}

/// Connection configuration for database adapters
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Maximum number of connections in pool
    pub max_connections: u32,
    /// Minimum idle connections
    pub min_idle_connections: u32,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Acquire connection timeout
    pub acquire_timeout: Duration,
    /// Idle connection timeout
    pub idle_timeout: Duration,
    /// Maximum connection lifetime
    pub max_lifetime: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_idle_connections: 2,
            connect_timeout: Duration::from_secs(30),
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600), // 10 minutes
            max_lifetime: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Database metadata collection result
#[derive(Debug, Clone)]
pub struct DatabaseMetadata {
    /// Database type (postgresql, mysql, sqlite, mongodb)
    pub database_type: String,
    /// Database version
    pub version: Option<String>,
    /// List of schemas/databases
    pub schemas: Vec<SchemaMetadata>,
}

/// Schema metadata
#[derive(Debug, Clone)]
pub struct SchemaMetadata {
    /// Schema name
    pub name: String,
    /// Tables in this schema
    pub tables: Vec<TableMetadata>,
}

/// Table metadata
#[derive(Debug, Clone)]
pub struct TableMetadata {
    /// Table name
    pub name: String,
    /// Schema name
    pub schema: String,
    /// Column definitions
    pub columns: Vec<ColumnMetadata>,
    /// Row count estimate
    pub row_count: Option<u64>,
}

/// Column metadata
#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    /// Column name
    pub name: String,
    /// Data type
    pub data_type: String,
    /// Nullable flag
    pub is_nullable: bool,
    /// Default value
    pub default_value: Option<String>,
}

/// Unified trait for database schema collection
///
/// This trait provides a consistent interface for connecting to and collecting
/// metadata from various database types while maintaining security guarantees.
///
/// # Security
///
/// All implementations must ensure:
/// - Database credentials are never logged or stored
/// - All operations are read-only
/// - Connection strings are sanitized in error messages
/// - Memory is properly cleaned up after use
///
/// # Example
///
/// ```rust,no_run
/// use dbsurveyor_collect::adapters::{SchemaCollector, ConnectionConfig};
///
/// async fn collect_schema() -> Result<(), Box<dyn std::error::Error>> {
///     # #[cfg(feature = "postgresql")]
///     # {
///     use dbsurveyor_collect::adapters::postgresql::PostgresAdapter;
///     
///     let config = ConnectionConfig::default();
///     let adapter = PostgresAdapter::new(
///         "postgresql://user:pass@localhost/db",
///         config
///     ).await?;
///     
///     adapter.test_connection().await?;
///     let metadata = adapter.collect_metadata().await?;
///     
///     println!("Collected {} schemas", metadata.schemas.len());
///     # }
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait SchemaCollector: Send + Sync {
    /// Get the database type identifier
    fn database_type(&self) -> &'static str;

    /// Test database connectivity without collecting data
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established
    async fn test_connection(&self) -> AdapterResult<()>;

    /// Collect complete database metadata
    ///
    /// # Errors
    ///
    /// Returns an error if metadata collection fails
    async fn collect_metadata(&self) -> AdapterResult<DatabaseMetadata>;

    /// Get a safe description for logging (no credentials)
    fn safe_description(&self) -> String;
}

// Feature-gated adapter modules
#[cfg(feature = "postgresql")]
pub mod postgresql;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "mssql")]
pub mod sqlserver;

#[cfg(feature = "oracle")]
pub mod oracle;

#[cfg(feature = "mongodb")]
pub mod mongodb;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config_defaults() {
        let config = ConnectionConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_idle_connections, 2);
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.acquire_timeout, Duration::from_secs(30));
        assert_eq!(config.idle_timeout, Duration::from_secs(600));
        assert_eq!(config.max_lifetime, Duration::from_secs(3600));
    }

    #[test]
    fn test_adapter_error_display() {
        let err = AdapterError::ConnectionFailed;
        assert_eq!(err.to_string(), "Database connection failed");

        let err = AdapterError::ConnectionTimeout(Duration::from_secs(30));
        assert!(err.to_string().contains("timeout"));

        let err = AdapterError::UnsupportedFeature("test".to_string());
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn test_metadata_structures() {
        let column = ColumnMetadata {
            name: "id".to_string(),
            data_type: "integer".to_string(),
            is_nullable: false,
            default_value: None,
        };

        let table = TableMetadata {
            name: "users".to_string(),
            schema: "public".to_string(),
            columns: vec![column],
            row_count: Some(100),
        };

        let schema = SchemaMetadata {
            name: "public".to_string(),
            tables: vec![table],
        };

        let metadata = DatabaseMetadata {
            database_type: "postgresql".to_string(),
            version: Some("14.1".to_string()),
            schemas: vec![schema],
        };

        assert_eq!(metadata.database_type, "postgresql");
        assert_eq!(metadata.schemas.len(), 1);
        assert_eq!(metadata.schemas[0].tables.len(), 1);
        assert_eq!(metadata.schemas[0].tables[0].columns.len(), 1);
    }
}
