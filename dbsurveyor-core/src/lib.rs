//! Core data structures and utilities for dbsurveyor toolchain
//!
//! This crate contains common data structures, types, and utilities
//! shared between the collector and postprocessor components.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    pub engine: String, // "postgresql", "mysql", etc.
    /// Database server version
    pub version: Option<String>,
    /// Database host (sanitized, no credentials)
    pub host: String, // Sanitized (no credentials)
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

    #[test]
    fn test_database_survey_json_format() -> Result<(), Box<dyn std::error::Error>> {
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
        let json = serde_json::to_string_pretty(&survey)?;

        // Verify format version is correct
        assert!(json.contains("\"format_version\": \"1.0\""));

        // Test JSON deserialization
        let _deserialized: DatabaseSurvey = serde_json::from_str(&json)?;

        Ok(())
    }

    #[test]
    fn test_format_version() {
        assert_eq!(SURVEY_FORMAT_VERSION, "1.0");
    }

    #[test]
    fn test_database_survey_creation() {
        let db_info = DatabaseInfo {
            engine: "mysql".to_string(),
            version: None,
            host: "example.com".to_string(),
            database_name: "myapp".to_string(),
            connection_info: ConnectionMetadata {
                connected_successfully: true,
                connected_at: Utc::now(),
                connection_latency_ms: Some(150),
                warnings: vec!["Test warning".to_string()],
            },
        };

        let survey = DatabaseSurvey::new(db_info);

        assert_eq!(survey.format_version, "1.0");
        assert_eq!(survey.database.engine, "mysql");
        assert_eq!(survey.database.database_name, "myapp");
        assert_eq!(survey.database.host, "example.com");
        assert!(survey.database.version.is_none());
        assert!(survey.schemas.is_empty());
        assert_eq!(survey.metadata.format_version, "1.0");
    }

    #[test]
    fn test_survey_metadata_default() {
        let metadata = SurveyMetadata::default();

        assert_eq!(metadata.format_version, "1.0");
        assert_eq!(metadata.database_type, "unknown");
        assert_eq!(metadata.stats.schema_count, 0);
        assert_eq!(metadata.stats.table_count, 0);
        assert_eq!(metadata.stats.column_count, 0);
        assert_eq!(metadata.stats.collection_duration_ms, 0);
    }

    #[test]
    fn test_connection_metadata_default() {
        let conn_meta = ConnectionMetadata::default();

        assert!(!conn_meta.connected_successfully);
        assert!(conn_meta.connection_latency_ms.is_none());
        assert!(conn_meta.warnings.is_empty());
    }

    #[test]
    fn test_survey_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let survey_error = SurveyError::Io(io_error);
        let error_string = format!("{survey_error}");
        assert!(error_string.contains("IO error"));
        assert!(error_string.contains("File not found"));

        // Test serialization error using a simpler approach
        let result: Result<String, _> = serde_json::from_str("{invalid json");
        if let Err(json_error) = result {
            let survey_error = SurveyError::Serialization(json_error);
            let error_string = format!("{survey_error}");
            assert!(error_string.contains("Serialization error"));
        }

        let generic_error = anyhow::anyhow!("Generic test error");
        let survey_error = SurveyError::Generic(generic_error);
        let error_string = format!("{survey_error}");
        assert!(error_string.contains("Generic error"));
        assert!(error_string.contains("Generic test error"));
    }

    #[test]
    fn test_complex_database_structures() {
        let mut survey = DatabaseSurvey::new(DatabaseInfo {
            engine: "postgresql".to_string(),
            version: Some("14.1".to_string()),
            host: "db.example.com".to_string(),
            database_name: "production".to_string(),
            connection_info: ConnectionMetadata {
                connected_successfully: true,
                connected_at: Utc::now(),
                connection_latency_ms: Some(25),
                warnings: vec![],
            },
        });

        // Add a schema with table, view, and function
        survey.schemas.push(SchemaInfo {
            name: "public".to_string(),
            tables: vec![TableInfo {
                name: "users".to_string(),
                schema: "public".to_string(),
                columns: vec![ColumnInfo {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    is_nullable: false,
                    default_value: Some("nextval('users_id_seq'::regclass)".to_string()),
                    character_maximum_length: None,
                    numeric_precision: Some(32),
                    numeric_scale: Some(0),
                }],
                indexes: vec![IndexInfo {
                    name: "users_pkey".to_string(),
                    columns: vec!["id".to_string()],
                    is_unique: true,
                    is_primary: true,
                    index_type: "btree".to_string(),
                }],
                constraints: vec![ConstraintInfo {
                    name: "users_pkey".to_string(),
                    constraint_type: "PRIMARY KEY".to_string(),
                    columns: vec!["id".to_string()],
                    referenced_table: None,
                    referenced_columns: None,
                }],
                row_count: Some(1000),
                size_bytes: Some(65536),
            }],
            views: vec![ViewInfo {
                name: "active_users".to_string(),
                schema: "public".to_string(),
                definition: Some("SELECT * FROM users WHERE active = true".to_string()),
                columns: vec![ColumnInfo {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    is_nullable: false,
                    default_value: None,
                    character_maximum_length: None,
                    numeric_precision: Some(32),
                    numeric_scale: Some(0),
                }],
            }],
            functions: vec![FunctionInfo {
                name: "get_user_count".to_string(),
                schema: "public".to_string(),
                return_type: Some("integer".to_string()),
                language: Some("sql".to_string()),
                definition: Some("SELECT count(*) FROM users".to_string()),
            }],
        });

        // Update metadata to reflect the collection
        survey.metadata.stats.schema_count = 1;
        survey.metadata.stats.table_count = 1;
        survey.metadata.stats.column_count = 1;
        survey.metadata.stats.collection_duration_ms = 500;

        // Test serialization of complex structure
        let json = serde_json::to_string_pretty(&survey);
        assert!(json.is_ok());

        if let Ok(json_str) = json {
            assert!(json_str.contains("\"name\": \"users\""));
            assert!(json_str.contains("\"name\": \"active_users\""));
            assert!(json_str.contains("\"name\": \"get_user_count\""));

            // Test deserialization
            let deserialized: Result<DatabaseSurvey, _> = serde_json::from_str(&json_str);
            assert!(deserialized.is_ok());
        }
    }

    #[test]
    fn test_survey_metadata_clone() {
        let original = SurveyMetadata::default();
        let cloned = original.clone();

        assert_eq!(original.format_version, cloned.format_version);
        assert_eq!(original.database_type, cloned.database_type);
        assert_eq!(original.stats.schema_count, cloned.stats.schema_count);
    }

    #[test]
    fn test_collection_stats_clone() {
        let original = CollectionStats {
            schema_count: 5,
            table_count: 25,
            column_count: 150,
            collection_duration_ms: 2500,
        };
        let cloned = original.clone();

        assert_eq!(original.schema_count, cloned.schema_count);
        assert_eq!(original.table_count, cloned.table_count);
        assert_eq!(original.column_count, cloned.column_count);
        assert_eq!(
            original.collection_duration_ms,
            cloned.collection_duration_ms
        );
    }
}
