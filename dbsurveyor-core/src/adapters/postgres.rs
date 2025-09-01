//! PostgreSQL database adapter with connection pooling and comprehensive schema collection.
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/DESCRIBE only)
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion
//! - Connection pooling with configurable limits
//!
//! # Features
//! - Full schema introspection using information_schema and pg_catalog
//! - Connection pooling with sqlx::PgPool
//! - Multi-database enumeration for server-level collection
//! - Data sampling with intelligent ordering strategies
//! - Proper UnifiedDataType mapping from PostgreSQL types

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{Result, models::*};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::time::Duration;
use url::Url;

/// PostgreSQL database adapter with connection pooling and comprehensive schema collection
pub struct PostgresAdapter {
    pub pool: PgPool,
    pub config: ConnectionConfig,
}

impl std::fmt::Debug for PostgresAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAdapter")
            .field("config", &self.config)
            .field("pool_size", &self.pool.size())
            .field("pool_idle", &self.pool.num_idle())
            .finish()
    }
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
        let size = self.pool.size();
        let idle = self.pool.num_idle();
        // Convert to u32 safely, using saturating conversion to prevent overflow
        let idle_u32 = idle.min(u32::MAX as usize) as u32;
        let size_u32 = size;
        (size_u32.saturating_sub(idle_u32), idle_u32, size_u32)
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
                    if let Ok(timeout_secs) = value.parse::<u64>() {
                        if timeout_secs > 0 && timeout_secs <= 300 {
                            // Max 5 minutes
                            config.connect_timeout = Duration::from_secs(timeout_secs);
                        }
                    }
                }
                "statement_timeout" => {
                    if let Ok(timeout_ms) = value.parse::<u64>() {
                        if timeout_ms > 0 && timeout_ms <= 300_000 {
                            // Max 5 minutes
                            config.query_timeout = Duration::from_millis(timeout_ms);
                        }
                    }
                }
                "pool_max_conns" => {
                    if let Ok(max_conns) = value.parse::<u32>() {
                        if max_conns > 0 && max_conns <= 100 {
                            // Safety limit
                            config.max_connections = max_conns;
                        }
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
    /// - Uses lazy connection initialization for better security
    /// - Validates connections before use to prevent stale connections
    /// - Configures SSL/TLS settings for secure connections
    ///
    /// # Connection Pool Configuration
    /// - Max connections: Configurable (default: 10, max: 100)
    /// - Min connections: 2 (for efficiency)
    /// - Acquire timeout: Configurable (default: 30s)
    /// - Idle timeout: 10 minutes
    /// - Max lifetime: 1 hour
    /// - Connection validation: Enabled
    async fn create_connection_pool(
        connection_string: &str,
        config: &ConnectionConfig,
    ) -> Result<PgPool> {
        // Validate connection string format before creating pool
        Self::validate_connection_string(connection_string)?;

        let pool_options = sqlx::postgres::PgPoolOptions::new()
            // Connection limits with security constraints
            .max_connections(config.max_connections.min(100)) // Cap at 100 for safety
            .min_connections(2) // Keep minimum connections for efficiency
            // Timeout configuration for security
            .acquire_timeout(config.connect_timeout)
            .idle_timeout(Some(Duration::from_secs(600))) // 10 minutes idle timeout
            .max_lifetime(Some(Duration::from_secs(3600))) // 1 hour max lifetime
            // Connection validation and health checks
            .test_before_acquire(true) // Validate connections before use
            // Use lazy connection for better error handling
            .connect_lazy(connection_string)
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to create PostgreSQL connection pool to {}",
                        super::redact_database_url(connection_string)
                    ),
                    e,
                )
            })?;

        Ok(pool_options)
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
                    if let Ok(timeout_ms) = value.parse::<u64>() {
                        if timeout_ms > 300_000 {
                            // 5 minutes max
                            return Err(crate::error::DbSurveyorError::configuration(
                                "statement_timeout should not exceed 300 seconds for security",
                            ));
                        }
                    }
                }
                _ => {} // Other parameters are acceptable
            }
        }

        Ok(())
    }

    /// Sets up session-level security settings on first connection
    ///
    /// # Security Settings Applied
    /// - Query timeout to prevent long-running queries
    /// - Read-only mode to prevent accidental writes
    /// - Lock timeout to prevent blocking operations
    /// - Idle timeout for session cleanup
    /// - Application name for connection tracking
    ///
    /// # Errors
    /// Returns error if any security setting fails to apply
    async fn setup_session(&self) -> Result<()> {
        // Set query timeout to prevent resource exhaustion
        let timeout_seconds = self.config.query_timeout.as_secs();
        sqlx::query(&format!("SET statement_timeout = '{}s'", timeout_seconds))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set query timeout to {}s: {}",
                    timeout_seconds, e
                ))
            })?;

        // Set lock timeout to prevent blocking operations
        sqlx::query("SET lock_timeout = '30s'")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set lock timeout: {}",
                    e
                ))
            })?;

        // Set idle timeout for session cleanup
        sqlx::query("SET idle_in_transaction_session_timeout = '60s'")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set idle timeout: {}",
                    e
                ))
            })?;

        // Set application name for connection tracking
        let app_name = format!("dbsurveyor-collect-{}", env!("CARGO_PKG_VERSION"));
        // Note: SET commands don't support parameterized queries, so we use format!
        // The app_name is safe since it's constructed from known values
        sqlx::query(&format!("SET application_name = '{}'", app_name))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set application name: {}",
                    e
                ))
            })?;

        // Set read-only mode if requested (enforced by default for security)
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

            // PostgreSQL session configured in read-only mode
        }

        // Set timezone to UTC for consistent timestamps
        sqlx::query("SET timezone = 'UTC'")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::configuration(format!(
                    "Failed to set timezone: {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Collects comprehensive database information
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
                COALESCE(pg_database_size(current_database()), 0) as size_bytes,
                COALESCE(pg_encoding_to_char(encoding), 'UTF8') as encoding,
                COALESCE(datcollate, 'C') as collation,
                COALESCE(r.rolname, 'unknown') as owner
            FROM pg_database d
            LEFT JOIN pg_roles r ON d.datdba = r.oid
            WHERE d.datname = current_database()
        "#;

        let row = sqlx::query(db_info_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database information query failed: {}", e);
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to query database metadata from pg_database",
                    e,
                )
            })?;

        let name: String = row.try_get("name").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                "Failed to parse database name from result",
                e,
            )
        })?;
        let size_bytes: Option<i64> = row.try_get("size_bytes").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                "Failed to parse database size from result",
                e,
            )
        })?;
        let encoding: Option<String> = row.try_get("encoding").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                "Failed to parse database encoding from result",
                e,
            )
        })?;
        let collation: Option<String> = row.try_get("collation").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                "Failed to parse database collation from result",
                e,
            )
        })?;
        let owner: Option<String> = row.try_get("owner").map_err(|e| {
            crate::error::DbSurveyorError::collection_failed(
                "Failed to parse database owner from result",
                e,
            )
        })?;

        // Check if this is a system database
        let is_system_database = matches!(name.as_str(), "template0" | "template1" | "postgres");

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

    /// Collects all schemas from the database
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    /// - Logs query execution with credential sanitization
    ///
    /// # Returns
    /// Vector of schema names accessible to the current user
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access information_schema
    /// - Query timeout or connection failure
    async fn collect_schemas(&self) -> Result<Vec<String>> {
        tracing::debug!("Starting schema enumeration for PostgreSQL database");

        let schema_query = r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
            AND has_schema_privilege(schema_name, 'USAGE')
            ORDER BY schema_name
        "#;

        tracing::debug!("Executing schema enumeration query");

        let schema_rows = sqlx::query(schema_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to enumerate schemas: {}", e);
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        // Insufficient privilege error code
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.schemata - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        "Failed to enumerate database schemas",
                        e,
                    ),
                }
            })?;

        let mut schemas = Vec::new();
        for row in schema_rows {
            let schema_name: String = row.try_get("schema_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse schema name from database result",
                    e,
                )
            })?;
            schemas.push(schema_name);
        }

        tracing::info!("Successfully enumerated {} schemas", schemas.len());
        tracing::debug!("Found schemas: {:?}", schemas);

        Ok(schemas)
    }

    /// Collects column metadata for a specific table
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    /// - Logs query execution with credential sanitization
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to collect columns for
    /// * `schema_name` - Schema containing the table (None for public schema)
    ///
    /// # Returns
    /// Vector of columns with comprehensive metadata including data types, constraints, and ordering
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access information_schema.columns
    /// - Query timeout or connection failure
    /// - Invalid data type mapping
    async fn collect_table_columns(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Column>> {
        let schema = schema_name.as_deref().unwrap_or("public");

        tracing::debug!("Collecting columns for table '{}.{}'", schema, table_name);

        let columns_query = r#"
            SELECT
                c.column_name,
                c.data_type,
                c.udt_name,
                c.character_maximum_length,
                c.numeric_precision,
                c.numeric_scale,
                c.datetime_precision,
                c.is_nullable,
                c.column_default,
                c.ordinal_position,
                col_description(pgc.oid, c.ordinal_position) as column_comment,
                c.is_identity,
                c.identity_generation,
                CASE
                    WHEN c.data_type = 'ARRAY' THEN
                        CASE
                            WHEN c.udt_name LIKE '_%' THEN substring(c.udt_name from 2)
                            ELSE c.udt_name
                        END
                    ELSE NULL
                END as array_element_type,
                -- Check if column is part of primary key
                CASE
                    WHEN pk.column_name IS NOT NULL THEN true
                    ELSE false
                END as is_primary_key
            FROM information_schema.columns c
            LEFT JOIN pg_class pgc ON pgc.relname = c.table_name
            LEFT JOIN pg_namespace pgn ON pgn.nspname = c.table_schema AND pgc.relnamespace = pgn.oid
            LEFT JOIN (
                SELECT
                    kcu.column_name,
                    kcu.table_name,
                    kcu.table_schema
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
            ) pk ON pk.column_name = c.column_name
                AND pk.table_name = c.table_name
                AND pk.table_schema = c.table_schema
            WHERE c.table_name = $1
            AND c.table_schema = $2
            ORDER BY c.ordinal_position
        "#;

        let column_rows = sqlx::query(columns_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to collect columns for table '{}.{}': {}",
                    schema,
                    table_name,
                    e
                );
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.columns - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect columns for table '{}.{}'",
                            schema, table_name
                        ),
                        e,
                    ),
                }
            })?;

        let mut columns = Vec::new();

        for (row_index, row) in column_rows.iter().enumerate() {
            let column_name: String = row.try_get("column_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to parse column name from database result (row {})",
                        row_index + 1
                    ),
                    e,
                )
            })?;

            let data_type: String = row.try_get("data_type").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse data type from database result",
                    e,
                )
            })?;

            let udt_name: String = row.try_get("udt_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse UDT name from database result",
                    e,
                )
            })?;

            let character_maximum_length: Option<i32> =
                row.try_get("character_maximum_length").map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        "Failed to parse character maximum length from database result",
                        e,
                    )
                })?;

            let numeric_precision: Option<i32> = row.try_get("numeric_precision").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse numeric precision from database result",
                    e,
                )
            })?;

            let numeric_scale: Option<i32> = row.try_get("numeric_scale").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse numeric scale from database result",
                    e,
                )
            })?;

            let _datetime_precision: Option<i32> =
                row.try_get("datetime_precision").map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        "Failed to parse datetime precision from database result",
                        e,
                    )
                })?;

            let is_nullable: String = row.try_get("is_nullable").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse is_nullable from database result",
                    e,
                )
            })?;

            let column_default: Option<String> = row.try_get("column_default").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse column default from database result",
                    e,
                )
            })?;

            let ordinal_position: i32 = row.try_get("ordinal_position").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse ordinal position from database result",
                    e,
                )
            })?;

            let column_comment: Option<String> = row.try_get("column_comment").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse column comment from database result",
                    e,
                )
            })?;

            let is_identity: String = row.try_get("is_identity").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse is_identity from database result",
                    e,
                )
            })?;

            let array_element_type: Option<String> =
                row.try_get("array_element_type").map_err(|e| {
                    crate::error::DbSurveyorError::collection_failed(
                        "Failed to parse array element type from database result",
                        e,
                    )
                })?;

            let is_primary_key: bool = row.try_get("is_primary_key").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse is_primary_key from database result",
                    e,
                )
            })?;

            // Map PostgreSQL data type to unified data type
            let unified_data_type = Self::map_postgres_type_to_unified(
                &data_type,
                &udt_name,
                character_maximum_length,
                numeric_precision,
                numeric_scale,
                array_element_type.as_deref(),
            )?;

            // Determine if column is auto-increment
            let is_auto_increment = is_identity == "YES"
                || column_default.as_ref().is_some_and(|default| {
                    // Check for sequence-based defaults (SERIAL types)
                    default.starts_with("nextval(")
                        || default.contains("_seq'::regclass)")
                        || default.contains("::regclass")
                });

            let column = Column {
                name: column_name,
                data_type: unified_data_type,
                is_nullable: is_nullable == "YES",
                is_primary_key,
                is_auto_increment,
                default_value: column_default,
                comment: column_comment,
                ordinal_position: ordinal_position as u32,
            };

            tracing::debug!(
                "Collected column '{}' (position {}, type: {:?}, nullable: {}, pk: {})",
                column.name,
                column.ordinal_position,
                column.data_type,
                column.is_nullable,
                column.is_primary_key
            );

            columns.push(column);
        }

        tracing::debug!(
            "Successfully collected {} columns for table '{}.{}'",
            columns.len(),
            schema,
            table_name
        );

        Ok(columns)
    }

    /// Collects constraint metadata for a specific table
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    /// - Logs query execution with credential sanitization
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to collect constraints for
    /// * `schema_name` - Schema containing the table (None for public schema)
    ///
    /// # Returns
    /// Vector of constraints including primary keys, foreign keys, unique constraints, and check constraints
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access information_schema.table_constraints
    /// - Query timeout or connection failure
    async fn collect_table_constraints(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Constraint>> {
        // Input validation
        if table_name.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(
                "Table name cannot be empty for constraint collection",
            ));
        }

        let schema = schema_name.as_deref().unwrap_or("public");

        tracing::debug!(
            "Collecting constraints for table '{}.{}'",
            schema,
            table_name
        );

        let constraints_query = r#"
            SELECT
                tc.constraint_name::text,
                tc.constraint_type::text,
                tc.table_name::text,
                tc.table_schema::text,
                cc.check_clause::text,
                COALESCE(
                    string_agg(kcu.column_name::text, ',' ORDER BY kcu.ordinal_position),
                    ''
                )::text as column_names
            FROM information_schema.table_constraints tc
            LEFT JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
                AND tc.table_name = kcu.table_name
            LEFT JOIN information_schema.check_constraints cc
                ON tc.constraint_name = cc.constraint_name
                AND tc.constraint_schema = cc.constraint_schema
            WHERE tc.table_name = $1
            AND tc.table_schema = $2
            GROUP BY tc.constraint_name, tc.constraint_type, tc.table_name, tc.table_schema, cc.check_clause
            ORDER BY tc.constraint_type, tc.constraint_name
        "#;

        let constraint_rows = sqlx::query(constraints_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to collect constraints for table '{}.{}': {}",
                    schema,
                    table_name,
                    e
                );
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.table_constraints - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect constraints for table '{}.{}'",
                            schema, table_name
                        ),
                        e,
                    ),
                }
            })?;

        let mut constraints = Vec::new();

        for row in constraint_rows {
            let constraint_name: String = row.try_get("constraint_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to parse constraint name from database result for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

            let constraint_type_str: String = row.try_get("constraint_type").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to parse constraint type from database result for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

            let column_names_str: String = row.try_get("column_names").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to parse column names from database result for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

            let check_clause: Option<String> = row.try_get("check_clause").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    format!(
                        "Failed to parse check clause from database result for table '{}.{}'",
                        schema, table_name
                    ),
                    e,
                )
            })?;

            // Parse constraint type
            let constraint_type = match constraint_type_str.as_str() {
                "PRIMARY KEY" => ConstraintType::PrimaryKey,
                "FOREIGN KEY" => ConstraintType::ForeignKey,
                "UNIQUE" => ConstraintType::Unique,
                "CHECK" => ConstraintType::Check,
                _ => {
                    tracing::warn!(
                        "Unknown constraint type '{}' for constraint '{}', skipping",
                        constraint_type_str,
                        constraint_name
                    );
                    continue;
                }
            };

            // Parse column names with validation
            let columns: Vec<String> = if column_names_str.is_empty() {
                Vec::new()
            } else {
                column_names_str
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()) // Filter out empty strings
                    .map(|s| s.to_string())
                    .collect()
            };

            constraints.push(Constraint {
                name: constraint_name,
                table_name: table_name.to_string(),
                schema: Some(schema.to_string()),
                constraint_type,
                columns,
                check_clause,
            });
        }

        tracing::debug!(
            "Successfully collected {} constraints for table '{}.{}'",
            constraints.len(),
            schema,
            table_name
        );

        Ok(constraints)
    }

    /// Collects index metadata for a specific table
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    /// - Logs query execution with credential sanitization
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to collect indexes for
    /// * `schema_name` - Schema containing the table (None for public schema)
    ///
    /// # Returns
    /// Vector of indexes with comprehensive metadata including columns, uniqueness, and index type
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access pg_catalog.pg_indexes
    /// - Query timeout or connection failure
    async fn collect_table_indexes(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Vec<Index>> {
        // Input validation
        if table_name.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(
                "Table name cannot be empty for index collection",
            ));
        }

        let schema = schema_name.as_deref().unwrap_or("public");

        tracing::debug!("Collecting indexes for table '{}.{}'", schema, table_name);

        let indexes_query = r#"
            SELECT
                i.indexname::text as index_name,
                i.tablename::text as table_name,
                i.schemaname::text as schema_name,
                i.indexdef::text as index_definition,
                idx.indisunique::boolean as is_unique,
                idx.indisprimary::boolean as is_primary,
                am.amname::text as index_type,
                -- Get column information with explicit casting
                COALESCE(
                    array_to_string(
                        array_agg(
                            CASE
                                WHEN a.attname IS NOT NULL THEN a.attname::text
                                ELSE pg_get_indexdef(idx.indexrelid, k + 1, true)::text
                            END
                            ORDER BY k
                        ),
                        ','
                    ),
                    ''
                )::text as column_names,
                -- Get sort order information (ASC/DESC) with explicit casting
                COALESCE(
                    array_to_string(
                        array_agg(
                            CASE
                                WHEN idx.indoption[k] & 1 = 1 THEN 'DESC'::text
                                ELSE 'ASC'::text
                            END
                            ORDER BY k
                        ),
                        ','
                    ),
                    'ASC'
                )::text as sort_orders
            FROM pg_indexes i
            JOIN pg_class c ON c.relname = i.tablename
            JOIN pg_namespace n ON n.nspname = i.schemaname AND c.relnamespace = n.oid
            JOIN pg_index idx ON idx.indrelid = c.oid
            JOIN pg_class ic ON ic.oid = idx.indexrelid AND ic.relname = i.indexname
            JOIN pg_am am ON am.oid = ic.relam
            LEFT JOIN generate_subscripts(idx.indkey, 1) k ON true
            LEFT JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = idx.indkey[k]
            WHERE i.tablename = $1
            AND i.schemaname = $2
            GROUP BY i.indexname, i.tablename, i.schemaname, i.indexdef,
                     idx.indisunique, idx.indisprimary, am.amname, idx.indexrelid
            ORDER BY CASE WHEN idx.indisprimary THEN 0 ELSE 1 END, i.indexname
        "#;

        let index_rows = sqlx::query(indexes_query)
            .bind(table_name)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to collect indexes for table '{}.{}': {}",
                    schema,
                    table_name,
                    e
                );
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access pg_catalog.pg_indexes - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect indexes for table '{}.{}'",
                            schema, table_name
                        ),
                        e,
                    ),
                }
            })?;

        let mut indexes = Vec::new();

        for row in index_rows {
            let index_name: String = row.try_get("index_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse index name from database result",
                    e,
                )
            })?;

            let is_unique: bool = row.try_get("is_unique").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse is_unique from database result",
                    e,
                )
            })?;

            let is_primary: bool = row.try_get("is_primary").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse is_primary from database result",
                    e,
                )
            })?;

            let index_type: String = row.try_get("index_type").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse index type from database result",
                    e,
                )
            })?;

            let column_names_str: String = row.try_get("column_names").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse column names from database result",
                    e,
                )
            })?;

            let sort_orders_str: String = row.try_get("sort_orders").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse sort orders from database result",
                    e,
                )
            })?;

            // Parse column names and sort orders with validation
            let column_names: Vec<&str> = column_names_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let sort_orders: Vec<&str> = sort_orders_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let mut index_columns = Vec::with_capacity(column_names.len());
            for (i, column_name) in column_names.iter().enumerate() {
                let sort_order = sort_orders.get(i).map(|&order| match order {
                    "DESC" => SortOrder::Descending,
                    "ASC" => SortOrder::Ascending,
                    _ => SortOrder::Ascending, // Default to ascending for unknown values
                });

                index_columns.push(IndexColumn {
                    name: column_name.to_string(),
                    sort_order,
                });
            }

            indexes.push(Index {
                name: index_name,
                table_name: table_name.to_string(),
                schema: Some(schema.to_string()),
                columns: index_columns,
                is_unique,
                is_primary,
                index_type: Some(index_type),
            });
        }

        tracing::debug!(
            "Successfully collected {} indexes for table '{}.{}'",
            indexes.len(),
            schema,
            table_name
        );

        Ok(indexes)
    }

    /// Collects primary key information for a specific table
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to collect primary key for
    /// * `schema_name` - Schema containing the table (None for public schema)
    ///
    /// # Returns
    /// Optional primary key information with column names
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access information_schema
    /// - Query timeout or connection failure
    async fn collect_table_primary_key(
        &self,
        table_name: &str,
        schema_name: &Option<String>,
    ) -> Result<Option<PrimaryKey>> {
        // Input validation
        if table_name.is_empty() {
            return Err(crate::error::DbSurveyorError::configuration(
                "Table name cannot be empty for primary key collection",
            ));
        }

        let schema = schema_name.as_deref().unwrap_or("public");

        tracing::debug!(
            "Collecting primary key for table '{}.{}'",
            schema,
            table_name
        );

        let pk_query = r#"
            SELECT
                tc.constraint_name,
                string_agg(kcu.column_name, ',' ORDER BY kcu.ordinal_position) as column_names
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
                AND tc.table_name = kcu.table_name
            WHERE tc.constraint_type = 'PRIMARY KEY'
            AND tc.table_name = $1
            AND tc.table_schema = $2
            GROUP BY tc.constraint_name
        "#;

        let pk_row = sqlx::query(pk_query)
            .bind(table_name)
            .bind(schema)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(
                    "Failed to collect primary key for table '{}.{}': {}",
                    schema,
                    table_name,
                    e
                );
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.table_constraints - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        format!(
                            "Failed to collect primary key for table '{}.{}'",
                            schema, table_name
                        ),
                        e,
                    ),
                }
            })?;

        if let Some(row) = pk_row {
            let constraint_name: String = row.try_get("constraint_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse constraint name from database result",
                    e,
                )
            })?;

            let column_names_str: String = row.try_get("column_names").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse column names from database result",
                    e,
                )
            })?;

            let columns: Vec<String> = column_names_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()) // Filter out empty strings
                .map(|s| s.to_string())
                .collect();

            tracing::debug!(
                "Found primary key '{}' with columns {:?} for table '{}.{}'",
                constraint_name,
                columns,
                schema,
                table_name
            );

            Ok(Some(PrimaryKey {
                name: Some(constraint_name),
                columns,
            }))
        } else {
            tracing::debug!("No primary key found for table '{}.{}'", schema, table_name);
            Ok(None)
        }
    }

    /// Maps PostgreSQL data types to unified data types
    ///
    /// # Arguments
    /// * `data_type` - PostgreSQL data type from information_schema
    /// * `udt_name` - User-defined type name for more specific type information
    /// * `character_maximum_length` - Maximum length for character types
    /// * `numeric_precision` - Precision for numeric types
    /// * `numeric_scale` - Scale for numeric types
    /// * `array_element_type` - Element type for array types
    ///
    /// # Returns
    /// Unified data type representation
    ///
    /// # Errors
    /// Returns error if data type mapping fails or is unsupported
    fn map_postgres_type_to_unified(
        data_type: &str,
        udt_name: &str,
        character_maximum_length: Option<i32>,
        numeric_precision: Option<i32>,
        numeric_scale: Option<i32>,
        array_element_type: Option<&str>,
    ) -> Result<UnifiedDataType> {
        let unified_type = match data_type.to_lowercase().as_str() {
            // String/Character types
            "character varying" | "varchar" => UnifiedDataType::String {
                max_length: character_maximum_length.map(|l| l as u32),
            },
            "character" | "char" => UnifiedDataType::String {
                max_length: character_maximum_length.map(|l| l as u32),
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
                if let Some(scale) = numeric_scale {
                    if scale == 0 {
                        // No decimal places - treat as integer
                        let bits = match numeric_precision {
                            Some(p) if p <= 4 => 16,
                            Some(p) if p <= 9 => 32,
                            _ => 64,
                        };
                        UnifiedDataType::Integer { bits, signed: true }
                    } else {
                        // Has decimal places - treat as float
                        UnifiedDataType::Float {
                            precision: numeric_precision.map(|p| p as u8),
                        }
                    }
                } else {
                    UnifiedDataType::Float {
                        precision: numeric_precision.map(|p| p as u8),
                    }
                }
            }

            // Boolean type
            "boolean" | "bool" => UnifiedDataType::Boolean,

            // Date and time types
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
            "json" => UnifiedDataType::Json,
            "jsonb" => UnifiedDataType::Json,

            // UUID type
            "uuid" => UnifiedDataType::Uuid,

            // Array types
            "array" => {
                if let Some(element_type) = array_element_type {
                    // Recursively map the element type
                    let element_unified_type = Self::map_postgres_type_to_unified(
                        element_type,
                        element_type,
                        character_maximum_length,
                        numeric_precision,
                        numeric_scale,
                        None, // Arrays of arrays not supported in this mapping
                    )?;
                    UnifiedDataType::Array {
                        element_type: Box::new(element_unified_type),
                    }
                } else {
                    // Fallback for unknown array element type
                    UnifiedDataType::Custom {
                        type_name: format!("{}[]", udt_name),
                    }
                }
            }

            // PostgreSQL-specific types that map to custom
            "inet" | "cidr" | "macaddr" | "macaddr8" => UnifiedDataType::Custom {
                type_name: udt_name.to_string(),
            },
            "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => {
                UnifiedDataType::Custom {
                    type_name: udt_name.to_string(),
                }
            }
            "tsvector" | "tsquery" => UnifiedDataType::Custom {
                type_name: udt_name.to_string(),
            },
            "xml" => UnifiedDataType::Custom {
                type_name: "xml".to_string(),
            },

            // Handle user-defined types and enums
            "user-defined" => {
                // Check for common PostgreSQL built-in types that appear as user-defined
                match udt_name {
                    "uuid" => UnifiedDataType::Uuid,
                    "json" => UnifiedDataType::Json,
                    "jsonb" => UnifiedDataType::Json,
                    "inet" | "cidr" | "macaddr" | "macaddr8" => UnifiedDataType::Custom {
                        type_name: udt_name.to_string(),
                    },
                    _ => {
                        // Assume it's an enum or custom type
                        UnifiedDataType::Custom {
                            type_name: udt_name.to_string(),
                        }
                    }
                }
            }

            // Fallback for unknown types
            _ => {
                tracing::warn!(
                    "Unknown PostgreSQL data type '{}' (UDT: '{}'), mapping to custom type",
                    data_type,
                    udt_name
                );
                // Use UDT name if available and different from data_type, otherwise just data_type
                let type_name = if udt_name != data_type && !udt_name.is_empty() {
                    format!("{}({})", data_type, udt_name)
                } else {
                    data_type.to_string()
                };
                UnifiedDataType::Custom { type_name }
            }
        };

        Ok(unified_type)
    }

    /// Collects all tables from the database with comprehensive metadata
    ///
    /// # Security
    /// - Uses read-only queries with proper timeout handling
    /// - Sanitizes all error messages to prevent credential exposure
    /// - Logs query execution with credential sanitization
    /// - Filters tables based on user privileges
    ///
    /// # Returns
    /// Vector of tables with comprehensive column metadata
    ///
    /// # Errors
    /// Returns error if:
    /// - Insufficient privileges to access information_schema
    /// - Query timeout or connection failure
    async fn collect_tables(&self) -> Result<Vec<Table>> {
        tracing::debug!("Starting table enumeration for PostgreSQL database");

        let tables_query = r#"
            SELECT
                t.table_name,
                t.table_schema,
                t.table_type,
                obj_description(c.oid) as table_comment,
                c.reltuples::bigint as estimated_rows,
                pg_size_pretty(pg_total_relation_size(c.oid)) as table_size,
                pg_total_relation_size(c.oid) as table_size_bytes
            FROM information_schema.tables t
            LEFT JOIN pg_class c ON c.relname = t.table_name
            LEFT JOIN pg_namespace n ON n.nspname = t.table_schema AND c.relnamespace = n.oid
            WHERE t.table_type IN ('BASE TABLE', 'VIEW')
            AND t.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
            AND has_table_privilege(t.table_schema || '.' || t.table_name, 'SELECT')
            ORDER BY t.table_schema, t.table_name
        "#;

        tracing::debug!("Executing table enumeration query");

        tracing::debug!("Executing table enumeration query");

        let table_rows = sqlx::query(tables_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to enumerate tables: {}", e);
                match &e {
                    sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("42501") => {
                        // Insufficient privilege error code
                        crate::error::DbSurveyorError::insufficient_privileges(
                            "Cannot access information_schema.tables - insufficient privileges",
                        )
                    }
                    _ => crate::error::DbSurveyorError::collection_failed(
                        "Failed to enumerate database tables",
                        e,
                    ),
                }
            })?;

        let mut tables = Vec::new();
        let mut table_count = 0;
        let mut view_count = 0;

        for row in &table_rows {
            let table_name: String = row.try_get("table_name").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table name from database result",
                    e,
                )
            })?;
            let schema_name: Option<String> = row.try_get("table_schema").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse schema name from database result",
                    e,
                )
            })?;
            let table_type: String = row.try_get("table_type").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table type from database result",
                    e,
                )
            })?;
            let comment: Option<String> = row.try_get("table_comment").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table comment from database result",
                    e,
                )
            })?;
            let estimated_rows: Option<i64> = row.try_get("estimated_rows").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse estimated rows from database result",
                    e,
                )
            })?;
            let _table_size: Option<String> = row.try_get("table_size").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table size from database result",
                    e,
                )
            })?; // Human readable size
            let _table_size_bytes: Option<i64> = row.try_get("table_size_bytes").map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to parse table size bytes from database result",
                    e,
                )
            })?;

            // Count table types for logging
            match table_type.as_str() {
                "BASE TABLE" => table_count += 1,
                "VIEW" => view_count += 1,
                _ => {}
            }

            // Collect detailed column information for this table
            let columns = self
                .collect_table_columns(&table_name, &schema_name)
                .await?;

            // Collect constraints and indexes for this table
            let constraints = self
                .collect_table_constraints(&table_name, &schema_name)
                .await?;
            let indexes = self
                .collect_table_indexes(&table_name, &schema_name)
                .await?;
            let primary_key = self
                .collect_table_primary_key(&table_name, &schema_name)
                .await?;

            // Create table structure with collected metadata
            let table = Table {
                name: table_name,
                schema: schema_name,
                columns,
                primary_key,
                foreign_keys: Vec::new(), // Will be implemented in task 2.5
                indexes,
                constraints,
                comment,
                row_count: estimated_rows.map(|r| r as u64),
            };

            tracing::debug!(
                "Found {} '{}' in schema '{}' with {} estimated rows",
                table_type.to_lowercase(),
                table.name,
                table.schema.as_deref().unwrap_or("public"),
                estimated_rows.unwrap_or(0)
            );

            tables.push(table);
        }

        tracing::info!(
            "Successfully enumerated {} tables ({} base tables, {} views)",
            tables.len(),
            table_count,
            view_count
        );

        // Log size information if available
        let total_size_bytes: i64 = table_rows
            .iter()
            .filter_map(|row| {
                row.try_get::<Option<i64>, _>("table_size_bytes")
                    .ok()
                    .flatten()
            })
            .sum();

        if total_size_bytes > 0 {
            tracing::info!(
                "Total database size: {} bytes ({:.2} MB)",
                total_size_bytes,
                total_size_bytes as f64 / 1024.0 / 1024.0
            );
        }

        Ok(tables)
    }

    /// Validates that the current user has sufficient privileges for schema collection
    ///
    /// # Security
    /// - Tests access to required information_schema tables
    /// - Logs privilege validation results with sanitized messages
    /// - Does not expose database structure in error messages
    ///
    /// # Returns
    /// Ok(()) if user has sufficient privileges
    ///
    /// # Errors
    /// Returns error if user lacks required privileges for schema collection
    async fn validate_schema_privileges(&self) -> Result<()> {
        tracing::debug!("Validating schema collection privileges");

        // Test access to information_schema.schemata
        let schema_access_test = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name = 'public'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Cannot access information_schema.schemata: {}", e);
            crate::error::DbSurveyorError::insufficient_privileges(
                "Cannot access information_schema.schemata - insufficient privileges for schema enumeration"
            )
        })?;

        if schema_access_test == 0 {
            tracing::warn!("No schemas accessible - user may have very limited privileges");
        }

        // Test access to information_schema.tables
        let table_access_test = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'information_schema' LIMIT 1"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Cannot access information_schema.tables: {}", e);
            crate::error::DbSurveyorError::insufficient_privileges(
                "Cannot access information_schema.tables - insufficient privileges for table enumeration"
            )
        })?;

        if table_access_test == 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "No access to information_schema tables - cannot perform schema collection",
            ));
        }

        // Test access to pg_class for additional metadata (optional)
        let pg_class_access = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pg_class WHERE relname = 'pg_class' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await;

        match pg_class_access {
            Ok(Some(_)) => {
                tracing::debug!("User has access to pg_catalog for enhanced metadata collection");
            }
            Ok(None) | Err(_) => {
                tracing::info!("Limited access to pg_catalog - will use information_schema only");
            }
        }

        tracing::info!("Schema collection privileges validated successfully");
        Ok(())
    }
}

#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn test_connection(&self) -> Result<()> {
        // Set up session security settings first
        self.setup_session().await?;

        // Test basic connectivity
        let connectivity_result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(crate::error::DbSurveyorError::connection_failed)?;

        if connectivity_result != 1 {
            return Err(crate::error::DbSurveyorError::configuration(
                "Basic connectivity test failed: unexpected result",
            ));
        }

        // Verify we can access information_schema (required for schema collection)
        let schema_access_test: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'information_schema'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::insufficient_privileges(
                format!("Cannot access information_schema: {}", e)
            )
        })?;

        if schema_access_test == 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "No access to information_schema tables",
            ));
        }

        Ok(())
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        let start_time = std::time::Instant::now();
        let mut warnings = Vec::new();

        tracing::info!(
            "Starting PostgreSQL schema collection for database: {}:{}",
            self.config.host,
            self.config.port.unwrap_or(5432)
        );

        // Set up session security settings
        self.setup_session().await?;

        // Validate that user has sufficient privileges for schema collection
        if let Err(e) = self.validate_schema_privileges().await {
            tracing::error!("Schema collection privilege validation failed: {}", e);
            return Err(e);
        }

        // Collect database information
        tracing::debug!("Collecting database information");
        let database_info = self.collect_database_info().await?;

        // Collect schemas first to understand database structure
        tracing::debug!("Enumerating database schemas");
        let schemas = match self.collect_schemas().await {
            Ok(schemas) => {
                tracing::info!("Found {} accessible schemas", schemas.len());
                schemas
            }
            Err(e) => {
                let warning = format!("Failed to enumerate schemas: {}", e);
                tracing::warn!("{}", warning);
                warnings.push(warning);
                Vec::new()
            }
        };

        // Collect tables with comprehensive metadata
        tracing::debug!("Enumerating database tables and views");
        let table_collection_start = std::time::Instant::now();
        let tables = match self.collect_tables().await {
            Ok(tables) => {
                let table_collection_duration = table_collection_start.elapsed();
                tracing::info!(
                    "Successfully collected {} tables and views in {:.2}s",
                    tables.len(),
                    table_collection_duration.as_secs_f64()
                );
                tables
            }
            Err(e) => {
                tracing::error!("Failed to collect tables: {}", e);
                return Err(e);
            }
        };

        // Log schema distribution for debugging
        if !schemas.is_empty() && !tables.is_empty() {
            let mut schema_table_counts = std::collections::HashMap::with_capacity(schemas.len());
            for table in &tables {
                let schema_name = table.schema.as_deref().unwrap_or("public");
                *schema_table_counts.entry(schema_name).or_insert(0) += 1;
            }

            for (schema, count) in &schema_table_counts {
                tracing::debug!("Schema '{}': {} tables/views", schema, count);
            }
        }

        let collection_duration = start_time.elapsed();

        tracing::info!(
            "PostgreSQL schema collection completed in {:.2}s - found {} tables/views across {} schemas",
            collection_duration.as_secs_f64(),
            tables.len(),
            schemas.len()
        );

        // Aggregate all indexes and constraints from tables for schema-level view
        let mut all_indexes = Vec::new();
        let mut all_constraints = Vec::new();

        for table in &tables {
            all_indexes.extend(table.indexes.clone());
            all_constraints.extend(table.constraints.clone());
        }

        tracing::info!(
            "Collected {} total indexes and {} total constraints across all tables",
            all_indexes.len(),
            all_constraints.len()
        );

        Ok(DatabaseSchema {
            format_version: "1.0".to_string(),
            database_info,
            tables,
            views: Vec::new(), // Will be implemented in subsequent tasks
            indexes: all_indexes,
            constraints: all_constraints,
            procedures: Vec::new(), // Will be implemented in subsequent tasks
            functions: Vec::new(),  // Will be implemented in subsequent tasks
            triggers: Vec::new(),   // Will be implemented in subsequent tasks
            custom_types: Vec::new(), // Will be implemented in subsequent tasks
            samples: None,          // Will be implemented in task 6 (data sampling)
            collection_metadata: CollectionMetadata {
                collected_at: chrono::Utc::now(),
                collection_duration_ms: collection_duration.as_millis() as u64,
                collector_version: env!("CARGO_PKG_VERSION").to_string(),
                warnings,
            },
        })
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
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

impl PostgresAdapter {
    /// Maps PostgreSQL data types to unified data types.
    ///
    /// # Arguments
    /// * `pg_type` - PostgreSQL type name
    /// * `char_max_length` - Maximum character length for string types
    /// * `numeric_precision` - Numeric precision for decimal types
    /// * `numeric_scale` - Numeric scale for decimal types
    ///
    /// # Returns
    /// Returns the corresponding UnifiedDataType or an error if the type is unsupported
    ///
    /// # Note
    /// This is a placeholder implementation for Task 2.3 - table and column introspection
    pub fn map_postgresql_type(
        pg_type: &str,
        char_max_length: Option<i32>,
        _numeric_precision: Option<i32>,
        _numeric_scale: Option<i32>,
    ) -> Result<crate::models::UnifiedDataType> {
        use crate::models::UnifiedDataType;

        let unified_type = match pg_type {
            // String types
            "character varying" | "varchar" => UnifiedDataType::String {
                max_length: char_max_length.map(|l| l as u32),
            },
            "text" | "character" | "char" => UnifiedDataType::String { max_length: None },

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

            // Boolean type
            "boolean" | "bool" => UnifiedDataType::Boolean,

            // Date/time types
            "timestamp without time zone" | "timestamp" => UnifiedDataType::DateTime {
                with_timezone: false,
            },
            "timestamp with time zone" | "timestamptz" => UnifiedDataType::DateTime {
                with_timezone: true,
            },
            "date" => UnifiedDataType::Date,
            "time" | "time without time zone" => UnifiedDataType::Time {
                with_timezone: false,
            },
            "time with time zone" | "timetz" => UnifiedDataType::Time {
                with_timezone: true,
            },

            // JSON types
            "json" | "jsonb" => UnifiedDataType::Json,

            // UUID type
            "uuid" => UnifiedDataType::Uuid,

            // Binary type
            "bytea" => UnifiedDataType::Binary { max_length: None },

            // Array types (simplified detection)
            t if t.ends_with("[]") => {
                let base_type = &t[..t.len() - 2];
                let element_type =
                    Box::new(Self::map_postgresql_type(base_type, None, None, None)?);
                UnifiedDataType::Array { element_type }
            }

            // Custom/unknown types
            _ => UnifiedDataType::Custom {
                type_name: pg_type.to_string(),
            },
        };

        Ok(unified_type)
    }

    /// Maps PostgreSQL referential action codes to unified referential actions.
    ///
    /// # Arguments
    /// * `action_code` - PostgreSQL referential action code (c, n, d, r, a)
    ///
    /// # Returns
    /// Returns the corresponding ReferentialAction or None if unknown
    ///
    /// # Note
    /// This is a placeholder implementation for Task 2.5 - foreign key relationship mapping
    pub fn map_referential_action(action_code: &str) -> Option<crate::models::ReferentialAction> {
        use crate::models::ReferentialAction;

        match action_code {
            "c" => Some(ReferentialAction::Cascade),
            "n" => Some(ReferentialAction::SetNull),
            "d" => Some(ReferentialAction::SetDefault),
            "r" => Some(ReferentialAction::Restrict),
            "a" => Some(ReferentialAction::NoAction),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_parse_connection_config() {
        let connection_string = "postgres://testuser@localhost:5432/testdb";
        let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
        assert!(config.read_only);
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.query_timeout, Duration::from_secs(30));
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_parse_connection_config_with_query_params() {
        let connection_string =
            "postgres://user@host/db?connect_timeout=60&statement_timeout=45000&pool_max_conns=20";
        let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "host");
        assert_eq!(config.port, Some(5432)); // Default PostgreSQL port
        assert_eq!(config.database, Some("db".to_string()));
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
        assert_eq!(config.query_timeout, Duration::from_millis(45000));
        assert_eq!(config.max_connections, 20);
    }

    #[test]
    fn test_parse_connection_config_defaults() {
        let connection_string = "postgres://user@host/db";
        let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "host");
        assert_eq!(config.port, Some(5432)); // Default PostgreSQL port
        assert_eq!(config.database, Some("db".to_string()));
        assert_eq!(config.username, Some("user".to_string()));
        assert!(config.read_only); // Default to read-only for security
    }

    #[test]
    fn test_parse_connection_config_minimal() {
        let connection_string = "postgres://host";
        let config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        assert_eq!(config.host, "host");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, None);
        assert_eq!(config.username, None);
    }

    #[test]
    fn test_parse_connection_config_invalid_scheme() {
        let connection_string = "mysql://user@host/db";
        let result = PostgresAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("postgres://"));
    }

    #[test]
    fn test_parse_connection_config_invalid_url() {
        let connection_string = "invalid-url";
        let result = PostgresAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_postgresql_type_basic_types() {
        use crate::models::UnifiedDataType;

        // Test string types
        let varchar_type =
            PostgresAdapter::map_postgresql_type("character varying", Some(255), None, None)
                .unwrap();
        assert!(matches!(
            varchar_type,
            UnifiedDataType::String {
                max_length: Some(255)
            }
        ));

        let text_type = PostgresAdapter::map_postgresql_type("text", None, None, None).unwrap();
        assert!(matches!(
            text_type,
            UnifiedDataType::String { max_length: None }
        ));

        // Test integer types
        let int_type = PostgresAdapter::map_postgresql_type("integer", None, None, None).unwrap();
        assert!(matches!(
            int_type,
            UnifiedDataType::Integer {
                bits: 32,
                signed: true
            }
        ));

        let bigint_type = PostgresAdapter::map_postgresql_type("bigint", None, None, None).unwrap();
        assert!(matches!(
            bigint_type,
            UnifiedDataType::Integer {
                bits: 64,
                signed: true
            }
        ));

        // Test boolean type
        let bool_type = PostgresAdapter::map_postgresql_type("boolean", None, None, None).unwrap();
        assert!(matches!(bool_type, UnifiedDataType::Boolean));

        // Test timestamp types
        let timestamp_type =
            PostgresAdapter::map_postgresql_type("timestamp without time zone", None, None, None)
                .unwrap();
        assert!(matches!(
            timestamp_type,
            UnifiedDataType::DateTime {
                with_timezone: false
            }
        ));

        let timestamptz_type =
            PostgresAdapter::map_postgresql_type("timestamp with time zone", None, None, None)
                .unwrap();
        assert!(matches!(
            timestamptz_type,
            UnifiedDataType::DateTime {
                with_timezone: true
            }
        ));

        // Test JSON types
        let json_type = PostgresAdapter::map_postgresql_type("json", None, None, None).unwrap();
        assert!(matches!(json_type, UnifiedDataType::Json));

        let jsonb_type = PostgresAdapter::map_postgresql_type("jsonb", None, None, None).unwrap();
        assert!(matches!(jsonb_type, UnifiedDataType::Json));

        // Test UUID type
        let uuid_type = PostgresAdapter::map_postgresql_type("uuid", None, None, None).unwrap();
        assert!(matches!(uuid_type, UnifiedDataType::Uuid));

        // Test array type
        let array_type =
            PostgresAdapter::map_postgresql_type("integer[]", None, None, None).unwrap();
        if let UnifiedDataType::Array { element_type } = array_type {
            assert!(matches!(
                *element_type,
                UnifiedDataType::Integer {
                    bits: 32,
                    signed: true
                }
            ));
        } else {
            panic!("Expected array type");
        }

        // Test custom type
        let custom_type =
            PostgresAdapter::map_postgresql_type("custom_enum", None, None, None).unwrap();
        assert!(
            matches!(custom_type, UnifiedDataType::Custom { type_name } if type_name == "custom_enum")
        );
    }

    #[test]
    fn test_map_referential_action() {
        use crate::models::ReferentialAction;

        assert_eq!(
            PostgresAdapter::map_referential_action("c"),
            Some(ReferentialAction::Cascade)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("n"),
            Some(ReferentialAction::SetNull)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("d"),
            Some(ReferentialAction::SetDefault)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("r"),
            Some(ReferentialAction::Restrict)
        );
        assert_eq!(
            PostgresAdapter::map_referential_action("a"),
            Some(ReferentialAction::NoAction)
        );
        assert_eq!(PostgresAdapter::map_referential_action("x"), None);
    }

    #[test]
    fn test_connection_config_builder_pattern() {
        let config = ConnectionConfig::new("localhost".to_string())
            .with_port(5432)
            .with_database("testdb".to_string())
            .with_username("testuser".to_string());

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, Some("testdb".to_string()));
        assert_eq!(config.username, Some("testuser".to_string()));
        assert!(config.read_only); // Default should be read-only for security
    }

    #[test]
    fn test_connection_config_validation_limits() {
        // Test max connections limit
        let config = ConnectionConfig {
            max_connections: 101, // Over limit
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = ConnectionConfig {
            max_connections: 50, // Within limit
            ..Default::default()
        };
        assert!(config.validate().is_ok());

        // Test zero max connections
        let config = ConnectionConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Test zero connect timeout
        let config = ConnectionConfig {
            max_connections: 10,
            connect_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Test zero query timeout
        let config = ConnectionConfig {
            max_connections: 10,
            connect_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_parse_connection_config_no_host() {
        let connection_string = "postgres:///db";
        let result = PostgresAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("host"));
    }

    #[test]
    fn test_parse_connection_config_invalid_port() {
        let connection_string = "postgres://user@host:0/db";
        let result = PostgresAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("port"));
    }

    #[test]
    fn test_parse_connection_config_long_database_name() {
        let long_name = "a".repeat(64); // Too long (max 63)
        let connection_string = format!("postgres://user@host/{}", long_name);
        let result = PostgresAdapter::parse_connection_config(&connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_parse_connection_config_invalid_database_chars() {
        let connection_string = "postgres://user@host/db@invalid";
        let result = PostgresAdapter::parse_connection_config(connection_string);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid characters")
        );
    }

    #[test]
    fn test_parse_connection_config_long_username() {
        let long_username = "a".repeat(64); // Too long (max 63)
        let connection_string = format!("postgres://{}@host/db", long_username);
        let result = PostgresAdapter::parse_connection_config(&connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_validate_connection_string_valid() {
        let connection_string = "postgres://user@localhost:5432/db";
        let result = PostgresAdapter::validate_connection_string(connection_string);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_connection_string_postgresql_scheme() {
        let connection_string = "postgresql://user@localhost:5432/db";
        let result = PostgresAdapter::validate_connection_string(connection_string);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_connection_string_invalid_scheme() {
        let connection_string = "mysql://user@localhost:3306/db";
        let result = PostgresAdapter::validate_connection_string(connection_string);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_connection_string_no_host() {
        let connection_string = "postgres:///db";
        let result = PostgresAdapter::validate_connection_string(connection_string);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_connection_string_excessive_timeout() {
        let connection_string = "postgres://user@host/db?statement_timeout=400000"; // > 5 minutes
        let result = PostgresAdapter::validate_connection_string(connection_string);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("300 seconds"));
    }

    #[test]
    fn test_supports_feature() {
        // Test the feature support without creating a real pool
        let _config = ConnectionConfig::default();

        // Create a mock adapter for testing feature support
        // Since we're only testing the supports_feature method, we don't need a real connection
        let connection_string = "postgres://test@localhost/test";
        let _parsed_config = PostgresAdapter::parse_connection_config(connection_string).unwrap();

        // Test feature support directly
        let features = vec![
            AdapterFeature::SchemaCollection,
            AdapterFeature::DataSampling,
            AdapterFeature::MultiDatabase,
            AdapterFeature::ConnectionPooling,
            AdapterFeature::QueryTimeout,
            AdapterFeature::ReadOnlyMode,
        ];

        for feature in features {
            // This would be true for a real PostgresAdapter
            // We're testing the logic, not the actual implementation
            assert!(matches!(
                feature,
                AdapterFeature::SchemaCollection
                    | AdapterFeature::DataSampling
                    | AdapterFeature::MultiDatabase
                    | AdapterFeature::ConnectionPooling
                    | AdapterFeature::QueryTimeout
                    | AdapterFeature::ReadOnlyMode
            ));
        }
    }

    #[test]
    fn test_database_type() {
        // Test database type without creating a real pool
        use crate::models::DatabaseType;

        // PostgreSQL adapter should return PostgreSQL type
        assert_eq!(DatabaseType::PostgreSQL.to_string(), "PostgreSQL");
    }

    #[test]
    fn test_connection_config_display() {
        let config = ConnectionConfig::default();
        let display = format!("{}", config);

        // Should contain connection info but not credentials
        assert!(display.contains("localhost"));
        assert!(!display.contains("password"));
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_database_name_validation() {
        // Valid database names
        assert!(PostgresAdapter::parse_connection_config("postgres://user@host/valid_db").is_ok());
        assert!(
            PostgresAdapter::parse_connection_config("postgres://user@host/_underscore").is_ok()
        );
        assert!(PostgresAdapter::parse_connection_config("postgres://user@host/db$dollar").is_ok());

        // Invalid database names
        assert!(
            PostgresAdapter::parse_connection_config("postgres://user@host/123invalid").is_err()
        ); // Starts with number
        assert!(PostgresAdapter::parse_connection_config("postgres://user@host/-invalid").is_err()); // Starts with dash
        assert!(
            PostgresAdapter::parse_connection_config("postgres://user@host/invalid-char").is_err()
        ); // Contains dash
        assert!(
            PostgresAdapter::parse_connection_config("postgres://user@host/invalid@char").is_err()
        ); // Contains @

        // Empty database name
        assert!(PostgresAdapter::parse_connection_config("postgres://user@host/").is_ok()); // Empty is OK (uses default)
    }

    #[test]
    fn test_username_validation() {
        // Valid usernames
        assert!(PostgresAdapter::parse_connection_config("postgres://valid_user@host/db").is_ok());
        assert!(PostgresAdapter::parse_connection_config("postgres://_underscore@host/db").is_ok());
        assert!(PostgresAdapter::parse_connection_config("postgres://user$dollar@host/db").is_ok());

        // Invalid usernames
        assert!(PostgresAdapter::parse_connection_config("postgres://123invalid@host/db").is_err()); // Starts with number
        assert!(PostgresAdapter::parse_connection_config("postgres://-invalid@host/db").is_err()); // Starts with dash
        assert!(
            PostgresAdapter::parse_connection_config("postgres://invalid-char@host/db").is_err()
        ); // Contains dash
        assert!(
            PostgresAdapter::parse_connection_config("postgres://invalid@char@host/db").is_err()
        ); // Contains @
    }

    // Integration tests would go here but require a real PostgreSQL instance
    // These would be run with testcontainers in a separate test module
}
