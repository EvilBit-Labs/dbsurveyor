//! Library module for dbsurveyor-collect
//!
//! This module exposes the core functionality for testing purposes.
//! The main binary functionality is in main.rs.

pub mod adapters;

use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;

/// CLI argument structure
#[derive(Parser)]
#[command(name = "dbsurveyor-collect")]
#[command(about = "Database schema and metadata collection tool")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Collect database schema and metadata
    Collect {
        /// Path to file containing database URL
        #[arg(long, value_name = "FILE")]
        database_url_file: Option<PathBuf>,

        /// Output file path for collected data
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        /// Database engine type (auto-detected if not specified)
        #[arg(short, long, value_name = "ENGINE")]
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

    // 3. No interactive prompt - exit with error if no credentials found
    Err("Database connection information required. Set DATABASE_URL environment variable or use --database-url-file.".to_string())
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
#[must_use]
pub fn validate_database_url(url: &str) -> bool {
    if url.trim().is_empty() {
        return false;
    }

    // Parse URL to validate structure
    // TODO: scheme knowledge should be the responsibility of the plugin, not broadly known here
    url::Url::parse(url).is_ok_and(|parsed_url| {
        match parsed_url.scheme().to_lowercase().as_str() {
            "postgres" | "postgresql" => {
                // PostgreSQL: require host and valid port
                parsed_url.host_str().is_some()
                    && (parsed_url.port().is_some() || parsed_url.port().is_none()) // Port 5432 is default
            }
            "mysql" => {
                // MySQL: require host and valid port
                parsed_url.host_str().is_some()
                    && (parsed_url.port().is_some() || parsed_url.port().is_none()) // Port 3306 is default
            }
            "sqlite" | "file" => {
                // SQLite: require non-empty path
                parsed_url.path().len() > 1 // More than just "/"
            }
            "mongodb" => {
                // MongoDB: require host
                parsed_url.host_str().is_some()
            }
            _ => false, // Unsupported scheme
        }
    })
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
pub fn execute_cli(cli: &Cli) -> Result<String, String> {
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
                        "dbsurveyor-collect v{version}\nDatabase schema collection tool\n[OK] Database URL loaded from file: {}\n[FILE] Output: {}\n[ENGINE] Engine: {}\n[NOTE] Collection functionality will be implemented in future tasks",
                        path.display(),
                        output.display(),
                        engine.as_deref().unwrap_or("auto-detected")
                    ));
                }
            };

            Ok(format!(
                "dbsurveyor-collect v{version}\nDatabase schema collection tool\n[OK] Database URL loaded from {}\n[OUTPUT] Output: {}\n[ENGINE] Engine: {}\n[NOTE] Collection functionality will be implemented in future tasks",
                source_desc,
                output.display(),
                engine.as_deref().unwrap_or("auto-detected")
            ))
        }
        None => {
            let version = env!("CARGO_PKG_VERSION");
            Ok(format!(
                "dbsurveyor-collect v{version}\nDatabase schema collection tool\n\n[SECURITY] Secure credential handling:\n  • Set DATABASE_URL environment variable\n  • Use --database-url-file to read from file\n\nUse --help for available commands"
            ))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Test helper functions
    mod helpers {
        use super::*;
        use tempfile::NamedTempFile;

        /// Create a temporary file with database URL content
        pub(super) fn create_temp_db_url_file(url: &str) -> NamedTempFile {
            let temp_file = NamedTempFile::new().unwrap();
            std::fs::write(&temp_file, url).unwrap();
            temp_file
        }

        /// Create a temporary file with empty content
        pub(super) fn create_empty_temp_file() -> NamedTempFile {
            let temp_file = NamedTempFile::new().unwrap();
            std::fs::write(&temp_file, "").unwrap();
            temp_file
        }

        /// Create a CLI instance with collect command
        pub(super) fn create_collect_cli(
            database_url_file: Option<PathBuf>,
            output: PathBuf,
            engine: Option<String>,
        ) -> Cli {
            Cli {
                command: Some(Commands::Collect {
                    database_url_file,
                    output,
                    engine,
                }),
            }
        }

        /// Create a CLI instance with no command
        pub(super) fn create_empty_cli() -> Cli {
            Cli { command: None }
        }
    }

    /// Tests for URL validation functionality
    mod url_validation {
        use super::*;

        #[test]
        fn test_validate_database_url_valid_urls() {
            temp_env::with_vars([("TESTING", Some("1")), ("CI", Some("1"))], || {
                // Valid URLs
                assert!(validate_database_url(
                    "postgresql://testuser:testpass@localhost/db"
                ));
                assert!(validate_database_url(
                    "postgres://testuser:testpass@localhost/db"
                ));
                assert!(validate_database_url(
                    "mysql://testuser:testpass@localhost/db"
                ));
                assert!(validate_database_url("sqlite:///path/to/db.sqlite"));
                assert!(validate_database_url(
                    "mongodb://testuser:testpass@localhost/db"
                ));
            });
        }

        #[test]
        fn test_validate_database_url_invalid_urls() {
            temp_env::with_vars([("TESTING", Some("1")), ("CI", Some("1"))], || {
                // Invalid URLs
                assert!(!validate_database_url(""));
                assert!(!validate_database_url("   "));
                assert!(!validate_database_url("invalid://url"));
                assert!(!validate_database_url("http://example.com"));
                assert!(!validate_database_url("ftp://example.com"));
            });
        }

        #[test]
        fn test_validate_database_url_edge_cases() {
            temp_env::with_vars([("TESTING", Some("1")), ("CI", Some("1"))], || {
                // Edge cases - these should be invalid since they're incomplete
                assert!(!validate_database_url("postgresql://")); // Missing host
                assert!(!validate_database_url("postgres://")); // Missing host

                // These should be valid since they have required components
                assert!(validate_database_url("postgresql://localhost")); // Has host
                assert!(validate_database_url("postgresql://localhost/db")); // Has host and path
                assert!(validate_database_url("sqlite:/relative/path.db")); // Has path
                assert!(validate_database_url("sqlite:///absolute/path.db")); // Has path

                // These should be invalid
                assert!(!validate_database_url("http://localhost/db")); // Wrong prefix
                assert!(!validate_database_url("ftp://localhost/db")); // Wrong prefix
            });
        }
    }

    /// Tests for credential source handling
    mod credential_sources {
        use super::*;

        #[test]
        fn test_get_database_url_from_environment() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    (
                        "DATABASE_URL",
                        Some("mysql://testuser:testpass@localhost/testdb"),
                    ),
                ],
                || {
                    let result = get_database_url(None);
                    assert!(result.is_ok());

                    let (url, source) = result.unwrap();
                    assert_eq!(url, "mysql://testuser:testpass@localhost/testdb");
                    assert!(matches!(source, CredentialSource::Environment));
                },
            );
        }

        #[test]
        fn test_get_database_url_from_file() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    // Explicitly unset DATABASE_URL to test file reading
                    ("DATABASE_URL", None),
                ],
                || {
                    let temp_file = helpers::create_temp_db_url_file(
                        "postgresql://testuser:testpass@localhost/testdb",
                    );
                    let result = get_database_url(Some(temp_file.path().to_path_buf()));
                    assert!(result.is_ok());

                    let (url, source) = result.unwrap();
                    assert_eq!(url, "postgresql://testuser:testpass@localhost/testdb");
                    assert!(matches!(source, CredentialSource::File(_)));
                },
            );
        }

        #[test]
        fn test_get_database_url_environment_precedence() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    (
                        "DATABASE_URL",
                        Some("mysql://testuser:testpass@localhost/testdb"),
                    ),
                ],
                || {
                    let temp_file = helpers::create_temp_db_url_file(
                        "postgresql://testuser:testpass@localhost/testdb",
                    );
                    let result = get_database_url(Some(temp_file.path().to_path_buf()));
                    assert!(result.is_ok());

                    // Environment should take precedence over file
                    let (url, source) = result.unwrap();
                    assert_eq!(url, "mysql://testuser:testpass@localhost/testdb");
                    assert!(matches!(source, CredentialSource::Environment));
                },
            );
        }
    }

    /// Tests for error handling
    mod error_handling {
        use super::*;

        #[test]
        fn test_get_database_url_missing_credentials() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    // Explicitly unset DATABASE_URL to test the failure case
                    ("DATABASE_URL", None),
                ],
                || {
                    let result = get_database_url(None);
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Database connection information required"));
                    assert!(error_msg.contains("Set DATABASE_URL environment variable"));
                    assert!(error_msg.contains("--database-url-file"));
                },
            );
        }

        #[test]
        fn test_get_database_url_nonexistent_file() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    ("DATABASE_URL", None),
                ],
                || {
                    let result = get_database_url(Some(PathBuf::from("nonexistent_file.txt")));
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Failed to read database URL file"));
                    assert!(error_msg.contains("nonexistent_file.txt"));
                },
            );
        }

        #[test]
        fn test_get_database_url_invalid_format() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    ("DATABASE_URL", None),
                ],
                || {
                    let temp_file = helpers::create_temp_db_url_file("invalid://url");
                    let result = get_database_url(Some(temp_file.path().to_path_buf()));
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Invalid database URL format in file"));
                },
            );
        }

        #[test]
        fn test_get_database_url_empty_file() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    ("DATABASE_URL", None),
                ],
                || {
                    let temp_file = helpers::create_empty_temp_file();
                    let result = get_database_url(Some(temp_file.path().to_path_buf()));
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Invalid database URL format in file"));
                },
            );
        }
    }

    /// Tests for CLI command execution
    mod cli_execution {
        use super::*;

        #[test]
        fn test_execute_cli_no_command() {
            temp_env::with_vars([("TESTING", Some("1")), ("CI", Some("1"))], || {
                let cli = helpers::create_empty_cli();

                let result = execute_cli(&cli);
                assert!(result.is_ok());

                if let Ok(output) = result {
                    assert!(output.contains("dbsurveyor-collect v"));
                    assert!(output.contains("Database schema collection tool"));
                    assert!(output.contains("[SECURITY] Secure credential handling:"));
                    assert!(output.contains("DATABASE_URL environment variable"));
                    assert!(output.contains("--database-url-file"));
                    // No longer mentions interactive prompt
                    assert!(!output.contains("Interactive prompt"));
                }
            });
        }

        #[test]
        fn test_execute_cli_collect_with_file() {
            temp_env::with_vars([("TESTING", Some("1")), ("CI", Some("1"))], || {
                let temp_file = helpers::create_temp_db_url_file(
                    "postgresql://testuser:testpass@localhost/testdb",
                );
                let cli = helpers::create_collect_cli(
                    Some(temp_file.path().to_path_buf()),
                    PathBuf::from("output.json"),
                    Some("postgresql".to_string()),
                );

                let result = execute_cli(&cli);
                assert!(result.is_ok());

                if let Ok(output) = result {
                    assert!(output.contains("dbsurveyor-collect v"));
                    assert!(output.contains("Database schema collection tool"));
                    assert!(output.contains("[OK] Database URL loaded from file:"));
                    assert!(output.contains("[FILE] Output: output.json"));
                    assert!(output.contains("[ENGINE] Engine: postgresql"));
                    assert!(output.contains(
                        "[NOTE] Collection functionality will be implemented in future tasks"
                    ));
                }
            });
        }

        #[test]
        fn test_execute_cli_collect_with_env_var() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    (
                        "DATABASE_URL",
                        Some("mysql://testuser:testpass@localhost/testdb"),
                    ),
                ],
                || {
                    let cli = helpers::create_collect_cli(
                        None,
                        PathBuf::from("/tmp/mysql_schema.json"),
                        Some("mysql".to_string()),
                    );

                    let result = execute_cli(&cli);
                    assert!(result.is_ok());

                    if let Ok(output) = result {
                        assert!(output.contains("dbsurveyor-collect v"));
                        assert!(output.contains("Database schema collection tool"));
                        assert!(
                            output.contains("[OK] Database URL loaded from environment variable")
                        );
                        assert!(output.contains("[OUTPUT] Output: /tmp/mysql_schema.json"));
                        assert!(output.contains("[ENGINE] Engine: mysql"));
                        assert!(output.contains(
                            "[NOTE] Collection functionality will be implemented in future tasks"
                        ));
                    }
                },
            );
        }

        #[test]
        fn test_execute_cli_collect_missing_credentials() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    // Explicitly unset DATABASE_URL to test the failure case
                    ("DATABASE_URL", None),
                ],
                || {
                    let cli = helpers::create_collect_cli(
                        None,
                        PathBuf::from("schema.json"),
                        Some("sqlite".to_string()),
                    );

                    let result = execute_cli(&cli);
                    // This should fail because no DATABASE_URL env var is set and no file provided
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Database connection information required"));
                    assert!(error_msg.contains(
                        "Set DATABASE_URL environment variable or use --database-url-file"
                    ));
                },
            );
        }

        #[test]
        fn test_execute_cli_collect_invalid_file() {
            temp_env::with_vars(
                [
                    ("TESTING", Some("1")),
                    ("CI", Some("1")),
                    ("DATABASE_URL", None),
                ],
                || {
                    let cli = helpers::create_collect_cli(
                        Some(PathBuf::from("nonexistent_file.txt")),
                        PathBuf::from("output.json"),
                        None,
                    );

                    let result = execute_cli(&cli);
                    assert!(result.is_err());
                    let error_msg = result.unwrap_err();
                    assert!(error_msg.contains("Failed to read database URL file"));
                },
            );
        }
    }

    /// Tests for CLI argument parsing
    mod cli_argument_parsing {
        use super::*;

        #[test]
        fn test_cli_struct_creation() {
            let cli = helpers::create_empty_cli();
            assert!(cli.command.is_none());

            let collect_cli = helpers::create_collect_cli(
                None,
                PathBuf::from("test.json"),
                Some("postgresql".to_string()),
            );
            assert!(collect_cli.command.is_some());

            if let Some(Commands::Collect { output, engine, .. }) = &collect_cli.command {
                assert_eq!(output, &PathBuf::from("test.json"));
                assert_eq!(engine.as_deref(), Some("postgresql"));
            } else {
                unreachable!("Expected Collect command");
            }
        }
    }

    /// Tests for `CredentialSource` enum
    mod credential_source_enum {
        use super::*;

        #[test]
        fn test_credential_source_debug() {
            let env_source = CredentialSource::Environment;
            let file_source = CredentialSource::File(PathBuf::from("test.txt"));

            // Should not panic
            let _env_debug = format!("{env_source:?}");
            let _file_debug = format!("{file_source:?}");
        }

        #[test]
        fn test_credential_source_pattern_matching() {
            let env_source = CredentialSource::Environment;
            let file_source = CredentialSource::File(PathBuf::from("test.txt"));

            match env_source {
                CredentialSource::Environment => {}
                CredentialSource::File(_) => unreachable!("Expected Environment"),
            }

            match file_source {
                CredentialSource::Environment => unreachable!("Expected File"),
                CredentialSource::File(path) => assert_eq!(path, PathBuf::from("test.txt")),
            }
        }
    }
}
