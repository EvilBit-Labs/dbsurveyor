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
use dbsurveyor_core::{Result, adapters::create_adapter, error::redact_database_url, init_logging};
use std::path::PathBuf;
use tracing::{error, info};

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
    init_logging(cli.global.verbose, cli.global.quiet)?;

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
            collect_schema(&args.database_url, &output, &cli).await
        }
        Some(Command::Test(args)) => test_connection(&args.database_url).await,
        Some(Command::List) => {
            list_supported_databases();
            Ok(())
        }
        None => {
            // Default behavior: collect schema if database_url is provided
            if let Some(ref database_url) = cli.database_url {
                collect_schema(database_url, &cli.output, &cli).await
            } else {
                eprintln!("Error: Database URL is required");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }
}

/// Tests database connection without collecting schema
async fn test_connection(database_url: &str) -> Result<()> {
    info!("Testing database connection...");

    let adapter = create_adapter(database_url).await.map_err(|e| {
        error!("Failed to create database adapter: {}", e);
        e
    })?;

    info!("Created {} adapter", adapter.database_type());

    adapter.test_connection().await.map_err(|e| {
        error!("Connection test failed: {}", e);
        e
    })?;

    info!("✓ Connection test successful");
    println!(
        "Connection to {} database successful",
        adapter.database_type()
    );

    Ok(())
}

/// Collects database schema and saves to file
async fn collect_schema(database_url: &str, output_path: &PathBuf, cli: &Cli) -> Result<()> {
    info!("Starting schema collection...");
    info!("Target: {}", redact_database_url(database_url));
    info!("Output: {}", output_path.display());

    let adapter = create_adapter(database_url).await.map_err(|e| {
        error!("Failed to create database adapter: {}", e);
        e
    })?;

    info!("Created {} adapter", adapter.database_type());

    // Collect schema
    let schema = adapter.collect_schema().await.map_err(|e| {
        error!("Schema collection failed: {}", e);
        e
    })?;

    info!("✓ Schema collection completed");
    info!("Found {} tables", schema.tables.len());
    info!("Found {} views", schema.views.len());
    info!("Found {} indexes", schema.indexes.len());

    // Save to file
    save_schema(&schema, output_path, cli).await?;

    info!("✓ Schema saved to {}", output_path.display());
    println!("Schema collection completed successfully");
    println!("Output: {}", output_path.display());
    println!("Tables: {}", schema.tables.len());
    println!("Views: {}", schema.views.len());
    println!("Indexes: {}", schema.indexes.len());

    Ok(())
}

/// Saves schema to file with optional compression and encryption
async fn save_schema(
    schema: &dbsurveyor_core::models::DatabaseSchema,
    output_path: &PathBuf,
    cli: &Cli,
) -> Result<()> {
    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(schema).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON serialization", e)
    })?;

    // Validate output against JSON Schema before saving
    let json_value: serde_json::Value = serde_json::from_str(&json_data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("JSON parsing for validation", e)
    })?;

    dbsurveyor_core::validate_schema_output(&json_value).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("Schema validation failed", e)
    })?;

    info!("✓ Output validation passed");

    if cli.encrypt {
        #[cfg(feature = "encryption")]
        {
            save_encrypted(&json_data, output_path).await
        }
        #[cfg(not(feature = "encryption"))]
        {
            Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Encryption not available. Compile with --features encryption",
            ))
        }
    } else if cli.compress {
        #[cfg(feature = "compression")]
        {
            save_compressed(&json_data, output_path).await
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                "Compression not available. Compile with --features compression",
            ))
        }
    } else {
        save_json(&json_data, output_path).await
    }
}

/// Saves JSON data to file
async fn save_json(json_data: &str, output_path: &PathBuf) -> Result<()> {
    tokio::fs::write(output_path, json_data)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write to {}", output_path.display()),
            source: e,
        })?;
    Ok(())
}

/// Saves compressed JSON data
#[cfg(feature = "compression")]
async fn save_compressed(json_data: &str, output_path: &PathBuf) -> Result<()> {
    use std::io::Write;

    let mut encoder = zstd::Encoder::new(Vec::new(), 3).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to create compressor: {}",
            e
        ))
    })?;

    encoder.write_all(json_data.as_bytes()).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!("Compression failed: {}", e))
    })?;

    let compressed_data = encoder.finish().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Compression finalization failed: {}",
            e
        ))
    })?;

    tokio::fs::write(output_path, compressed_data)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!(
                "Failed to write compressed file to {}",
                output_path.display()
            ),
            source: e,
        })?;

    Ok(())
}

/// Saves encrypted JSON data
#[cfg(feature = "encryption")]
async fn save_encrypted(json_data: &str, output_path: &PathBuf) -> Result<()> {
    use dbsurveyor_core::security::encryption::encrypt_data;
    use std::io::{self, Write};

    // Get password from user
    print!("Enter encryption password: ");
    io::stdout().flush().unwrap();
    let password = rpassword::read_password().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to read password: {}",
            e
        ))
    })?;

    if password.is_empty() {
        return Err(dbsurveyor_core::error::DbSurveyorError::configuration(
            "Password cannot be empty",
        ));
    }

    let encrypted = encrypt_data(json_data.as_bytes(), &password)?;
    let encrypted_json = serde_json::to_string_pretty(&encrypted).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::collection_failed("Encryption serialization", e)
    })?;

    tokio::fs::write(output_path, encrypted_json)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!(
                "Failed to write encrypted file to {}",
                output_path.display()
            ),
            source: e,
        })?;

    Ok(())
}

/// Lists supported database types and their connection string formats
fn list_supported_databases() {
    println!("Supported Database Types:");
    println!();

    #[cfg(feature = "postgresql")]
    {
        println!("PostgreSQL:");
        println!("  Connection: postgres://user:password@host:port/database");
        println!("  Example:    postgres://admin:secret@localhost:5432/mydb");
        println!();
    }

    #[cfg(feature = "mysql")]
    {
        println!("MySQL:");
        println!("  Connection: mysql://user:password@host:port/database");
        println!("  Example:    mysql://root:password@localhost:3306/mydb");
        println!();
    }

    #[cfg(feature = "sqlite")]
    {
        println!("SQLite:");
        println!("  Connection: sqlite:///path/to/database.db");
        println!("  Example:    sqlite:///home/user/data.db");
        println!("  Example:    /path/to/database.sqlite");
        println!();
    }

    #[cfg(feature = "mongodb")]
    {
        println!("MongoDB:");
        println!("  Connection: mongodb://user:password@host:port/database");
        println!("  Example:    mongodb://admin:secret@localhost:27017/mydb");
        println!();
    }

    #[cfg(feature = "mssql")]
    {
        println!("SQL Server:");
        println!("  Connection: mssql://user:password@host:port/database");
        println!("  Example:    mssql://sa:password@localhost:1433/mydb");
        println!();
    }

    println!("Output Formats:");
    println!("  .json      - Plain JSON (default)");

    #[cfg(feature = "compression")]
    println!("  .json.zst  - Compressed JSON (--compress)");

    #[cfg(feature = "encryption")]
    println!("  .enc       - Encrypted JSON (--encrypt)");

    println!();
    println!("Security Features:");
    println!("  • Read-only database operations");
    println!("  • Credential sanitization in logs");
    println!("  • Optional AES-GCM encryption");
    println!("  • Offline operation after connection");
}
