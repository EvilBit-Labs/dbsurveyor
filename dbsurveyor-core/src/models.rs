//! Core data models for database schema representation.
//!
//! This module defines the unified data structures used to represent
//! database schemas across different database engines. All models are
//! designed to be serializable and maintain security guarantees.

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: UnifiedDataType,
    pub is_nullable: bool,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub ordinal_position: u32,
}

/// Database table information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub comment: Option<String>,
    pub row_count: Option<u64>,
}

/// Primary key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// Foreign key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_schema: Option<String>,
    pub referenced_columns: Vec<String>,
    pub on_delete: Option<ReferentialAction>,
    pub on_update: Option<ReferentialAction>,
}

/// Referential actions for foreign keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
    Restrict,
    NoAction,
}

/// Database index information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    pub columns: Vec<IndexColumn>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: Option<String>,
}

/// Index column with ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexColumn {
    pub name: String,
    pub sort_order: Option<SortOrder>,
}

/// Sort order for index columns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Database constraint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    pub constraint_type: ConstraintType,
    pub columns: Vec<String>,
    pub check_clause: Option<String>,
}

/// Types of database constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstraintType {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    NotNull,
}

/// Database view information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub schema: Option<String>,
    pub definition: Option<String>,
    pub columns: Vec<Column>,
    pub comment: Option<String>,
}

/// Database procedure/function information
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub data_type: UnifiedDataType,
    pub direction: ParameterDirection,
    pub default_value: Option<String>,
}

/// Parameter direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterDirection {
    In,
    Out,
    InOut,
}

/// Database trigger information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub name: String,
    pub table_name: String,
    pub schema: Option<String>,
    pub event: TriggerEvent,
    pub timing: TriggerTiming,
    pub definition: Option<String>,
}

/// Trigger events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
}

/// Trigger timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

/// Custom type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomType {
    pub name: String,
    pub schema: Option<String>,
    pub definition: String,
    pub category: TypeCategory,
}

/// Categories of custom types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeCategory {
    Enum,
    Composite,
    Domain,
    Range,
}

/// Collection metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMetadata {
    pub collected_at: chrono::DateTime<chrono::Utc>,
    pub collection_duration_ms: u64,
    pub collector_version: String,
    pub warnings: Vec<String>,
}

/// Database information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub version: Option<String>,
    pub size_bytes: Option<u64>,
    pub encoding: Option<String>,
    pub collation: Option<String>,
    pub owner: Option<String>,
    pub is_system_database: bool,
    pub access_level: AccessLevel,
    pub collection_status: CollectionStatus,
}

/// Access level for database operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessLevel {
    /// Full read access to all objects
    Full,
    /// Limited access to some objects
    Limited,
    /// No access (connection failed or insufficient privileges)
    None,
}

/// Status of database collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionStatus {
    /// Collection completed successfully
    Success,
    /// Collection failed with error
    Failed { error: String },
    /// Collection was skipped
    Skipped { reason: String },
}

/// Server-level information for multi-database collection
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseServerSchema {
    pub format_version: String,
    pub server_info: ServerInfo,
    pub databases: Vec<DatabaseSchema>,
    pub collection_metadata: CollectionMetadata,
}

/// Data sampling strategy used for table sampling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Most recent records based on ordering
    MostRecent { limit: u32 },
    /// Random sampling
    Random { limit: u32 },
    /// No sampling performed
    None,
}

/// Ordering strategy for data sampling
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Sample data from a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSample {
    pub table_name: String,
    pub schema_name: Option<String>,
    pub rows: Vec<serde_json::Value>,
    pub sample_size: u32,
    pub total_rows: Option<u64>,
    pub sampling_strategy: SamplingStrategy,
    pub collected_at: chrono::DateTime<chrono::Utc>,
    pub warnings: Vec<String>,
}

/// Complete database schema representation
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            format_version: "1.0".to_string(),
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
            collection_metadata: CollectionMetadata {
                collected_at: chrono::Utc::now(),
                collection_duration_ms: 0,
                collector_version: env!("CARGO_PKG_VERSION").to_string(),
                warnings: Vec::new(),
            },
        }
    }

    /// Adds a warning to the collection metadata
    pub fn add_warning(&mut self, warning: String) {
        self.collection_metadata.warnings.push(warning);
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
    pub fn add_samples(&mut self, samples: Vec<TableSample>) {
        self.samples = Some(samples);
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
    fn test_add_warning() {
        let db_info = DatabaseInfo::new("test_db".to_string());

        let mut schema = DatabaseSchema::new(db_info);
        schema.add_warning("Test warning".to_string());

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
    fn test_add_samples() {
        let db_info = DatabaseInfo::new("test_db".to_string());
        let mut schema = DatabaseSchema::new(db_info);

        let sample = TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: vec![serde_json::json!({"id": 1, "name": "test"})],
            sample_size: 1,
            total_rows: Some(100),
            sampling_strategy: SamplingStrategy::MostRecent { limit: 1 },
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
        };

        schema.add_samples(vec![sample]);
        assert_eq!(schema.sample_count(), 1);
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(DatabaseType::PostgreSQL.to_string(), "PostgreSQL");
        assert_eq!(DatabaseType::MySQL.to_string(), "MySQL");
        assert_eq!(DatabaseType::SQLite.to_string(), "SQLite");
        assert_eq!(DatabaseType::MongoDB.to_string(), "MongoDB");
    }
}
