//! PostgreSQL database adapter with connection pooling and comprehensive schema collection.
//!
//! # Module Structure
//! - `connection`: Connection pool management and validation
//! - `type_mapping`: PostgreSQL to unified data type conversion
//! - `schema_collection`: Table, column, constraint, and index collection
//! - `sampling`: Data sampling utilities and ordering strategy detection
//! - `enumeration`: Database enumeration for multi-database collection
//! - `multi_database`: Multi-database collection orchestration
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/DESCRIBE only)
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion
//! - Connection pooling with configurable limits

mod connection;
mod enumeration;
mod multi_database;
mod sampling;
mod schema_collection;
mod type_mapping;

#[cfg(test)]
mod tests;

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::{Result, models::*};
use async_trait::async_trait;
use sqlx::PgPool;

// Re-export public items from submodules
pub use connection::PoolStats;
pub use enumeration::{
    EnumeratedDatabase, ListDatabasesOptions, SYSTEM_DATABASES, list_accessible_databases,
    list_databases,
};
pub use multi_database::{
    DatabaseCollectionResult, DatabaseFailure, MultiDatabaseConfig, MultiDatabaseMetadata,
    MultiDatabaseResult, collect_all_databases,
};
pub use sampling::{detect_ordering_strategy, generate_order_by_clause, sample_table};
pub use type_mapping::{map_postgresql_type, map_referential_action};

/// PostgreSQL database adapter with connection pooling and comprehensive schema collection
pub struct PostgresAdapter {
    /// Connection pool for database operations
    pub pool: PgPool,
    /// Connection configuration (pool settings, timeouts, etc.)
    pub config: ConnectionConfig,
    /// Original connection URL (stored for creating connections to other databases)
    /// This is kept private to prevent credential exposure
    connection_url: String,
}

impl std::fmt::Debug for PostgresAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAdapter")
            .field("config", &self.config)
            .field("pool_size", &self.pool.size())
            .field("pool_idle", &self.pool.num_idle())
            // Note: connection_url is intentionally omitted to prevent credential exposure
            .finish_non_exhaustive()
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
        schema_collection::collect_schema(self).await
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

// Additional PostgresAdapter methods for data sampling
impl PostgresAdapter {
    /// Detect the best ordering strategy for sampling a table.
    ///
    /// This method analyzes the table structure to determine the most reliable
    /// way to order rows when sampling data. The detection priority is:
    ///
    /// 1. Primary key columns (most reliable)
    /// 2. Timestamp columns (created_at, updated_at, etc.)
    /// 3. Auto-increment/serial columns
    /// 4. Unordered fallback (uses RANDOM() for sampling)
    ///
    /// # Arguments
    ///
    /// * `schema` - Schema name (e.g., "public")
    /// * `table` - Table name
    ///
    /// # Returns
    ///
    /// Returns the detected `OrderingStrategy` or an error if detection fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    /// let strategy = adapter.detect_ordering_strategy("public", "users").await?;
    /// ```
    pub async fn detect_ordering_strategy(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<OrderingStrategy> {
        sampling::detect_ordering_strategy(&self.pool, schema, table).await
    }

    /// Generate an ORDER BY clause for the given ordering strategy.
    ///
    /// This is a convenience method that wraps `generate_order_by_clause`.
    ///
    /// # Arguments
    ///
    /// * `strategy` - The ordering strategy to generate SQL for
    /// * `descending` - If true, order descending (most recent first)
    ///
    /// # Returns
    ///
    /// Returns a SQL ORDER BY clause string.
    pub fn generate_order_by(&self, strategy: &OrderingStrategy, descending: bool) -> String {
        sampling::generate_order_by_clause(strategy, descending)
    }

    /// Sample data from a table with rate limiting and intelligent ordering.
    ///
    /// This method samples rows from a table using automatically detected ordering
    /// to provide meaningful samples (e.g., most recent records). Rate limiting
    /// prevents overwhelming the database with sampling queries.
    ///
    /// # Arguments
    ///
    /// * `schema` - Schema name (e.g., "public")
    /// * `table` - Table name
    /// * `config` - Sampling configuration including sample size and throttle settings
    ///
    /// # Returns
    ///
    /// Returns a `TableSample` containing:
    /// - Sampled rows as JSON objects
    /// - Metadata about the table and sampling operation
    /// - Warnings (e.g., if no reliable ordering was found)
    ///
    /// # Ordering Strategy
    ///
    /// The function automatically detects the best ordering:
    /// 1. **Primary key** - Most reliable, uses DESC for most recent
    /// 2. **Timestamp columns** - Good for "most recent" semantics
    /// 3. **Auto-increment** - Reliable insertion order
    /// 4. **Random** - Fallback when no ordering exists (with warning)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    /// let config = SamplingConfig::new()
    ///     .with_sample_size(10)
    ///     .with_throttle_ms(100);
    ///
    /// let sample = adapter.sample_table("public", "users", &config).await?;
    /// for row in &sample.rows {
    ///     println!("{}", row);
    /// }
    /// ```
    pub async fn sample_table(
        &self,
        schema: &str,
        table: &str,
        config: &super::SamplingConfig,
    ) -> Result<crate::models::TableSample> {
        sampling::sample_table(&self.pool, schema, table, config).await
    }
}

// Database enumeration methods for multi-database collection
impl PostgresAdapter {
    /// List all databases on the PostgreSQL server, excluding system databases.
    ///
    /// This method queries `pg_database` to enumerate all databases that the
    /// current user has access to. System databases (template0, template1) are
    /// excluded by default.
    ///
    /// # Returns
    ///
    /// A vector of `EnumeratedDatabase` structs containing metadata about each
    /// database including name, owner, encoding, size, and accessibility.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    /// let databases = adapter.list_databases().await?;
    /// for db in databases {
    ///     println!("Database: {} (accessible: {})", db.name, db.is_accessible);
    /// }
    /// ```
    pub async fn list_databases(&self) -> Result<Vec<EnumeratedDatabase>> {
        enumeration::list_databases(&self.pool, false).await
    }

    /// List all databases on the PostgreSQL server with configurable options.
    ///
    /// This method allows fine-grained control over which databases are included
    /// in the listing.
    ///
    /// # Arguments
    ///
    /// * `include_system` - If true, includes system databases (template0, template1)
    ///
    /// # Returns
    ///
    /// A vector of `EnumeratedDatabase` structs.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    ///
    /// // Include system databases
    /// let all_dbs = adapter.list_databases_with_options(true).await?;
    ///
    /// // Exclude system databases (same as list_databases())
    /// let user_dbs = adapter.list_databases_with_options(false).await?;
    /// ```
    pub async fn list_databases_with_options(
        &self,
        include_system: bool,
    ) -> Result<Vec<EnumeratedDatabase>> {
        enumeration::list_databases(&self.pool, include_system).await
    }

    /// List only accessible databases on the PostgreSQL server.
    ///
    /// This is a convenience method that filters out databases that the
    /// current user cannot connect to.
    ///
    /// # Arguments
    ///
    /// * `include_system` - If true, includes accessible system databases
    ///
    /// # Returns
    ///
    /// A vector of `EnumeratedDatabase` structs for accessible databases only.
    pub async fn list_accessible_databases(
        &self,
        include_system: bool,
    ) -> Result<Vec<EnumeratedDatabase>> {
        enumeration::list_accessible_databases(&self.pool, include_system).await
    }

    /// Create a new adapter connected to a specific database on the same server.
    ///
    /// Uses the same connection configuration (host, port, credentials, pool settings)
    /// but targets a different database. This is useful for multi-database collection
    /// after enumerating databases with `list_databases()`.
    ///
    /// # Arguments
    ///
    /// * `database` - Name of the database to connect to
    ///
    /// # Returns
    ///
    /// A new `PostgresAdapter` instance connected to the specified database.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database name is invalid (empty, too long, or contains dangerous characters)
    /// - Connection to the new database fails
    ///
    /// # Security
    ///
    /// Database names are validated to prevent SQL injection:
    /// - Must be 1-63 characters (PostgreSQL identifier limit)
    /// - Cannot contain semicolons, single quotes, or double quotes
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    /// let databases = adapter.list_accessible_databases(false).await?;
    ///
    /// for db in databases {
    ///     let db_adapter = adapter.connect_to_database(&db.name).await?;
    ///     let schema = db_adapter.collect_schema().await?;
    ///     println!("Collected schema for: {}", db.name);
    /// }
    /// ```
    pub async fn connect_to_database(&self, database: &str) -> Result<PostgresAdapter> {
        // Build new connection string with different database
        let new_url = self.connection_url_for_database(database)?;

        tracing::debug!("Connecting to database: {}", database);

        // Create new adapter with the same config but different database
        Self::with_config(&new_url, self.config.clone()).await
    }

    /// Generate connection URL for a different database on the same server.
    ///
    /// This method takes the current connection URL and replaces the database
    /// component with the specified database name.
    ///
    /// # Arguments
    ///
    /// * `database` - Name of the database to generate URL for
    ///
    /// # Returns
    ///
    /// A new connection URL string targeting the specified database.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database name is empty or longer than 63 characters
    /// - Database name contains dangerous characters (`;`, `'`, `"`)
    ///
    /// # Security
    ///
    /// This method validates database names to prevent URL injection attacks.
    /// The validation is intentionally strict to ensure safety.
    pub fn connection_url_for_database(&self, database: &str) -> Result<String> {
        // Validate database name length
        if database.is_empty() || database.len() > 63 {
            return Err(crate::error::DbSurveyorError::configuration(format!(
                "Invalid database name length: must be 1-63 characters, got {}",
                database.len()
            )));
        }

        // Check for dangerous characters that could enable injection attacks
        if database.contains(';') || database.contains('\'') || database.contains('"') {
            return Err(crate::error::DbSurveyorError::configuration(
                "Database name contains invalid characters (semicolon, single quote, or double quote not allowed)",
            ));
        }

        // Parse the original URL
        let mut url = url::Url::parse(&self.connection_url).map_err(|e| {
            crate::error::DbSurveyorError::configuration(format!(
                "Failed to parse connection URL: {}",
                e
            ))
        })?;

        // Replace the path (database name) in the URL
        // The path in a postgres URL is "/database_name"
        url.set_path(&format!("/{}", database));

        Ok(url.to_string())
    }

    /// Collect schemas from all accessible databases on the server.
    ///
    /// This method orchestrates multi-database collection:
    /// 1. Enumerates all databases on the server
    /// 2. Filters databases based on configuration
    /// 3. Collects schemas concurrently with rate limiting
    /// 4. Aggregates results and failures
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration controlling collection behavior
    ///
    /// # Returns
    ///
    /// A `MultiDatabaseResult` containing:
    /// - Server information
    /// - Successfully collected schemas
    /// - Failed collection details
    /// - Collection metadata
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = PostgresAdapter::new(&database_url).await?;
    /// let config = MultiDatabaseConfig::new()
    ///     .with_max_concurrency(8)
    ///     .with_exclude_patterns(vec!["test_*".to_string()]);
    ///
    /// let result = adapter.collect_all_databases(&config).await?;
    /// println!("Collected {} databases", result.databases.len());
    /// for db in &result.databases {
    ///     println!("  - {} ({} tables)",
    ///         db.database_name,
    ///         db.schema.tables.len());
    /// }
    /// ```
    pub async fn collect_all_databases(
        &self,
        config: &MultiDatabaseConfig,
    ) -> Result<MultiDatabaseResult> {
        multi_database::collect_all_databases(self, config).await
    }
}
