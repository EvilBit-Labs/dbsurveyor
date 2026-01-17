//! Database schema documentation and analysis tool.
//!
//! This binary processes collected database schema files and generates
//! comprehensive documentation in various formats including Markdown,
//! HTML, and reconstructed SQL DDL.
//!
//! # Security Guarantees
//! - Operates completely offline (no network connectivity required)
//! - Supports encrypted input files with AES-GCM decryption
//! - Optional data redaction for privacy compliance
//! - No telemetry or external reporting

use clap::{Args, Parser, Subcommand, ValueEnum};
use dbsurveyor_core::{Result, init_logging, models::DatabaseSchema};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "dbsurveyor")]
#[command(about = "Database schema documentation and analysis tool")]
#[command(version)]
#[command(long_about = "
DBSurveyor Postprocessor - Offline schema documentation generator

This tool processes database schema files collected by dbsurveyor-collect
and generates comprehensive documentation and analysis reports.

FEATURES:
- Markdown and HTML report generation
- SQL DDL reconstruction
- Entity Relationship Diagrams (ERD)
- Data classification and analysis
- Privacy-compliant data redaction

INPUT FORMATS:
- .dbsurveyor.json (standard JSON)
- .dbsurveyor.json.zst (compressed)
- .dbsurveyor.enc (encrypted)

OUTPUT FORMATS:
- Markdown documentation
- HTML reports with search
- SQL DDL scripts
- Mermaid ERD diagrams

EXAMPLES:
  dbsurveyor generate schema.dbsurveyor.json
  dbsurveyor --format html --output report.html schema.json
  dbsurveyor --redact-mode conservative schema.enc
")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Input schema file
    #[arg(help = "Path to schema file (.json, .json.zst, or .enc)")]
    pub input: Option<PathBuf>,

    /// Output format
    #[arg(
        short,
        long,
        value_enum,
        default_value = "markdown",
        help = "Output format for documentation"
    )]
    pub format: OutputFormat,

    /// Output file path
    #[arg(
        short,
        long,
        help = "Output file path (auto-detected from format if not specified)"
    )]
    pub output: Option<PathBuf>,

    /// Data redaction mode
    #[arg(
        long,
        value_enum,
        default_value = "balanced",
        help = "Data redaction level for privacy compliance"
    )]
    pub redact_mode: RedactionMode,

    /// Disable data redaction
    #[arg(long, help = "Disable all data redaction (show original sample data)")]
    pub no_redact: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate documentation from schema file
    Generate(GenerateArgs),
    /// Analyze schema for insights and statistics
    Analyze(AnalyzeArgs),
    /// Reconstruct SQL DDL from schema
    Sql(SqlArgs),
    /// Validate schema file format
    Validate(ValidateArgs),
}

#[derive(Args)]
pub struct GenerateArgs {
    /// Input schema file
    #[arg(help = "Path to schema file")]
    pub input: PathBuf,

    /// Output format
    #[arg(short, long, value_enum, default_value = "markdown")]
    pub format: OutputFormat,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct AnalyzeArgs {
    /// Input schema file
    #[arg(help = "Path to schema file")]
    pub input: PathBuf,

    /// Show detailed statistics
    #[arg(long, help = "Show detailed analysis statistics")]
    pub detailed: bool,
}

#[derive(Args)]
pub struct SqlArgs {
    /// Input schema file
    #[arg(help = "Path to schema file")]
    pub input: PathBuf,

    /// Target SQL dialect
    #[arg(
        long,
        value_enum,
        default_value = "postgresql",
        help = "Target SQL dialect for DDL generation"
    )]
    pub dialect: SqlDialect,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ValidateArgs {
    /// Input schema file
    #[arg(help = "Path to schema file")]
    pub input: PathBuf,
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

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// Markdown documentation
    Markdown,
    /// HTML report with search
    Html,
    /// JSON analysis report
    Json,
    /// Mermaid ERD diagram
    Mermaid,
}

#[derive(Clone, ValueEnum)]
pub enum RedactionMode {
    /// No redaction (show all data)
    None,
    /// Minimal redaction (only obvious sensitive fields)
    Minimal,
    /// Balanced redaction (recommended default)
    Balanced,
    /// Conservative redaction (maximum privacy)
    Conservative,
}

#[derive(Clone, ValueEnum)]
pub enum SqlDialect {
    /// PostgreSQL dialect
    PostgreSQL,
    /// MySQL dialect
    MySQL,
    /// SQLite dialect
    SQLite,
    /// SQL Server dialect
    SqlServer,
    /// Generic SQL (ANSI standard)
    Generic,
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
        Some(Command::Generate(args)) => {
            generate_documentation(&args.input, args.format.clone(), args.output.as_ref(), &cli)
                .await
        }
        Some(Command::Analyze(args)) => analyze_schema(&args.input, args.detailed).await,
        Some(Command::Sql(args)) => {
            generate_sql(&args.input, args.dialect.clone(), args.output.as_ref()).await
        }
        Some(Command::Validate(args)) => validate_schema(&args.input).await,
        None => {
            // Default behavior: generate documentation if input is provided
            if let Some(ref input) = cli.input {
                generate_documentation(input, cli.format.clone(), cli.output.as_ref(), &cli).await
            } else {
                eprintln!("Error: Input file is required");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }
}

/// Loads schema from file with support for different formats
async fn load_schema(input_path: &PathBuf) -> Result<DatabaseSchema> {
    info!("Loading schema from {}", input_path.display());

    let file_content = tokio::fs::read(input_path).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to read {}", input_path.display()),
            source: e,
        }
    })?;

    // Detect file format based on extension and content
    let extension = input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    match extension {
        "enc" => {
            #[cfg(feature = "encryption")]
            {
                load_encrypted_schema(&file_content).await
            }
            #[cfg(not(feature = "encryption"))]
            {
                Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                    "Encryption support not available. Compile with --features encryption",
                ))
            }
        }
        "zst" => {
            #[cfg(feature = "compression")]
            {
                load_compressed_schema(&file_content).await
            }
            #[cfg(not(feature = "compression"))]
            {
                Err(dbsurveyor_core::error::DbSurveyorError::configuration(
                    "Compression support not available. Compile with --features compression",
                ))
            }
        }
        _ => {
            // Assume JSON format
            load_json_schema(&file_content).await
        }
    }
}

/// Loads JSON schema from bytes
async fn load_json_schema(data: &[u8]) -> Result<DatabaseSchema> {
    let json_str = std::str::from_utf8(data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in schema file: {}",
            e
        ))
    })?;

    // Use the validation function that combines parsing, validation, and deserialization
    dbsurveyor_core::validate_and_parse_schema(json_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Schema validation failed: {}",
            e
        ))
    })
}

/// Loads compressed schema
#[cfg(feature = "compression")]
async fn load_compressed_schema(data: &[u8]) -> Result<DatabaseSchema> {
    use std::io::Read;

    let mut decoder = zstd::Decoder::new(data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to create decompressor: {}",
            e
        ))
    })?;

    let mut decompressed = String::new();
    decoder.read_to_string(&mut decompressed).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Decompression failed: {}",
            e
        ))
    })?;

    serde_json::from_str(&decompressed).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Serialization {
            context: "Failed to parse decompressed schema JSON".to_string(),
            source: e,
        }
    })
}

/// Loads encrypted schema
#[cfg(feature = "encryption")]
async fn load_encrypted_schema(data: &[u8]) -> Result<DatabaseSchema> {
    use dbsurveyor_core::security::encryption::{EncryptedData, decrypt_data};
    use std::io::{self, Write};

    let json_str = std::str::from_utf8(data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in encrypted file: {}",
            e
        ))
    })?;

    let encrypted: EncryptedData = serde_json::from_str(json_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Serialization {
            context: "Failed to parse encrypted data structure".to_string(),
            source: e,
        }
    })?;

    // Get password from user
    print!("Enter decryption password: ");
    io::stdout().flush().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to flush stdout before reading password: {}",
            e
        ))
    })?;
    let password = rpassword::read_password().map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Failed to read password: {}",
            e
        ))
    })?;

    let decrypted_data = decrypt_data(&encrypted, &password)?;
    let decrypted_str = std::str::from_utf8(&decrypted_data).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::configuration(format!(
            "Invalid UTF-8 in decrypted data: {}",
            e
        ))
    })?;

    serde_json::from_str(decrypted_str).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Serialization {
            context: "Failed to parse decrypted schema JSON".to_string(),
            source: e,
        }
    })
}

/// Generates documentation from schema
async fn generate_documentation(
    input_path: &PathBuf,
    format: OutputFormat,
    output_path: Option<&PathBuf>,
    _cli: &Cli,
) -> Result<()> {
    let schema = load_schema(input_path).await?;

    info!("Loaded schema for database: {}", schema.database_info.name);
    info!("Format version: {}", schema.format_version);
    info!("Tables: {}", schema.tables.len());

    let output_file = match output_path {
        Some(path) => path.clone(),
        None => {
            let base_name = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("schema");

            match format {
                OutputFormat::Markdown => format!("{}.md", base_name).into(),
                OutputFormat::Html => format!("{}.html", base_name).into(),
                OutputFormat::Json => format!("{}_analysis.json", base_name).into(),
                OutputFormat::Mermaid => format!("{}.mmd", base_name).into(),
            }
        }
    };

    match format {
        OutputFormat::Markdown => generate_markdown(&schema, &output_file).await,
        OutputFormat::Html => generate_html(&schema, &output_file).await,
        OutputFormat::Json => generate_json_analysis(&schema, &output_file).await,
        OutputFormat::Mermaid => generate_mermaid(&schema, &output_file).await,
    }?;

    info!("✓ Documentation generated: {}", output_file.display());
    println!("Documentation generated: {}", output_file.display());

    Ok(())
}

/// Generates Markdown documentation (placeholder)
async fn generate_markdown(schema: &DatabaseSchema, output_path: &PathBuf) -> Result<()> {
    let content = format!(
        "# Database Schema: {}\n\n\
        Generated by DBSurveyor v{}\n\
        Collection Date: {}\n\n\
        ## Summary\n\n\
        - **Tables**: {}\n\
        - **Views**: {}\n\
        - **Indexes**: {}\n\n\
        ## Tables\n\n",
        schema.database_info.name,
        schema.collection_metadata.collector_version,
        schema
            .collection_metadata
            .collected_at
            .format("%Y-%m-%d %H:%M:%S UTC"),
        schema.tables.len(),
        schema.views.len(),
        schema.indexes.len()
    );

    tokio::fs::write(output_path, content).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write Markdown to {}", output_path.display()),
            source: e,
        }
    })?;

    Ok(())
}

/// Generates HTML documentation (placeholder)
async fn generate_html(_schema: &DatabaseSchema, output_path: &PathBuf) -> Result<()> {
    let content = "<!DOCTYPE html><html><head><title>Database Schema</title></head><body><h1>Schema Documentation</h1><p>HTML generation not yet implemented</p></body></html>";

    tokio::fs::write(output_path, content).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write HTML to {}", output_path.display()),
            source: e,
        }
    })?;

    Ok(())
}

/// Generates JSON analysis (placeholder)
async fn generate_json_analysis(schema: &DatabaseSchema, output_path: &PathBuf) -> Result<()> {
    let analysis = serde_json::json!({
        "database_name": schema.database_info.name,
        "table_count": schema.tables.len(),
        "view_count": schema.views.len(),
        "index_count": schema.indexes.len(),
        "collection_date": schema.collection_metadata.collected_at
    });

    let content = serde_json::to_string_pretty(&analysis).map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Serialization {
            context: "Failed to serialize analysis".to_string(),
            source: e,
        }
    })?;

    tokio::fs::write(output_path, content).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write JSON analysis to {}", output_path.display()),
            source: e,
        }
    })?;

    Ok(())
}

/// Generates Mermaid ERD (placeholder)
async fn generate_mermaid(_schema: &DatabaseSchema, output_path: &PathBuf) -> Result<()> {
    let content = "erDiagram\n    %% Mermaid ERD generation not yet implemented\n    PLACEHOLDER {\n        string note\n    }";

    tokio::fs::write(output_path, content).await.map_err(|e| {
        dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write Mermaid to {}", output_path.display()),
            source: e,
        }
    })?;

    Ok(())
}

/// Analyzes schema for insights (placeholder)
async fn analyze_schema(input_path: &PathBuf, detailed: bool) -> Result<()> {
    let schema = load_schema(input_path).await?;

    println!("Schema Analysis: {}", schema.database_info.name);
    println!("================");
    println!("Tables: {}", schema.tables.len());
    println!("Views: {}", schema.views.len());
    println!("Indexes: {}", schema.indexes.len());
    println!("Constraints: {}", schema.constraints.len());

    if detailed {
        println!("\nDetailed Analysis:");
        println!("- Procedures: {}", schema.procedures.len());
        println!("- Functions: {}", schema.functions.len());
        println!("- Triggers: {}", schema.triggers.len());
        println!("- Custom Types: {}", schema.custom_types.len());
    }

    Ok(())
}

/// Generates SQL DDL (placeholder)
async fn generate_sql(
    input_path: &PathBuf,
    _dialect: SqlDialect,
    output_path: Option<&PathBuf>,
) -> Result<()> {
    let schema = load_schema(input_path).await?;

    let sql_content = format!(
        "-- Database Schema: {}\n\
        -- Generated by DBSurveyor\n\n\
        -- SQL DDL generation not yet implemented\n\
        -- Tables: {}\n",
        schema.database_info.name,
        schema.tables.len()
    );

    let output_file = match output_path {
        Some(path) => path.clone(),
        None => {
            let base_name = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("schema");
            format!("{}.sql", base_name).into()
        }
    };

    tokio::fs::write(&output_file, sql_content)
        .await
        .map_err(|e| dbsurveyor_core::error::DbSurveyorError::Io {
            context: format!("Failed to write SQL to {}", output_file.display()),
            source: e,
        })?;

    println!("SQL DDL generated: {}", output_file.display());
    Ok(())
}

/// Validates schema file format
async fn validate_schema(input_path: &PathBuf) -> Result<()> {
    let schema = load_schema(input_path).await?;

    println!("✓ Schema file is valid");
    println!("Format version: {}", schema.format_version);
    println!("Database: {}", schema.database_info.name);
    println!("Objects: {}", schema.object_count());

    if !schema.collection_metadata.warnings.is_empty() {
        println!("\nWarnings from collection:");
        for warning in &schema.collection_metadata.warnings {
            println!("  - {}", warning);
        }
    }

    Ok(())
}
