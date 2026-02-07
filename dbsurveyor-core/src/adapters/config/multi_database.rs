//! Multi-database collection configuration and result types.
//!
//! These types are shared across all database adapters that support
//! multi-database collection (e.g., PostgreSQL, MySQL).

use crate::models::{DatabaseSchema, ServerInfo};
use serde::{Deserialize, Serialize};

/// Configuration for multi-database collection operations.
///
/// Controls which databases are collected and how the collection
/// process handles errors and concurrency.
#[derive(Debug, Clone)]
pub struct MultiDatabaseConfig {
    /// Maximum number of concurrent database collections.
    ///
    /// Higher values speed up collection but increase resource usage.
    /// Default: 4
    pub max_concurrency: usize,

    /// Include system databases (template0, template1) in collection.
    ///
    /// System databases are typically not useful for schema analysis
    /// and are excluded by default.
    /// Default: false
    pub include_system: bool,

    /// Database name patterns to exclude (glob patterns).
    ///
    /// Patterns use glob syntax:
    /// - `*` matches any sequence of characters
    /// - `?` matches any single character
    /// - `[abc]` matches any character in the set
    ///
    /// Example: `["test_*", "*_backup"]` excludes all test and backup databases.
    pub exclude_patterns: Vec<String>,

    /// Continue collection if a database fails.
    ///
    /// When true, failures are recorded but collection continues with
    /// remaining databases. When false, the first error stops collection.
    /// Default: true
    pub continue_on_error: bool,
}

impl Default for MultiDatabaseConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            include_system: false,
            exclude_patterns: Vec::new(),
            continue_on_error: true,
        }
    }
}

impl MultiDatabaseConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum concurrency for database collection.
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = max_concurrency.max(1); // Ensure at least 1
        self
    }

    /// Sets whether to include system databases.
    pub fn with_include_system(mut self, include_system: bool) -> Self {
        self.include_system = include_system;
        self
    }

    /// Adds patterns to exclude from collection.
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Sets whether to continue on error.
    pub fn with_continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }
}

/// Result of collecting from a single database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCollectionResult {
    /// Name of the database that was collected
    pub database_name: String,

    /// The collected schema (if successful)
    pub schema: DatabaseSchema,

    /// Time taken to collect this database (in milliseconds)
    pub collection_duration_ms: u64,
}

/// Information about a failed database collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseFailure {
    /// Name of the database that failed
    pub database_name: String,

    /// Error message describing the failure
    pub error_message: String,

    /// Whether this was a connection failure vs collection failure
    pub is_connection_error: bool,
}

/// Metadata about the multi-database collection operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiDatabaseMetadata {
    /// When the collection started
    pub started_at: chrono::DateTime<chrono::Utc>,

    /// Total collection duration (in milliseconds)
    pub total_duration_ms: u64,

    /// Number of databases discovered on the server
    pub databases_discovered: usize,

    /// Number of databases filtered out by patterns
    pub databases_filtered: usize,

    /// Number of databases successfully collected
    pub databases_collected: usize,

    /// Number of databases that failed collection
    pub databases_failed: usize,

    /// Number of databases skipped (inaccessible)
    pub databases_skipped: usize,

    /// Maximum concurrency used
    pub max_concurrency: usize,

    /// Collector version
    pub collector_version: String,

    /// Any warnings generated during collection
    pub warnings: Vec<String>,
}

/// Result of collecting schemas from all databases on a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiDatabaseResult {
    /// Server-level information
    pub server_info: ServerInfo,

    /// Successfully collected database schemas
    pub databases: Vec<DatabaseCollectionResult>,

    /// Failed database collections
    pub failures: Vec<DatabaseFailure>,

    /// Collection metadata and statistics
    pub collection_metadata: MultiDatabaseMetadata,
}
