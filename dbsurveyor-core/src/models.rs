//! Core data models for database schema representation.
//!
//! This module defines the unified data structures used to represent
//! database schemas across different database engines. All models are
//! designed to be serializable and maintain security guarantees.

use serde::{Deserialize, Serialize};

/// Current schema format version.
pub const FORMAT_VERSION: &str = "1.0";

/// Supported database types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
    SqlServer,
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::PostgreSQL => write!(f, "PostgreSQL"),
            DatabaseType::MySQL => write!(f, "MySQL"),
            DatabaseType::SQLite => write!(f, "SQLite"),
            DatabaseType::MongoDB => write!(f, "MongoDB"),
            DatabaseType::SqlServer => write!(f, "SQL Server"),
        }
    }
}

/// Unified data type representation across database engines
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnifiedDataType {
    /// String/text types with optional length
    String { max_length: Option<u32> },
    /// Integer types with bit width
    Integer { bits: u8, signed: bool },
    /// Floating point types
    Float { precision: Option<u8> },
    /// Boolean type
    Boolean,
    /// Date and time types
    DateTime { with_timezone: bool },
    /// Date only
    Date,
    /// Time only
    Time { with_timezone: bool },
    /// Binary data
    Binary { max_length: Option<u32> },
    /// JSON/JSONB data
    Json,
    /// UUID type
    Uuid,
    /// Array types
    Array { element_type: Box<UnifiedDataType> },
    /// Custom/database-specific types
    Custom { type_name: String },
}

/// Database column information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: UnifiedDataType,
    /// Whether the column accepts NULL values (from the schema definition, not data)
    pub is_nullable: bool,
    pub is_primary_key: bool,
    /// Whether the column auto-generates values (SERIAL, AUTO_INCREMENT, IDENTITY, etc.)
    pub is_auto_increment: bool,
    /// SQL expression for the column default, as reported by the database catalog
    pub default_value: Option<String>,
    pub comment: Option<String>,
    /// 1-based position of the column within its table
    pub ordinal_position: u32,
}

/// Database table information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    /// Schema or namespace (e.g. "public" in PostgreSQL, database name in MySQL, None for SQLite)
    pub schema: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub comment: Option<String>,
    /// Estimated row count from database statistics; may be stale or unavailable
    pub row_count: Option<u64>,
}

/// Primary key constraint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimaryKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// Foreign key constraint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignKey {
    pub name: Option<String>,
    /// Local columns participating in the foreign key (order matches `referenced_columns`)
    pub columns: Vec<String>,
    /// Unqualified name of the referenced (parent) table
    pub referenced_table: String,
    /// Schema of the referenced table, if it differs from the local table's schema
    pub referenced_schema: Option<String>,
    /// Columns in the referenced table (order matches `columns`)
    pub referenced_columns: Vec<String>,
    /// Action taken on child rows when the parent row is deleted (None = database default)
    pub on_delete: Option<ReferentialAction>,
    /// Action taken on child rows when the parent key is updated (None = database default)
    pub on_update: Option<ReferentialAction>,
}

/// Referential actions for foreign keys
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
    Restrict,
    NoAction,
}

/// Database index information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    /// Ordered list of columns in the index (order defines the key prefix)
    pub columns: Vec<IndexColumn>,
    /// Whether the index enforces a uniqueness constraint
    pub is_unique: bool,
    /// Whether this index backs the table's primary key
    pub is_primary: bool,
    /// Engine-specific index type (e.g. "btree", "hash", "gin")
    pub index_type: Option<String>,
}

/// Index column with ordering
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexColumn {
    pub name: String,
    pub sort_order: Option<SortDirection>,
}

/// Database constraint information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    pub constraint_type: ConstraintType,
    pub columns: Vec<String>,
    pub check_clause: Option<String>,
}

/// Types of database constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    NotNull,
}

/// Database view information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub schema: Option<String>,
    pub definition: Option<String>,
    pub columns: Vec<Column>,
    pub comment: Option<String>,
}

/// Database procedure/function information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Procedure {
    pub name: String,
    pub schema: Option<String>,
    pub definition: Option<String>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<UnifiedDataType>,
    pub language: Option<String>,
    pub comment: Option<String>,
}

/// Procedure parameter information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub data_type: UnifiedDataType,
    pub direction: ParameterDirection,
    pub default_value: Option<String>,
}

/// Parameter direction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterDirection {
    In,
    Out,
    InOut,
}

/// Database trigger information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    pub event: TriggerEvent,
    pub timing: TriggerTiming,
    pub definition: Option<String>,
}

/// Trigger events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
}

/// Trigger timing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

/// Custom type definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomType {
    pub name: String,
    pub schema: Option<String>,
    pub definition: String,
    pub category: TypeCategory,
}

/// Categories of custom types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeCategory {
    Enum,
    Composite,
    Domain,
    Range,
}

/// Collection metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionMetadata {
    pub collected_at: chrono::DateTime<chrono::Utc>,
    /// Wall-clock duration of the collection in milliseconds
    pub collection_duration_ms: u64,
    pub collector_version: String,
    /// Non-fatal issues encountered during collection (e.g. permission errors on specific tables)
    pub warnings: Vec<String>,
}

/// Database information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    /// Database engine version string (e.g. "16.2")
    pub version: Option<String>,
    /// Estimated on-disk size reported by the database engine; not all engines provide this
    pub size_bytes: Option<u64>,
    pub encoding: Option<String>,
    pub collation: Option<String>,
    pub owner: Option<String>,
    /// True for built-in databases (e.g. postgres, template0, information_schema)
    #[serde(default)]
    pub is_system_database: bool,
    pub access_level: AccessLevel,
    pub collection_status: CollectionStatus,
}

/// Access level for database operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AccessLevel {
    /// Full read access to all objects
    Full,
    /// Limited access to some objects
    Limited,
    /// No access (connection failed or insufficient privileges)
    None,
}

/// Status of database collection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollectionStatus {
    /// Collection completed successfully
    Success,
    /// Collection failed with error
    Failed { error: String },
    /// Collection was skipped
    Skipped { reason: String },
}

/// Server-level information for multi-database collection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub server_type: DatabaseType,
    pub version: String,
    pub host: String,
    pub port: Option<u16>,
    pub total_databases: usize,
    pub collected_databases: usize,
    pub system_databases_excluded: usize,
    pub connection_user: String,
    pub has_superuser_privileges: bool,
    pub collection_mode: CollectionMode,
}

/// Collection mode for database operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CollectionMode {
    /// Single database collection
    SingleDatabase,
    /// Multi-database server collection
    MultiDatabase {
        discovered: usize,
        collected: usize,
        failed: usize,
    },
}

/// Complete database server schema representation for multi-database collection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseServerSchema {
    pub format_version: String,
    pub server_info: ServerInfo,
    pub databases: Vec<DatabaseSchema>,
    pub collection_metadata: CollectionMetadata,
}

/// Data sampling strategy used for table sampling
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Most recent records based on ordering
    MostRecent { limit: u32 },
    /// Random sampling
    Random { limit: u32 },
    /// No sampling performed
    None,
}

/// Ordering strategy for data sampling
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderingStrategy {
    /// Primary key ordering
    PrimaryKey { columns: Vec<String> },
    /// Timestamp column ordering
    Timestamp {
        column: String,
        direction: SortDirection,
    },
    /// Auto-increment column ordering
    AutoIncrement { column: String },
    /// System row ID ordering
    SystemRowId { column: String },
    /// No reliable ordering available
    Unordered,
}

/// Sort direction for ordering
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Status of a sampling operation.
///
/// Stored as `Option<SampleStatus>` on [`TableSample`] -- `None` indicates
/// that status tracking was not used (e.g., legacy data without this field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleStatus {
    /// Sampling completed successfully
    Complete,
    /// Sampling partially completed with a reduced limit (not yet emitted by any adapter)
    PartialRetry { original_limit: u32 },
    /// Sampling was skipped
    Skipped { reason: String },
}

/// Sample data from a table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSample {
    pub table_name: String,
    pub schema_name: Option<String>,
    /// Each element is a JSON object mapping column names to sampled values
    pub rows: Vec<serde_json::Value>,
    /// Number of rows actually returned (may be less than the requested limit)
    pub sample_size: u32,
    /// Estimated total row count from database statistics; may be stale or unavailable
    pub total_rows: Option<u64>,
    pub sampling_strategy: SamplingStrategy,
    pub collected_at: chrono::DateTime<chrono::Utc>,
    pub warnings: Vec<String>,
    /// Outcome of the sampling operation; None for legacy data without status tracking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_status: Option<SampleStatus>,
}

impl TableSample {
    /// Extracts column names from the first row of sample data.
    ///
    /// Returns `None` if the sample is empty or the first row is not a JSON object.
    /// Column names are derived from the first row only.
    pub fn column_names(&self) -> Option<Vec<String>> {
        self.rows
            .first()?
            .as_object()
            .map(|obj| obj.keys().cloned().collect())
    }
}

/// Complete database schema representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub format_version: String,
    pub database_info: DatabaseInfo,
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub procedures: Vec<Procedure>,
    pub functions: Vec<Procedure>, // Functions are similar to procedures
    pub triggers: Vec<Trigger>,
    pub custom_types: Vec<CustomType>,
    pub samples: Option<Vec<TableSample>>, // Optional data samples
    pub quality_metrics: Option<Vec<crate::quality::TableQualityMetrics>>, // Optional quality metrics
    pub collection_metadata: CollectionMetadata,
}

impl DatabaseInfo {
    /// Creates a new database info with default values
    pub fn new(name: String) -> Self {
        Self {
            name,
            version: None,
            size_bytes: None,
            encoding: None,
            collation: None,
            owner: None,
            is_system_database: false,
            access_level: AccessLevel::Full,
            collection_status: CollectionStatus::Success,
        }
    }
}

impl DatabaseSchema {
    /// Creates a new empty database schema
    pub fn new(database_info: DatabaseInfo) -> Self {
        Self {
            format_version: FORMAT_VERSION.to_string(),
            database_info,
            tables: Vec::new(),
            views: Vec::new(),
            indexes: Vec::new(),
            constraints: Vec::new(),
            procedures: Vec::new(),
            functions: Vec::new(),
            triggers: Vec::new(),
            custom_types: Vec::new(),
            samples: None,
            quality_metrics: None,
            collection_metadata: CollectionMetadata {
                collected_at: chrono::Utc::now(),
                collection_duration_ms: 0,
                collector_version: env!("CARGO_PKG_VERSION").to_string(),
                warnings: Vec::new(),
            },
        }
    }

    /// Adds quality metrics to the schema.
    ///
    /// # Arguments
    /// * `metrics` - Vector of quality metrics, one for each analyzed table
    pub fn with_quality_metrics(
        mut self,
        metrics: Vec<crate::quality::TableQualityMetrics>,
    ) -> Self {
        self.quality_metrics = Some(metrics);
        self
    }

    /// Returns the number of tables with quality metrics.
    ///
    /// Returns 0 if quality metrics have not been collected.
    pub fn quality_metrics_count(&self) -> usize {
        self.quality_metrics.as_ref().map_or(0, |m| m.len())
    }

    /// Adds a warning to the collection metadata
    pub fn with_warning(mut self, warning: String) -> Self {
        self.collection_metadata.warnings.push(warning);
        self
    }

    /// Populates the schema-level `indexes` and `constraints` vectors by
    /// aggregating from per-table data.
    ///
    /// This avoids cloning entire vectors during schema construction.
    /// Call this after all tables have been added to the schema.
    pub fn with_aggregated_indexes_and_constraints(mut self) -> Self {
        let total_indexes: usize = self.tables.iter().map(|t| t.indexes.len()).sum();
        let total_constraints: usize = self.tables.iter().map(|t| t.constraints.len()).sum();

        let mut indexes = Vec::with_capacity(total_indexes);
        let mut constraints = Vec::with_capacity(total_constraints);

        for table in &self.tables {
            indexes.extend(table.indexes.iter().cloned());
            constraints.extend(table.constraints.iter().cloned());
        }

        self.indexes = indexes;
        self.constraints = constraints;
        self
    }

    /// Gets the total number of database objects
    pub fn object_count(&self) -> usize {
        self.tables.len()
            + self.views.len()
            + self.indexes.len()
            + self.constraints.len()
            + self.procedures.len()
            + self.functions.len()
            + self.triggers.len()
            + self.custom_types.len()
    }

    /// Adds sample data to the schema
    pub fn with_samples(mut self, samples: Vec<TableSample>) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Gets the number of sampled tables
    pub fn sample_count(&self) -> usize {
        self.samples.as_ref().map_or(0, |s| s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_schema_creation() {
        let mut db_info = DatabaseInfo::new("test_db".to_string());
        db_info.version = Some("13.0".to_string());
        db_info.size_bytes = Some(1024);
        db_info.encoding = Some("UTF8".to_string());
        db_info.collation = Some("en_US.UTF-8".to_string());

        let schema = DatabaseSchema::new(db_info);
        assert_eq!(schema.format_version, "1.0");
        assert_eq!(schema.database_info.name, "test_db");
        assert_eq!(schema.object_count(), 0);
        assert_eq!(schema.sample_count(), 0);
    }

    #[test]
    fn test_with_warning() {
        let db_info = DatabaseInfo::new("test_db".to_string());

        let schema = DatabaseSchema::new(db_info);
        let schema = schema.with_warning("Test warning".to_string());

        assert_eq!(schema.collection_metadata.warnings.len(), 1);
        assert_eq!(schema.collection_metadata.warnings[0], "Test warning");
    }

    #[test]
    fn test_database_info_creation() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        assert_eq!(db_info.name, "test_db");
        assert!(!db_info.is_system_database);
        assert!(matches!(db_info.access_level, AccessLevel::Full));
        assert!(matches!(
            db_info.collection_status,
            CollectionStatus::Success
        ));
    }

    #[test]
    fn test_with_samples() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        let schema = DatabaseSchema::new(db_info);

        let sample = TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: vec![serde_json::json!({"id": 1, "name": "test"})],
            sample_size: 1,
            total_rows: Some(100),
            sampling_strategy: SamplingStrategy::MostRecent { limit: 1 },
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
            sample_status: None,
        };

        let schema = schema.with_samples(vec![sample]);
        assert_eq!(schema.sample_count(), 1);
    }

    #[test]
    fn test_with_quality_metrics() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        let schema = DatabaseSchema::new(db_info);

        assert_eq!(schema.quality_metrics_count(), 0);

        let metrics =
            crate::quality::TableQualityMetrics::new("users", Some("public".to_string()), 50);
        let schema = schema.with_quality_metrics(vec![metrics]);
        assert_eq!(schema.quality_metrics_count(), 1);
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(DatabaseType::PostgreSQL.to_string(), "PostgreSQL");
        assert_eq!(DatabaseType::MySQL.to_string(), "MySQL");
        assert_eq!(DatabaseType::SQLite.to_string(), "SQLite");
        assert_eq!(DatabaseType::MongoDB.to_string(), "MongoDB");
    }

    #[test]
    fn test_table_sample_serialize_omits_none_sample_status() {
        let sample = TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: vec![],
            sample_size: 0,
            total_rows: None,
            sampling_strategy: SamplingStrategy::None,
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
            sample_status: None,
        };

        let json = serde_json::to_value(&sample).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("sample_status"),
            "sample_status should be omitted when None"
        );
    }

    #[test]
    fn test_table_sample_deserialize_without_sample_status() {
        let json = serde_json::json!({
            "table_name": "orders",
            "schema_name": "public",
            "rows": [],
            "sample_size": 0,
            "total_rows": null,
            "sampling_strategy": "None",
            "collected_at": "2025-01-01T00:00:00Z",
            "warnings": []
        });

        let sample: TableSample = serde_json::from_value(json).unwrap();
        assert!(
            sample.sample_status.is_none(),
            "sample_status should default to None when absent in JSON"
        );
    }

    #[test]
    fn test_table_sample_deserialize_sample_status_complete() {
        let json = serde_json::json!({
            "table_name": "orders",
            "schema_name": "public",
            "rows": [],
            "sample_size": 5,
            "total_rows": 100,
            "sampling_strategy": {"MostRecent": {"limit": 5}},
            "collected_at": "2025-01-01T00:00:00Z",
            "warnings": [],
            "sample_status": "Complete"
        });

        let sample: TableSample = serde_json::from_value(json).unwrap();
        assert!(matches!(sample.sample_status, Some(SampleStatus::Complete)));
    }

    #[test]
    fn test_table_sample_deserialize_sample_status_partial_retry() {
        let json = serde_json::json!({
            "table_name": "orders",
            "schema_name": null,
            "rows": [],
            "sample_size": 3,
            "total_rows": 100,
            "sampling_strategy": {"MostRecent": {"limit": 10}},
            "collected_at": "2025-01-01T00:00:00Z",
            "warnings": [],
            "sample_status": {"PartialRetry": {"original_limit": 10}}
        });

        let sample: TableSample = serde_json::from_value(json).unwrap();
        assert!(matches!(
            sample.sample_status,
            Some(SampleStatus::PartialRetry { original_limit: 10 })
        ));
    }

    #[test]
    fn test_table_sample_deserialize_sample_status_skipped() {
        let json = serde_json::json!({
            "table_name": "large_table",
            "schema_name": null,
            "rows": [],
            "sample_size": 0,
            "total_rows": 1000000,
            "sampling_strategy": "None",
            "collected_at": "2025-01-01T00:00:00Z",
            "warnings": [],
            "sample_status": {"Skipped": {"reason": "table too large"}}
        });

        let sample: TableSample = serde_json::from_value(json).unwrap();
        match &sample.sample_status {
            Some(SampleStatus::Skipped { reason }) => {
                assert_eq!(reason, "table too large");
            }
            other => panic!("Expected Skipped variant, got {:?}", other),
        }
    }
}
