//! Core data structures and utilities for dbsurveyor toolchain
//!
//! This crate contains common data structures, types, and utilities
//! shared between the collector and postprocessor components.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Version information for survey data format
pub const SURVEY_FORMAT_VERSION: &str = "1.0";

/// Common error types used across the toolchain
#[derive(thiserror::Error, Debug)]
pub enum SurveyError {
    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// File system or network I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error from other subsystems
    #[error("Generic error: {0}")]
    Generic(#[from] anyhow::Error),
}

/// Versioned output format for compatibility
#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseSurvey {
    /// Format version for compatibility tracking
    pub format_version: String, // "1.0"
    /// Unique identifier for this survey
    pub survey_id: Uuid,
    /// Timestamp when the survey was collected
    pub collected_at: DateTime<Utc>,
    /// Version of the collector tool
    pub collector_version: String,
    /// Database information and connection details
    pub database: DatabaseInfo,
    /// List of database schemas collected
    pub schemas: Vec<SchemaInfo>,
    /// Survey metadata and statistics
    pub metadata: SurveyMetadata,
}

impl DatabaseSurvey {
    /// Create a new database survey with current timestamp and generated ID
    #[must_use]
    pub fn new(database: DatabaseInfo) -> Self {
        Self {
            format_version: SURVEY_FORMAT_VERSION.to_string(),
            survey_id: Uuid::new_v4(),
            collected_at: Utc::now(),
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            database,
            schemas: Vec::new(),
            metadata: SurveyMetadata::default(),
        }
    }
}

/// Database connection and server information
#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database engine type (postgresql, mysql, etc.)
    pub engine: String,     // "postgresql", "mysql", etc.
    /// Database server version
    pub version: Option<String>,
    /// Database host (sanitized, no credentials)
    pub host: String,       // Sanitized (no credentials)
    /// Database name
    pub database_name: String,
    /// Connection metadata and statistics
    pub connection_info: ConnectionMetadata,
}

/// Connection metadata and statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionMetadata {
    /// Connection success status
    pub connected_successfully: bool,
    /// Connection timestamp  
    pub connected_at: DateTime<Utc>,
    /// Connection latency in milliseconds
    pub connection_latency_ms: Option<u64>,
    /// Any connection warnings (sanitized)
    pub warnings: Vec<String>,
}

/// Database schema information
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaInfo {
    /// Schema name
    pub name: String,
    /// Tables in this schema
    pub tables: Vec<TableInfo>,
    /// Views in this schema
    pub views: Vec<ViewInfo>,
    /// Functions in this schema
    pub functions: Vec<FunctionInfo>,
}

/// Database table information
#[derive(Debug, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Schema name containing this table
    pub schema: String,
    /// Table columns
    pub columns: Vec<ColumnInfo>,
    /// Table indexes
    pub indexes: Vec<IndexInfo>,
    /// Table constraints
    pub constraints: Vec<ConstraintInfo>,
    /// Approximate row count
    pub row_count: Option<u64>,
    /// Table size in bytes
    pub size_bytes: Option<u64>,
}

/// Database column information
#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// Column data type
    pub data_type: String,
    /// Whether column allows NULL values
    pub is_nullable: bool,
    /// Default value for column
    pub default_value: Option<String>,
    /// Maximum character length for string types
    pub character_maximum_length: Option<i32>,
    /// Numeric precision for numeric types
    pub numeric_precision: Option<i32>,
    /// Numeric scale for numeric types
    pub numeric_scale: Option<i32>,
}

/// Database index information
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexInfo {
    /// Index name
    pub name: String,
    /// Columns included in index
    pub columns: Vec<String>,
    /// Whether index enforces uniqueness
    pub is_unique: bool,
    /// Whether index is a primary key
    pub is_primary: bool,
    /// Index type (btree, hash, etc.)
    pub index_type: String,
}

/// Database constraint information
#[derive(Debug, Serialize, Deserialize)]
pub struct ConstraintInfo {
    /// Constraint name
    pub name: String,
    /// Constraint type (PRIMARY KEY, FOREIGN KEY, CHECK, etc.)
    pub constraint_type: String, // "PRIMARY KEY", "FOREIGN KEY", "CHECK", etc.
    /// Columns affected by constraint
    pub columns: Vec<String>,
    /// Referenced table for foreign keys
    pub referenced_table: Option<String>,
    /// Referenced columns for foreign keys
    pub referenced_columns: Option<Vec<String>>,
}

/// Database view information
#[derive(Debug, Serialize, Deserialize)]
pub struct ViewInfo {
    /// View name
    pub name: String,
    /// Schema name containing this view
    pub schema: String,
    /// View definition SQL
    pub definition: Option<String>,
    /// View columns
    pub columns: Vec<ColumnInfo>,
}

/// Database function information
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Schema name containing this function
    pub schema: String,
    /// Function return type
    pub return_type: Option<String>,
    /// Function implementation language
    pub language: Option<String>,
    /// Function definition
    pub definition: Option<String>,
}

/// Basic survey metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurveyMetadata {
    /// Format version for compatibility checking
    pub format_version: String,
    /// Timestamp when survey was created
    pub created_at: DateTime<Utc>,
    /// Tool version that created the survey
    pub tool_version: String,
    /// Database type that was surveyed
    pub database_type: String,
    /// Collection statistics
    pub stats: CollectionStats,
}

/// Survey metadata and collection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    /// Total number of schemas collected
    pub schema_count: u32,
    /// Total number of tables collected
    pub table_count: u32,
    /// Total number of columns collected
    pub column_count: u32,
    /// Collection duration in milliseconds
    pub collection_duration_ms: u64,
}

impl Default for SurveyMetadata {
    fn default() -> Self {
        Self {
            format_version: SURVEY_FORMAT_VERSION.to_string(),
            created_at: Utc::now(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            database_type: "unknown".to_string(),
            stats: CollectionStats {
                schema_count: 0,
                table_count: 0,
                column_count: 0,
                collection_duration_ms: 0,
            },
        }
    }
}

impl Default for ConnectionMetadata {
    fn default() -> Self {
        Self {
            connected_successfully: false,
            connected_at: Utc::now(),
            connection_latency_ms: None,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_database_survey_json_format() {
        // Create a sample database survey
        let db_info = DatabaseInfo {
            engine: "postgresql".to_string(),
            version: Some("13.5".to_string()),
            host: "localhost".to_string(),
            database_name: "testdb".to_string(),
            connection_info: ConnectionMetadata::default(),
        };

        let survey = DatabaseSurvey::new(db_info);
        
        // Test JSON serialization
        let json = serde_json::to_string_pretty(&survey).expect("Failed to serialize survey");
        
        // Verify format version is correct
        assert!(json.contains("\"format_version\": \"1.0\""));
        
        // Test JSON deserialization
        let _deserialized: DatabaseSurvey = serde_json::from_str(&json)
            .expect("Failed to deserialize survey");
    }

    #[test]
    fn test_format_version() {
        assert_eq!(SURVEY_FORMAT_VERSION, "1.0");
    }
}
