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
#[non_exhaustive]
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
///
/// This configuration supports multiple sources with the following precedence:
/// 1. Explicit configuration (highest priority)
/// 2. Environment variables (prefixed with `DBSURVEYOR_`)
/// 3. Default values (lowest priority)
///
/// # Environment Variables
///
/// - `DBSURVEYOR_MAX_CONNECTIONS`: Maximum pool connections (default: 10)
/// - `DBSURVEYOR_MIN_IDLE_CONNECTIONS`: Minimum idle connections (default: 2)
/// - `DBSURVEYOR_CONNECT_TIMEOUT_SECS`: Connection timeout in seconds (default: 30)
/// - `DBSURVEYOR_ACQUIRE_TIMEOUT_SECS`: Acquire timeout in seconds (default: 30)
/// - `DBSURVEYOR_IDLE_TIMEOUT_SECS`: Idle timeout in seconds (default: 600)
/// - `DBSURVEYOR_MAX_LIFETIME_SECS`: Max lifetime in seconds (default: 3600)
///
/// # Example
///
/// ```rust
/// use dbsurveyor_collect::adapters::ConnectionConfig;
///
/// // Use defaults
/// let config = ConnectionConfig::default();
///
/// // Load from environment
/// let config = ConnectionConfig::from_env();
///
/// // Custom configuration
/// let config = ConnectionConfig::builder()
///     .max_connections(20)
///     .min_idle_connections(5)
///     .build();
/// ```
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

impl ConnectionConfig {
    /// Create a new configuration builder
    pub fn builder() -> ConnectionConfigBuilder {
        ConnectionConfigBuilder::default()
    }

    /// Load configuration from environment variables
    ///
    /// Environment variables are prefixed with `DBSURVEYOR_` and override defaults.
    /// Invalid values are logged and defaults are used instead.
    ///
    /// # Example
    ///
    /// ```bash
    /// export DBSURVEYOR_MAX_CONNECTIONS=20
    /// export DBSURVEYOR_CONNECT_TIMEOUT_SECS=60
    /// ```
    pub fn from_env() -> Self {
        Self::builder().with_env_overrides().build()
    }

    /// Validate configuration parameters
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `max_connections` is 0 or exceeds 1000
    /// - `min_idle_connections` exceeds `max_connections`
    /// - Any timeout is 0 or exceeds 1 hour
    pub fn validate(&self) -> AdapterResult<()> {
        // Validate max_connections
        if self.max_connections == 0 {
            return Err(AdapterError::InvalidParameters);
        }
        if self.max_connections > 1000 {
            return Err(AdapterError::Generic(
                "max_connections cannot exceed 1000".to_string(),
            ));
        }

        // Validate min_idle_connections
        if self.min_idle_connections > self.max_connections {
            return Err(AdapterError::Generic(
                "min_idle_connections cannot exceed max_connections".to_string(),
            ));
        }

        // Validate timeouts
        let max_timeout = Duration::from_secs(3600); // 1 hour
        if self.connect_timeout.is_zero() || self.connect_timeout > max_timeout {
            return Err(AdapterError::Generic(
                "connect_timeout must be between 1s and 1h".to_string(),
            ));
        }
        if self.acquire_timeout.is_zero() || self.acquire_timeout > max_timeout {
            return Err(AdapterError::Generic(
                "acquire_timeout must be between 1s and 1h".to_string(),
            ));
        }
        if self.idle_timeout.is_zero() || self.idle_timeout > max_timeout {
            return Err(AdapterError::Generic(
                "idle_timeout must be between 1s and 1h".to_string(),
            ));
        }
        if self.max_lifetime.is_zero() || self.max_lifetime > max_timeout {
            return Err(AdapterError::Generic(
                "max_lifetime must be between 1s and 1h".to_string(),
            ));
        }

        Ok(())
    }

    /// Adjust configuration parameters to safe values
    ///
    /// This method ensures all parameters are within acceptable ranges:
    /// - Clamps `max_connections` to 1-1000
    /// - Ensures `min_idle_connections` <= `max_connections`
    /// - Clamps timeouts to 1s-1h range
    ///
    /// Logs a warning for each value that was adjusted.
    pub fn adjust(&mut self) {
        let min_timeout = Duration::from_secs(1);
        let max_timeout = Duration::from_secs(3600);

        let orig_max_conn = self.max_connections;
        self.max_connections = self.max_connections.clamp(1, 1000);
        if self.max_connections != orig_max_conn {
            tracing::warn!(
                "Adjusted max_connections from {} to {}",
                orig_max_conn,
                self.max_connections
            );
        }

        if self.min_idle_connections > self.max_connections {
            tracing::warn!(
                "Adjusted min_idle_connections from {} to {} (cannot exceed max_connections)",
                self.min_idle_connections,
                self.max_connections
            );
            self.min_idle_connections = self.max_connections;
        }

        Self::adjust_timeout(
            &mut self.connect_timeout,
            "connect_timeout",
            Duration::from_secs(30),
            min_timeout,
            max_timeout,
        );
        Self::adjust_timeout(
            &mut self.acquire_timeout,
            "acquire_timeout",
            Duration::from_secs(30),
            min_timeout,
            max_timeout,
        );
        Self::adjust_timeout(
            &mut self.idle_timeout,
            "idle_timeout",
            Duration::from_secs(600),
            min_timeout,
            max_timeout,
        );
        Self::adjust_timeout(
            &mut self.max_lifetime,
            "max_lifetime",
            Duration::from_secs(3600),
            min_timeout,
            max_timeout,
        );
    }

    fn adjust_timeout(
        timeout: &mut Duration,
        name: &str,
        default: Duration,
        min: Duration,
        max: Duration,
    ) {
        let orig = *timeout;
        if timeout.is_zero() {
            *timeout = default;
        }
        *timeout = (*timeout).clamp(min, max);
        if *timeout != orig {
            tracing::warn!("Adjusted {} from {:?} to {:?}", name, orig, *timeout);
        }
    }
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

/// Builder for `ConnectionConfig` with environment variable support
#[derive(Debug, Clone, Default)]
pub struct ConnectionConfigBuilder {
    max_connections: Option<u32>,
    min_idle_connections: Option<u32>,
    connect_timeout: Option<Duration>,
    acquire_timeout: Option<Duration>,
    idle_timeout: Option<Duration>,
    max_lifetime: Option<Duration>,
}

impl ConnectionConfigBuilder {
    /// Set maximum number of connections
    pub fn max_connections(mut self, value: u32) -> Self {
        self.max_connections = Some(value);
        self
    }

    /// Set minimum idle connections
    pub fn min_idle_connections(mut self, value: u32) -> Self {
        self.min_idle_connections = Some(value);
        self
    }

    /// Set connection timeout
    pub fn connect_timeout(mut self, value: Duration) -> Self {
        self.connect_timeout = Some(value);
        self
    }

    /// Set acquire timeout
    pub fn acquire_timeout(mut self, value: Duration) -> Self {
        self.acquire_timeout = Some(value);
        self
    }

    /// Set idle timeout
    pub fn idle_timeout(mut self, value: Duration) -> Self {
        self.idle_timeout = Some(value);
        self
    }

    /// Set maximum lifetime
    pub fn max_lifetime(mut self, value: Duration) -> Self {
        self.max_lifetime = Some(value);
        self
    }

    /// Load overrides from environment variables
    ///
    /// Environment variables are prefixed with `DBSURVEYOR_`.
    /// Invalid values are logged as warnings and ignored.
    pub fn with_env_overrides(mut self) -> Self {
        self.try_parse_env_u32("DBSURVEYOR_MAX_CONNECTIONS", |s, v| {
            s.max_connections = Some(v);
        });
        self.try_parse_env_u32("DBSURVEYOR_MIN_IDLE_CONNECTIONS", |s, v| {
            s.min_idle_connections = Some(v);
        });
        self.try_parse_env_duration("DBSURVEYOR_CONNECT_TIMEOUT_SECS", |s, v| {
            s.connect_timeout = Some(v);
        });
        self.try_parse_env_duration("DBSURVEYOR_ACQUIRE_TIMEOUT_SECS", |s, v| {
            s.acquire_timeout = Some(v);
        });
        self.try_parse_env_duration("DBSURVEYOR_IDLE_TIMEOUT_SECS", |s, v| {
            s.idle_timeout = Some(v);
        });
        self.try_parse_env_duration("DBSURVEYOR_MAX_LIFETIME_SECS", |s, v| {
            s.max_lifetime = Some(v);
        });

        self
    }

    fn try_parse_env_u32(&mut self, var: &str, apply: impl FnOnce(&mut Self, u32)) {
        if let Ok(val) = std::env::var(var) {
            match val.parse::<u32>() {
                Ok(parsed) => apply(self, parsed),
                Err(_) => tracing::warn!(
                    "Ignoring invalid {} value '{}': expected a positive integer",
                    var,
                    val
                ),
            }
        }
    }

    fn try_parse_env_duration(
        &mut self,
        var: &str,
        apply: impl FnOnce(&mut Self, Duration),
    ) {
        if let Ok(val) = std::env::var(var) {
            match val.parse::<u64>() {
                Ok(parsed) => apply(self, Duration::from_secs(parsed)),
                Err(_) => tracing::warn!(
                    "Ignoring invalid {} value '{}': expected seconds as a positive integer",
                    var,
                    val
                ),
            }
        }
    }

    /// Build the configuration with defaults for unset values
    pub fn build(self) -> ConnectionConfig {
        let defaults = ConnectionConfig::default();

        ConnectionConfig {
            max_connections: self.max_connections.unwrap_or(defaults.max_connections),
            min_idle_connections: self
                .min_idle_connections
                .unwrap_or(defaults.min_idle_connections),
            connect_timeout: self.connect_timeout.unwrap_or(defaults.connect_timeout),
            acquire_timeout: self.acquire_timeout.unwrap_or(defaults.acquire_timeout),
            idle_timeout: self.idle_timeout.unwrap_or(defaults.idle_timeout),
            max_lifetime: self.max_lifetime.unwrap_or(defaults.max_lifetime),
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
    fn test_connection_config_builder() {
        let config = ConnectionConfig::builder()
            .max_connections(20)
            .min_idle_connections(5)
            .connect_timeout(Duration::from_secs(60))
            .build();

        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_idle_connections, 5);
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        // Other values should use defaults
        assert_eq!(config.acquire_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_connection_config_validation() {
        // Valid configuration
        let config = ConnectionConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: max_connections = 0
        let config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: max_connections > 1000
        let config = ConnectionConfig {
            max_connections: 1001,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: min_idle > max_connections
        let config = ConnectionConfig {
            max_connections: 10,
            min_idle_connections: 20,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid: zero timeout
        let config = ConnectionConfig {
            connect_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_connection_config_adjustment() {
        // Test max_connections clamping
        let mut config = ConnectionConfig {
            max_connections: 2000,
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.max_connections, 1000);

        // Test min_idle adjustment
        let mut config = ConnectionConfig {
            max_connections: 5,
            min_idle_connections: 10,
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.min_idle_connections, 5);

        // Test zero timeout adjustment
        let mut config = ConnectionConfig {
            connect_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        config.adjust();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_connection_config_from_env() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("20")),
                ("DBSURVEYOR_MIN_IDLE_CONNECTIONS", Some("5")),
                ("DBSURVEYOR_CONNECT_TIMEOUT_SECS", Some("60")),
            ],
            || {
                let config = ConnectionConfig::from_env();
                assert_eq!(config.max_connections, 20);
                assert_eq!(config.min_idle_connections, 5);
                assert_eq!(config.connect_timeout, Duration::from_secs(60));
            },
        );
    }

    #[test]
    fn test_connection_config_env_invalid_values() {
        temp_env::with_vars(
            vec![
                ("DBSURVEYOR_MAX_CONNECTIONS", Some("invalid")),
                ("DBSURVEYOR_CONNECT_TIMEOUT_SECS", Some("not_a_number")),
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
