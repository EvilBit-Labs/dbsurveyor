//! PostgreSQL database adapter with connection pooling and comprehensive schema collection.
//!
//! # Module Structure
//! - `connection`: Connection pool management and validation
//! - `type_mapping`: PostgreSQL to unified data type conversion
//! - `schema_collection`: Table, column, constraint, and index collection
//! - `sampling`: Data sampling utilities and ordering strategy detection
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/DESCRIBE only)
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion
//! - Connection pooling with configurable limits

mod connection;
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
pub use sampling::{detect_ordering_strategy, generate_order_by_clause, sample_table};
pub use type_mapping::{map_postgresql_type, map_referential_action};

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
