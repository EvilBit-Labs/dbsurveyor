//! Core data structures and utilities for dbsurveyor toolchain
//!
//! This crate contains common data structures, types, and utilities
//! shared between the collector and postprocessor components.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Version information for survey data format
pub const SURVEY_FORMAT_VERSION: &str = "1.0";

/// Safe host representation that redacts credentials
///
/// This newtype ensures that database hosts are always sanitized
/// and never contain credentials in debug output or serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeHost(String);

impl SafeHost {
    /// Create a new `SafeHost` from a string, redacting any credentials
    ///
    /// # Arguments
    ///
    /// * `host` - Host string that may contain credentials
    ///
    /// # Returns
    ///
    /// Returns a `SafeHost` with credentials redacted, or an error if parsing fails
    ///
    /// # Errors
    ///
    /// Returns an error if the host string cannot be parsed or contains invalid characters
    pub fn new(host: String) -> Result<Self, String> {
        // Check if it contains @ (potential credentials)
        if host.contains('@') {
            // Try to parse as URL first (only if it has a scheme)
            if host.contains("://") {
                url::Url::parse(&host).map_or_else(
                    |_| {
                        // If URL parsing fails, fall back to @ extraction
                        if let Some(at_pos) = host.rfind('@') {
                            let after_at = &host[at_pos.saturating_add(1)..];
                            Ok(Self(after_at.to_string()))
                        } else {
                            Ok(Self(host))
                        }
                    },
                    |url| {
                        // Extract host:port/path without credentials
                        let host_str = url.host_str().unwrap_or("unknown");
                        let port_str = url.port().map(|p| format!(":{p}")).unwrap_or_default();
                        let path_str = url.path();
                        let sanitized = format!("{host_str}{port_str}{path_str}");
                        Ok(Self(sanitized))
                    },
                )
            } else {
                // No scheme, extract everything after @
                if let Some(at_pos) = host.rfind('@') {
                    let after_at = &host[at_pos.saturating_add(1)..];
                    Ok(Self(after_at.to_string()))
                } else {
                    Ok(Self(host))
                }
            }
        } else {
            // No credentials, just use as-is
            Ok(Self(host))
        }
    }

    /// Get the sanitized host string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for SafeHost {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

impl fmt::Display for SafeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for SafeHost {
    type Error = String;

    fn try_from(host: String) -> Result<Self, Self::Error> {
        Self::new(host)
    }
}

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

    /// Create a new database survey with a specific timestamp (for testing)
    #[must_use]
    pub fn with_timestamp(database: DatabaseInfo, timestamp: DateTime<Utc>) -> Self {
        Self {
            format_version: SURVEY_FORMAT_VERSION.to_string(),
            survey_id: Uuid::new_v4(),
            collected_at: timestamp,
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            database,
            schemas: Vec::new(),
            metadata: SurveyMetadata::with_timestamp(timestamp),
        }
    }
}

/// Database connection and server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database engine type (postgresql, mysql, etc.)
    pub engine: String, // "postgresql", "mysql", etc.
    /// Database server version
    pub version: Option<String>,
    /// Database host (sanitized, no credentials)
    pub host: SafeHost,
    /// Database name
    pub database_name: String,
    /// Connection metadata and statistics
    pub connection_info: ConnectionMetadata,
}

/// Connection metadata and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl ConnectionMetadata {
    /// Create a new connection metadata with current timestamp
    #[must_use]
    pub fn new() -> Self {
        Self {
            connected_successfully: false,
            connected_at: Utc::now(),
            connection_latency_ms: None,
            warnings: Vec::new(),
        }
    }

    /// Create a new connection metadata with a specific timestamp (for testing)
    #[must_use]
    pub const fn with_timestamp(timestamp: DateTime<Utc>) -> Self {
        Self {
            connected_successfully: false,
            connected_at: timestamp,
            connection_latency_ms: None,
            warnings: Vec::new(),
        }
    }
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

impl SurveyMetadata {
    /// Create a new survey metadata with current timestamp
    #[must_use]
    pub fn new() -> Self {
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

    /// Create a new survey metadata with a specific timestamp (for testing)
    #[must_use]
    pub fn with_timestamp(timestamp: DateTime<Utc>) -> Self {
        Self {
            format_version: SURVEY_FORMAT_VERSION.to_string(),
            created_at: timestamp,
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

/// Survey metadata and collection statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        Self::new()
    }
}

impl Default for ConnectionMetadata {
    fn default() -> Self {
        Self::new()
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
            host: SafeHost::new("localhost:5432/testdb".to_string())?,
            database_name: "testdb".to_string(),
            connection_info: ConnectionMetadata::new(),
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
    fn test_database_survey_creation() -> Result<(), Box<dyn std::error::Error>> {
        let db_info = DatabaseInfo {
            engine: "mysql".to_string(),
            version: None,
            host: SafeHost::new("example.com:3306/myapp".to_string())?,
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
        assert_eq!(survey.database.host.as_str(), "example.com:3306/myapp");
        assert!(survey.database.version.is_none());
        assert!(survey.schemas.is_empty());
        assert_eq!(survey.metadata.format_version, "1.0");

        Ok(())
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
    fn test_complex_database_structures() -> Result<(), Box<dyn std::error::Error>> {
        let mut survey = DatabaseSurvey::new(DatabaseInfo {
            engine: "postgresql".to_string(),
            version: Some("14.1".to_string()),
            host: SafeHost::new("db.example.com:5432/production".to_string())?,
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

        Ok(())
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

    #[test]
    fn test_safe_host_creation() -> Result<(), Box<dyn std::error::Error>> {
        // Test basic host without credentials
        let host = SafeHost::new("localhost:5432".to_string())?;
        assert_eq!(host.as_str(), "localhost:5432");

        // Test host with credentials (should be redacted)
        let host_with_creds = SafeHost::new("user:pass@localhost:5432/db".to_string())?;
        assert_eq!(host_with_creds.as_str(), "localhost:5432/db");

        // Test URL parsing
        let url_host =
            SafeHost::new("postgresql://user:secret@db.example.com:5432/production".to_string())?;
        assert_eq!(url_host.as_str(), "db.example.com:5432/production");

        // Test FromStr implementation
        let host_from_str: SafeHost = "localhost:3306".parse()?;
        assert_eq!(host_from_str.as_str(), "localhost:3306");

        Ok(())
    }

    #[test]
    fn test_safe_host_display() -> Result<(), Box<dyn std::error::Error>> {
        let host = SafeHost::new("localhost:5432".to_string())?;
        let display = format!("{host}");
        assert_eq!(display, "localhost:5432");

        // Test that credentials are not displayed
        let host_with_creds = SafeHost::new("admin:secret@prod.db:5432/main".to_string())?;
        let display_with_creds = format!("{host_with_creds}");
        assert_eq!(display_with_creds, "prod.db:5432/main");
        assert!(!display_with_creds.contains("secret"));
        assert!(!display_with_creds.contains("admin:secret"));

        Ok(())
    }

    #[test]
    fn test_timestamp_injection() -> Result<(), Box<dyn std::error::Error>> {
        let fixed_timestamp = chrono::DateTime::parse_from_rfc3339("2023-01-01T12:00:00Z")
            .map_err(|e| format!("Failed to parse timestamp: {e}"))?
            .with_timezone(&Utc);

        let db_info = DatabaseInfo {
            engine: "postgresql".to_string(),
            version: Some("14.1".to_string()),
            host: SafeHost::new("localhost:5432/test".to_string())
                .map_err(|e| format!("Failed to create SafeHost: {e}"))?,
            database_name: "test".to_string(),
            connection_info: ConnectionMetadata::with_timestamp(fixed_timestamp),
        };

        let survey = DatabaseSurvey::with_timestamp(db_info, fixed_timestamp);

        assert_eq!(survey.collected_at, fixed_timestamp);
        assert_eq!(survey.metadata.created_at, fixed_timestamp);
        assert_eq!(
            survey.database.connection_info.connected_at,
            fixed_timestamp
        );

        Ok(())
    }

    #[test]
    fn test_deterministic_survey_creation() -> Result<(), Box<dyn std::error::Error>> {
        let fixed_timestamp = chrono::DateTime::parse_from_rfc3339("2023-01-01T12:00:00Z")
            .map_err(|e| format!("Failed to parse timestamp: {e}"))?
            .with_timezone(&Utc);

        let db_info = DatabaseInfo {
            engine: "postgresql".to_string(),
            version: Some("14.1".to_string()),
            host: SafeHost::new("localhost:5432/test".to_string())
                .map_err(|e| format!("Failed to create SafeHost: {e}"))?,
            database_name: "test".to_string(),
            connection_info: ConnectionMetadata::with_timestamp(fixed_timestamp),
        };

        let survey1 = DatabaseSurvey::with_timestamp(db_info.clone(), fixed_timestamp);
        let survey2 = DatabaseSurvey::with_timestamp(db_info, fixed_timestamp);

        // Timestamps should be identical
        assert_eq!(survey1.collected_at, survey2.collected_at);
        assert_eq!(survey1.metadata.created_at, survey2.metadata.created_at);
        assert_eq!(
            survey1.database.connection_info.connected_at,
            survey2.database.connection_info.connected_at
        );

        // Only survey_id should differ (randomly generated)
        assert_ne!(survey1.survey_id, survey2.survey_id);

        Ok(())
    }
}
