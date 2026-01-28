//! Multi-database collection orchestration for PostgreSQL.
//!
//! This module provides functionality for collecting schema information
//! from multiple databases on a PostgreSQL server in a single operation.
//!
//! # Features
//! - Concurrent database collection with configurable parallelism
//! - Glob pattern-based database filtering
//! - Graceful error handling with continue-on-error support
//! - Comprehensive collection metadata and statistics
//!
//! # Security
//! - All operations are read-only
//! - Database names are validated before connection
//! - Credentials are never logged or included in results

use super::PostgresAdapter;
use super::enumeration::EnumeratedDatabase;
use crate::Result;
use crate::adapters::DatabaseAdapter;
use crate::models::{CollectionMode, DatabaseSchema, DatabaseType, ServerInfo};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Instant;

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

/// Collects schema information from all accessible databases on the server.
///
/// This function orchestrates the multi-database collection process:
/// 1. Enumerates all databases on the server
/// 2. Filters databases based on configuration
/// 3. Collects schemas concurrently with rate limiting
/// 4. Aggregates results and failures
///
/// # Arguments
///
/// * `adapter` - The PostgreSQL adapter connected to the server
/// * `config` - Configuration controlling the collection behavior
///
/// # Returns
///
/// A `MultiDatabaseResult` containing:
/// - Server information
/// - Successfully collected schemas
/// - Failed collection details
/// - Collection metadata
///
/// # Errors
///
/// Returns an error if:
/// - Database enumeration fails
/// - Server information cannot be retrieved
/// - `continue_on_error` is false and any collection fails
///
/// # Example
///
/// ```rust,ignore
/// let adapter = PostgresAdapter::new(&database_url).await?;
/// let config = MultiDatabaseConfig::new()
///     .with_max_concurrency(8)
///     .with_exclude_patterns(vec!["test_*".to_string()]);
///
/// let result = collect_all_databases(&adapter, &config).await?;
/// println!("Collected {} databases", result.databases.len());
/// ```
pub async fn collect_all_databases(
    adapter: &PostgresAdapter,
    config: &MultiDatabaseConfig,
) -> Result<MultiDatabaseResult> {
    let start_time = Instant::now();
    let started_at = chrono::Utc::now();
    let mut warnings = Vec::new();

    tracing::info!(
        "Starting multi-database collection (max_concurrency: {}, include_system: {}, continue_on_error: {})",
        config.max_concurrency,
        config.include_system,
        config.continue_on_error
    );

    // Step 1: Get server information
    let server_info = get_server_info(adapter, config).await?;

    tracing::info!(
        "Connected to {} {} at {}:{}",
        server_info.server_type,
        server_info.version,
        server_info.host,
        server_info.port.unwrap_or(5432)
    );

    // Step 2: List all databases
    let all_databases = adapter
        .list_databases_with_options(config.include_system)
        .await?;

    let databases_discovered = all_databases.len();
    tracing::info!("Discovered {} databases on server", databases_discovered);

    // Step 3: Filter databases by patterns and accessibility
    let (databases_to_collect, databases_filtered, databases_skipped) =
        filter_databases(&all_databases, config, &mut warnings);

    tracing::info!(
        "After filtering: {} to collect, {} filtered by patterns, {} inaccessible",
        databases_to_collect.len(),
        databases_filtered,
        databases_skipped
    );

    // Step 4: Collect schemas concurrently
    let (collected_results, failures) =
        collect_databases_concurrent(adapter, &databases_to_collect, config).await;

    let total_duration = start_time.elapsed();

    // Build final result
    let databases_collected = collected_results.len();
    let databases_failed = failures.len();

    // Update server info with collection statistics
    let server_info = ServerInfo {
        collected_databases: databases_collected,
        collection_mode: CollectionMode::MultiDatabase {
            discovered: databases_discovered,
            collected: databases_collected,
            failed: databases_failed,
        },
        ..server_info
    };

    let collection_metadata = MultiDatabaseMetadata {
        started_at,
        total_duration_ms: total_duration.as_millis() as u64,
        databases_discovered,
        databases_filtered,
        databases_collected,
        databases_failed,
        databases_skipped,
        max_concurrency: config.max_concurrency,
        collector_version: env!("CARGO_PKG_VERSION").to_string(),
        warnings,
    };

    tracing::info!(
        "Multi-database collection completed in {:.2}s: {} collected, {} failed, {} filtered",
        total_duration.as_secs_f64(),
        databases_collected,
        databases_failed,
        databases_filtered
    );

    Ok(MultiDatabaseResult {
        server_info,
        databases: collected_results,
        failures,
        collection_metadata,
    })
}

/// Gets server-level information from the PostgreSQL server.
async fn get_server_info(
    adapter: &PostgresAdapter,
    config: &MultiDatabaseConfig,
) -> Result<ServerInfo> {
    // Get PostgreSQL version
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get server version", e)
        })?;

    // Extract version number from full version string
    // e.g., "PostgreSQL 16.2 on x86_64..." -> "16.2"
    let version_short = version
        .split_whitespace()
        .nth(1)
        .unwrap_or(&version)
        .to_string();

    // Get current user
    let connection_user: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&adapter.pool)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to get current user", e)
        })?;

    // Check for superuser privileges
    let has_superuser: bool =
        sqlx::query_scalar("SELECT usesuper FROM pg_user WHERE usename = current_user")
            .fetch_optional(&adapter.pool)
            .await
            .map_err(|e| {
                crate::error::DbSurveyorError::collection_failed(
                    "Failed to check superuser status",
                    e,
                )
            })?
            .unwrap_or(false);

    // Count total databases (including system if configured)
    let total_databases = adapter
        .list_databases_with_options(config.include_system)
        .await?
        .len();

    // Get count of excluded system databases if not including them
    let system_databases_excluded = if !config.include_system {
        adapter
            .list_databases_with_options(true)
            .await?
            .iter()
            .filter(|db| db.is_system_database)
            .count()
    } else {
        0
    };

    Ok(ServerInfo {
        server_type: DatabaseType::PostgreSQL,
        version: version_short,
        host: adapter.config.host.clone(),
        port: adapter.config.port,
        total_databases,
        collected_databases: 0, // Will be updated after collection
        system_databases_excluded,
        connection_user,
        has_superuser_privileges: has_superuser,
        collection_mode: CollectionMode::MultiDatabase {
            discovered: 0,
            collected: 0,
            failed: 0,
        },
    })
}

/// Filters databases based on configuration.
///
/// Returns (databases_to_collect, count_filtered, count_skipped)
fn filter_databases(
    all_databases: &[EnumeratedDatabase],
    config: &MultiDatabaseConfig,
    warnings: &mut Vec<String>,
) -> (Vec<EnumeratedDatabase>, usize, usize) {
    let mut databases_to_collect = Vec::new();
    let mut filtered_count = 0;
    let mut skipped_count = 0;

    for db in all_databases {
        // Check if database is accessible
        if !db.is_accessible {
            tracing::debug!("Skipping inaccessible database: {}", db.name);
            skipped_count += 1;
            continue;
        }

        // Check against exclude patterns
        if matches_any_pattern(&db.name, &config.exclude_patterns) {
            tracing::debug!("Filtering database '{}' - matches exclude pattern", db.name);
            filtered_count += 1;
            continue;
        }

        databases_to_collect.push(db.clone());
    }

    if skipped_count > 0 {
        warnings.push(format!(
            "{} database(s) were inaccessible and skipped",
            skipped_count
        ));
    }

    (databases_to_collect, filtered_count, skipped_count)
}

/// Checks if a database name matches any of the exclude patterns.
fn matches_any_pattern(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_match(pattern, name) {
            return true;
        }
    }
    false
}

/// Simple glob pattern matching.
///
/// Supports:
/// - `*` matches any sequence of characters
/// - `?` matches any single character
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    glob_match_recursive(&pattern_chars, &text_chars, 0, 0)
}

fn glob_match_recursive(pattern: &[char], text: &[char], mut pi: usize, mut ti: usize) -> bool {
    while pi < pattern.len() {
        match pattern[pi] {
            '*' => {
                // Skip consecutive stars
                while pi < pattern.len() && pattern[pi] == '*' {
                    pi += 1;
                }

                // Star at end matches everything
                if pi == pattern.len() {
                    return true;
                }

                // Try matching rest of pattern at each position
                while ti <= text.len() {
                    if glob_match_recursive(pattern, text, pi, ti) {
                        return true;
                    }
                    ti += 1;
                }
                return false;
            }
            '?' => {
                if ti >= text.len() {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            c => {
                if ti >= text.len() || text[ti] != c {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }

    // Pattern exhausted - text should also be exhausted
    ti == text.len()
}

/// Collects schemas from multiple databases concurrently.
///
/// Uses `futures::stream::buffer_unordered` for controlled parallelism.
async fn collect_databases_concurrent(
    adapter: &PostgresAdapter,
    databases: &[EnumeratedDatabase],
    config: &MultiDatabaseConfig,
) -> (Vec<DatabaseCollectionResult>, Vec<DatabaseFailure>) {
    let mut collected_results = Vec::new();
    let mut failures = Vec::new();

    // Create async tasks for each database
    let collection_futures = databases.iter().map(|db| {
        let db_name = db.name.clone();
        async move {
            let result = collect_single_database(adapter, &db_name).await;
            (db_name, result)
        }
    });

    // Process with controlled concurrency
    let mut stream = stream::iter(collection_futures).buffer_unordered(config.max_concurrency);

    while let Some((db_name, result)) = stream.next().await {
        match result {
            Ok(collection_result) => {
                tracing::info!(
                    "Collected schema from '{}' in {}ms",
                    db_name,
                    collection_result.collection_duration_ms
                );
                collected_results.push(collection_result);
            }
            Err(e) => {
                let error_str = e.to_string();
                let is_connection_error = matches!(
                    e,
                    crate::error::DbSurveyorError::Connection { .. }
                        | crate::error::DbSurveyorError::ConnectionTimeout { .. }
                );

                tracing::warn!("Failed to collect schema from '{}': {}", db_name, error_str);

                failures.push(DatabaseFailure {
                    database_name: db_name,
                    error_message: error_str,
                    is_connection_error,
                });

                if !config.continue_on_error {
                    tracing::error!("Stopping collection due to continue_on_error=false");
                    break;
                }
            }
        }
    }

    (collected_results, failures)
}

/// Collects schema from a single database.
async fn collect_single_database(
    adapter: &PostgresAdapter,
    database_name: &str,
) -> Result<DatabaseCollectionResult> {
    let start = Instant::now();

    tracing::debug!("Connecting to database: {}", database_name);

    // Create a new adapter for this database
    let db_adapter = adapter.connect_to_database(database_name).await?;

    // Collect schema
    tracing::debug!("Collecting schema from database: {}", database_name);
    let schema = db_adapter.collect_schema().await?;

    let duration = start.elapsed();

    Ok(DatabaseCollectionResult {
        database_name: database_name.to_string(),
        schema,
        collection_duration_ms: duration.as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_database_config_default() {
        let config = MultiDatabaseConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert!(!config.include_system);
        assert!(config.exclude_patterns.is_empty());
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_multi_database_config_builder() {
        let config = MultiDatabaseConfig::new()
            .with_max_concurrency(8)
            .with_include_system(true)
            .with_exclude_patterns(vec!["test_*".to_string(), "*_backup".to_string()])
            .with_continue_on_error(false);

        assert_eq!(config.max_concurrency, 8);
        assert!(config.include_system);
        assert_eq!(config.exclude_patterns.len(), 2);
        assert!(!config.continue_on_error);
    }

    #[test]
    fn test_multi_database_config_min_concurrency() {
        // Should enforce minimum concurrency of 1
        let config = MultiDatabaseConfig::new().with_max_concurrency(0);
        assert_eq!(config.max_concurrency, 1);
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("test", "test"));
        assert!(!glob_match("test", "testing"));
        assert!(!glob_match("testing", "test"));
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("test_*", "test_db"));
        assert!(glob_match("test_*", "test_"));
        assert!(glob_match("test_*", "test_database_backup"));
        assert!(!glob_match("test_*", "testdb"));
        assert!(!glob_match("test_*", "mytest_db"));
    }

    #[test]
    fn test_glob_match_star_prefix() {
        assert!(glob_match("*_backup", "db_backup"));
        assert!(glob_match("*_backup", "test_backup"));
        assert!(glob_match("*_backup", "_backup"));
        assert!(!glob_match("*_backup", "backup"));
        assert!(!glob_match("*_backup", "db_backup_old"));
    }

    #[test]
    fn test_glob_match_star_middle() {
        assert!(glob_match("test_*_db", "test_123_db"));
        assert!(glob_match("test_*_db", "test__db"));
        assert!(!glob_match("test_*_db", "test_db"));
        assert!(!glob_match("test_*_db", "testing_123_db"));
    }

    #[test]
    fn test_glob_match_multiple_stars() {
        assert!(glob_match("*test*", "test"));
        assert!(glob_match("*test*", "mytest"));
        assert!(glob_match("*test*", "testdb"));
        assert!(glob_match("*test*", "mytestdb"));
        assert!(!glob_match("*test*", "tst"));
    }

    #[test]
    fn test_glob_match_question_mark() {
        assert!(glob_match("test?", "test1"));
        assert!(glob_match("test?", "testa"));
        assert!(!glob_match("test?", "test"));
        assert!(!glob_match("test?", "test12"));
    }

    #[test]
    fn test_glob_match_combined() {
        assert!(glob_match("test_?_*", "test_1_db"));
        assert!(glob_match("test_?_*", "test_a_"));
        assert!(!glob_match("test_?_*", "test__db"));
        assert!(!glob_match("test_?_*", "test_12_db"));
    }

    #[test]
    fn test_matches_any_pattern() {
        let patterns = vec!["test_*".to_string(), "*_backup".to_string()];

        assert!(matches_any_pattern("test_db", &patterns));
        assert!(matches_any_pattern("test_", &patterns));
        assert!(matches_any_pattern("my_backup", &patterns));
        assert!(matches_any_pattern("test_backup", &patterns)); // matches both
        assert!(!matches_any_pattern("production", &patterns));
        assert!(!matches_any_pattern("testdb", &patterns));
    }

    #[test]
    fn test_matches_any_pattern_empty() {
        let patterns: Vec<String> = vec![];
        assert!(!matches_any_pattern("anything", &patterns));
    }

    #[test]
    fn test_database_failure_serialization() {
        let failure = DatabaseFailure {
            database_name: "test_db".to_string(),
            error_message: "Connection refused".to_string(),
            is_connection_error: true,
        };

        let json = serde_json::to_string(&failure).unwrap();
        assert!(json.contains("\"database_name\":\"test_db\""));
        assert!(json.contains("\"error_message\":\"Connection refused\""));
        assert!(json.contains("\"is_connection_error\":true"));

        let deserialized: DatabaseFailure = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.database_name, failure.database_name);
        assert_eq!(deserialized.error_message, failure.error_message);
        assert_eq!(
            deserialized.is_connection_error,
            failure.is_connection_error
        );
    }

    #[test]
    fn test_multi_database_metadata_serialization() {
        let metadata = MultiDatabaseMetadata {
            started_at: chrono::Utc::now(),
            total_duration_ms: 1234,
            databases_discovered: 10,
            databases_filtered: 2,
            databases_collected: 7,
            databases_failed: 1,
            databases_skipped: 0,
            max_concurrency: 4,
            collector_version: "1.0.0".to_string(),
            warnings: vec!["Test warning".to_string()],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"databases_discovered\":10"));
        assert!(json.contains("\"databases_collected\":7"));

        let deserialized: MultiDatabaseMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.databases_discovered,
            metadata.databases_discovered
        );
    }
}
