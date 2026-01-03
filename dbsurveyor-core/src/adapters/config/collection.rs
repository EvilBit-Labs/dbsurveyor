//! Schema collection configuration.
//!
//! This module provides the `CollectionConfig` struct for configuring
//! database schema collection operations.

use super::{ConnectionConfig, SamplingConfig};
use serde::{Deserialize, Serialize};

/// Output format options for collected schema data.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Standard JSON format (.dbsurveyor.json)
    #[default]
    Json,
    /// Compressed JSON format (.dbsurveyor.json.zst)
    CompressedJson,
    /// Encrypted format (.dbsurveyor.enc)
    Encrypted,
}

/// Configuration for database schema collection.
///
/// This struct controls all aspects of database schema collection including
/// connection settings, what database objects to include, and output options.
///
/// # Security
/// - Connection credentials are handled separately and never stored here
/// - All database operations are read-only by default
/// - Query timeouts prevent resource exhaustion
///
/// # Example
/// ```rust
/// use dbsurveyor_core::adapters::{CollectionConfig, ConnectionConfig};
///
/// let config = CollectionConfig::new()
///     .with_connection(ConnectionConfig::new("localhost".to_string()));
///
/// assert!(config.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    /// Database connection configuration (credentials handled separately)
    pub connection: ConnectionConfig,
    /// Data sampling configuration
    pub sampling: SamplingConfig,
    /// Whether to include system/internal databases
    pub include_system_databases: bool,
    /// List of database names to exclude from collection
    pub exclude_databases: Vec<String>,
    /// Whether to collect database views
    pub include_views: bool,
    /// Whether to collect stored procedures
    pub include_procedures: bool,
    /// Whether to collect functions
    pub include_functions: bool,
    /// Whether to collect triggers
    pub include_triggers: bool,
    /// Whether to collect indexes
    pub include_indexes: bool,
    /// Whether to collect constraints
    pub include_constraints: bool,
    /// Whether to collect custom/user-defined types
    pub include_custom_types: bool,
    /// Maximum number of concurrent database queries (1-50)
    pub max_concurrent_queries: u32,
    /// Whether to enable data sampling from tables
    pub enable_data_sampling: bool,
    /// Output format for collected schema
    pub output_format: OutputFormat,
    /// Whether to enable compression of output
    pub compression_enabled: bool,
    /// Whether to enable encryption of output
    pub encryption_enabled: bool,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            sampling: SamplingConfig::default(),
            include_system_databases: false,
            exclude_databases: Vec::new(),
            include_views: true,
            include_procedures: true,
            include_functions: true,
            include_triggers: true,
            include_indexes: true,
            include_constraints: true,
            include_custom_types: true,
            max_concurrent_queries: 5,
            enable_data_sampling: false,
            output_format: OutputFormat::Json,
            compression_enabled: false,
            encryption_enabled: false,
        }
    }
}

impl CollectionConfig {
    /// Validates the collection configuration.
    ///
    /// # Errors
    /// Returns error if configuration values are invalid or unsafe
    pub fn validate(&self) -> crate::Result<()> {
        if self.max_concurrent_queries == 0 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries must be greater than 0",
            ));
        }

        if self.max_concurrent_queries > 50 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries should not exceed 50 for safety",
            ));
        }

        self.connection.validate()?;

        Ok(())
    }

    /// Creates a new collection config with safe defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set connection config.
    pub fn with_connection(mut self, connection: ConnectionConfig) -> Self {
        self.connection = connection;
        self
    }

    /// Builder method to set sampling config.
    pub fn with_sampling(mut self, sampling: SamplingConfig) -> Self {
        self.sampling = sampling;
        self
    }

    /// Builder method to set max concurrent queries with validation.
    pub fn with_max_concurrent_queries(mut self, max: u32) -> crate::Result<Self> {
        if max == 0 || max > 50 {
            return Err(crate::error::DbSurveyorError::configuration(
                "max_concurrent_queries must be between 1 and 50",
            ));
        }
        self.max_concurrent_queries = max;
        Ok(self)
    }

    /// Builder method to enable/disable views collection.
    pub fn with_views(mut self, include: bool) -> Self {
        self.include_views = include;
        self
    }

    /// Builder method to enable/disable procedures collection.
    pub fn with_procedures(mut self, include: bool) -> Self {
        self.include_procedures = include;
        self
    }

    /// Builder method to enable/disable functions collection.
    pub fn with_functions(mut self, include: bool) -> Self {
        self.include_functions = include;
        self
    }

    /// Builder method to enable/disable triggers collection.
    pub fn with_triggers(mut self, include: bool) -> Self {
        self.include_triggers = include;
        self
    }

    /// Builder method to enable/disable indexes collection.
    pub fn with_indexes(mut self, include: bool) -> Self {
        self.include_indexes = include;
        self
    }

    /// Builder method to enable/disable constraints collection.
    pub fn with_constraints(mut self, include: bool) -> Self {
        self.include_constraints = include;
        self
    }

    /// Builder method to enable/disable custom types collection.
    pub fn with_custom_types(mut self, include: bool) -> Self {
        self.include_custom_types = include;
        self
    }

    /// Builder method to enable/disable data sampling.
    pub fn with_data_sampling(mut self, enabled: bool) -> Self {
        self.enable_data_sampling = enabled;
        self
    }

    /// Builder method to set output format.
    pub fn with_output_format(mut self, format: OutputFormat) -> Self {
        self.output_format = format;
        self
    }

    /// Builder method to enable/disable compression.
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.compression_enabled = enabled;
        self
    }

    /// Builder method to enable/disable encryption.
    pub fn with_encryption(mut self, enabled: bool) -> Self {
        self.encryption_enabled = enabled;
        self
    }

    /// Builder method to exclude specific databases.
    pub fn exclude_database(mut self, database: impl Into<String>) -> Self {
        self.exclude_databases.push(database.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_config_default() {
        let config = CollectionConfig::default();
        assert!(!config.include_system_databases);
        assert!(config.include_views);
        assert!(config.include_procedures);
        assert!(config.include_functions);
        assert!(config.include_triggers);
        assert!(config.include_indexes);
        assert!(config.include_constraints);
        assert!(config.include_custom_types);
        assert_eq!(config.max_concurrent_queries, 5);
        assert!(!config.enable_data_sampling);
        assert_eq!(config.output_format, OutputFormat::Json);
        assert!(!config.compression_enabled);
        assert!(!config.encryption_enabled);
    }

    #[test]
    fn test_collection_config_validation() {
        // Valid config should pass
        let config = CollectionConfig::new();
        assert!(config.validate().is_ok());

        // Zero concurrent queries should fail
        let mut config = CollectionConfig::new();
        config.max_concurrent_queries = 0;
        assert!(config.validate().is_err());

        // Too many concurrent queries should fail
        let mut config = CollectionConfig::new();
        config.max_concurrent_queries = 51;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_collection_config_builder() {
        let config = CollectionConfig::new()
            .with_views(false)
            .with_procedures(false)
            .with_data_sampling(true)
            .with_output_format(OutputFormat::CompressedJson)
            .with_compression(true)
            .exclude_database("system_db");

        assert!(!config.include_views);
        assert!(!config.include_procedures);
        assert!(config.enable_data_sampling);
        assert_eq!(config.output_format, OutputFormat::CompressedJson);
        assert!(config.compression_enabled);
        assert!(config.exclude_databases.contains(&"system_db".to_string()));
    }

    #[test]
    fn test_with_max_concurrent_queries_validation() {
        // Valid value should succeed
        let result = CollectionConfig::new().with_max_concurrent_queries(10);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().max_concurrent_queries, 10);

        // Zero should fail
        let result = CollectionConfig::new().with_max_concurrent_queries(0);
        assert!(result.is_err());

        // Too high should fail
        let result = CollectionConfig::new().with_max_concurrent_queries(51);
        assert!(result.is_err());
    }

    #[test]
    fn test_output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Json);
    }
}
