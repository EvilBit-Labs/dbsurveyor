//! Database survey data postprocessing and documentation generation tool
//!
//! This tool processes collected database survey data to generate documentation,
//! reports, and analysis outputs in various formats.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

/// Command-line interface for the database postprocessor
#[derive(Parser, Clone, Debug)]
#[command(name = "dbsurveyor")]
#[command(about = "Database metadata processor and report generator")]
#[command(version)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available commands for the postprocessor
#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Process database survey data
    Process {
        /// Input survey file path
        #[arg(short, long)]
        input: PathBuf,

        /// Output format
        #[arg(short, long, value_enum)]
        format: OutputFormat,

        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

/// Available output formats
#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Markdown documentation
    Markdown,
    /// JSON structured output
    Json,
    /// SQL reconstruction
    Sql,
}

/// Security validation error types
#[derive(Debug, thiserror::Error)]
pub enum PathValidationError {
    /// Path contains directory traversal attempt using ".." segments
    #[error("Path contains directory traversal attempt: {path}")]
    DirectoryTraversal {
        /// The malicious path that was rejected
        path: String,
    },

    /// Path contains leading path separator that could escape base directory
    #[error("Path contains leading path separator: {path}")]
    LeadingSeparator {
        /// The path with leading separator that was rejected
        path: String,
    },

    /// Path is outside the allowed base directory
    #[error("Path is outside allowed base directory: {path}")]
    OutsideBaseDirectory {
        /// The path that was outside the allowed directory
        path: String,
    },

    /// Failed to canonicalize the path
    #[error("Failed to canonicalize path: {path}")]
    CanonicalizationFailed {
        /// The path that failed to canonicalize
        path: String,
        /// The underlying IO error
        source: std::io::Error,
    },

    /// Failed to get current working directory
    #[error("Failed to get current working directory")]
    CurrentDirFailed {
        /// The underlying IO error
        source: std::io::Error,
    },
}

/// Validate and sanitize a file path for security
///
/// # Arguments
///
/// * `path` - The path to validate
/// * `base_dir` - The base directory that the path must be within
///
/// # Returns
///
/// Returns the canonicalized path if valid, or an error if validation fails
///
/// # Security
///
/// This function prevents:
/// - Directory traversal attacks using ".." segments
/// - Paths with leading separators that could escape the base directory
/// - Paths outside the allowed base directory
///
/// # Errors
///
/// This function will return an error if:
/// - The path contains ".." segments (directory traversal attempt)
/// - The path has leading separators on relative paths
/// - The path is outside the allowed base directory
/// - The path cannot be canonicalized
pub fn validate_file_path(path: &Path, base_dir: &Path) -> Result<PathBuf, PathValidationError> {
    let path_str = path.to_string_lossy();

    // Check for directory traversal attempts
    if path_str.contains("..") {
        return Err(PathValidationError::DirectoryTraversal {
            path: path_str.to_string(),
        });
    }

    // Check for leading path separators only on relative paths
    if !path.is_absolute() && (path_str.starts_with('/') || path_str.starts_with('\\')) {
        return Err(PathValidationError::LeadingSeparator {
            path: path_str.to_string(),
        });
    }

    // Resolve the path relative to base directory
    let resolved_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };

    // For absolute paths, check if they're within the base directory
    if path.is_absolute() && !resolved_path.starts_with(base_dir) {
        return Err(PathValidationError::OutsideBaseDirectory {
            path: resolved_path.to_string_lossy().to_string(),
        });
    }

    // Try to canonicalize the path if it exists
    let canonical_path = if resolved_path.exists() {
        resolved_path
            .canonicalize()
            .map_err(|e| PathValidationError::CanonicalizationFailed {
                path: path_str.to_string(),
                source: e,
            })?
    } else {
        // For non-existent paths, just use the resolved path
        resolved_path
    };

    // Canonicalize the base directory as well for comparison
    let canonical_base_dir = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());

    // Final check: ensure the canonicalized path is within the canonicalized base directory
    if !canonical_path.starts_with(&canonical_base_dir) {
        return Err(PathValidationError::OutsideBaseDirectory {
            path: canonical_path.to_string_lossy().to_string(),
        });
    }

    Ok(canonical_path)
}

/// Validate all paths in CLI arguments before execution
///
/// # Arguments
///
/// * `cli` - The parsed CLI arguments to validate
///
/// # Returns
///
/// Returns the validated CLI with sanitized paths, or an error if validation fails
///
/// # Security
///
/// This function ensures all file and database paths are secure before processing
///
/// # Errors
///
/// This function will return an error if:
/// - Any path contains directory traversal attempts
/// - Any path has leading separators on relative paths
/// - Any path is outside the current working directory
/// - The current working directory cannot be determined
pub fn validate_cli_paths(cli: &Cli) -> Result<Cli, PathValidationError> {
    // Get current working directory as base directory
    let base_dir =
        std::env::current_dir().map_err(|e| PathValidationError::CurrentDirFailed { source: e })?;

    match &cli.command {
        Some(Commands::Process {
            input,
            format,
            output,
        }) => {
            // Validate input path
            let validated_input = validate_file_path(input, &base_dir)?;

            // Validate output path if provided
            let validated_output = if let Some(output_path) = output {
                Some(validate_file_path(output_path, &base_dir)?)
            } else {
                None
            };

            Ok(Cli {
                command: Some(Commands::Process {
                    input: validated_input,
                    format: format.clone(),
                    output: validated_output,
                }),
            })
        }
        None => Ok(cli.clone()),
    }
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
/// Currently, this function always returns `Ok`. Future implementations
/// may return errors for invalid inputs or command execution failures.
pub fn execute_cli(cli: &Cli) -> Result<String, String> {
    match cli.command {
        Some(Commands::Process {
            input: _,
            format: _,
            output: _,
        }) => {
            let version = env!("CARGO_PKG_VERSION");
            Ok(format!(
                "dbsurveyor v{version}\nDatabase survey postprocessing and documentation tool\n⚠️  Processing functionality will be implemented in future tasks"
            ))
        }
        None => {
            let version = env!("CARGO_PKG_VERSION");
            Ok(format!(
                "dbsurveyor v{version}\nDatabase survey postprocessing and documentation tool\nUse --help for available commands"
            ))
        }
    }
}

fn main() {
    let cli = Cli::parse();

    // Validate and sanitize all paths before execution
    let validated_cli = match validate_cli_paths(&cli) {
        Ok(validated) => validated,
        Err(error) => {
            eprintln!("Security validation failed: {error}");
            std::process::exit(1);
        }
    };

    match execute_cli(&validated_cli) {
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
    use tempfile::tempdir;

    #[test]
    fn test_cli_no_command() {
        let cli = Cli { command: None };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor v"));
            assert!(output.contains("Database survey postprocessing and documentation tool"));
            assert!(output.contains("Use --help for available commands"));
        }
    }

    #[test]
    fn test_cli_process_command() {
        let cli = Cli {
            command: Some(Commands::Process {
                input: PathBuf::from("test.json"),
                format: OutputFormat::Markdown,
                output: Some(PathBuf::from("output.md")),
            }),
        };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor v"));
            assert!(output.contains("Database survey postprocessing and documentation tool"));
            assert!(output.contains("Processing functionality will be implemented in future tasks"));
        }
    }

    #[test]
    fn test_output_format_debug() {
        let format = OutputFormat::Markdown;
        let debug_string = format!("{format:?}");
        assert_eq!(debug_string, "Markdown");

        let format = OutputFormat::Json;
        let debug_string = format!("{format:?}");
        assert_eq!(debug_string, "Json");

        let format = OutputFormat::Sql;
        let debug_string = format!("{format:?}");
        assert_eq!(debug_string, "Sql");
    }

    #[test]
    fn test_output_format_clone() {
        let format = OutputFormat::Markdown;
        let cloned_format = format.clone();

        // Both should format the same way
        assert_eq!(format!("{format:?}"), format!("{cloned_format:?}"));
    }

    #[test]
    fn test_validate_file_path_success() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        // Create a test file
        let test_file = base_dir.join("test.json");
        fs::write(&test_file, "test content")?;

        // Use relative path for validation
        let relative_path = PathBuf::from("test.json");
        let result = validate_file_path(&relative_path, base_dir);
        assert!(result.is_ok());

        let canonical_path = result?;
        let canonical_base_dir = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());
        assert!(canonical_path.starts_with(&canonical_base_dir));

        Ok(())
    }

    #[test]
    fn test_validate_file_path_directory_traversal() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        let malicious_path = PathBuf::from("../../../etc/passwd");
        let result = validate_file_path(&malicious_path, base_dir);

        assert!(result.is_err());
        match result {
            Err(PathValidationError::DirectoryTraversal { path }) => {
                assert!(path.contains(".."));
            }
            Err(_) => return Err("Expected DirectoryTraversal error".into()),
            Ok(_) => return Err("Expected error but got success".into()),
        }

        Ok(())
    }

    #[test]
    fn test_validate_file_path_leading_separator_relative() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        let malicious_path = PathBuf::from("/etc/passwd");
        let result = validate_file_path(&malicious_path, base_dir);

        // Absolute paths should be allowed, but checked against base directory
        // This test should fail because /etc/passwd is outside the base directory
        assert!(result.is_err());
        match result {
            Err(PathValidationError::OutsideBaseDirectory { path }) => {
                assert!(path.contains("/etc/passwd"));
            }
            Err(_) => return Err("Expected OutsideBaseDirectory error".into()),
            Ok(_) => return Err("Expected error but got success".into()),
        }

        Ok(())
    }

    #[test]
    fn test_validate_file_path_leading_separator() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        // Test relative path with leading separator
        let malicious_path = PathBuf::from("/relative/path");
        let result = validate_file_path(&malicious_path, base_dir);

        // This should fail because it's outside the base directory
        assert!(result.is_err());
        match result {
            Err(PathValidationError::OutsideBaseDirectory { path }) => {
                assert!(path.starts_with("/relative/path"));
            }
            Err(_) => return Err("Expected OutsideBaseDirectory error".into()),
            Ok(_) => return Err("Expected error but got success".into()),
        }

        Ok(())
    }

    #[test]
    fn test_validate_file_path_outside_base_directory() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        // Create a file outside the base directory
        let outside_dir = tempfile::tempdir()?;
        let outside_file = outside_dir.path().join("test.json");
        fs::write(&outside_file, "test content")?;

        let result = validate_file_path(&outside_file, base_dir);

        assert!(result.is_err());
        match result {
            Err(PathValidationError::OutsideBaseDirectory { path }) => {
                assert!(!path.starts_with(&base_dir.to_string_lossy().to_string()));
            }
            Err(_) => return Err("Expected OutsideBaseDirectory error".into()),
            Ok(_) => return Err("Expected error but got success".into()),
        }

        Ok(())
    }

    #[test]
    fn test_validate_cli_paths_success() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let base_dir = temp_dir.path();

        // Create test files
        let input_file = base_dir.join("input.json");
        let output_file = base_dir.join("output.md");
        fs::write(&input_file, "test input")?;
        fs::write(&output_file, "test output")?;

        // Change to the temp directory for this test
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(base_dir)?;

        let cli = Cli {
            command: Some(Commands::Process {
                input: PathBuf::from("input.json"),
                format: OutputFormat::Markdown,
                output: Some(PathBuf::from("output.md")),
            }),
        };

        let result = validate_cli_paths(&cli);
        assert!(result.is_ok());

        let validated_cli = result?;
        if let Some(Commands::Process { input, output, .. }) = validated_cli.command {
            // The paths should be within the base directory
            let canonical_base_dir = base_dir
                .canonicalize()
                .unwrap_or_else(|_| base_dir.to_path_buf());
            assert!(input.starts_with(&canonical_base_dir));
            if let Some(output_path) = output {
                assert!(output_path.starts_with(&canonical_base_dir));
            }
        }

        // Restore original directory
        std::env::set_current_dir(original_dir)?;

        Ok(())
    }

    #[test]
    fn test_validate_cli_paths_failure() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let _base_dir = temp_dir.path();

        let malicious_input = PathBuf::from("../../../etc/passwd");

        let cli = Cli {
            command: Some(Commands::Process {
                input: malicious_input,
                format: OutputFormat::Markdown,
                output: None,
            }),
        };

        let result = validate_cli_paths(&cli);
        assert!(result.is_err());
        match result {
            Err(PathValidationError::DirectoryTraversal { .. }) => {
                // Expected error
            }
            Err(_) => return Err("Expected DirectoryTraversal error".into()),
            Ok(_) => return Err("Expected error but got success".into()),
        }

        Ok(())
    }
}
