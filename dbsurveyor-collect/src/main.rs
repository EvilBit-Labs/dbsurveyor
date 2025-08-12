//! Database schema and metadata collection tool
//!
//! This tool connects to databases to collect schema information,
//! table structures, relationships, and metadata for offline analysis.
//!
//! # Security
//!
//! Database credentials are never exposed on the command line or in help text.
//! Credentials are obtained through secure methods in order of preference:
//! 1. Environment variable (`DATABASE_URL`)
//! 2. Configuration file (--database-url-file)
//! 3. Interactive secure prompt
//!
//! This prevents credential leakage via process lists, shell history, or logs.

use clap::{Parser, Subcommand};
use rpassword::read_password;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Command-line interface for the database collector
#[derive(Parser)]
#[command(name = "dbsurveyor-collect")]
#[command(about = "Database metadata collector for dbsurveyor toolchain")]
#[command(version)]
struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available commands for the collector
#[derive(Subcommand)]
pub enum Commands {
    /// Collect database metadata
    Collect {
        /// Path to file containing database connection URL
        #[arg(long)]
        database_url_file: Option<PathBuf>,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Database engine type
        #[arg(short, long)]
        engine: Option<String>,
    },
}

/// Secure credential input methods
#[derive(Debug)]
pub enum CredentialSource {
    /// Credentials loaded from environment variable
    Environment,
    /// Credentials loaded from file
    File(PathBuf),
    /// Credentials obtained through interactive prompt
    Interactive,
}

/// Get database connection URL from secure sources
///
/// # Arguments
///
/// * `database_url_file` - Optional path to file containing database URL
///
/// # Returns
///
/// Returns the database URL and its source, or an error if no valid URL is found
///
/// # Security
///
/// - Never logs or displays credentials
/// - Uses secure input methods to prevent credential leakage
/// - Validates URL format without exposing sensitive components
///
/// # Errors
///
/// Returns an error if no valid database URL can be found from any source
pub fn get_database_url(
    database_url_file: Option<PathBuf>,
) -> Result<(String, CredentialSource), String> {
    // 1. Try environment variable first
    if let Ok(url) = env::var("DATABASE_URL") {
        if validate_database_url(&url) {
            return Ok((url, CredentialSource::Environment));
        }
    }

    // 2. Try file if provided
    if let Some(file_path) = database_url_file {
        match fs::read_to_string(&file_path) {
            Ok(url) => {
                let url = url.trim();
                if validate_database_url(url) {
                    return Ok((url.to_string(), CredentialSource::File(file_path)));
                }
                return Err(format!(
                    "Invalid database URL format in file: {}",
                    file_path.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Failed to read database URL file {}: {}",
                    file_path.display(),
                    e
                ));
            }
        }
    }

    // 3. Interactive prompt as fallback
    match get_database_url_interactive() {
        Ok(url) => Ok((url, CredentialSource::Interactive)),
        Err(e) => Err(format!("Failed to get database URL interactively: {e}")),
    }
}

/// Validate database URL format without exposing credentials
///
/// # Arguments
///
/// * `url` - Database connection URL to validate
///
/// # Returns
///
/// Returns true if the URL format is valid, false otherwise
///
/// # Security
///
/// This function validates URL format without logging or displaying the URL
fn validate_database_url(url: &str) -> bool {
    if url.trim().is_empty() {
        return false;
    }

    // Basic format validation for common database URLs
    let url_lower = url.to_lowercase();
    url_lower.starts_with("postgres://")
        || url_lower.starts_with("postgresql://")
        || url_lower.starts_with("mysql://")
        || url_lower.starts_with("sqlite://")
        || url_lower.starts_with("mongodb://")
}

/// Get database URL through interactive secure prompt
///
/// # Returns
///
/// Returns the database URL entered by the user
///
/// # Security
///
/// - Uses secure input for password components
/// - Never echoes credentials to terminal
/// - Provides clear guidance without exposing sensitive information
///
/// # Errors
///
/// Returns an error if the interactive prompt fails or is not available
#[allow(clippy::too_many_lines)]
fn get_database_url_interactive() -> Result<String, String> {
    // Check if we're in a test environment (no terminal or CI)
    if !atty::is(atty::Stream::Stdin)
        || std::env::var("CI").is_ok()
        || std::env::var("TESTING").is_ok()
    {
        return Err("Database connection information required. Set DATABASE_URL environment variable or use --database-url-file.".to_string());
    }

    println!("Database connection information required.");
    println!("Enter database connection details securely:");

    // Get database type
    print!("Database type (postgres/mysql/sqlite/mongodb): ");
    io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush stdout: {e}"))?;

    let mut db_type = String::new();
    io::stdin()
        .read_line(&mut db_type)
        .map_err(|e| format!("Failed to read database type: {e}"))?;
    let db_type = db_type.trim().to_lowercase();

    // Get host
    print!("Host (default: localhost): ");
    io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush stdout: {e}"))?;

    let mut host = String::new();
    io::stdin()
        .read_line(&mut host)
        .map_err(|e| format!("Failed to read host: {e}"))?;
    let host = host.trim();
    let host = if host.is_empty() { "localhost" } else { host };

    // Get port based on database type
    let default_port = match db_type.as_str() {
        "postgres" | "postgresql" => "5432",
        "mysql" => "3306",
        "mongodb" => "27017",
        "sqlite" => "", // SQLite doesn't use port
        _ => return Err(format!("Unsupported database type: {db_type}")),
    };

    let port = if default_port.is_empty() {
        String::new()
    } else {
        print!("Port (default: {default_port}): ");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush stdout: {e}"))?;

        let mut port_input = String::new();
        io::stdin()
            .read_line(&mut port_input)
            .map_err(|e| format!("Failed to read port: {e}"))?;
        let port_input = port_input.trim();
        if port_input.is_empty() {
            default_port.to_string()
        } else {
            port_input.to_string()
        }
    };

    // Get database name
    print!("Database name: ");
    io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush stdout: {e}"))?;

    let mut database = String::new();
    io::stdin()
        .read_line(&mut database)
        .map_err(|e| format!("Failed to read database name: {e}"))?;
    let database = database.trim();

    // Get username (except for SQLite)
    let username = if db_type == "sqlite" {
        String::new()
    } else {
        print!("Username: ");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush stdout: {e}"))?;

        let mut username = String::new();
        io::stdin()
            .read_line(&mut username)
            .map_err(|e| format!("Failed to read username: {e}"))?;
        username.trim().to_string()
    };

    // Get password securely (except for SQLite)
    let password = if db_type == "sqlite" {
        String::new()
    } else {
        print!("Password: ");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush stdout: {e}"))?;

        read_password().map_err(|e| format!("Failed to read password: {e}"))?
    };

    // Construct URL based on database type
    let url = match db_type.as_str() {
        "postgres" | "postgresql" => {
            if password.is_empty() {
                format!("postgresql://{username}@{host}:{port}/{database}")
            } else {
                format!("postgresql://{username}:{password}@{host}:{port}/{database}")
            }
        }
        "mysql" => {
            if password.is_empty() {
                format!("mysql://{username}@{host}:{port}/{database}")
            } else {
                format!("mysql://{username}:{password}@{host}:{port}/{database}")
            }
        }
        "mongodb" => {
            if password.is_empty() {
                format!("mongodb://{username}@{host}:{port}/{database}")
            } else {
                format!("mongodb://{username}:{password}@{host}:{port}/{database}")
            }
        }
        "sqlite" => {
            format!("sqlite:{database}")
        }
        _ => return Err(format!("Unsupported database type: {db_type}")),
    };

    Ok(url)
}

/// Execute the CLI command with the given arguments
///
/// # Arguments
///
/// * `cli` - The parsed CLI arguments
///
/// # Returns
///
/// A result indicating success or failure of the operation
///
/// # Errors
///
/// Returns errors for invalid inputs, missing credentials, or command execution failures
///
/// # Security
///
/// - Never logs or displays database credentials
/// - Uses secure credential input methods
/// - Validates inputs without exposing sensitive information
pub(crate) fn execute_cli(cli: &Cli) -> Result<String, String> {
    match &cli.command {
        Some(Commands::Collect {
            database_url_file,
            output,
            engine,
        }) => {
            // Get database URL securely
            let (_database_url, source) = get_database_url(database_url_file.clone())?;

            let version = env!("CARGO_PKG_VERSION");
            let source_desc = match source {
                CredentialSource::Environment => "environment variable",
                CredentialSource::File(path) => {
                    return Ok(format!(
                        "dbsurveyor-collect v{version}\nDatabase schema collection tool\n✅ Database URL loaded from file: {}\n📁 Output: {}\n🔧 Engine: {}\n⚠️  Collection functionality will be implemented in future tasks",
                        path.display(),
                        output.display(),
                        engine.as_deref().unwrap_or("auto-detected")
                    ));
                }
                CredentialSource::Interactive => "interactive prompt",
            };

            Ok(format!(
                "dbsurveyor-collect v{version}\nDatabase schema collection tool\n✅ Database URL loaded from {}\n📁 Output: {}\n🔧 Engine: {}\n⚠️  Collection functionality will be implemented in future tasks",
                source_desc,
                output.display(),
                engine.as_deref().unwrap_or("auto-detected")
            ))
        }
        None => {
            let version = env!("CARGO_PKG_VERSION");
            Ok(format!(
                "dbsurveyor-collect v{version}\nDatabase schema collection tool\n\n🔒 Secure credential handling:\n  • Set DATABASE_URL environment variable\n  • Use --database-url-file to read from file\n  • Interactive prompt as fallback\n\nUse --help for available commands"
            ))
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match execute_cli(&cli) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn setup_test_environment() {
        // Set TESTING environment variable to prevent interactive prompts
        std::env::set_var("TESTING", "1");
    }

    #[test]
    fn test_cli_no_command() {
        setup_test_environment();
        let cli = Cli { command: None };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor-collect v"));
            assert!(output.contains("Database schema collection tool"));
            assert!(output.contains("🔒 Secure credential handling:"));
            assert!(output.contains("DATABASE_URL environment variable"));
            assert!(output.contains("--database-url-file"));
            assert!(output.contains("Interactive prompt as fallback"));
        }
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_cli_collect_command_with_file() {
        setup_test_environment();
        // Create a temporary file with a valid database URL
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        fs::write(&temp_file, "postgresql://user:pass@localhost/testdb")
            .expect("Failed to write to temp file");

        let cli = Cli {
            command: Some(Commands::Collect {
                database_url_file: Some(temp_file.path().to_path_buf()),
                output: PathBuf::from("output.json"),
                engine: Some("postgresql".to_string()),
            }),
        };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor-collect v"));
            assert!(output.contains("Database schema collection tool"));
            assert!(output.contains("✅ Database URL loaded from file:"));
            assert!(output.contains("📁 Output: output.json"));
            assert!(output.contains("🔧 Engine: postgresql"));
            assert!(
                output.contains("⚠️  Collection functionality will be implemented in future tasks")
            );
        }
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_cli_collect_command_without_file() {
        setup_test_environment();
        // Ensure no DATABASE_URL environment variable is set
        env::remove_var("DATABASE_URL");

        let cli = Cli {
            command: Some(Commands::Collect {
                database_url_file: None,
                output: PathBuf::from("schema.json"),
                engine: Some("sqlite".to_string()),
            }),
        };

        let result = execute_cli(&cli);
        // This should fail because no DATABASE_URL env var is set and no file provided
        // The interactive prompt will be called, but in tests it will fail
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Database connection information required"));

        // Clean up
        env::remove_var("DATABASE_URL");
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_cli_collect_command_with_env_var() {
        setup_test_environment();
        // Set environment variable for testing
        env::set_var("DATABASE_URL", "mysql://user:pass@localhost/testdb");

        let cli = Cli {
            command: Some(Commands::Collect {
                database_url_file: None,
                output: PathBuf::from("/tmp/mysql_schema.json"),
                engine: Some("mysql".to_string()),
            }),
        };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor-collect v"));
            assert!(output.contains("Database schema collection tool"));
            assert!(output.contains("✅ Database URL loaded from environment variable"));
            assert!(output.contains("📁 Output: /tmp/mysql_schema.json"));
            assert!(output.contains("🔧 Engine: mysql"));
            assert!(
                output.contains("⚠️  Collection functionality will be implemented in future tasks")
            );
        }

        // Clean up environment variable
        env::remove_var("DATABASE_URL");
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_validate_database_url() {
        setup_test_environment();
        // Valid URLs
        assert!(validate_database_url("postgresql://user:pass@localhost/db"));
        assert!(validate_database_url("postgres://user:pass@localhost/db"));
        assert!(validate_database_url("mysql://user:pass@localhost/db"));
        assert!(validate_database_url("sqlite:///path/to/db.sqlite"));
        assert!(validate_database_url("mongodb://user:pass@localhost/db"));

        // Invalid URLs
        assert!(!validate_database_url(""));
        assert!(!validate_database_url("   "));
        assert!(!validate_database_url("invalid://url"));
        assert!(!validate_database_url("http://example.com"));
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_get_database_url_from_file() {
        setup_test_environment();
        // Create a temporary file with a valid database URL
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        fs::write(&temp_file, "postgresql://user:pass@localhost/testdb")
            .expect("Failed to write to temp file");

        let result = get_database_url(Some(temp_file.path().to_path_buf()));
        assert!(result.is_ok());

        let (url, source) = result.unwrap();
        assert_eq!(url, "postgresql://user:pass@localhost/testdb");
        assert!(matches!(source, CredentialSource::File(_)));
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_get_database_url_from_env() {
        setup_test_environment();
        // Set environment variable
        env::set_var("DATABASE_URL", "mysql://user:pass@localhost/testdb");

        let result = get_database_url(None);
        assert!(result.is_ok());

        let (url, source) = result.unwrap();
        assert_eq!(url, "mysql://user:pass@localhost/testdb");
        assert!(matches!(source, CredentialSource::Environment));

        // Clean up
        env::remove_var("DATABASE_URL");
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_get_database_url_invalid_file() {
        setup_test_environment();
        let result = get_database_url(Some(PathBuf::from("nonexistent_file.txt")));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Failed to read database URL file"));
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_get_database_url_invalid_format() {
        setup_test_environment();
        // Create a temporary file with an invalid database URL
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        fs::write(&temp_file, "invalid://url").expect("Failed to write to temp file");

        let result = get_database_url(Some(temp_file.path().to_path_buf()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid database URL format in file"));
    }

    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_get_database_url_empty_file() {
        setup_test_environment();
        // Create a temporary file with empty content
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        fs::write(&temp_file, "").expect("Failed to write to temp file");

        let result = get_database_url(Some(temp_file.path().to_path_buf()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid database URL format in file"));
    }
}
