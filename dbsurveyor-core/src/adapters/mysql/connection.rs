//! MySQL connection pool management and validation.
//!
//! # Security Features
//! - Validates connection string format and parameters
//! - Enforces connection limits to prevent resource exhaustion
//! - Sets appropriate timeouts for all operations

use super::{ConnectionConfig, MySqlAdapter};
use crate::Result;
use sqlx::MySqlPool;
use std::time::Duration;
use url::Url;

/// Pool statistics for monitoring connection pool health and usage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    /// Number of idle connections ready to be used
    pub idle_connections: u32,
    /// Number of connections currently in use
    pub active_connections: u32,
    /// Total number of connections in the pool
    pub total_connections: u32,
    /// Maximum allowed connections (from configuration)
    pub max_connections: u32,
}

impl MySqlAdapter {
    /// Creates a new MySQL adapter with connection pooling
    ///
    /// # Arguments
    /// * `connection_string` - MySQL connection URL (credentials sanitized in errors)
    ///
    /// # Security
    /// - Enforces read-only mode by default
    /// - Sets query timeout for safety
    /// - Sanitizes connection string in all error messages
    /// - Validates connection parameters for security
    ///
    /// # Errors
    /// Returns error if:
    /// - Connection string format is invalid
    /// - Database connection fails
    /// - Pool configuration is invalid
    pub async fn new(connection_string: &str) -> Result<Self> {
        // Parse and validate connection configuration
        let config = parse_mysql_connection_config(connection_string)?;

        // Create connection pool with security settings
        let pool = create_mysql_connection_pool(connection_string, &config).await?;

        let adapter = Self {
            pool,
            config,
            connection_url: connection_string.to_string(),
        };
        Ok(adapter)
    }

    /// Creates a new MySQL adapter with custom configuration
    ///
    /// # Arguments
    /// * `connection_string` - MySQL connection URL
    /// * `config` - Custom connection configuration
    ///
    /// # Security
    /// Same security guarantees as `new()` but allows custom configuration
    pub async fn with_config(connection_string: &str, config: ConnectionConfig) -> Result<Self> {
        // Validate the provided configuration
        config.validate()?;

        // Validate connection string
        validate_mysql_connection_string(connection_string)?;

        // Create connection pool
        let pool = create_mysql_connection_pool(connection_string, &config).await?;

        let adapter = Self {
            pool,
            config,
            connection_url: connection_string.to_string(),
        };
        Ok(adapter)
    }

    /// Gets connection pool statistics for monitoring
    ///
    /// # Returns
    /// Tuple of (active_connections, idle_connections, total_connections)
    pub fn pool_stats(&self) -> (u32, u32, u32) {
        let stats = self.pool_statistics();
        (
            stats.active_connections,
            stats.idle_connections,
            stats.total_connections,
        )
    }

    /// Gets detailed connection pool statistics for monitoring
    ///
    /// # Returns
    /// A `PoolStats` struct with detailed pool information
    pub fn pool_statistics(&self) -> PoolStats {
        let size = self.pool.size();
        let idle = self.pool.num_idle();
        let idle_u32 = idle.min(u32::MAX as usize) as u32;
        let size_u32 = size;
        PoolStats {
            idle_connections: idle_u32,
            active_connections: size_u32.saturating_sub(idle_u32),
            total_connections: size_u32,
            max_connections: self.config.max_connections,
        }
    }

    /// Closes the connection pool gracefully
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Checks the health of the connection pool
    ///
    /// # Returns
    /// True if the pool is healthy and can acquire connections
    pub async fn is_pool_healthy(&self) -> bool {
        match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
        {
            Ok(result) => result == 1,
            Err(_) => false,
        }
    }

    /// Generate connection URL for a different database on the same server.
    ///
    /// # Arguments
    /// * `database` - Name of the database to generate URL for
    ///
    /// # Returns
    /// A new connection URL string targeting the specified database.
    pub fn connection_url_for_database(&self, database: &str) -> Result<String> {
        // Validate database name length
        if database.is_empty() || database.len() > 64 {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Invalid database name length: must be 1-64 characters, got {}",
                database.len()
            )));
        }

        // Check for dangerous characters
        if database.contains(';') || database.contains('\'') || database.contains('"') {
            return Err(crate::error::DbSurveyorError::configuration(
                "Database name contains invalid characters",
            ));
        }

        // Parse the original URL
        let mut url = Url::parse(&self.connection_url).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Failed to parse connection URL: {}",
                e
            ))
        })?;

        // Replace the path (database name)
        url.set_path(&format!("/{}", database));

        Ok(url.to_string())
    }
}

/// Parses MySQL connection string to extract configuration parameters
///
/// # Arguments
/// * `connection_string` - MySQL connection URL
///
/// # Returns
/// Validated connection configuration
pub fn parse_mysql_connection_config(connection_string: &str) -> Result<ConnectionConfig> {
    // Validate connection string first
    validate_mysql_connection_string(connection_string)?;

    let url = Url::parse(connection_string).map_err(|e| {
        crate::error::DbSurveyorError::configuration(format!(
            "Invalid MySQL connection string format: {}",
            e
        ))
    })?;

    // Start with defaults
    let mut config = ConnectionConfig::new(url.host_str().unwrap_or("localhost").to_string());

    // Set port with MySQL default
    if let Some(port) = url.port() {
        if port == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "Invalid port number: must be greater than 0",
            ));
        }
        config = config.with_port(port);
    } else {
        config = config.with_port(3306); // MySQL default port
    }

    // Extract database name
    if !url.path().is_empty() && url.path() != "/" {
        let database = url.path().trim_start_matches('/');
        if !database.is_empty() {
            if database.len() > 64 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Database name too long: maximum 64 characters",
                ));
            }
            config = config.with_database(database.to_string());
        }
    }

    // Extract username
    let username = url.username();
    if !username.is_empty() {
        if username.len() > 32 {
            return Err(crate::error::DbSurveyorError::configuration(
                "Username too long: maximum 32 characters for MySQL",
            ));
        }
        config = config.with_username(username.to_string());
    }

    // Parse query parameters for additional configuration
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "connect_timeout" => {
                if let Ok(timeout_secs) = value.parse::<u64>()
                    && timeout_secs > 0
                    && timeout_secs <= 300
                {
                    config.connect_timeout = Duration::from_secs(timeout_secs);
                }
            }
            "pool_max_conns" => {
                if let Ok(max_conns) = value.parse::<u32>()
                    && max_conns > 0
                    && max_conns <= 100
                {
                    config.max_connections = max_conns;
                }
            }
            _ => {} // Ignore other parameters
        }
    }

    // Final validation
    config.validate()?;

    Ok(config)
}

/// Validates MySQL connection string format and security requirements
///
/// # Arguments
/// * `connection_string` - MySQL connection URL to validate
///
/// # Errors
/// Returns error if connection string is invalid or unsafe
pub fn validate_mysql_connection_string(connection_string: &str) -> Result<()> {
    let url = Url::parse(connection_string).map_err(|e| {
        crate::error::DbSurveyorError::configuration(format!(
            "Invalid MySQL connection string format: {}",
            e
        ))
    })?;

    // Validate scheme
    if url.scheme() != "mysql" {
        return Err(crate::error::DbSurveyorError::configuration(
            "Connection string must use mysql:// scheme",
        ));
    }

    // Validate host is present
    if url.host_str().is_none() {
        return Err(crate::error::DbSurveyorError::configuration(
            "Connection string must specify a host",
        ));
    }

    Ok(())
}

/// Creates a MySQL connection pool with proper configuration
///
/// # Security Features
/// - Enforces connection limits
/// - Sets appropriate timeouts
/// - Validates connections before use
async fn create_mysql_connection_pool(
    connection_string: &str,
    config: &ConnectionConfig,
) -> Result<MySqlPool> {
    use sqlx::Executor;

    // Validate connection string format
    validate_mysql_connection_string(connection_string)?;

    // Clone config values needed for the after_connect closure
    let query_timeout_secs = config.query_timeout.as_secs();
    let read_only = config.read_only;

    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(config.max_connections.min(100))
        .min_connections(config.min_idle_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .test_before_acquire(true)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                // Set query timeout
                conn.execute(
                    format!("SET max_execution_time = {}", query_timeout_secs * 1000).as_str(),
                )
                .await?;

                // Set session to read-only if configured
                if read_only {
                    conn.execute("SET SESSION TRANSACTION READ ONLY").await?;
                }

                // Set timezone to UTC for consistent timestamps
                conn.execute("SET time_zone = '+00:00'").await?;

                Ok(())
            })
        })
        .connect_lazy(connection_string)
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                format!(
                    "Failed to create MySQL connection pool to {}",
                    crate::adapters::redact_database_url(connection_string)
                ),
                e,
            )
        })?;

    Ok(pool)
}
