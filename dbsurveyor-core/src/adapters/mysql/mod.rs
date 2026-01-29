//! MySQL database adapter with connection pooling and schema collection.
//!
//! # Module Structure
//! - `connection`: Connection pool management and validation
//! - `type_mapping`: MySQL to unified data type conversion
//! - `schema_collection`: Table, column, constraint, and index collection
//! - `sampling`: Data sampling utilities and ordering strategy detection
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/SHOW only)
//! - Connection strings are sanitized in error messages
//! - Query timeouts prevent resource exhaustion
//! - Connection pooling with configurable limits

pub mod connection;
pub mod sampling;
pub mod schema_collection;
pub mod type_mapping;

#[cfg(test)]
mod tests;

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter};
use crate::Result;
use crate::models::*;
use async_trait::async_trait;
use sqlx::MySqlPool;

// Re-export public items from submodules
pub use sampling::{detect_ordering_strategy, generate_order_by_clause, sample_table};
pub use type_mapping::map_mysql_type;

/// MySQL database adapter with connection pooling and schema collection
pub struct MySqlAdapter {
    /// Connection pool for database operations
    pub pool: MySqlPool,
    /// Connection configuration (pool settings, timeouts, etc.)
    pub config: ConnectionConfig,
    /// Original connection URL (stored for creating connections to other databases)
    /// This is kept private to prevent credential exposure
    connection_url: String,
}

impl std::fmt::Debug for MySqlAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlAdapter")
            .field("config", &self.config)
            .field("pool_size", &self.pool.size())
            .field("pool_idle", &self.pool.num_idle())
            // Note: connection_url is intentionally omitted to prevent credential exposure
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn test_connection(&self) -> Result<()> {
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

        // Verify we can access INFORMATION_SCHEMA (required for schema collection)
        let schema_access_test: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = 'information_schema'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::insufficient_privileges(
                format!("Cannot access INFORMATION_SCHEMA: {}", e)
            )
        })?;

        if schema_access_test == 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "No access to INFORMATION_SCHEMA tables",
            ));
        }

        Ok(())
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        schema_collection::collect_schema(self).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
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

// Additional MySqlAdapter methods for data sampling
impl MySqlAdapter {
    /// Detect the best ordering strategy for sampling a table.
    ///
    /// This method analyzes the table structure to determine the most reliable
    /// way to order rows when sampling data. The detection priority is:
    ///
    /// 1. Primary key columns (most reliable)
    /// 2. Timestamp columns (created_at, updated_at, etc.)
    /// 3. Auto-increment columns
    /// 4. Unordered fallback (uses RAND() for sampling)
    ///
    /// # Arguments
    ///
    /// * `db_name` - Database name
    /// * `table` - Table name
    ///
    /// # Returns
    ///
    /// Returns the detected `OrderingStrategy` or an error if detection fails.
    pub async fn detect_ordering_strategy(
        &self,
        db_name: &str,
        table: &str,
    ) -> Result<OrderingStrategy> {
        sampling::detect_ordering_strategy(&self.pool, db_name, table).await
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
    /// Returns a SQL ORDER BY clause string with MySQL-style backtick quoting.
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
    /// * `db_name` - Database name
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
    pub async fn sample_table(
        &self,
        db_name: &str,
        table: &str,
        config: &super::SamplingConfig,
    ) -> Result<crate::models::TableSample> {
        sampling::sample_table(&self.pool, db_name, table, config).await
    }
}
