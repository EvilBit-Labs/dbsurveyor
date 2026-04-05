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

mod output;
mod redaction;
mod schema;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use dbsurveyor_core::{Result, init_logging};
pub use redaction::RedactionMode;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dbsurveyor")]
#[command(about = "Database schema documentation and analysis tool")]
#[command(version)]
#[command(long_about = "
DBSurveyor Postprocessor - Offline schema documentation generator

This tool processes database schema files collected by dbsurveyor-collect
and generates comprehensive documentation and analysis reports.

FEATURES:
- Markdown report generation
- Data classification and analysis

EXPERIMENTAL FEATURES (compile-time gated):
- JSON analysis reports
- HTML output
- Mermaid output
- SQL reconstruction

INPUT FORMATS:
- .dbsurveyor.json (standard JSON)
- .dbsurveyor.json.zst (compressed)
- .dbsurveyor.enc (encrypted)

OUTPUT FORMATS:
- Markdown documentation

EXAMPLES:
  dbsurveyor generate schema.dbsurveyor.json
  dbsurveyor generate --format markdown schema.json
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
    #[arg(
        long,
        conflicts_with = "redact_mode",
        help = "Disable all data redaction (show original sample data)"
    )]
    pub no_redact: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Generate documentation from schema file
    Generate(GenerateArgs),
    #[cfg(feature = "experimental")]
    /// Analyze schema for insights and statistics
    Analyze(AnalyzeArgs),
    #[cfg(feature = "experimental")]
    /// Reconstruct SQL DDL from schema
    Sql(SqlArgs),
    /// Validate schema file format
    Validate(ValidateArgs),
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
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

#[cfg(feature = "experimental")]
#[derive(Args)]
pub struct AnalyzeArgs {
    /// Input schema file
    #[arg(help = "Path to schema file")]
    pub input: PathBuf,

    /// Show detailed statistics
    #[arg(long, help = "Show detailed analysis statistics")]
    pub detailed: bool,
}

#[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
    /// HTML report with search (not yet implemented)
    Html,
    #[cfg(feature = "experimental")]
    /// JSON analysis report
    Json,
    #[cfg(feature = "experimental")]
    /// Mermaid ERD diagram (not yet implemented)
    Mermaid,
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
            output::generate_documentation(
                &args.input,
                args.format.clone(),
                args.output.as_ref(),
                &cli,
            )
            .await
        }
        #[cfg(feature = "experimental")]
        Some(Command::Analyze(args)) => output::analyze_schema(&args.input, args.detailed).await,
        #[cfg(feature = "experimental")]
        Some(Command::Sql(args)) => {
            output::generate_sql(&args.input, args.dialect.clone(), args.output.as_ref()).await
        }
        Some(Command::Validate(args)) => output::validate_schema(&args.input).await,
        Some(Command::Completions { shell }) => print_completions(*shell),
        None => {
            // Default behavior: generate documentation if input is provided
            if let Some(ref input) = cli.input {
                output::generate_documentation(input, cli.format.clone(), cli.output.as_ref(), &cli)
                    .await
            } else {
                eprintln!("Error: Input file is required");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }
}

/// Creates a progress spinner with the given message and a 120ms tick interval.
///
/// The spinner draw target is hidden when `NO_COLOR` is set, `TERM=dumb`,
/// or stdout is not a TTY.
pub(crate) fn create_spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    if dbsurveyor_core::should_disable_color()
        || !std::io::IsTerminal::is_terminal(&std::io::stdout())
    {
        pb.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(120));
    pb
}

/// Prints shell completion script to stdout. Invoked via the hidden `completions` subcommand.
fn print_completions(shell: clap_complete::Shell) -> Result<()> {
    use std::io::Write;

    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "dbsurveyor", &mut buf);
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
