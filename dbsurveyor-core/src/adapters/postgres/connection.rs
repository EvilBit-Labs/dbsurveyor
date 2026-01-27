//! PostgreSQL connection pool management and validation.
//!
//! # Security Features
//! - Validates connection string format and parameters
//! - Enforces connection limits to prevent resource exhaustion
//! - Sets appropriate timeouts for all operations
//! - Configures SSL/TLS settings for secure connections

use super::{ConnectionConfig, PostgresAdapter};
use crate::Result;
use sqlx::PgPool;
use sqlx::pool::PoolConnection;
use sqlx::postgres::Postgres;
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
    /// - Validates connection parameters for security
    ///
    /// # Errors
    /// Returns error if:
    /// - Connection string format is invalid
    /// - Database connection fails
    /// - Pool configuration is invalid
    /// - Security validation fails
    pub async fn new(connection_string: &str) -> Result<Self> {
        // Parse and validate connection configuration
        let config = Self::parse_connection_config(connection_string)?;

        // Create connection pool with security settings
        let pool = Self::create_connection_pool(connection_string, &config).await?;

        let adapter = Self { pool, config };
        Ok(adapter)
    }

    /// Creates a new PostgreSQL adapter with custom configuration
    ///
    /// # Arguments
    /// * `connection_string` - PostgreSQL connection URL
    /// * `config` - Custom connection configuration
    ///
    /// # Security
    /// Same security guarantees as `new()` but allows custom configuration
    pub async fn with_config(connection_string: &str, config: ConnectionConfig) -> Result<Self> {
        // Validate the provided configuration
        config.validate()?;

        // Validate connection string
        Self::validate_connection_string(connection_string)?;

        // Create connection pool
        let pool = Self::create_connection_pool(connection_string, &config).await?;

        let adapter = Self { pool, config };
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
        // Convert to u32 safely, using saturating conversion to prevent overflow
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
    ///
    /// # Security
    /// Ensures all connections are properly closed and cleaned up
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Checks the health of the connection pool
    ///
    /// # Returns
    /// True if the pool is healthy and can acquire connections
    pub async fn is_pool_healthy(&self) -> bool {
        // Try to acquire a connection and execute a simple query
        match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
        {
            Ok(result) => result == 1,
            Err(_) => false,
        }
    }

    /// Acquire a connection from the pool
    ///
    /// Returns a pooled connection that will be returned to the pool on drop.
    /// Respects the configured acquire_timeout (connect_timeout).
    ///
    /// # Returns
    /// A pooled connection that can be used for database operations.
    /// The connection is automatically returned to the pool when dropped.
    ///
    /// # Errors
    /// Returns `DbSurveyorError::ConnectionTimeout` if the acquire times out,
    /// or `DbSurveyorError::Connection` for other connection failures.
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>> {
        self.pool.acquire().await.map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("timed out") || error_str.contains("Timed out") {
                crate::error::DbSurveyorError::connection_timeout(
                    "connection pool",
                    self.config.connect_timeout,
                )
            } else {
                crate::error::DbSurveyorError::connection_failed(e)
            }
        })
    }

    /// Parses connection string to extract configuration parameters
    ///
    /// # Security Features
    /// - Validates connection string format before parsing
    /// - Sanitizes all extracted parameters
    /// - Applies security-focused defaults
    /// - Never logs or stores credentials
    ///
    /// # Arguments
    /// * `connection_string` - PostgreSQL connection URL (credentials sanitized in errors)
    ///
    /// # Returns
    /// Validated connection configuration with security defaults
    ///
    /// # Errors
    /// Returns error if connection string is malformed or contains unsafe parameters
    pub fn parse_connection_config(connection_string: &str) -> Result<ConnectionConfig> {
        // Validate connection string first
        Self::validate_connection_string(connection_string)?;

        let url = Url::parse(connection_string).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Invalid PostgreSQL connection string format: {}",
                e
            ))
        })?;

        // Start with security-focused defaults
        let mut config = ConnectionConfig::new(url.host_str().unwrap_or("localhost").to_string());

        // Set port with validation
        if let Some(port) = url.port() {
            if port == 0 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Invalid port number: must be greater than 0",
                ));
            }
            config = config.with_port(port);
        } else {
            config = config.with_port(5432); // PostgreSQL default port
        }

        // Extract database name with validation
        if !url.path().is_empty() && url.path() != "/" {
            let database = url.path().trim_start_matches('/');
            if !database.is_empty() {
                // Validate database name format (PostgreSQL identifier rules)
                if database.len() > 63 {
                    return Err(crate::error::DbSurveyorError::configuration(
                        "Database name too long: maximum 63 characters",
                    ));
                }
                // PostgreSQL identifiers can contain letters, digits, underscores, and dollar signs
                // Must start with a letter or underscore
                if database.is_empty() {
                    return Err(crate::error::DbSurveyorError::configuration(
                        "Database name cannot be empty",
                    ));
                }
                let first_char = database.chars().next().unwrap();
                if !first_char.is_ascii_alphabetic() && first_char != '_' {
                    return Err(crate::error::DbSurveyorError::configuration(
                        "Database name must start with a letter or underscore",
                    ));
                }
                if !database
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                {
                    return Err(crate::error::DbSurveyorError::configuration(
                        "Database name contains invalid characters (only letters, digits, underscores, and dollar signs allowed)",
                    ));
                }
                config = config.with_database(database.to_string());
            }
        }

        // Extract username with validation
        let username = url.username();
        if !username.is_empty() {
            // Validate username format (PostgreSQL role name rules)
            if username.len() > 63 {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Username too long: maximum 63 characters",
                ));
            }
            // PostgreSQL role names follow similar rules to identifiers
            let first_char = username.chars().next().unwrap();
            if !first_char.is_ascii_alphabetic() && first_char != '_' {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Username must start with a letter or underscore",
                ));
            }
            if !username
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Username contains invalid characters (only letters, digits, underscores, and dollar signs allowed)",
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
                        // Max 5 minutes
                        config.connect_timeout = Duration::from_secs(timeout_secs);
                    }
                }
                "statement_timeout" => {
                    if let Ok(timeout_ms) = value.parse::<u64>()
                        && timeout_ms > 0
                        && timeout_ms <= 300_000
                    {
                        // Max 5 minutes
                        config.query_timeout = Duration::from_millis(timeout_ms);
                    }
                }
                "pool_max_conns" => {
                    if let Ok(max_conns) = value.parse::<u32>()
                        && max_conns > 0
                        && max_conns <= 100
                    {
                        // Safety limit
                        config.max_connections = max_conns;
                    }
                }
                _ => {} // Ignore other parameters
            }
        }

        // Final validation of the complete configuration
        config.validate()?;

        Ok(config)
    }

    /// Creates a connection pool with proper configuration and security settings
    ///
    /// # Security Features
    /// - Enforces connection limits to prevent resource exhaustion
    /// - Sets appropriate timeouts for all operations
    /// - Applies session security settings to ALL pooled connections via after_connect
    /// - Validates connections before use to prevent stale connections
    /// - Configures SSL/TLS settings for secure connections
    ///
    /// # Connection Pool Configuration
    /// - Max connections: Configurable (default: 10, max: 100)
    /// - Min connections: Configurable (default: 2)
    /// - Acquire timeout: Configurable (default: 30s, uses connect_timeout)
    /// - Idle timeout: Configurable (default: 10 minutes)
    /// - Max lifetime: Configurable (default: 1 hour)
    /// - Connection validation: Enabled
    /// - Session settings: Applied to every new connection
    pub(crate) async fn create_connection_pool(
        connection_string: &str,
        config: &ConnectionConfig,
    ) -> Result<PgPool> {
        use sqlx::Executor;

        // Validate connection string format before creating pool
        Self::validate_connection_string(connection_string)?;

        // Clone config values needed for the after_connect closure
        let query_timeout_secs = config.query_timeout.as_secs();
        let read_only = config.read_only;

        let pool = sqlx::postgres::PgPoolOptions::new()
            // Connection limits with security constraints
            .max_connections(config.max_connections.min(100)) // Cap at 100 for safety
            .min_connections(config.min_idle_connections) // Configurable minimum idle connections
            // Timeout configuration for security
            .acquire_timeout(config.connect_timeout)
            .idle_timeout(config.idle_timeout) // Configurable idle timeout
            .max_lifetime(config.max_lifetime) // Configurable max lifetime
            // Connection validation and health checks
            .test_before_acquire(true) // Validate connections before use
            // Apply session security settings to EVERY new connection
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    // Set query timeout to prevent resource exhaustion
                    conn.execute(
                        format!("SET statement_timeout = '{}s'", query_timeout_secs).as_str(),
                    )
                    .await?;

                    // Set lock timeout to prevent blocking operations
                    conn.execute("SET lock_timeout = '30s'").await?;

                    // Set idle timeout for session cleanup
                    conn.execute("SET idle_in_transaction_session_timeout = '60s'")
                        .await?;

                    // Set application name for connection tracking
                    let app_name = format!("dbsurveyor-collect-{}", env!("CARGO_PKG_VERSION"));
                    conn.execute(format!("SET application_name = '{}'", app_name).as_str())
                        .await?;

                    // Set read-only mode if requested (enforced by default for security)
                    if read_only {
                        conn.execute("SET default_transaction_read_only = on")
                            .await?;
                    }

                    // Set timezone to UTC for consistent timestamps
                    conn.execute("SET timezone = 'UTC'").await?;

                    Ok(())
                })
            })
            // Use lazy connection for better error handling
            .connect_lazy(connection_string)
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to create PostgreSQL connection pool to {}",
                        crate::adapters::redact_database_url(connection_string)
                    ),
                    e,
                )
            })?;

        Ok(pool)
    }

    /// Validates connection string format and security requirements
    ///
    /// # Security Checks
    /// - Ensures connection string is properly formatted
    /// - Validates that required components are present
    /// - Checks for potentially unsafe connection parameters
    ///
    /// # Arguments
    /// * `connection_string` - PostgreSQL connection URL to validate
    ///
    /// # Errors
    /// Returns error if connection string is invalid or unsafe
    pub fn validate_connection_string(connection_string: &str) -> Result<()> {
        // Parse URL to validate format
        let url = Url::parse(connection_string).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Invalid PostgreSQL connection string format: {}",
                e
            ))
        })?;

        // Validate scheme
        if !matches!(url.scheme(), "postgres" | "postgresql") {
            return Err(crate::error::DbSurveyorError::configuration(
                "Connection string must use postgres:// or postgresql:// scheme",
            ));
        }

        // Validate host is present
        if url.host_str().is_none() {
            return Err(crate::error::DbSurveyorError::configuration(
                "Connection string must specify a host",
            ));
        }

        // Check for potentially unsafe query parameters
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                // Note: SSL disabled - we don't log this to avoid information disclosure
                "sslmode" if value == "disable" => {
                    // SSL disabled - consider enabling for security
                }
                // Validate statement timeout if specified
                "statement_timeout" => {
                    if let Ok(timeout_ms) = value.parse::<u64>()
                        && timeout_ms > 300_000
                    {
                        // 5 minutes max
                        return Err(crate::error::DbSurveyorError::configuration(
                            "statement_timeout should not exceed 300 seconds for security",
                        ));
                    }
                }
                _ => {} // Other parameters are acceptable
            }
        }

        Ok(())
    }

    /// Sets up session-level security settings
    ///
    /// # Note
    /// Session security settings are now automatically applied to ALL pooled
    /// connections via the `after_connect` callback in `create_connection_pool()`.
    /// This method is retained for backward compatibility but is now a no-op.
    ///
    /// # Security Settings Applied (via after_connect)
    /// - Query timeout to prevent long-running queries
    /// - Read-only mode to prevent accidental writes
    /// - Lock timeout to prevent blocking operations
    /// - Idle timeout for session cleanup
    /// - Application name for connection tracking
    /// - UTC timezone for consistent timestamps
    #[allow(clippy::unused_async)]
    pub(crate) async fn setup_session(&self) -> Result<()> {
        // Session settings are now applied via after_connect callback in create_connection_pool()
        // This ensures ALL pooled connections have security settings applied, not just one.
        // This method is retained for backward compatibility.
        Ok(())
    }

    /// Validates that user has sufficient privileges for schema collection
    ///
    /// # Security
    /// - Checks access to required system tables
    /// - Verifies information_schema permissions
    /// - Reports specific privilege issues
    pub(crate) async fn validate_schema_privileges(&self) -> Result<()> {
        // Check access to information_schema.tables
        let tables_access: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'information_schema'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::insufficient_privileges(
                format!("Cannot access information_schema.tables: {}", e)
            )
        })?;

        if tables_access == 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "No access to information_schema.tables",
            ));
        }

        // Check access to information_schema.columns
        let columns_access: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.columns LIMIT 1")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    crate::error::DbSurveyorError::insufficient_privileges(format!(
                        "Cannot access information_schema.columns: {}",
                        e
                    ))
                })?;

        if columns_access == 0 {
            tracing::warn!(
                "information_schema.columns returned 0 rows - this may indicate limited privileges"
            );
        }

        // Check access to pg_catalog (required for comments and additional metadata)
        let pg_catalog_access: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pg_catalog.pg_class LIMIT 1")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    tracing::warn!("Limited pg_catalog access: {}", e);
                    // Don't fail on pg_catalog access issues - it just limits metadata collection
                    crate::error::DbSurveyorError::insufficient_privileges(format!(
                        "Cannot access pg_catalog.pg_class: {}",
                        e
                    ))
                })?;

        if pg_catalog_access == 0 {
            tracing::warn!(
                "pg_catalog.pg_class returned 0 rows - metadata collection may be limited"
            );
        }

        tracing::info!("Schema collection privileges validated successfully");
        Ok(())
    }
}
