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

mod collect;
mod output;

use clap::{Args, CommandFactory, Parser, Subcommand};
use dbsurveyor_core::{Result, adapters::create_adapter, init_logging};
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
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
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
            collect::collect_schema(&args.database_url, &output, &cli).await
        }
        Some(Command::Test(args)) => test_connection(&args.database_url).await,
        Some(Command::List) => {
            collect::list_supported_databases();
            Ok(())
        }
        Some(Command::Completions { shell }) => print_completions(*shell),
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

/// Prints shell completion script to stdout. Invoked via the hidden `completions` subcommand.
fn print_completions(shell: clap_complete::Shell) -> Result<()> {
    use std::io::Write;

    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "dbsurveyor-collect", &mut buf);
    std::io::stdout()
        .write_all(&buf)
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: "Failed to write shell completions to stdout".to_string(),
            source: e,
        })?;
    std::io::stdout()
        .flush()
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: "Failed to flush stdout after writing completions".to_string(),
            source: e,
        })?;
    Ok(())
}

/// Tests database connection without collecting schema.
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

    info!("[OK]Connection test successful");
    println!(
        "Connection to {} database successful",
        adapter.database_type()
    );

    Ok(())
}
