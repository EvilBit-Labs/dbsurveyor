//! SQLite database adapter with schema collection and data sampling.
//!
//! # Module Structure
//! - `connection`: Connection handling (no pooling needed for SQLite)
//! - `type_mapping`: SQLite to unified data type conversion
//! - `schema_collection`: Table, column, constraint, and index collection via sqlite_master
//! - `sampling`: Data sampling utilities and ordering strategy detection
//!
//! # SQLite-Specific Features
//! - Uses `sqlite_master` for schema introspection
//! - Uses PRAGMA commands for detailed metadata
//! - Supports both file-based and in-memory databases
//! - No connection pooling (single connection is sufficient)
//!
//! # Security Guarantees
//! - All operations are read-only (SELECT/PRAGMA only)
//! - File paths are validated
//! - No network access required

mod connection;
mod sampling;
mod schema_collection;
mod type_mapping;

#[cfg(test)]
mod tests;

use super::{AdapterFeature, ConnectionConfig, DatabaseAdapter, TableRef};
use crate::Result;
use crate::models::*;
use async_trait::async_trait;
use sqlx::SqlitePool;
use zeroize::Zeroizing;

// Re-export public items from submodules
pub use sampling::{detect_ordering_strategy, generate_order_by_clause, sample_table};
pub use type_mapping::map_sqlite_type;

/// Escapes a SQLite identifier by double-quoting (for use in DML statements).
///
/// Embedded double quotes are doubled per the SQL standard, so a column named
/// `weird"col` becomes `"weird""col"`.
pub(crate) fn escape_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Escapes a value for use in PRAGMA arguments (single-quoting).
///
/// Embedded single quotes are doubled, so a table named `it's` becomes
/// `'it''s'`.
pub(crate) fn escape_pragma_arg(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

/// SQLite database adapter with schema collection and data sampling.
///
/// SQLite uses file-based databases, so this adapter works differently from
/// pooled database adapters like PostgreSQL or MySQL. A single connection
/// is typically sufficient for schema collection.
pub struct SqliteAdapter {
    /// Connection pool (typically single connection for SQLite)
    pub pool: SqlitePool,
    /// Connection configuration
    pub config: ConnectionConfig,
    /// Original connection string (kept for reference).
    /// Wrapped in `Zeroizing` so the connection string is scrubbed from
    /// memory when the adapter is dropped (CWE-316).
    pub(crate) connection_string: Zeroizing<String>,
}

impl std::fmt::Debug for SqliteAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteAdapter")
            .field("config", &self.config)
            .field("is_in_memory", &self.is_in_memory())
            // Note: connection_string is intentionally omitted
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
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

        // Verify we can access sqlite_master (required for schema collection)
        let schema_access_test: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view', 'index', 'trigger')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::insufficient_privileges(format!(
                "Cannot access sqlite_master: {}",
                e
            ))
        })?;

        // sqlite_master should be accessible (count can be 0 for empty DB)
        if schema_access_test < 0 {
            return Err(crate::error::DbSurveyorError::insufficient_privileges(
                "Unexpected result from sqlite_master query",
            ));
        }

        Ok(())
    }

    async fn collect_schema(&self) -> Result<DatabaseSchema> {
        schema_collection::collect_schema(self).await
    }

    async fn sample_table(
        &self,
        table_ref: TableRef<'_>,
        config: &super::SamplingConfig,
    ) -> Result<TableSample> {
        sampling::sample_table(&self.pool, table_ref.table_name, config).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    fn supports_feature(&self, feature: AdapterFeature) -> bool {
        matches!(
            feature,
            AdapterFeature::SchemaCollection
                | AdapterFeature::DataSampling
                | AdapterFeature::QueryTimeout
                | AdapterFeature::ReadOnlyMode
        )
        // Note: SQLite does NOT support:
        // - ConnectionPooling (single connection is sufficient)
        // - MultiDatabase (SQLite is single-database per file)
    }

    fn connection_config(&self) -> ConnectionConfig {
        self.config.clone()
    }
}

// Additional SqliteAdapter methods for data sampling
impl SqliteAdapter {
    /// Detect the best ordering strategy for sampling a table.
    ///
    /// This method analyzes the table structure to determine the most reliable
    /// way to order rows when sampling data. The detection priority is:
    ///
    /// 1. Primary key columns (most reliable)
    /// 2. Timestamp columns (created_at, updated_at, etc.)
    /// 3. Auto-increment columns (INTEGER PRIMARY KEY)
    /// 4. ROWID (SQLite's built-in row identifier)
    /// 5. Unordered fallback (uses RANDOM() for sampling)
    ///
    /// # Arguments
    ///
    /// * `table` - Table name
    ///
    /// # Returns
    ///
    /// Returns the detected `OrderingStrategy` or an error if detection fails.
    pub async fn detect_ordering_strategy(&self, table: &str) -> Result<OrderingStrategy> {
        sampling::detect_ordering_strategy(&self.pool, table).await
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
    /// Returns a SQL ORDER BY clause string with SQLite-style double-quote quoting.
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
    /// 4. **ROWID** - SQLite's internal row identifier
    /// 5. **Random** - Fallback when no ordering exists (with warning)
    pub async fn sample_table(
        &self,
        table: &str,
        config: &super::SamplingConfig,
    ) -> Result<crate::models::TableSample> {
        sampling::sample_table(&self.pool, table, config).await
    }
}

#[cfg(test)]
mod escape_tests {
    use super::*;

    #[test]
    fn test_escape_identifier_plain() {
        assert_eq!(escape_identifier("users"), "\"users\"");
    }

    #[test]
    fn test_escape_identifier_with_double_quote() {
        assert_eq!(escape_identifier("weird\"col"), "\"weird\"\"col\"");
    }

    #[test]
    fn test_escape_identifier_empty() {
        assert_eq!(escape_identifier(""), "\"\"");
    }

    #[test]
    fn test_escape_pragma_arg_plain() {
        assert_eq!(escape_pragma_arg("users"), "'users'");
    }

    #[test]
    fn test_escape_pragma_arg_with_single_quote() {
        assert_eq!(escape_pragma_arg("it's"), "'it''s'");
    }

    #[test]
    fn test_escape_pragma_arg_empty() {
        assert_eq!(escape_pragma_arg(""), "''");
    }
}
