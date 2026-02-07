//! Database schema collection tool.
//!
//! This binary connects to databases and extracts comprehensive schema
//! information including tables, columns, indexes, constraints, and
//! relationships. It operates with security-first principles.
//!
//! # Security Guarantees
//! - Read-only database operations only
//! - No credentials stored or logged
//! - Offline operation after database connection
//! - Optional AES-GCM encryption for outputs

use clap::{Args, Parser, Subcommand};
use dbsurveyor_core::Result;
use std::path::PathBuf;

mod collect;
mod multi_db;
mod output;

#[derive(Parser)]
#[command(name = "dbsurveyor-collect")]
#[command(about = "Database schema collection tool")]
#[command(version)]
#[command(long_about = "
DBSurveyor Collector - Secure database schema collection

This tool connects to databases and extracts comprehensive metadata including:
- Tables, columns, and data types
- Indexes and constraints
- Foreign key relationships
- Views, procedures, and functions

SECURITY FEATURES:
- Read-only operations only
- No credentials stored or logged
- Optional AES-GCM encryption
- Offline operation after connection

SUPPORTED DATABASES:
- PostgreSQL (postgres://)
- MySQL (mysql://)
- SQLite (sqlite:// or .db/.sqlite files)
- MongoDB (mongodb://)
- SQL Server (mssql://) [if compiled with --features mssql]

EXAMPLES:
  dbsurveyor-collect postgres://user:pass@localhost/db
  dbsurveyor-collect --encrypt --output schema.enc postgres://localhost/db
  dbsurveyor-collect --compress sqlite:///path/to/database.db
")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Database connection URL
    #[arg(
        long,
        env = "DATABASE_URL",
        help = "Database connection string (credentials will be sanitized in logs)"
    )]
    pub database_url: Option<String>,

    /// Output file path
    #[arg(
        short,
        long,
        default_value = "schema.dbsurveyor.json",
        help = "Output file path (.json, .json.zst, or .enc)"
    )]
    pub output: PathBuf,

    /// Number of sample rows per table
    #[arg(
        long,
        default_value = "100",
        help = "Number of sample rows to collect per table"
    )]
    pub sample: u32,

    /// Throttle delay between operations (ms)
    #[arg(
        long,
        help = "Delay in milliseconds between database operations for stealth"
    )]
    pub throttle: Option<u64>,

    /// Enable compression
    #[arg(long, help = "Compress output using Zstandard (.json.zst)")]
    pub compress: bool,

    /// Enable encryption
    #[arg(long, help = "Encrypt output using AES-GCM (.enc)")]
    pub encrypt: bool,

    /// Collect all accessible databases
    #[arg(
        long,
        help = "Collect schemas from all accessible databases on the server"
    )]
    pub all_databases: bool,

    /// Include system databases
    #[arg(long, help = "Include system databases in multi-database collection")]
    pub include_system_databases: bool,

    /// Exclude specific databases
    #[arg(
        long,
        value_delimiter = ',',
        help = "Comma-separated list of databases to exclude"
    )]
    pub exclude_databases: Vec<String>,

    /// Enable quality analysis
    #[arg(long, help = "Enable data quality analysis on sampled data")]
    pub enable_quality: bool,

    /// Quality threshold overrides (format: metric:value)
    #[arg(
        long,
        value_delimiter = ',',
        help = "Quality thresholds (completeness:0.9,uniqueness:0.95,consistency:0.85)"
    )]
    pub quality_threshold: Vec<String>,

    /// Disable anomaly detection
    #[arg(
        long,
        help = "Disable statistical anomaly detection in quality analysis"
    )]
    pub disable_anomaly_detection: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Collect schema from database
    Collect(CollectArgs),
    /// Test database connection
    Test(TestArgs),
    /// List supported database types
    List,
}

#[derive(Args)]
pub struct CollectArgs {
    /// Database connection URL
    #[arg(help = "Database connection string")]
    pub database_url: String,

    /// Output file path
    #[arg(short, long, help = "Output file path")]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct TestArgs {
    /// Database connection URL
    #[arg(help = "Database connection string to test")]
    pub database_url: String,
}

#[derive(Args)]
pub struct GlobalArgs {
    /// Increase verbosity
    #[arg(
        short,
        long,
        action = clap::ArgAction::Count,
        help = "Increase verbosity (-v, -vv, -vvv)"
    )]
    pub verbose: u8,

    /// Suppress output
    #[arg(short, long, help = "Suppress all output except errors")]
    pub quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    dbsurveyor_core::init_logging(cli.global.verbose, cli.global.quiet)?;

    // Initialize JSON Schema validator
    dbsurveyor_core::initialize_schema_validator().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to initialize schema validator: {}",
            e
        ))
    })?;

    // Handle commands
    match &cli.command {
        Some(Command::Collect(args)) => {
            let output = args
                .output
                .clone()
                .unwrap_or_else(|| "schema.dbsurveyor.json".into());
            collect::collect_schema(&args.database_url, &output, &cli).await
        }
        Some(Command::Test(args)) => collect::test_connection(&args.database_url).await,
        Some(Command::List) => {
            collect::list_supported_databases();
            Ok(())
        }
        None => {
            // Default behavior: collect schema if database_url is provided
            if let Some(ref database_url) = cli.database_url {
                collect::collect_schema(database_url, &cli.output, &cli).await
            } else {
                eprintln!("Error: Database URL is required");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use dbsurveyor_core::MultiDatabaseConfig;
    use dbsurveyor_core::models::*;

    /// Creates a test schema with the given tables.
    pub fn make_test_schema(tables: Vec<Table>) -> DatabaseSchema {
        // Use serde to construct CollectionMetadata with a fixed timestamp
        // to avoid needing chrono as a direct dependency
        let metadata_json = serde_json::json!({
            "collected_at": "2025-01-01T00:00:00Z",
            "collection_duration_ms": 0,
            "collector_version": "test",
            "warnings": []
        });
        let collection_metadata: CollectionMetadata =
            serde_json::from_value(metadata_json).unwrap();

        DatabaseSchema {
            format_version: "1.0".to_string(),
            database_info: DatabaseInfo {
                name: "test_db".to_string(),
                version: Some("15.0".to_string()),
                size_bytes: None,
                encoding: None,
                collation: None,
                owner: None,
                is_system_database: false,
                access_level: AccessLevel::Full,
                collection_status: CollectionStatus::Success,
            },
            tables,
            views: Vec::new(),
            indexes: Vec::new(),
            constraints: Vec::new(),
            procedures: Vec::new(),
            functions: Vec::new(),
            triggers: Vec::new(),
            custom_types: Vec::new(),
            samples: None,
            quality_metrics: None,
            collection_metadata,
        }
    }

    /// Creates a test column with the given name.
    pub fn make_column(name: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: UnifiedDataType::String { max_length: None },
            is_nullable: true,
            is_primary_key: false,
            is_auto_increment: false,
            default_value: None,
            comment: None,
            ordinal_position: 1,
        }
    }

    /// Creates a test table with the given name and columns.
    pub fn make_table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            name: name.to_string(),
            schema: Some("public".to_string()),
            columns,
            primary_key: None,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            constraints: Vec::new(),
            comment: None,
            row_count: None,
        }
    }

    // Multi-database configuration tests

    #[test]
    fn test_multi_db_config_defaults() {
        let config = MultiDatabaseConfig::new();
        assert_eq!(config.max_concurrency, 4);
        assert!(!config.include_system);
        assert!(config.exclude_patterns.is_empty());
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_multi_db_config_from_cli_flags() {
        // Simulate CLI flags: --include-system-databases --exclude-databases test_db,staging
        let config = MultiDatabaseConfig::new()
            .with_include_system(true)
            .with_exclude_patterns(vec!["test_db".to_string(), "staging".to_string()]);
        assert!(config.include_system);
        assert_eq!(config.exclude_patterns.len(), 2);
        assert_eq!(config.exclude_patterns[0], "test_db");
        assert_eq!(config.exclude_patterns[1], "staging");
    }

    #[test]
    fn test_multi_db_config_exclude_patterns_with_globs() {
        let config = MultiDatabaseConfig::new()
            .with_exclude_patterns(vec!["test_*".to_string(), "*_backup".to_string()]);
        assert_eq!(config.exclude_patterns.len(), 2);
    }

    #[test]
    fn test_multi_db_config_continue_on_error_default() {
        let config = MultiDatabaseConfig::new();
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_multi_db_config_min_concurrency() {
        let config = MultiDatabaseConfig::new().with_max_concurrency(0);
        assert_eq!(config.max_concurrency, 1); // Enforced minimum
    }

    #[test]
    fn test_multi_db_result_serialization() {
        use dbsurveyor_core::adapters::config::multi_database::{
            DatabaseCollectionResult, DatabaseFailure, MultiDatabaseMetadata, MultiDatabaseResult,
        };
        use dbsurveyor_core::models::{CollectionMode, ServerInfo};

        let metadata_json = serde_json::json!({
            "started_at": "2025-01-01T00:00:00Z",
            "total_duration_ms": 1500,
            "databases_discovered": 5,
            "databases_filtered": 1,
            "databases_collected": 3,
            "databases_failed": 1,
            "databases_skipped": 0,
            "max_concurrency": 4,
            "collector_version": "test",
            "warnings": []
        });
        let metadata: MultiDatabaseMetadata = serde_json::from_value(metadata_json).unwrap();

        let result = MultiDatabaseResult {
            server_info: ServerInfo {
                server_type: dbsurveyor_core::models::DatabaseType::PostgreSQL,
                version: "16.0".to_string(),
                host: "localhost".to_string(),
                port: Some(5432),
                total_databases: 5,
                collected_databases: 3,
                system_databases_excluded: 2,
                connection_user: "test".to_string(),
                has_superuser_privileges: false,
                collection_mode: CollectionMode::MultiDatabase {
                    discovered: 5,
                    collected: 3,
                    failed: 1,
                },
            },
            databases: vec![DatabaseCollectionResult {
                database_name: "app_db".to_string(),
                schema: make_test_schema(vec![make_table(
                    "users",
                    vec![make_column("id"), make_column("name")],
                )]),
                collection_duration_ms: 500,
            }],
            failures: vec![DatabaseFailure {
                database_name: "broken_db".to_string(),
                error_message: "Connection refused".to_string(),
                is_connection_error: true,
            }],
            collection_metadata: metadata,
        };

        // Verify it serializes to valid JSON
        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("\"app_db\""));
        assert!(json.contains("\"broken_db\""));
        assert!(json.contains("\"databases_collected\": 3"));

        // Verify roundtrip deserialization
        let deserialized: MultiDatabaseResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.databases.len(), 1);
        assert_eq!(deserialized.failures.len(), 1);
        assert_eq!(
            deserialized.collection_metadata.databases_collected,
            3
        );
    }
}
